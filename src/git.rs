use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use blocking::unblock;
use futures::stream::{self, StreamExt};

pub type Result<T> = std::result::Result<T, String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiffStats {
    pub added: u32,
    pub removed: u32,
}

impl DiffStats {
    pub fn is_clean(&self) -> bool {
        self.added == 0 && self.removed == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub name: String,
    pub path: PathBuf,
    pub is_main: bool,
    pub diff: Option<DiffStats>,
}

impl Worktree {
    /// Leading branch name segment, like "feat" in "feat/lalala".
    pub fn branch_prefix(&self) -> &str {
        &self.name[..self.name.find(['/', '-', '_']).unwrap_or(self.name.len())]
    }

    /// A worktree known only by its path, not yet listed by git.
    pub fn placeholder(path: PathBuf) -> Self {
        Self {
            name: dir_name(&path),
            path,
            is_main: false,
            diff: None,
        }
    }
}

/// A repository's main worktree plus a stable identity (the marcgit root, or the main worktree itself).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectInfo {
    pub root: PathBuf,
    pub main: PathBuf,
    pub name: String,
}

pub fn is_flatpak() -> bool {
    std::env::var("FLATPAK_ID").is_ok()
}

/// Whether the host can wrap spawned shells in transient systemd scopes.
pub fn host_has_systemd_run() -> bool {
    run("systemd-run", &["--version"], Path::new("/")).is_ok()
}

fn host_command(program: &str, cwd: &Path) -> Command {
    if is_flatpak() {
        let mut cmd = Command::new("flatpak-spawn");
        cmd.arg("--host");
        cmd.arg(format!("--directory={}", cwd.display()));
        cmd.arg(program);
        cmd
    } else {
        let mut cmd = Command::new(program);
        cmd.current_dir(cwd);
        cmd
    }
}

/// Run `program` on the host (through `flatpak-spawn` when sandboxed), capturing stdout.
fn run(program: &str, args: &[&str], cwd: &Path) -> Result<String> {
    let mut cmd = host_command(program, cwd);
    cmd.args(args);
    cmd.stdin(Stdio::null());
    let out = cmd
        .output()
        .map_err(|e| format!("failed to run {program}: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "{program} {} failed: {}",
            args.join(" "),
            stderr.trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn dir_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// Entries reported by git, main worktree first (git's guaranteed order).
fn worktree_entries(cwd: &Path) -> Result<Vec<Worktree>> {
    let out = run("git", &["worktree", "list", "--porcelain"], cwd)?;
    let mut worktrees: Vec<Worktree> = out.split("\n\n").filter_map(parse_worktree).collect();
    if let Some(first) = worktrees.first_mut() {
        first.is_main = true;
    }
    Ok(worktrees)
}

/// Resolve a project from any path inside a git repository or a marcgit root.
pub fn detect_project(path: &Path) -> Result<ProjectInfo> {
    if path.join("trunk").join(".git").exists() {
        return detect_project(&path.join("trunk"));
    }
    if !path.is_dir() {
        return Err(format!("{} is not a directory", path.display()));
    }
    let entries = worktree_entries(path)
        .map_err(|_| format!("{} is not inside a git repository", path.display()))?;
    let main = entries
        .into_iter()
        .next()
        .ok_or("git reported no worktrees")?
        .path;
    let root = match (main.file_name(), main.parent()) {
        (Some(name), Some(parent)) if name == "trunk" => parent.to_path_buf(),
        _ => main.clone(),
    };
    Ok(ProjectInfo {
        name: dir_name(&root),
        root,
        main,
    })
}

/// Whether `root` still looks like a project, without invoking git.
pub fn project_exists(root: &Path) -> bool {
    root.join(".git").exists() || root.join("trunk").join(".git").exists()
}

/// List the repository's worktrees, main first, skipping hidden rows' diff stats.
pub async fn list_worktrees(
    main: PathBuf,
    skip_diffs: Vec<String>,
    skip_all_diffs: bool,
) -> Result<Vec<Worktree>> {
    let mut worktrees = unblock(move || worktree_entries(&main)).await?;

    let diffs: Vec<Option<DiffStats>> = stream::iter(worktrees.iter().map(|worktree| {
        let path = worktree.path.clone();
        let wanted = !(skip_all_diffs || skip_diffs.contains(&worktree.name));
        async move {
            if wanted {
                Some(unblock(move || diff_stats(&path)).await)
            } else {
                None
            }
        }
    }))
    .buffered(4)
    .collect()
    .await;

    for (worktree, diff) in worktrees.iter_mut().zip(diffs) {
        worktree.diff = diff;
    }
    Ok(worktrees)
}

/// Lines added/removed against HEAD (staged + unstaged, binary files ignored).
pub fn diff_stats(worktree: &Path) -> DiffStats {
    let Ok(out) = run("git", &["diff", "HEAD", "--numstat"], worktree) else {
        return DiffStats::default();
    };
    let mut stats = DiffStats::default();
    for line in out.lines() {
        let mut parts = line.split_whitespace();
        if let (Some(added), Some(removed)) = (parts.next(), parts.next()) {
            stats.added += added.parse::<u32>().unwrap_or(0);
            stats.removed += removed.parse::<u32>().unwrap_or(0);
        }
    }
    stats
}

fn parse_worktree(block: &str) -> Option<Worktree> {
    let path = block
        .lines()
        .find_map(|line| line.strip_prefix("worktree "))
        .map(PathBuf::from)?;
    path.file_name()?;
    Some(Worktree::placeholder(path))
}

/// Run blocking work (subprocesses, fs) off the UI thread.
pub async fn run_async<T: Send + 'static>(
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    unblock(f).await
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;

    fn sh(dir: &Path, cmd: &str) {
        let status = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .current_dir(dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "command failed: {cmd}");
    }

    fn make_project(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("marcterm-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let trunk = root.join("trunk");
        std::fs::create_dir_all(&trunk).unwrap();
        sh(&trunk, "git init -q -b main");
        sh(
            &trunk,
            "git config user.email test@test && git config user.name Test",
        );
        sh(
            &trunk,
            "echo hello > file.txt && git add . && git commit -qm init",
        );
        root
    }

    #[test]
    fn marcgit_layout() {
        let root = make_project("lifecycle");
        let trunk = root.join("trunk");

        for path in [&root, &trunk] {
            let info = detect_project(path).unwrap();
            assert_eq!(info.root, root);
            assert_eq!(info.main, trunk);
            assert_eq!(info.name, root.file_name().unwrap().to_str().unwrap());
        }
        assert!(detect_project(&std::env::temp_dir()).is_err());
        assert!(project_exists(&root));

        sh(&trunk, "git worktree add -q -b feat/login ../feat-login");
        let wt_path = root.join("feat-login");
        assert_eq!(detect_project(&wt_path).unwrap().root, root);

        let listed = block_on(list_worktrees(trunk.clone(), vec![], false)).unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].path, trunk);
        assert!(listed[0].is_main);
        assert_eq!(listed[1].name, "feat-login");
        assert!(!listed[1].is_main);
        assert_eq!(listed[1].diff, Some(DiffStats::default()));

        let skipped =
            block_on(list_worktrees(trunk.clone(), vec!["feat-login".to_string()], false)).unwrap();
        assert_eq!(skipped[1].diff, None);

        std::fs::write(wt_path.join("file.txt"), "hello\nworld\n").unwrap();
        let stats = diff_stats(&wt_path);
        assert_eq!((stats.added, stats.removed), (1, 0));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn generic_repo() {
        let base =
            std::env::temp_dir().join(format!("marcterm-test-generic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let repo = base.join("myrepo");
        std::fs::create_dir_all(&repo).unwrap();
        sh(&repo, "git init -q -b main");
        sh(
            &repo,
            "git config user.email test@test && git config user.name Test",
        );
        sh(
            &repo,
            "echo hi > file.txt && git add . && git commit -qm init",
        );

        let info = detect_project(&repo).unwrap();
        assert_eq!(info.root, repo);
        assert_eq!(info.main, repo);
        assert_eq!(info.name, "myrepo");
        assert!(project_exists(&repo));

        sh(
            &repo,
            "git worktree add -q -b feature ../elsewhere/feature-wt",
        );
        let wt_path = base.join("elsewhere/feature-wt");
        assert_eq!(detect_project(&wt_path).unwrap().root, repo);

        let listed = block_on(list_worktrees(repo.clone(), vec![], false)).unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].path, repo);
        assert!(listed[0].is_main);
        assert_eq!(listed[1].name, "feature-wt");

        let _ = std::fs::remove_dir_all(&base);
    }
}

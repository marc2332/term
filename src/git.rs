use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use blocking::unblock;
use futures::stream::{self, StreamExt};
use gix::bstr::{BString, ByteSlice};
use gix::diff::blob::pipeline::{Mode, WorktreeRoots};
use gix::diff::blob::platform::prepare_diff::Operation;
use gix::diff::blob::{Diff, ResourceKind};
use gix::object::tree::EntryKind;

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
    /// Checked out branch, `None` when HEAD is detached.
    pub branch: Option<String>,
    pub diff: Option<DiffStats>,
    pub last_commit: Option<SystemTime>,
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
            branch: None,
            diff: None,
            last_commit: None,
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

pub fn dir_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn git_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

/// The main worktree first, then every linked worktree git knows about.
fn worktree_entries(main: &Path) -> Result<Vec<Worktree>> {
    let repo = gix::open(main).map_err(git_error)?;
    let mut worktrees = vec![Worktree {
        is_main: true,
        branch: repo
            .head_name()
            .ok()
            .flatten()
            .map(|name| name.shorten().to_string()),
        ..Worktree::placeholder(main.to_path_buf())
    }];

    for proxy in repo.worktrees().map_err(git_error)? {
        let Ok(path) = proxy.base() else {
            continue;
        };
        let branch = proxy
            .into_repo_with_possibly_inaccessible_worktree()
            .ok()
            .and_then(|repo| repo.head_name().ok().flatten())
            .map(|name| name.shorten().to_string());
        worktrees.push(Worktree {
            branch,
            ..Worktree::placeholder(path)
        });
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
    let repo = gix::discover(path)
        .map_err(|_| format!("{} is not inside a git repository", path.display()))?;
    let main = repo
        .main_repo()
        .map_err(git_error)?
        .workdir()
        .ok_or("bare repositories have no worktree")?
        .to_path_buf();

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
    let (mut worktrees, commit_times) = unblock(move || {
        worktree_entries(&main).map(|worktrees| (worktrees, branch_commit_times(&main)))
    })
    .await?;

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
        worktree.last_commit = worktree
            .branch
            .as_ref()
            .and_then(|branch| commit_times.get(branch))
            .copied();
    }
    Ok(worktrees)
}

/// Committer time of every local branch's tip.
fn branch_commit_times(main: &Path) -> HashMap<String, SystemTime> {
    let times = || -> Option<HashMap<String, SystemTime>> {
        let repo = gix::open(main).ok()?;
        let references = repo.references().ok()?;
        let branches = references.local_branches().ok()?;
        Some(
            branches
                .flatten()
                .filter_map(|mut branch| {
                    let name = branch.name().shorten().to_string();
                    let seconds = branch.peel_to_commit().ok()?.time().ok()?.seconds;
                    let seconds = u64::try_from(seconds).ok()?;
                    Some((name, UNIX_EPOCH + Duration::from_secs(seconds)))
                })
                .collect(),
        )
    };
    times().unwrap_or_default()
}

/// Lines added/removed against HEAD (staged + unstaged, binary files ignored).
fn diff_stats(worktree: &Path) -> DiffStats {
    let stats = || -> Option<DiffStats> {
        let repo = gix::open(worktree).ok()?;
        let head = repo.head_tree().ok()?;
        let changes = repo
            .status(gix::progress::Discard)
            .ok()?
            .untracked_files(gix::status::UntrackedFiles::None)
            .tree_index_track_renames(gix::status::tree_index::TrackRenames::Disabled)
            .index_worktree_rewrites(None)
            .into_iter(Vec::new())
            .ok()?;
        let paths: HashSet<BString> = changes
            .flatten()
            .map(|change| change.location().to_owned())
            .collect();

        let roots = WorktreeRoots {
            old_root: None,
            new_root: Some(worktree.to_path_buf()),
        };
        let mut cache = repo.diff_resource_cache(Mode::ToGit, roots).ok()?;
        let null = gix::ObjectId::null(repo.object_hash());

        let mut stats = DiffStats::default();
        for path in paths {
            let (id, kind) = match head
                .lookup_entry_by_path(gix::path::from_bstr(path.as_bstr()))
                .ok()?
            {
                Some(entry) => (entry.id().detach(), entry.mode().kind()),
                None => (null, EntryKind::Blob),
            };
            let Ok(()) =
                cache.set_resource(id, kind, path.as_bstr(), ResourceKind::OldOrSource, &repo)
            else {
                continue;
            };
            let Ok(()) = cache.set_resource(
                null,
                EntryKind::Blob,
                path.as_bstr(),
                ResourceKind::NewOrDestination,
                &repo,
            ) else {
                continue;
            };
            let Ok(outcome) = cache.prepare_diff() else {
                continue;
            };
            let Operation::InternalDiff { algorithm } = outcome.operation else {
                continue;
            };
            let diff = Diff::compute(algorithm, &outcome.interned_input());
            stats.added += diff.count_additions();
            stats.removed += diff.count_removals();
        }
        Some(stats)
    };
    stats().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::process::{Command, Stdio};

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
        assert_eq!(listed[1].branch.as_deref(), Some("feat/login"));
        assert!(listed[1].last_commit.is_some());

        let skipped = block_on(list_worktrees(
            trunk.clone(),
            vec!["feat-login".to_string()],
            false,
        ))
        .unwrap();
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

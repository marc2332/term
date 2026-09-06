use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::flatpak;
use crate::state::{AppState, PanelNode, Tab, WorktreeGroup};

/// Seconds since the Unix epoch, and how to render them for humans.
pub struct Timestamp;

impl Timestamp {
    pub fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    pub fn ago(secs: u64) -> String {
        let elapsed = Self::now().saturating_sub(secs);
        match elapsed {
            0..60 => "just now".to_string(),
            60..3600 => format!("{}m ago", elapsed / 60),
            3600..86_400 => format!("{}h ago", elapsed / 3600),
            _ => format!("{}d ago", elapsed / 86_400),
        }
    }
}

/// Machine-state dir, targeting the host's ~/.local/state inside Flatpak.
pub struct StateDir;

impl StateDir {
    pub fn path() -> PathBuf {
        if flatpak::is_flatpak() {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            return PathBuf::from(home).join(".local/state/marcterm");
        }
        dirs::state_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("marcterm")
    }

    fn load_json<T: for<'de> Deserialize<'de>>(file: &str) -> Option<T> {
        let raw = std::fs::read_to_string(Self::path().join(file)).ok()?;
        serde_json::from_str(&raw).ok()
    }

    fn save_json<T: Serialize>(file: &str, value: &T) {
        let dir = Self::path();
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        if let Ok(raw) = serde_json::to_string_pretty(value) {
            let _ = std::fs::write(dir.join(file), raw);
        }
    }
}

/// Persisted shape of a tab's panel tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PanelLayout {
    Leaf { cwd: Option<PathBuf> },
    Horizontal(Box<PanelLayout>, Box<PanelLayout>),
    Vertical(Box<PanelLayout>, Box<PanelLayout>),
}

impl PanelLayout {
    fn capture(node: &PanelNode) -> Self {
        match node {
            PanelNode::Leaf(_, handle, _) => Self::Leaf { cwd: handle.cwd() },
            PanelNode::Horizontal(a, b) => {
                Self::Horizontal(Box::new(Self::capture(a)), Box::new(Self::capture(b)))
            }
            PanelNode::Vertical(a, b) => {
                Self::Vertical(Box::new(Self::capture(a)), Box::new(Self::capture(b)))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionTab {
    #[serde(default)]
    pub worktree: Option<PathBuf>,
    pub custom_title: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    pub layout: PanelLayout,
    pub active_leaf: usize,
}

impl SessionTab {
    fn capture(tab: &Tab) -> Self {
        let active_leaf = tab
            .panels
            .leaves()
            .iter()
            .position(|id| *id == tab.active_panel)
            .unwrap_or(0);
        Self {
            worktree: tab.worktree.clone(),
            custom_title: tab.custom_title.clone(),
            group: tab.group.clone(),
            layout: PanelLayout::capture(&tab.panels),
            active_leaf,
        }
    }
}

/// An open project and its open tabs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionProject {
    pub root: PathBuf,
    #[serde(default)]
    pub tabs: Vec<SessionTab>,
}

/// One run's open projects, loose tabs and active tab.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub started_at: u64,
    pub saved_at: u64,
    pub projects: Vec<SessionProject>,
    pub loose_tabs: Vec<SessionTab>,
    pub active_tab: usize,
}

impl Session {
    const FILE: &'static str = "sessions.json";
    const MAX: usize = 7;

    /// Snapshot open projects, their tabs (with panel layouts) and loose tabs.
    pub fn capture(state: &AppState) -> Self {
        let mut ordered: Vec<&Tab> = Vec::with_capacity(state.tabs.len());
        let projects = state
            .projects
            .iter()
            .map(|project| {
                let tabs: Vec<&Tab> = state
                    .tabs
                    .iter()
                    .filter(|t| t.project == Some(project.id))
                    .collect();
                ordered.extend(&tabs);
                SessionProject {
                    root: project.root.clone(),
                    tabs: tabs.iter().map(|t| SessionTab::capture(t)).collect(),
                }
            })
            .collect();
        let loose: Vec<&Tab> = state.tabs.iter().filter(|t| t.project.is_none()).collect();
        ordered.extend(&loose);

        let active_tab = state
            .active_tab()
            .and_then(|active| ordered.iter().position(|t| t.id == active.id))
            .unwrap_or(0);

        Self {
            started_at: state.started_at,
            saved_at: Timestamp::now(),
            projects,
            loose_tabs: loose.iter().map(|t| SessionTab::capture(t)).collect(),
            active_tab,
        }
    }

    pub fn load_all() -> Vec<Self> {
        let mut sessions: Vec<Self> = StateDir::load_json(Self::FILE).unwrap_or_default();
        sessions.retain(|s| !s.is_empty());
        sessions.truncate(Self::MAX);
        sessions
    }

    pub fn remove(started_at: u64) {
        let mut sessions = Self::load_all();
        sessions.retain(|s| s.started_at != started_at);
        StateDir::save_json(Self::FILE, &sessions);
    }

    /// Insert or update this run's entry, newest first, deduplicated and capped.
    pub fn save_as_current(&self) {
        if self.is_empty() {
            return;
        }
        let mut sessions = Self::load_all();
        sessions.retain(|s| s.started_at != self.started_at && !s.content_eq(self));
        sessions.insert(0, self.clone());
        sessions.truncate(Self::MAX);
        StateDir::save_json(Self::FILE, &sessions);
    }

    pub fn is_empty(&self) -> bool {
        self.projects.is_empty() && self.loose_tabs.is_empty()
    }

    pub fn content_eq(&self, other: &Self) -> bool {
        self.projects == other.projects
            && self.loose_tabs == other.loose_tabs
            && self.active_tab == other.active_tab
    }

    /// Drop every worktree tab but the active one, so startup spawns a single terminal per session.
    pub fn without_inactive_worktrees(&self) -> Self {
        let mut pruned = self.clone();
        let mut active_tab = self.active_tab;
        let mut index = 0;
        let lists = pruned
            .projects
            .iter_mut()
            .map(|project| &mut project.tabs)
            .chain([&mut pruned.loose_tabs]);
        for tabs in lists {
            tabs.retain(|tab| {
                let keep = tab.worktree.is_none() || index == self.active_tab;
                if !keep && index < self.active_tab {
                    active_tab -= 1;
                }
                index += 1;
                keep
            });
        }
        pruned.active_tab = active_tab;
        pruned
    }

    pub fn summary(&self) -> String {
        let names: Vec<String> = self
            .projects
            .iter()
            .filter_map(|p| p.root.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .collect();
        let count =
            self.loose_tabs.len() + self.projects.iter().map(|p| p.tabs.len()).sum::<usize>();
        let tabs = format!("{count} terminal{}", if count == 1 { "" } else { "s" });
        match (names.is_empty(), count == 0) {
            (true, _) => tabs,
            (false, true) => names.join(", "),
            (false, false) => format!("{} · {tabs}", names.join(", ")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecentProject {
    pub root: PathBuf,
}

impl RecentProject {
    const FILE: &'static str = "projects.json";
    const MAX: usize = 7;

    pub fn load_all() -> Vec<Self> {
        let mut recent: Vec<Self> = StateDir::load_json(Self::FILE).unwrap_or_default();
        recent.truncate(Self::MAX);
        recent
    }

    /// Move `roots` to the front of the recent-projects history, in order.
    pub fn touch_many(roots: &[PathBuf]) {
        let mut recent = Self::load_all();
        for root in roots.iter().rev() {
            recent.retain(|p| &p.root != root);
            recent.insert(0, Self { root: root.clone() });
        }
        recent.truncate(Self::MAX);
        StateDir::save_json(Self::FILE, &recent);
    }

    pub fn touch(root: &Path) {
        Self::touch_many(std::slice::from_ref(&root.to_path_buf()));
    }

    pub fn remove(root: &Path) {
        let mut recent = Self::load_all();
        recent.retain(|p| p.root != root);
        StateDir::save_json(Self::FILE, &recent);
    }
}

/// Per-project sidebar prefs, `worktrees` is the legacy name for `archived`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ProjectPrefs {
    root: PathBuf,
    #[serde(default, rename = "worktrees")]
    pub archived: Vec<String>,
    #[serde(default)]
    pub order: Vec<String>,
    #[serde(default)]
    pub groups: Vec<WorktreeGroup>,
}

impl ProjectPrefs {
    const FILE: &'static str = "archived.json";

    pub fn load(root: &Path) -> Self {
        StateDir::load_json::<Vec<Self>>(Self::FILE)
            .unwrap_or_default()
            .into_iter()
            .find(|e| e.root == root)
            .unwrap_or_default()
    }

    pub fn save_archived(root: &Path, worktrees: &[String]) {
        Self::update(root, |prefs| prefs.archived = worktrees.to_vec());
    }

    pub fn save_order(root: &Path, order: &[String]) {
        Self::update(root, |prefs| prefs.order = order.to_vec());
    }

    pub fn save_groups(root: &Path, groups: &[WorktreeGroup]) {
        Self::update(root, |prefs| prefs.groups = groups.to_vec());
    }

    fn update(root: &Path, f: impl FnOnce(&mut Self)) {
        let mut entries: Vec<Self> = StateDir::load_json(Self::FILE).unwrap_or_default();
        let mut entry = entries
            .iter()
            .position(|e| e.root == root)
            .map(|i| entries.remove(i))
            .unwrap_or_else(|| Self {
                root: root.to_path_buf(),
                ..Default::default()
            });
        f(&mut entry);
        if !entry.archived.is_empty() || !entry.order.is_empty() || !entry.groups.is_empty() {
            entries.push(entry);
        }
        StateDir::save_json(Self::FILE, &entries);
    }
}

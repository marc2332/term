use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::git;
use crate::state::{AppState, PanelNode, Tab};

const MAX_SESSIONS: usize = 10;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PanelLayout {
    Leaf { cwd: Option<PathBuf> },
    Horizontal(Box<PanelLayout>, Box<PanelLayout>),
    Vertical(Box<PanelLayout>, Box<PanelLayout>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionTab {
    #[serde(default)]
    pub worktree: Option<PathBuf>,
    pub custom_title: Option<String>,
    pub layout: PanelLayout,
    pub active_leaf: usize,
}

/// An open project and its open tabs; the worktree list itself belongs to git.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionProject {
    pub root: PathBuf,
    #[serde(default)]
    pub tabs: Vec<SessionTab>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub started_at: u64,
    pub saved_at: u64,
    pub projects: Vec<SessionProject>,
    pub loose_tabs: Vec<SessionTab>,
    pub active_tab: usize,
}

impl Session {
    pub fn is_empty(&self) -> bool {
        self.projects.is_empty() && self.loose_tabs.is_empty()
    }

    pub fn content_eq(&self, other: &Self) -> bool {
        self.projects == other.projects
            && self.loose_tabs == other.loose_tabs
            && self.active_tab == other.active_tab
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

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn time_ago(secs: u64) -> String {
    let elapsed = now_secs().saturating_sub(secs);
    match elapsed {
        0..60 => "just now".to_string(),
        60..3600 => format!("{}m ago", elapsed / 60),
        3600..86_400 => format!("{}h ago", elapsed / 3600),
        _ => format!("{}d ago", elapsed / 86_400),
    }
}

/// Machine-state dir, targeting the host's ~/.local/state inside Flatpak.
fn state_dir() -> PathBuf {
    if git::is_flatpak() {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        return PathBuf::from(home).join(".local/state/marcterm");
    }
    dirs::state_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("marcterm")
}

fn load_json<T: for<'de> Deserialize<'de>>(file: &str) -> Option<T> {
    let raw = std::fs::read_to_string(state_dir().join(file)).ok()?;
    serde_json::from_str(&raw).ok()
}

fn save_json<T: Serialize>(file: &str, value: &T) {
    let dir = state_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(raw) = serde_json::to_string_pretty(value) {
        let _ = std::fs::write(dir.join(file), raw);
    }
}

pub fn load_recent_projects() -> Vec<RecentProject> {
    load_json("projects.json").unwrap_or_default()
}

/// Move `roots` to the front of the recent-projects history, in order.
pub fn touch_recent_projects(roots: &[PathBuf]) {
    let mut recent = load_recent_projects();
    for root in roots.iter().rev() {
        recent.retain(|p| &p.root != root);
        recent.insert(0, RecentProject { root: root.clone() });
    }
    recent.truncate(20);
    save_json("projects.json", &recent);
}

pub fn touch_recent_project(root: &Path) {
    touch_recent_projects(std::slice::from_ref(&root.to_path_buf()));
}

pub fn remove_recent_project(root: &Path) {
    let mut recent = load_recent_projects();
    recent.retain(|p| p.root != root);
    save_json("projects.json", &recent);
}

pub fn load_sessions() -> Vec<Session> {
    let mut sessions: Vec<Session> = load_json("sessions.json").unwrap_or_default();
    sessions.retain(|s| !s.is_empty());
    sessions
}

pub fn remove_session(started_at: u64) {
    let mut sessions = load_sessions();
    sessions.retain(|s| s.started_at != started_at);
    save_json("sessions.json", &sessions);
}

/// Insert or update this run's entry, newest first, deduplicated and capped.
pub fn update_current_session(session: &Session) {
    if session.is_empty() {
        return;
    }
    let mut sessions = load_sessions();
    sessions.retain(|s| s.started_at != session.started_at && !s.content_eq(session));
    sessions.insert(0, session.clone());
    sessions.truncate(MAX_SESSIONS);
    save_json("sessions.json", &sessions);
}

/// Per-project sidebar prefs; `worktrees` keeps the legacy field name.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
struct ProjectPrefs {
    root: PathBuf,
    #[serde(default, rename = "worktrees")]
    archived: Vec<String>,
    #[serde(default)]
    order: Vec<String>,
}

fn update_prefs(root: &Path, f: impl FnOnce(&mut ProjectPrefs)) {
    let mut entries: Vec<ProjectPrefs> = load_json("archived.json").unwrap_or_default();
    let mut entry = entries
        .iter()
        .position(|e| e.root == root)
        .map(|i| entries.remove(i))
        .unwrap_or_else(|| ProjectPrefs {
            root: root.to_path_buf(),
            ..Default::default()
        });
    f(&mut entry);
    if !entry.archived.is_empty() || !entry.order.is_empty() {
        entries.push(entry);
    }
    save_json("archived.json", &entries);
}

fn load_prefs(root: &Path) -> ProjectPrefs {
    load_json::<Vec<ProjectPrefs>>("archived.json")
        .unwrap_or_default()
        .into_iter()
        .find(|e| e.root == root)
        .unwrap_or_default()
}

/// A project's (archived worktree names, custom worktree order).
pub fn load_project_prefs(root: &Path) -> (Vec<String>, Vec<String>) {
    let prefs = load_prefs(root);
    (prefs.archived, prefs.order)
}

pub fn save_archived(root: &Path, worktrees: &[String]) {
    update_prefs(root, |prefs| prefs.archived = worktrees.to_vec());
}

pub fn save_worktree_order(root: &Path, order: &[String]) {
    update_prefs(root, |prefs| prefs.order = order.to_vec());
}

fn capture_layout(node: &PanelNode) -> PanelLayout {
    match node {
        PanelNode::Leaf(_, handle, _) => PanelLayout::Leaf { cwd: handle.cwd() },
        PanelNode::Horizontal(a, b) => {
            PanelLayout::Horizontal(Box::new(capture_layout(a)), Box::new(capture_layout(b)))
        }
        PanelNode::Vertical(a, b) => {
            PanelLayout::Vertical(Box::new(capture_layout(a)), Box::new(capture_layout(b)))
        }
    }
}

fn capture_tab(tab: &Tab) -> SessionTab {
    let active_leaf = tab
        .panels
        .leaves()
        .iter()
        .position(|id| *id == tab.active_panel)
        .unwrap_or(0);
    SessionTab {
        worktree: tab.worktree.clone(),
        custom_title: tab.custom_title.clone(),
        layout: capture_layout(&tab.panels),
        active_leaf,
    }
}

/// Snapshot open projects, their tabs (with panel layouts) and loose tabs.
pub fn capture(state: &AppState) -> Session {
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
                tabs: tabs.iter().map(|t| capture_tab(t)).collect(),
            }
        })
        .collect();
    let loose: Vec<&Tab> = state.tabs.iter().filter(|t| t.project.is_none()).collect();
    ordered.extend(&loose);

    let active_tab = state
        .active_tab()
        .and_then(|active| ordered.iter().position(|t| t.id == active.id))
        .unwrap_or(0);

    Session {
        started_at: state.started_at,
        saved_at: now_secs(),
        projects,
        loose_tabs: loose.iter().map(|t| capture_tab(t)).collect(),
        active_tab,
    }
}

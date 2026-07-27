use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use async_io::Timer;
use freya::prelude::{
    AccessibilityId, AccessibilityIdExt, Clipboard, TaskHandle, UseId, spawn_forever,
};
use freya::radio::{RadioChannel, RadioStation};
use freya::terminal::*;
use futures::FutureExt;

use crate::git::{self, ProjectInfo, Worktree};
use crate::session::{self, PanelLayout, Session, SessionTab};

#[derive(PartialEq)]
pub struct PanelTask(TaskHandle);

impl PanelTask {
    pub fn new(handle: TaskHandle) -> Self {
        Self(handle)
    }
}

impl Drop for PanelTask {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TabId(pub usize);

impl TabId {
    pub fn new() -> Self {
        Self(UseId::<TabId>::get_in_hook())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProjectId(pub usize);

impl ProjectId {
    pub fn new() -> Self {
        Self(UseId::<ProjectId>::get_in_hook())
    }
}

/// An open project.
#[derive(Clone, PartialEq)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    /// Stable identity: the marcgit root or the main worktree itself.
    pub root: PathBuf,
    /// The repository's main worktree (trunk for marcgit layouts).
    pub main: PathBuf,
    pub worktrees: Vec<Worktree>,
    /// Worktree names hidden from the sidebar, persisted per project root.
    pub archived: Vec<String>,
    /// Custom sidebar order of non-main worktree names.
    pub worktree_order: Vec<String>,
    pub collapsed: bool,
    /// Reveal archived worktrees in the sidebar (not persisted).
    pub show_archived: bool,
}

/// Main worktree first, then the custom order, then alphabetical.
pub fn sorted_worktrees(worktrees: &[Worktree], order: &[String]) -> Vec<Worktree> {
    let mut worktrees = worktrees.to_vec();
    worktrees.sort_by_key(|wt| {
        if wt.is_main {
            (0, 0, String::new())
        } else {
            match order.iter().position(|n| n == &wt.name) {
                Some(i) => (1, i, String::new()),
                None => (2, 0, wt.name.clone()),
            }
        }
    });
    worktrees
}

/// A visible sidebar row for one of a project's worktrees.
#[derive(Clone, PartialEq)]
pub struct WorktreeEntry {
    pub worktree: Worktree,
    pub tab: Option<TabId>,
    pub archived: bool,
}

#[derive(Clone, PartialEq)]
pub enum Modal {
    AddProject,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Clone, PartialEq)]
pub enum PanelNode {
    Leaf(AccessibilityId, TerminalHandle, Option<Rc<PanelTask>>),
    Horizontal(Box<PanelNode>, Box<PanelNode>),
    Vertical(Box<PanelNode>, Box<PanelNode>),
}

fn is_flatpak() -> bool {
    std::env::var("FLATPAK_ID").is_ok()
}

fn make_handle(shell: &str, cwd: Option<PathBuf>) -> TerminalHandle {
    let cmd = if is_flatpak() {
        let mut cmd = CommandBuilder::new("flatpak-spawn");
        cmd.args(["--host", "--watch-bus"]);
        cmd.arg("--env=TERM=xterm-256color");
        cmd.arg("--env=COLORTERM=truecolor");
        cmd.arg("--env=LANG=en_GB.UTF-8");
        if let Some(ref dir) = cwd {
            cmd.arg(format!("--directory={}", dir.display()));
        }
        cmd.arg(shell);
        // https://github.com/flatpak/flatpak/issues/3697
        cmd.set_controlling_tty(false);
        cmd
    } else {
        let mut cmd = CommandBuilder::new(shell);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("LANG", "en_GB.UTF-8");
        if let Some(dir) = cwd {
            cmd.cwd(dir);
        }
        cmd
    };
    TerminalHandle::new(TerminalId::new(), cmd, Some(10_000)).expect("failed to spawn PTY")
}

impl PanelNode {
    pub fn new_leaf(shell: &str, cwd: Option<PathBuf>) -> (AccessibilityId, Self) {
        let id = AccessibilityId::new_unique();
        (id, PanelNode::Leaf(id, make_handle(shell, cwd), None))
    }

    /// Find the neighbour of `target` in the given direction.
    pub fn find_neighbour(
        &self,
        target: AccessibilityId,
        dir: NavDirection,
    ) -> Option<AccessibilityId> {
        match self {
            PanelNode::Leaf(..) => None,
            PanelNode::Horizontal(a, b) => {
                let in_a = a.contains(target);
                let in_b = b.contains(target);
                match dir {
                    NavDirection::Right if in_a => a.find_neighbour(target, dir).or_else(|| {
                        a.leaf_fraction(target, Axis::Vertical)
                            .and_then(|frac| b.leaf_at_fraction(frac, Axis::Vertical))
                    }),
                    NavDirection::Left if in_b => b.find_neighbour(target, dir).or_else(|| {
                        b.leaf_fraction(target, Axis::Vertical)
                            .and_then(|frac| a.leaf_at_fraction(frac, Axis::Vertical))
                    }),
                    _ if in_a => a.find_neighbour(target, dir),
                    _ if in_b => b.find_neighbour(target, dir),
                    _ => None,
                }
            }
            PanelNode::Vertical(a, b) => {
                let in_a = a.contains(target);
                let in_b = b.contains(target);
                match dir {
                    NavDirection::Down if in_a => a.find_neighbour(target, dir).or_else(|| {
                        a.leaf_fraction(target, Axis::Horizontal)
                            .and_then(|frac| b.leaf_at_fraction(frac, Axis::Horizontal))
                    }),
                    NavDirection::Up if in_b => b.find_neighbour(target, dir).or_else(|| {
                        b.leaf_fraction(target, Axis::Horizontal)
                            .and_then(|frac| a.leaf_at_fraction(frac, Axis::Horizontal))
                    }),
                    _ if in_a => a.find_neighbour(target, dir),
                    _ if in_b => b.find_neighbour(target, dir),
                    _ => None,
                }
            }
        }
    }

    pub fn contains(&self, id: AccessibilityId) -> bool {
        match self {
            PanelNode::Leaf(pid, ..) => *pid == id,
            PanelNode::Horizontal(a, b) | PanelNode::Vertical(a, b) => {
                a.contains(id) || b.contains(id)
            }
        }
    }

    pub fn leaves(&self) -> Vec<AccessibilityId> {
        match self {
            PanelNode::Leaf(id, ..) => vec![*id],
            PanelNode::Horizontal(a, b) | PanelNode::Vertical(a, b) => {
                let mut v = a.leaves();
                v.extend(b.leaves());
                v
            }
        }
    }

    pub fn leaf_fraction(&self, id: AccessibilityId, axis: Axis) -> Option<f64> {
        match self {
            PanelNode::Leaf(pid, ..) if *pid == id => Some(0.0),
            PanelNode::Leaf(..) => None,
            PanelNode::Horizontal(a, b) => {
                if a.contains(id) {
                    a.leaf_fraction(id, axis)
                        .map(|f| if axis == Axis::Horizontal { f * 0.5 } else { f })
                } else if b.contains(id) {
                    b.leaf_fraction(id, axis).map(|f| {
                        if axis == Axis::Horizontal {
                            0.5 + f * 0.5
                        } else {
                            f
                        }
                    })
                } else {
                    None
                }
            }
            PanelNode::Vertical(a, b) => {
                if a.contains(id) {
                    a.leaf_fraction(id, axis).map(
                        |f| {
                            if axis == Axis::Vertical { f * 0.5 } else { f }
                        },
                    )
                } else if b.contains(id) {
                    b.leaf_fraction(id, axis).map(|f| {
                        if axis == Axis::Vertical {
                            0.5 + f * 0.5
                        } else {
                            f
                        }
                    })
                } else {
                    None
                }
            }
        }
    }

    pub fn leaf_at_fraction(&self, fraction: f64, axis: Axis) -> Option<AccessibilityId> {
        match self {
            PanelNode::Leaf(id, ..) => Some(*id),
            PanelNode::Horizontal(a, b) => {
                if axis == Axis::Horizontal {
                    if fraction < 0.5 {
                        a.leaf_at_fraction(fraction * 2.0, axis)
                    } else {
                        b.leaf_at_fraction((fraction - 0.5) * 2.0, axis)
                    }
                } else {
                    a.leaf_at_fraction(fraction, axis)
                }
            }
            PanelNode::Vertical(a, b) => {
                if axis == Axis::Vertical {
                    if fraction < 0.5 {
                        a.leaf_at_fraction(fraction * 2.0, axis)
                    } else {
                        b.leaf_at_fraction((fraction - 0.5) * 2.0, axis)
                    }
                } else {
                    a.leaf_at_fraction(fraction, axis)
                }
            }
        }
    }

    pub fn leaf_handle(&self) -> Option<&TerminalHandle> {
        match self {
            PanelNode::Leaf(_, h, _) => Some(h),
            _ => None,
        }
    }

    pub fn panel_task(&self, id: AccessibilityId) -> Option<Rc<PanelTask>> {
        match self {
            PanelNode::Leaf(pid, _, task) if *pid == id => task.clone(),
            PanelNode::Leaf(..) => None,
            PanelNode::Horizontal(a, b) | PanelNode::Vertical(a, b) => {
                a.panel_task(id).or_else(|| b.panel_task(id))
            }
        }
    }

    pub fn set_task(&mut self, id: AccessibilityId, task: Rc<PanelTask>) {
        match self {
            PanelNode::Leaf(pid, _, t) if *pid == id => *t = Some(task),
            PanelNode::Leaf(..) => {}
            PanelNode::Horizontal(a, b) | PanelNode::Vertical(a, b) => {
                a.set_task(id, task.clone());
                b.set_task(id, task);
            }
        }
    }

    pub fn handle(&self, id: AccessibilityId) -> Option<&TerminalHandle> {
        match self {
            PanelNode::Leaf(pid, h, _) if *pid == id => Some(h),
            PanelNode::Leaf(..) => None,
            PanelNode::Horizontal(a, b) | PanelNode::Vertical(a, b) => {
                a.handle(id).or_else(|| b.handle(id))
            }
        }
    }

    pub fn replace_leaf(self, target: AccessibilityId, replacement: PanelNode) -> PanelNode {
        match self {
            PanelNode::Leaf(id, ..) if id == target => replacement,
            PanelNode::Leaf(..) => self,
            PanelNode::Horizontal(a, b) => PanelNode::Horizontal(
                Box::new(a.replace_leaf(target, replacement.clone())),
                Box::new(b.replace_leaf(target, replacement)),
            ),
            PanelNode::Vertical(a, b) => PanelNode::Vertical(
                Box::new(a.replace_leaf(target, replacement.clone())),
                Box::new(b.replace_leaf(target, replacement)),
            ),
        }
    }

    pub fn remove_leaf(self, target: AccessibilityId) -> Option<PanelNode> {
        match self {
            PanelNode::Leaf(id, ..) if id == target => None,
            PanelNode::Leaf(..) => Some(self),
            PanelNode::Horizontal(a, b) => {
                if a.contains(target) {
                    if matches!(*a, PanelNode::Leaf(id, ..) if id == target) {
                        return Some(*b);
                    }
                    let new_a = a.remove_leaf(target)?;
                    Some(PanelNode::Horizontal(Box::new(new_a), b))
                } else {
                    if matches!(*b, PanelNode::Leaf(id, ..) if id == target) {
                        return Some(*a);
                    }
                    let new_b = b.remove_leaf(target)?;
                    Some(PanelNode::Horizontal(a, Box::new(new_b)))
                }
            }
            PanelNode::Vertical(a, b) => {
                if a.contains(target) {
                    if matches!(*a, PanelNode::Leaf(id, ..) if id == target) {
                        return Some(*b);
                    }
                    let new_a = a.remove_leaf(target)?;
                    Some(PanelNode::Vertical(Box::new(new_a), b))
                } else {
                    if matches!(*b, PanelNode::Leaf(id, ..) if id == target) {
                        return Some(*a);
                    }
                    let new_b = b.remove_leaf(target)?;
                    Some(PanelNode::Vertical(a, Box::new(new_b)))
                }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct Tab {
    pub id: TabId,
    pub title: String,
    pub custom_title: Option<String>,
    pub panels: PanelNode,
    pub active_panel: AccessibilityId,
    pub outputting: bool,
    pub last_output: Instant,
    /// Project this tab is filed under, `None` for loose tabs.
    pub project: Option<ProjectId>,
    /// The worktree this tab represents, `Some` pins the tab to its project.
    pub worktree: Option<PathBuf>,
}

impl Tab {
    pub fn new(
        shell: &str,
        cwd: Option<PathBuf>,
        project: Option<ProjectId>,
        worktree: Option<PathBuf>,
    ) -> Self {
        let (active_panel, root) = PanelNode::new_leaf(shell, cwd);
        Self::from_panels(root, active_panel, None, project, worktree)
    }

    pub fn from_panels(
        panels: PanelNode,
        active_panel: AccessibilityId,
        custom_title: Option<String>,
        project: Option<ProjectId>,
        worktree: Option<PathBuf>,
    ) -> Self {
        let id = TabId::new();
        Self {
            id,
            title: format!("Terminal {}", id.0),
            custom_title,
            panels,
            active_panel,
            outputting: false,
            last_output: Instant::now(),
            project,
            worktree,
        }
    }

    pub fn display_title(&self) -> &str {
        match &self.custom_title {
            Some(t) if !t.is_empty() => t,
            _ => &self.title,
        }
    }

    pub fn update_title_from_active_panel(&mut self) {
        if let Some(handle) = self.panels.handle(self.active_panel) {
            if let Some(title) = handle.title() {
                if !title.is_empty() {
                    self.title = title;
                }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct AppState {
    pub tabs: Vec<Tab>,
    pub active_tab: usize,
    pub projects: Vec<Project>,
    pub font_size: f32,
    pub shell: String,
    pub sidebar_collapsed: bool,
    pub modal: Option<Modal>,
    pub notice: Option<String>,
    /// Identifies this run in the sessions ring.
    pub started_at: u64,
}

impl AppState {
    pub fn new(font_size: f32, shell: String) -> Self {
        Self {
            tabs: vec![],
            active_tab: 0,
            projects: vec![],
            font_size,
            shell,
            sidebar_collapsed: false,
            modal: None,
            notice: None,
            started_at: session::now_secs(),
        }
    }

    pub fn project(&self, id: ProjectId) -> Option<&Project> {
        self.projects.iter().find(|p| p.id == id)
    }

    pub fn project_mut(&mut self, id: ProjectId) -> Option<&mut Project> {
        self.projects.iter_mut().find(|p| p.id == id)
    }

    /// Register a project (idempotent by root), seeded with its main worktree.
    pub fn add_project(&mut self, info: ProjectInfo) -> ProjectId {
        if let Some(project) = self.projects.iter().find(|p| p.root == info.root) {
            return project.id;
        }
        let id = ProjectId::new();
        let mut main_seed = Worktree::placeholder(info.main.clone());
        main_seed.is_main = true;
        let (archived, worktree_order) = session::load_project_prefs(&info.root);
        self.projects.push(Project {
            id,
            name: info.name,
            root: info.root,
            main: info.main,
            worktrees: vec![main_seed],
            archived,
            worktree_order,
            collapsed: false,
            show_archived: false,
        });
        id
    }

    /// Toggle archived rows, closing their terminals when hiding.
    pub fn toggle_show_archived(&mut self, id: ProjectId) {
        let hidden_paths = {
            let Some(project) = self.project_mut(id) else {
                return;
            };
            project.show_archived = !project.show_archived;
            if project.show_archived {
                return;
            }
            project
                .worktrees
                .iter()
                .filter(|wt| project.archived.contains(&wt.name))
                .map(|wt| wt.path.clone())
                .collect::<Vec<_>>()
        };
        for path in hidden_paths {
            self.close_tabs_in_worktree(&path);
        }
    }

    /// Sort worktrees: open with changes, then open, then the rest.
    pub fn sort_worktrees(&mut self, id: ProjectId) {
        let Some(project) = self.project(id) else {
            return;
        };
        let mut ranked: Vec<(u8, String)> =
            sorted_worktrees(&project.worktrees, &project.worktree_order)
                .into_iter()
                .filter(|wt| !wt.is_main)
                .map(|wt| {
                    let open = self.tab_for_worktree(id, &wt.path).is_some();
                    let dirty = wt.diff.is_some_and(|d| !d.is_clean());
                    let rank = match (open, dirty) {
                        (true, true) => 0,
                        (true, false) => 1,
                        (false, true) => 2,
                        (false, false) => 3,
                    };
                    (rank, wt.name)
                })
                .collect();
        ranked.sort_by_key(|(rank, _)| *rank);
        let order: Vec<String> = ranked.into_iter().map(|(_, name)| name).collect();
        if let Some(project) = self.project_mut(id) {
            project.worktree_order = order;
            session::save_worktree_order(&project.root, &project.worktree_order);
        }
    }

    /// Move `dragged` to `target`'s position in the persisted sidebar order.
    pub fn reorder_worktree(&mut self, id: ProjectId, dragged: &str, target: &str) {
        let Some(project) = self.project_mut(id) else {
            return;
        };
        let is_main = |name: &str| {
            project
                .worktrees
                .iter()
                .any(|wt| wt.name == name && wt.is_main)
        };
        if dragged == target || is_main(dragged) {
            return;
        }
        let mut names: Vec<String> = sorted_worktrees(&project.worktrees, &project.worktree_order)
            .iter()
            .filter(|wt| !wt.is_main)
            .map(|wt| wt.name.clone())
            .collect();
        let Some(from) = names.iter().position(|n| n == dragged) else {
            return;
        };
        names.remove(from);
        let to = if is_main(target) {
            0
        } else {
            names
                .iter()
                .position(|n| n == target)
                .map(|i| i + usize::from(i >= from))
                .unwrap_or(names.len())
        };
        names.insert(to, dragged.to_string());
        project.worktree_order = names;
        session::save_worktree_order(&project.root, &project.worktree_order);
    }

    /// Replace a project's archived worktree list and persist it.
    pub fn set_archived(&mut self, id: ProjectId, archived: Vec<String>) {
        if let Some(project) = self.project_mut(id) {
            project.archived = archived;
            session::save_archived(&project.root, &project.archived);
        }
    }

    /// Close every tab open on the given worktree.
    pub fn close_tabs_in_worktree(&mut self, path: &Path) {
        let active_id = self.active_tab().map(|t| t.id);
        self.tabs.retain(|t| t.worktree.as_deref() != Some(path));
        self.restore_active(active_id);
    }

    /// The project's visible sidebar rows, in display order.
    pub fn worktree_entries(&self, project: &Project) -> Vec<WorktreeEntry> {
        let mut entries: Vec<WorktreeEntry> =
            sorted_worktrees(&project.worktrees, &project.worktree_order)
                .into_iter()
                .filter_map(|worktree| {
                    let archived = project.archived.contains(&worktree.name);
                    if archived && !project.show_archived {
                        return None;
                    }
                    let tab = self
                        .tab_for_worktree(project.id, &worktree.path)
                        .map(|t| t.id);
                    Some(WorktreeEntry {
                        worktree,
                        tab,
                        archived,
                    })
                })
                .collect();
        for tab in &self.tabs {
            if tab.project == Some(project.id)
                && let Some(path) = &tab.worktree
                && !project.worktrees.iter().any(|wt| &wt.path == path)
            {
                let worktree = Worktree::placeholder(path.clone());
                let archived = project.archived.contains(&worktree.name);
                if archived && !project.show_archived {
                    continue;
                }
                entries.push(WorktreeEntry {
                    worktree,
                    tab: Some(tab.id),
                    archived,
                });
            }
        }
        entries
    }

    /// Close a project and all of its tabs. Never touches the disk.
    pub fn remove_project(&mut self, id: ProjectId) {
        let active_id = self.active_tab().map(|t| t.id);
        self.tabs.retain(|t| t.project != Some(id));
        self.projects.retain(|p| p.id != id);
        self.restore_active(active_id);
    }

    fn restore_active(&mut self, active_id: Option<TabId>) {
        self.active_tab = active_id
            .and_then(|id| self.tabs.iter().position(|t| t.id == id))
            .unwrap_or_else(|| self.tabs.len().saturating_sub(1));
        self.focus_active_panel();
    }

    pub fn tab_for_worktree(&self, project: ProjectId, path: &Path) -> Option<&Tab> {
        self.tabs
            .iter()
            .find(|t| t.project == Some(project) && t.worktree.as_deref() == Some(path))
    }

    pub fn toggle_sidebar(&mut self) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active_tab)
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active_tab)
    }

    /// The active panel's working directory.
    pub fn active_cwd(&self) -> Option<PathBuf> {
        self.active_tab()
            .and_then(|tab| tab.panels.handle(tab.active_panel))
            .and_then(|h| h.cwd())
    }

    /// New tab in the active tab's project (if any), inheriting its cwd.
    pub fn new_tab(&mut self) -> (TabId, AccessibilityId, TerminalHandle) {
        let project = self.active_tab().and_then(|tab| tab.project);
        let cwd = self.active_cwd();
        self.new_tab_with(project, None, cwd)
    }

    pub fn new_tab_with(
        &mut self,
        project: Option<ProjectId>,
        worktree: Option<PathBuf>,
        cwd: Option<PathBuf>,
    ) -> (TabId, AccessibilityId, TerminalHandle) {
        let cwd = cwd.or_else(|| worktree.clone());
        let tab = Tab::new(&self.shell, cwd, project, worktree);
        let tab_id = tab.id;
        let panel_id = tab.active_panel;
        let handle = tab.panels.leaf_handle().unwrap().clone();
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        self.focus_active_panel();
        (tab_id, panel_id, handle)
    }

    pub fn close_active_tab(&mut self) {
        if let Some(id) = self.tabs.get(self.active_tab).map(|t| t.id) {
            self.close_tab_by_id(id);
        }
    }

    pub fn close_tab_by_id(&mut self, tab_id: TabId) {
        if let Some(idx) = self.tabs.iter().position(|t| t.id == tab_id) {
            self.tabs.remove(idx);
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.tabs.len().saturating_sub(1);
            }
            self.focus_active_panel();
        }
    }

    pub fn focus_active_panel(&self) {
        if let Some(tab) = self.active_tab() {
            tab.active_panel.request_focus();
        }
    }

    pub fn rename_tab(&mut self, tab_id: TabId, name: String) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            if name.is_empty() {
                tab.custom_title = None;
            } else {
                tab.custom_title = Some(name);
            }
        }
    }

    pub fn switch_to_tab(&mut self, tab_id: TabId) {
        if let Some(idx) = self.tabs.iter().position(|t| t.id == tab_id) {
            self.active_tab = idx;
            self.focus_active_panel();
        }
    }

    /// Tab ids in sidebar order.
    pub fn display_order(&self) -> Vec<TabId> {
        let mut order = Vec::with_capacity(self.tabs.len());
        for project in &self.projects {
            order.extend(self.worktree_entries(project).iter().filter_map(|e| e.tab));
            order.extend(
                self.tabs
                    .iter()
                    .filter(|t| t.project == Some(project.id) && t.worktree.is_none())
                    .map(|t| t.id),
            );
        }
        order.extend(
            self.tabs
                .iter()
                .filter(|t| t.project.is_none())
                .map(|t| t.id),
        );
        order
    }

    fn is_cyclable(&self, tab_id: TabId) -> bool {
        let Some(tab) = self.tabs.iter().find(|t| t.id == tab_id) else {
            return false;
        };
        tab.project
            .and_then(|id| self.project(id))
            .is_none_or(|p| !p.collapsed)
    }

    fn step_tab(&mut self, forward: bool) {
        let order = self.display_order();
        if order.is_empty() {
            return;
        }
        let current = self
            .tabs
            .get(self.active_tab)
            .and_then(|tab| order.iter().position(|id| *id == tab.id))
            .unwrap_or(0);
        let len = order.len();
        for step in 1..=len {
            let index = if forward {
                (current + step) % len
            } else {
                (current + len - step) % len
            };
            if self.is_cyclable(order[index]) {
                self.switch_to_tab(order[index]);
                return;
            }
        }
    }

    pub fn next_tab(&mut self) {
        self.step_tab(true);
    }

    /// Reorder `from` next to `to`, re-parenting plain tabs across groups.
    pub fn move_tab(&mut self, from_id: TabId, to_id: TabId) {
        if from_id == to_id {
            return;
        }
        let Some(from_idx) = self.tabs.iter().position(|t| t.id == from_id) else {
            return;
        };
        let Some(to_idx) = self.tabs.iter().position(|t| t.id == to_id) else {
            return;
        };
        let target_project = self.tabs[to_idx].project;
        if self.tabs[from_idx].project != target_project {
            if self.tabs[from_idx].worktree.is_some() {
                return;
            }
            self.tabs[from_idx].project = target_project;
        }
        let active_id = self.tabs[self.active_tab].id;
        if from_idx < to_idx {
            self.tabs.insert(to_idx + 1, self.tabs[from_idx].clone());
            self.tabs.remove(from_idx);
        } else {
            let tab = self.tabs.remove(from_idx);
            self.tabs.insert(to_idx, tab);
        }

        // Keep active_tab pointing at the same tab
        if let Some(new_active) = self.tabs.iter().position(|t| t.id == active_id) {
            self.active_tab = new_active;
        }
    }

    /// File a plain tab under `project`, or detach it with `None`.
    pub fn reparent_tab(&mut self, tab_id: TabId, project: Option<ProjectId>) {
        let Some(idx) = self.tabs.iter().position(|t| t.id == tab_id) else {
            return;
        };
        if self.tabs[idx].worktree.is_some() || self.tabs[idx].project == project {
            return;
        }
        let active_id = self.tabs[self.active_tab].id;
        let mut tab = self.tabs.remove(idx);
        tab.project = project;
        self.tabs.push(tab);
        if let Some(new_active) = self.tabs.iter().position(|t| t.id == active_id) {
            self.active_tab = new_active;
        }
    }

    pub fn prev_tab(&mut self) {
        self.step_tab(false);
    }

    pub fn split(&mut self, axis: Axis) -> Option<(AccessibilityId, TerminalHandle)> {
        let cwd = self.active_cwd();
        let (new_id, new_leaf) = PanelNode::new_leaf(&self.shell, cwd);
        let tab = self.active_tab_mut()?;
        let new_handle = new_leaf.leaf_handle().unwrap().clone();
        let current = Box::new(PanelNode::Leaf(
            tab.active_panel,
            tab.panels.handle(tab.active_panel).cloned().unwrap(),
            tab.panels.panel_task(tab.active_panel),
        ));
        let split = match axis {
            Axis::Horizontal => PanelNode::Horizontal(current, Box::new(new_leaf)),
            Axis::Vertical => PanelNode::Vertical(current, Box::new(new_leaf)),
        };
        tab.panels = tab.panels.clone().replace_leaf(tab.active_panel, split);
        tab.active_panel = new_id;
        Some((new_id, new_handle))
    }

    /// Collapses the current tab to only its active panel, closing all others.
    pub fn close_all_except_active(&mut self) {
        if let Some(tab) = self.active_tab_mut() {
            let active_id = tab.active_panel;
            let active_leaf = PanelNode::Leaf(
                active_id,
                tab.panels.handle(active_id).cloned().unwrap(),
                tab.panels.panel_task(active_id),
            );
            tab.panels = active_leaf;
            active_id.request_focus();
        }
    }

    pub fn close_active_panel(&mut self) {
        if let Some(tab) = self.active_tab_mut() {
            if let Some(new_root) = tab.panels.clone().remove_leaf(tab.active_panel) {
                let leaves = new_root.leaves();
                tab.panels = new_root;
                if let Some(panel) = leaves.into_iter().last() {
                    tab.active_panel = panel;
                    tab.update_title_from_active_panel();
                    panel.request_focus();
                }
            }
        }
    }

    pub fn navigate(&mut self, dir: NavDirection) {
        if let Some(tab) = self.active_tab_mut()
            && let Some(neighbour) = tab.panels.find_neighbour(tab.active_panel, dir)
        {
            tab.active_panel = neighbour;
            tab.update_title_from_active_panel();
            neighbour.request_focus();
        }
    }

    pub fn increase_font_size(&mut self) {
        self.font_size = (self.font_size + 1.0).min(48.0);
    }

    pub fn decrease_font_size(&mut self) {
        self.font_size = (self.font_size - 1.0).max(6.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavDirection {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppChannel {
    Tabs,
}

impl RadioChannel<AppState> for AppChannel {}

/// Scope-independent handle to the app state, safe in detached tasks.
pub type AppStation = RadioStation<AppState, AppChannel>;

pub fn watch_panel(
    mut station: AppStation,
    tab_id: TabId,
    panel_id: AccessibilityId,
    handle: TerminalHandle,
) -> Rc<PanelTask> {
    // Detached from the component scope, cancelled by PanelTask on drop.
    let task = spawn_forever(async move {
        let idle = Duration::from_secs(1);
        loop {
            futures::select! {
                _ = handle.title_changed().fuse() => {
                    let title = handle.title().unwrap_or_default();
                    if !title.is_empty() {
                        let mut state = station.write_channel(AppChannel::Tabs);
                        if let Some(tab) =
                            state.tabs.iter_mut().find(|t| t.id == tab_id)
                            && tab.active_panel == panel_id
                        {
                            tab.title = title;
                        }
                    }
                }
                _ = handle.output_received().fuse() => {
                    {
                        let mut state = station.write_channel(AppChannel::Tabs);
                        if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id) {
                            tab.last_output = Instant::now();
                            tab.outputting = true;
                        }
                    }

                    // Keep consuming output until idle for 1 second.
                    loop {
                        futures::select! {
                            _ = handle.output_received().fuse() => {
                                let mut state = station.write_channel(AppChannel::Tabs);
                                if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id) {
                                    tab.last_output = Instant::now();
                                }
                            }
                            _ = Timer::after(idle).fuse() => break,
                        }
                    }

                    // Only clear if no other panel refreshed the timestamp.
                    let mut state = station.write_channel(AppChannel::Tabs);
                    if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id)
                        && tab.last_output.elapsed() > idle
                    {
                        tab.outputting = false;
                    }
                }
                _ = handle.clipboard_changed().fuse() => {
                    let text = handle.clipboard_content().unwrap_or_default();
                    let _ = Clipboard::set(text);
                }
                _ = handle.closed().fuse() => break,
            }
        }
    });
    Rc::new(PanelTask::new(task))
}

fn wire_panel(
    station: AppStation,
    state: &mut AppState,
    tab_id: TabId,
    panel_id: AccessibilityId,
    handle: TerminalHandle,
) {
    let task = watch_panel(station, tab_id, panel_id, handle);
    state
        .active_tab_mut()
        .unwrap()
        .panels
        .set_task(panel_id, task);
}

fn wire_tab(
    mut station: AppStation,
    new_tab: impl FnOnce(&mut AppState) -> (TabId, AccessibilityId, TerminalHandle),
) {
    let mut state = station.write_channel(AppChannel::Tabs);
    let (tab_id, panel_id, handle) = new_tab(&mut state);
    wire_panel(station, &mut state, tab_id, panel_id, handle);
}

/// Create a tab and wire its panel watcher.
pub fn create_tab(
    station: AppStation,
    project: Option<ProjectId>,
    worktree: Option<PathBuf>,
    cwd: Option<PathBuf>,
) {
    wire_tab(station, |state| state.new_tab_with(project, worktree, cwd));
}

/// Context-aware new tab plus its panel watcher.
pub fn create_context_tab(station: AppStation) {
    wire_tab(station, AppState::new_tab);
}

/// Split the active panel and wire the new panel's watcher.
pub fn split_active_panel(mut station: AppStation, axis: Axis) {
    let mut state = station.write_channel(AppChannel::Tabs);
    if let Some((panel_id, handle)) = state.split(axis) {
        let tab_id = state.active_tab().unwrap().id;
        wire_panel(station, &mut state, tab_id, panel_id, handle);
    }
}

/// New plain tab in `project`, or a loose one when `None`.
pub fn create_plain_tab(station: AppStation, project: Option<ProjectId>) {
    let cwd = {
        let state = station.peek();
        match project {
            Some(id) => state
                .active_tab()
                .filter(|t| t.project == Some(id))
                .and_then(|t| t.panels.handle(t.active_panel))
                .and_then(|h| h.cwd())
                .or_else(|| state.project(id).map(|p| p.main.clone())),
            None => state.active_cwd(),
        }
    };
    create_tab(station, project, None, cwd);
}

/// Open (or switch to) a project and refresh its worktrees.
pub fn open_project(mut station: AppStation, info: ProjectInfo) {
    session::touch_recent_project(&info.root);
    let main = info.main.clone();
    let (project_id, existing_tab) = {
        let mut state = station.write_channel(AppChannel::Tabs);
        let id = state.add_project(info);
        let tab = state
            .tabs
            .iter()
            .find(|t| t.project == Some(id))
            .map(|t| t.id);
        (id, tab)
    };
    match existing_tab {
        Some(tab_id) => station
            .write_channel(AppChannel::Tabs)
            .switch_to_tab(tab_id),
        None => create_tab(station, Some(project_id), Some(main), None),
    }
    refresh_worktrees(station, project_id);
}

/// Refresh a project's worktrees in the background, writing only on change.
pub fn refresh_worktrees(mut station: AppStation, project_id: ProjectId) {
    let Some((main, skip_diffs, skip_all)) = ({
        let state = station.peek();
        state.project(project_id).map(|p| {
            let hidden = if p.show_archived {
                vec![]
            } else {
                p.archived.clone()
            };
            (p.main.clone(), hidden, p.collapsed)
        })
    }) else {
        return;
    };
    spawn_forever(async move {
        match git::run_async(move || git::list_worktrees(&main, &skip_diffs, skip_all)).await {
            Ok(worktrees) => {
                let changed = station
                    .peek()
                    .project(project_id)
                    .is_some_and(|p| p.worktrees != worktrees);
                if changed {
                    let mut state = station.write_channel(AppChannel::Tabs);
                    if let Some(project) = state.project_mut(project_id) {
                        project.worktrees = worktrees;
                    }
                }
            }
            Err(e) => station.write_channel(AppChannel::Tabs).notice = Some(e),
        }
    });
}

fn build_panels(layout: &PanelLayout, shell: &str, fallback_cwd: Option<&Path>) -> PanelNode {
    match layout {
        PanelLayout::Leaf { cwd } => {
            let cwd = cwd
                .as_deref()
                .filter(|c| c.is_dir())
                .or_else(|| fallback_cwd.filter(|c| c.is_dir()))
                .map(Path::to_path_buf);
            PanelNode::new_leaf(shell, cwd).1
        }
        PanelLayout::Horizontal(a, b) => PanelNode::Horizontal(
            Box::new(build_panels(a, shell, fallback_cwd)),
            Box::new(build_panels(b, shell, fallback_cwd)),
        ),
        PanelLayout::Vertical(a, b) => PanelNode::Vertical(
            Box::new(build_panels(a, shell, fallback_cwd)),
            Box::new(build_panels(b, shell, fallback_cwd)),
        ),
    }
}

fn restore_tab(mut station: AppStation, saved: &SessionTab, project: Option<ProjectId>) {
    if let Some(worktree) = &saved.worktree {
        if !worktree.is_dir() {
            return;
        }
        let archived = project.is_some_and(|id| {
            station
                .peek()
                .project(id)
                .is_some_and(|p| p.archived.contains(&git::dir_name(worktree)))
        });
        if archived {
            return;
        }
    }
    let shell = station.peek().shell.clone();
    let panels = build_panels(&saved.layout, &shell, saved.worktree.as_deref());
    let leaves = panels.leaves();
    let active_panel = leaves
        .get(saved.active_leaf)
        .or_else(|| leaves.first())
        .copied()
        .unwrap();
    let tab = Tab::from_panels(
        panels,
        active_panel,
        saved.custom_title.clone(),
        project,
        saved.worktree.clone(),
    );
    let tab_id = tab.id;
    let mut state = station.write_channel(AppChannel::Tabs);
    let handles: Vec<(AccessibilityId, TerminalHandle)> = leaves
        .iter()
        .filter_map(|id| tab.panels.handle(*id).map(|h| (*id, h.clone())))
        .collect();
    state.tabs.push(tab);
    state.active_tab = state.tabs.len() - 1;
    for (panel_id, handle) in handles {
        wire_panel(station, &mut state, tab_id, panel_id, handle);
    }
}

/// Reopen a saved session's projects and tabs, detecting projects off the UI thread.
pub fn restore_session(mut station: AppStation, saved: &Session) {
    // Autosave updates the restored session entry in place.
    station.write_channel(AppChannel::Tabs).started_at = saved.started_at;
    let saved_projects = saved.projects.clone();
    let loose_tabs = saved.loose_tabs.clone();
    let active_tab = saved.active_tab;
    spawn_forever(async move {
        let roots: Vec<PathBuf> = saved_projects.iter().map(|p| p.root.clone()).collect();
        let results = git::run_async(move || {
            Ok(roots
                .iter()
                .map(|r| git::detect_project(r))
                .collect::<Vec<_>>())
        })
        .await
        .unwrap_or_default();
        let mut skipped = 0;
        let mut opened_roots = Vec::new();
        for (saved_project, result) in saved_projects.iter().zip(results) {
            let Ok(info) = result else {
                skipped += 1;
                continue;
            };
            opened_roots.push(info.root.clone());
            let project_id = station.write_channel(AppChannel::Tabs).add_project(info);
            for tab in &saved_project.tabs {
                restore_tab(station, tab, Some(project_id));
            }
            refresh_worktrees(station, project_id);
        }
        session::touch_recent_projects(&opened_roots);
        for tab in &loose_tabs {
            restore_tab(station, tab, None);
        }
        let mut state = station.write_channel(AppChannel::Tabs);
        if !state.tabs.is_empty() {
            state.active_tab = active_tab.min(state.tabs.len() - 1);
            state.focus_active_panel();
        }
        if skipped > 0 {
            state.notice = Some(format!(
                "{skipped} project{} from this session no longer exist",
                if skipped == 1 { "" } else { "s" }
            ));
        }
    });
}

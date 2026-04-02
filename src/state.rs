use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use async_io::Timer;
use freya::prelude::{AccessibilityId, Clipboard, Focus, TaskHandle, UseId, spawn};
use freya::radio::{Radio, RadioChannel};
use freya::terminal::*;
use futures::FutureExt;

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
        let id = Focus::new_id();
        (id, PanelNode::Leaf(id, make_handle(shell, cwd), None))
    }

    /// Returns the `PanelId` if this node is a `Leaf`, otherwise `None`.
    /// Find the neighbour of `target` in the given direction.
    /// Walks the tree looking for the closest split that can resolve the move.
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
}

impl Tab {
    pub fn new(shell: &str, cwd: Option<PathBuf>) -> Self {
        let (active_panel, root) = PanelNode::new_leaf(shell, cwd);
        let id = TabId::new();
        Self {
            id,
            title: format!("Terminal {}", id.0),
            custom_title: None,
            panels: root,
            active_panel,
            outputting: false,
            last_output: Instant::now(),
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
    pub font_size: f32,
    pub shell: String,
    pub sidebar_collapsed: bool,
}

impl AppState {
    pub fn new(font_size: f32, shell: String) -> Self {
        Self {
            tabs: vec![],
            active_tab: 0,
            font_size,
            shell,
            sidebar_collapsed: false,
        }
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

    pub fn new_tab(&mut self) -> (TabId, AccessibilityId, TerminalHandle) {
        let cwd = self
            .active_tab()
            .and_then(|tab| tab.panels.handle(tab.active_panel))
            .and_then(|h| h.cwd());
        let tab = Tab::new(&self.shell.clone(), cwd);
        let tab_id = tab.id;
        let panel_id = tab.active_panel;
        let handle = tab.panels.leaf_handle().unwrap().clone();
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        self.focus_active_panel();
        (tab_id, panel_id, handle)
    }

    pub fn close_active_tab(&mut self) {
        if self.tabs.len() <= 1 {
            return;
        }
        self.tabs.remove(self.active_tab);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        self.focus_active_panel();
    }

    pub fn close_tab_by_id(&mut self, tab_id: TabId) {
        if self.tabs.len() <= 1 {
            return;
        }
        if let Some(idx) = self.tabs.iter().position(|t| t.id == tab_id) {
            self.tabs.remove(idx);
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.tabs.len() - 1;
            }
            self.focus_active_panel();
        }
    }

    fn focus_active_panel(&self) {
        if let Some(tab) = self.active_tab() {
            Focus::new_for_id(tab.active_panel).request_focus();
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

    pub fn next_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        self.active_tab = (self.active_tab + 1) % self.tabs.len();
        self.focus_active_panel();
    }

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

    pub fn prev_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        self.active_tab = self
            .active_tab
            .checked_sub(1)
            .unwrap_or(self.tabs.len() - 1);
        self.focus_active_panel();
    }

    pub fn split_horizontal(&mut self) -> Option<(AccessibilityId, TerminalHandle)> {
        let shell = self.shell.clone();
        let cwd = self
            .active_tab()
            .and_then(|tab| tab.panels.handle(tab.active_panel))
            .and_then(|h| h.cwd());
        let (new_id, new_leaf) = PanelNode::new_leaf(&shell, cwd);
        let tab = self.active_tab_mut()?;
        let new_handle = new_leaf.leaf_handle().unwrap().clone();
        let split = PanelNode::Horizontal(
            Box::new(PanelNode::Leaf(
                tab.active_panel,
                tab.panels.handle(tab.active_panel).cloned().unwrap(),
                tab.panels.panel_task(tab.active_panel),
            )),
            Box::new(new_leaf),
        );
        tab.panels = tab.panels.clone().replace_leaf(tab.active_panel, split);
        tab.active_panel = new_id;
        Some((new_id, new_handle))
    }

    pub fn split_vertical(&mut self) -> Option<(AccessibilityId, TerminalHandle)> {
        let shell = self.shell.clone();
        let cwd = self
            .active_tab()
            .and_then(|tab| tab.panels.handle(tab.active_panel))
            .and_then(|h| h.cwd());
        let (new_id, new_leaf) = PanelNode::new_leaf(&shell, cwd);
        let tab = self.active_tab_mut()?;
        let new_handle = new_leaf.leaf_handle().unwrap().clone();
        let split = PanelNode::Vertical(
            Box::new(PanelNode::Leaf(
                tab.active_panel,
                tab.panels.handle(tab.active_panel).cloned().unwrap(),
                tab.panels.panel_task(tab.active_panel),
            )),
            Box::new(new_leaf),
        );
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
            Focus::new_for_id(active_id).request_focus();
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
                    Focus::new_for_id(panel).request_focus();
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
            Focus::new_for_id(neighbour).request_focus();
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

pub fn watch_panel(
    mut radio: Radio<AppState, AppChannel>,
    tab_id: TabId,
    panel_id: AccessibilityId,
    handle: TerminalHandle,
) -> Rc<PanelTask> {
    let task = spawn(async move {
        let idle = Duration::from_secs(1);
        loop {
            futures::select! {
                _ = handle.title_changed().fuse() => {
                    let title = handle.title().unwrap_or_default();
                    if !title.is_empty() {
                        let mut state = radio.write_channel(AppChannel::Tabs);
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
                        let mut state = radio.write_channel(AppChannel::Tabs);
                        if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id) {
                            tab.last_output = Instant::now();
                            tab.outputting = true;
                        }
                    }

                    // Keep consuming output until idle for 1 second.
                    loop {
                        futures::select! {
                            _ = handle.output_received().fuse() => {
                                let mut state = radio.write_channel(AppChannel::Tabs);
                                if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id) {
                                    tab.last_output = Instant::now();
                                }
                            }
                            _ = Timer::after(idle).fuse() => break,
                        }
                    }

                    // Only clear if no other panel refreshed the timestamp.
                    let mut state = radio.write_channel(AppChannel::Tabs);
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

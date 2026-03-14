use std::path::PathBuf;

use freya::radio::RadioChannel;
use freya::{
    prelude::{AccessibilityId, Focus, UseId},
    terminal::*,
};

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
    Leaf(AccessibilityId, TerminalHandle),
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
    TerminalHandle::new(TerminalId::new(), cmd, None).expect("failed to spawn PTY")
}

impl PanelNode {
    pub fn new_leaf(shell: &str, cwd: Option<PathBuf>) -> (AccessibilityId, Self) {
        let id = Focus::new_id();
        (id, PanelNode::Leaf(id, make_handle(shell, cwd)))
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
            PanelNode::Leaf(_, _) => None,
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
            PanelNode::Leaf(pid, _) => *pid == id,
            PanelNode::Horizontal(a, b) | PanelNode::Vertical(a, b) => {
                a.contains(id) || b.contains(id)
            }
        }
    }

    pub fn leaves(&self) -> Vec<AccessibilityId> {
        match self {
            PanelNode::Leaf(id, _) => vec![*id],
            PanelNode::Horizontal(a, b) | PanelNode::Vertical(a, b) => {
                let mut v = a.leaves();
                v.extend(b.leaves());
                v
            }
        }
    }

    pub fn leaf_fraction(&self, id: AccessibilityId, axis: Axis) -> Option<f64> {
        match self {
            PanelNode::Leaf(pid, _) if *pid == id => Some(0.0),
            PanelNode::Leaf(_, _) => None,
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
            PanelNode::Leaf(id, _) => Some(*id),
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

    pub fn all_handles(&self) -> Vec<TerminalHandle> {
        match self {
            PanelNode::Leaf(_, h) => vec![h.clone()],
            PanelNode::Horizontal(a, b) | PanelNode::Vertical(a, b) => {
                let mut v = a.all_handles();
                v.extend(b.all_handles());
                v
            }
        }
    }

    pub fn handle(&self, id: AccessibilityId) -> Option<&TerminalHandle> {
        match self {
            PanelNode::Leaf(pid, h) if *pid == id => Some(h),
            PanelNode::Leaf(_, _) => None,
            PanelNode::Horizontal(a, b) | PanelNode::Vertical(a, b) => {
                a.handle(id).or_else(|| b.handle(id))
            }
        }
    }

    pub fn replace_leaf(self, target: AccessibilityId, replacement: PanelNode) -> PanelNode {
        match self {
            PanelNode::Leaf(id, _) if id == target => replacement,
            PanelNode::Leaf(_, _) => self,
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
            PanelNode::Leaf(id, _) if id == target => None,
            PanelNode::Leaf(_, _) => Some(self),
            PanelNode::Horizontal(a, b) => {
                if a.contains(target) {
                    if matches!(*a, PanelNode::Leaf(id, _) if id == target) {
                        return Some(*b);
                    }
                    let new_a = a.remove_leaf(target)?;
                    Some(PanelNode::Horizontal(Box::new(new_a), b))
                } else {
                    if matches!(*b, PanelNode::Leaf(id, _) if id == target) {
                        return Some(*a);
                    }
                    let new_b = b.remove_leaf(target)?;
                    Some(PanelNode::Horizontal(a, Box::new(new_b)))
                }
            }
            PanelNode::Vertical(a, b) => {
                if a.contains(target) {
                    if matches!(*a, PanelNode::Leaf(id, _) if id == target) {
                        return Some(*b);
                    }
                    let new_a = a.remove_leaf(target)?;
                    Some(PanelNode::Vertical(Box::new(new_a), b))
                } else {
                    if matches!(*b, PanelNode::Leaf(id, _) if id == target) {
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
    pub panels: PanelNode,
    pub active_panel: AccessibilityId,
}

impl Tab {
    pub fn new(index: usize, shell: &str, cwd: Option<PathBuf>) -> Self {
        let (active_panel, root) = PanelNode::new_leaf(shell, cwd);
        Self {
            id: TabId::new(),
            title: format!("Terminal {}", index + 1),
            panels: root,
            active_panel,
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct AppState {
    pub tabs: Vec<Tab>,
    pub active_tab: usize,
    pub font_size: f32,
    pub shell: String,
}

impl AppState {
    pub fn new(font_size: f32, shell: String) -> Self {
        let tab = Tab::new(0, &shell, None);
        Self {
            tabs: vec![tab],
            active_tab: 0,
            font_size,
            shell,
        }
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active_tab)
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active_tab)
    }

    pub fn new_tab(&mut self) {
        let index = self.tabs.len();
        let cwd = self
            .active_tab()
            .and_then(|tab| tab.panels.handle(tab.active_panel))
            .and_then(|h| h.cwd());
        let tab = Tab::new(index, &self.shell.clone(), cwd);
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        if let Some(tab) = self.active_tab() {
            Focus::new_for_id(tab.active_panel).request_focus();
        }
    }

    pub fn close_active_tab(&mut self) {
        if self.tabs.len() <= 1 {
            return;
        }
        self.tabs.remove(self.active_tab);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
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
            if let Some(tab) = self.tabs.get(self.active_tab) {
                Focus::new_for_id(tab.active_panel).request_focus();
            }
        }
    }

    pub fn next_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        self.active_tab = (self.active_tab + 1) % self.tabs.len();
    }

    pub fn prev_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        self.active_tab = self
            .active_tab
            .checked_sub(1)
            .unwrap_or(self.tabs.len() - 1);
    }

    pub fn split_horizontal(&mut self) {
        let shell = self.shell.clone();
        let cwd = self
            .active_tab()
            .and_then(|tab| tab.panels.handle(tab.active_panel))
            .and_then(|h| h.cwd());
        let (new_id, new_leaf) = PanelNode::new_leaf(&shell, cwd);
        if let Some(tab) = self.active_tab_mut() {
            let split = PanelNode::Horizontal(
                Box::new(PanelNode::Leaf(
                    tab.active_panel,
                    tab.panels.handle(tab.active_panel).cloned().unwrap(),
                )),
                Box::new(new_leaf),
            );
            tab.panels = tab.panels.clone().replace_leaf(tab.active_panel, split);
            tab.active_panel = new_id;
        }
    }

    pub fn split_vertical(&mut self) {
        let shell = self.shell.clone();
        let cwd = self
            .active_tab()
            .and_then(|tab| tab.panels.handle(tab.active_panel))
            .and_then(|h| h.cwd());
        let (new_id, new_leaf) = PanelNode::new_leaf(&shell, cwd);
        if let Some(tab) = self.active_tab_mut() {
            let split = PanelNode::Vertical(
                Box::new(PanelNode::Leaf(
                    tab.active_panel,
                    tab.panels.handle(tab.active_panel).cloned().unwrap(),
                )),
                Box::new(new_leaf),
            );
            tab.panels = tab.panels.clone().replace_leaf(tab.active_panel, split);
            tab.active_panel = new_id;
        }
    }

    // 4 panels grid
    pub fn split_into_grid(&mut self) {
        let shell = self.shell.clone();
        let cwd = self
            .active_tab()
            .and_then(|tab| tab.panels.handle(tab.active_panel))
            .and_then(|h| h.cwd());

        let (_, new_1) = PanelNode::new_leaf(&shell, cwd.clone());
        let (_, new_2) = PanelNode::new_leaf(&shell, cwd.clone());
        let (_, new_3) = PanelNode::new_leaf(&shell, cwd);

        if let Some(tab) = self.active_tab_mut() {
            let original_leaf = PanelNode::Leaf(
                tab.active_panel,
                tab.panels.handle(tab.active_panel).cloned().unwrap(),
            );
            let grid = PanelNode::Horizontal(
                Box::new(PanelNode::Vertical(
                    Box::new(original_leaf),
                    Box::new(new_1),
                )),
                Box::new(PanelNode::Vertical(Box::new(new_2), Box::new(new_3))),
            );
            tab.panels = tab.panels.clone().replace_leaf(tab.active_panel, grid);
            // active_panel stays as the original (top-left) leaf
        }
    }

    /// Collapses the current tab to only its active panel, closing all others.
    pub fn close_all_except_active(&mut self) {
        if let Some(tab) = self.active_tab_mut() {
            let active_id = tab.active_panel;
            let active_leaf =
                PanelNode::Leaf(active_id, tab.panels.handle(active_id).cloned().unwrap());
            tab.panels = active_leaf;
            // active_panel stays the same
            Focus::new_for_id(active_id).request_focus();
        }
    }

    pub fn close_active_panel(&mut self) {
        if let Some(tab) = self.active_tab_mut()
            && let Some(new_root) = tab.panels.clone().remove_leaf(tab.active_panel)
        {
            let leaves = new_root.leaves();
            tab.panels = new_root;
            if let Some(panel) = leaves.into_iter().last() {
                tab.active_panel = panel;
                Focus::new_for_id(panel).request_focus();
            }
        }
    }

    pub fn navigate(&mut self, dir: NavDirection) {
        if let Some(tab) = self.active_tab_mut()
            && let Some(neighbour) = tab.panels.find_neighbour(tab.active_panel, dir)
        {
            tab.active_panel = neighbour;
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

use freya::icons::lucide;
use freya::material_design::ButtonRippleExt;
use freya::prelude::*;
use freya::radio::*;

use std::collections::HashMap;

use crate::git::Worktree;
use crate::state::{
    AppChannel, AppState, AppStation, Modal, ProjectId, TabId, create_plain_tab, create_tab,
};

type AppRadio = Radio<AppState, AppChannel>;

/// One payload type for every sidebar drag so any row can be a drop target.
#[derive(Clone, PartialEq)]
enum DragPayload {
    Tab(TabId),
    Worktree(ProjectId, String),
}

#[derive(PartialEq, Clone, Copy)]
pub struct TabBar;

#[derive(PartialEq, Clone)]
struct ProjectGroup {
    id: ProjectId,
    name: String,
    collapsed: bool,
    has_archived: bool,
    worktree_rows: Vec<WorktreeRow>,
    plain_tabs: Vec<TabButton>,
}

impl Component for TabBar {
    fn render(&self) -> impl IntoElement {
        let mut radio = use_radio(AppChannel::Tabs);
        let station = use_radio_station::<AppState, AppChannel>();

        let (groups, loose_tabs, sidebar_collapsed) = {
            let state = radio.read();
            let index_of: HashMap<TabId, usize> = state
                .display_order()
                .into_iter()
                .enumerate()
                .map(|(i, id)| (id, i))
                .collect();
            let active_id = state.active_tab().map(|t| t.id);
            let tab_button = |tab: &crate::state::Tab| TabButton {
                tab_id: tab.id,
                index: index_of.get(&tab.id).copied().unwrap_or(0),
                title: tab.display_title().to_string(),
                custom_title: tab.custom_title.clone().unwrap_or_default(),
                is_active: active_id == Some(tab.id),
                outputting: tab.outputting,
                collapsed: state.sidebar_collapsed,
            };
            let groups: Vec<ProjectGroup> = state
                .projects
                .iter()
                .map(|project| {
                    let worktree_rows = state
                        .worktree_entries(project)
                        .into_iter()
                        .map(|entry| WorktreeRow {
                            project_id: project.id,
                            is_main: entry.worktree.is_main,
                            worktree: entry.worktree,
                            tab: entry.tab.map(|id| OpenTab {
                                id,
                                index: index_of.get(&id).copied().unwrap_or(0),
                                active: active_id == Some(id),
                                outputting: state
                                    .tabs
                                    .iter()
                                    .find(|t| t.id == id)
                                    .is_some_and(|t| t.outputting),
                            }),
                            compact: state.sidebar_collapsed,
                        })
                        .collect();
                    let plain_tabs = state
                        .tabs
                        .iter()
                        .filter(|t| t.project == Some(project.id) && t.worktree.is_none())
                        .map(tab_button)
                        .collect();
                    ProjectGroup {
                        id: project.id,
                        name: project.name.clone(),
                        collapsed: project.collapsed,
                        has_archived: !project.archived.is_empty(),
                        worktree_rows,
                        plain_tabs,
                    }
                })
                .collect();
            let loose_tabs: Vec<TabButton> = state
                .tabs
                .iter()
                .filter(|t| t.project.is_none())
                .map(tab_button)
                .collect();
            (groups, loose_tabs, state.sidebar_collapsed)
        };

        let has_projects = !groups.is_empty();
        let has_loose = !loose_tabs.is_empty();

        let mut items: Vec<Element> = vec![];
        for group in groups {
            let group_id = group.id;
            items.push(
                DropZone::new(
                    ProjectHeader {
                        id: group.id,
                        name: group.name.clone(),
                        collapsed: group.collapsed,
                        has_archived: group.has_archived,
                        compact: sidebar_collapsed,
                    },
                    move |payload: DragPayload| {
                        if let DragPayload::Tab(dragged_id) = payload {
                            radio
                                .write_channel(AppChannel::Tabs)
                                .reparent_tab(dragged_id, Some(group_id));
                        }
                    },
                )
                .key(&("project", group.id.0))
                .into_element(),
            );
            if group.collapsed {
                continue;
            }
            for row in group.worktree_rows {
                items.push(draggable_worktree_row(radio, row));
            }
            for tab in group.plain_tabs {
                items.push(draggable_tab(radio, tab));
            }
            items.push(new_tab_button(station, Some(group_id), sidebar_collapsed).into_element());
        }

        if has_projects && has_loose {
            items.push(
                rect()
                    .width(Size::fill())
                    .height(Size::px(1.))
                    .background((45, 45, 45))
                    .into_element(),
            );
        }

        let loose_section = DropZone::new(
            rect()
                .width(Size::fill())
                .min_height(Size::px(8.))
                .spacing(4.)
                .vertical()
                .children(loose_tabs.into_iter().map(|tab| draggable_tab(radio, tab))),
            move |payload: DragPayload| {
                if let DragPayload::Tab(dragged_id) = payload {
                    radio
                        .write_channel(AppChannel::Tabs)
                        .reparent_tab(dragged_id, None);
                }
            },
        );

        rect()
            .expanded()
            .background((20, 20, 20))
            .padding(4.)
            .spacing(4.)
            .direction(Direction::Vertical)
            .content(Content::flex())
            .child(
                ScrollView::new()
                    .height(Size::flex(1.))
                    .width(Size::fill())
                    .spacing(4.)
                    .show_scrollbar(false)
                    .children(items)
                    .child(loose_section),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(1.))
                    .background((45, 45, 45)),
            )
            .child(new_tab_button(station, None, sidebar_collapsed))
            .child(add_project_button(radio, sidebar_collapsed))
            .into_element()
    }
}

fn drag_preview(content: impl IntoElement) -> Rect {
    rect()
        .width(Size::px(200.))
        .background((45, 45, 45))
        .corner_radius(6.)
        .padding(8.)
        .layer(Layer::Overlay)
        .shadow(
            Shadow::new()
                .x(0.)
                .y(3.)
                .blur(10.)
                .spread(1.)
                .color(Color::from_argb(120, 0, 0, 0)),
        )
        .child(content)
}

fn draggable_tab(mut radio: AppRadio, tab: TabButton) -> Element {
    let drop_tab_id = tab.tab_id;
    let drag_title = tab.title.clone();
    DropZone::new(
        DragZone::new(DragPayload::Tab(tab.tab_id), tab)
            .show_while_dragging(false)
            .drag_element(drag_preview(
                label()
                    .text(drag_title)
                    .font_size(14.)
                    .color((230, 230, 230)),
            )),
        move |payload: DragPayload| {
            if let DragPayload::Tab(dragged_id) = payload {
                radio
                    .write_channel(AppChannel::Tabs)
                    .move_tab(dragged_id, drop_tab_id);
            }
        },
    )
    .key(&("tab", drop_tab_id.0))
    .into_element()
}

/// Worktree rows drag to reorder within their own project; main stays pinned.
fn draggable_worktree_row(mut radio: AppRadio, row: WorktreeRow) -> Element {
    let project_id = row.project_id;
    let name = row.worktree.name.clone();
    let target_name = name.clone();
    let row_key = name.clone();

    let inner: Element = if row.is_main {
        row.into_element()
    } else {
        let drag_title = name.clone();
        DragZone::new(DragPayload::Worktree(project_id, name), row)
            .show_while_dragging(false)
            .drag_element(drag_preview(
                rect()
                    .horizontal()
                    .spacing(6.)
                    .cross_align(Alignment::Center)
                    .child(
                        svg(lucide::git_branch())
                            .width(Size::px(13.))
                            .height(Size::px(13.))
                            .stroke((230, 230, 230)),
                    )
                    .child(
                        label()
                            .text(drag_title)
                            .font_size(14.)
                            .color((230, 230, 230)),
                    ),
            ))
            .into_element()
    };

    DropZone::new(inner, move |payload: DragPayload| {
        if let DragPayload::Worktree(dragged_project, dragged_name) = payload
            && dragged_project == project_id
        {
            radio
                .write_channel(AppChannel::Tabs)
                .reorder_worktree(project_id, &dragged_name, &target_name);
        }
    })
    .key(&("worktree", project_id.0, row_key))
    .into_element()
}

fn menu_item(text: &'static str, mut action: impl FnMut() + 'static) -> MenuButton {
    MenuButton::new()
        .on_press(move |e: Event<PressEventData>| {
            e.stop_propagation();
            e.prevent_default();
            ContextMenu::close();
            action();
        })
        .child(text)
}

fn open_project_menu(mut radio: AppRadio, id: ProjectId, has_archived: bool) {
    let mut menu = Menu::new().child(menu_item("Archive All Worktrees", move || {
        let mut state = radio.write_channel(AppChannel::Tabs);
        let archived: Vec<String> = state
            .project(id)
            .map(|p| {
                p.worktrees
                    .iter()
                    .filter(|wt| !wt.is_main)
                    .map(|wt| wt.name.clone())
                    .collect()
            })
            .unwrap_or_default();
        state.set_archived(id, archived);
    }));
    if has_archived {
        menu = menu.child(menu_item("Unarchive All Worktrees", move || {
            radio.write_channel(AppChannel::Tabs).set_archived(id, vec![]);
        }));
    }
    menu = menu.child(menu_item("Close Project", move || {
        radio.write_channel(AppChannel::Tabs).remove_project(id);
    }));
    ContextMenu::open(menu);
}

#[derive(PartialEq, Clone)]
struct ProjectHeader {
    id: ProjectId,
    name: String,
    collapsed: bool,
    has_archived: bool,
    compact: bool,
}

impl Component for ProjectHeader {
    fn render(&self) -> impl IntoElement {
        let id = self.id;
        let has_archived = self.has_archived;
        let mut radio = use_radio(AppChannel::Tabs);

        let chevron = if self.collapsed {
            lucide::chevron_right()
        } else {
            lucide::chevron_down()
        };

        let content: Element = if self.compact {
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .center()
                .on_secondary_down(move |_| open_project_menu(radio, id, has_archived))
                .child(
                    svg(chevron)
                        .width(Size::px(14.))
                        .height(Size::px(14.))
                        .stroke((150, 150, 150)),
                )
                .into_element()
        } else {
            rect()
                .width(Size::fill())
                .horizontal()
                .cross_align(Alignment::Center)
                .spacing(6.)
                .on_secondary_down(move |_| open_project_menu(radio, id, has_archived))
                .child(
                    svg(chevron)
                        .width(Size::px(14.))
                        .height(Size::px(14.))
                        .stroke((150, 150, 150)),
                )
                .child(
                    svg(lucide::folder_git_2())
                        .width(Size::px(14.))
                        .height(Size::px(14.))
                        .stroke((150, 150, 150)),
                )
                .child(
                    OverflowedContent::new()
                        .width(Size::flex(1.))
                        .height(Size::auto())
                        .child(label().text(self.name.clone()).font_size(13.).max_lines(1)),
                )
                .into_element()
        };

        Button::new()
            .flat()
            .width(Size::fill())
            .height(Size::px(28.))
            .compact()
            .rounded_lg()
            .hover_background((45, 45, 45))
            .on_press(move |_| {
                let mut state = radio.write_channel(AppChannel::Tabs);
                if let Some(project) = state.project_mut(id) {
                    project.collapsed = !project.collapsed;
                }
            })
            .color((200, 200, 200))
            .child(content)
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.id.0)
    }
}

fn open_worktree_menu(
    mut radio: AppRadio,
    project_id: ProjectId,
    worktree: &Worktree,
    is_main: bool,
    tab_id: Option<TabId>,
) {
    let mut menu = Menu::new();
    if let Some(tab_id) = tab_id {
        menu = menu.child(menu_item("Close Tab", move || {
            radio.write_channel(AppChannel::Tabs).close_tab_by_id(tab_id);
        }));
    }
    if !is_main {
        let name = worktree.name.clone();
        menu = menu.child(menu_item("Archive Worktree", move || {
            let mut state = radio.write_channel(AppChannel::Tabs);
            let mut archived = state
                .project(project_id)
                .map(|p| p.archived.clone())
                .unwrap_or_default();
            if !archived.contains(&name) {
                archived.push(name.clone());
                state.set_archived(project_id, archived);
            }
        }));
    }
    ContextMenu::open(menu);
}

#[derive(PartialEq, Clone, Copy)]
struct OpenTab {
    id: TabId,
    index: usize,
    active: bool,
    outputting: bool,
}

#[derive(PartialEq, Clone)]
struct WorktreeRow {
    project_id: ProjectId,
    is_main: bool,
    worktree: Worktree,
    tab: Option<OpenTab>,
    compact: bool,
}

impl Component for WorktreeRow {
    fn render(&self) -> impl IntoElement {
        let project_id = self.project_id;
        let worktree = self.worktree.clone();
        let tab = self.tab;
        let tab_id = tab.map(|t| t.id);
        let mut radio = use_radio(AppChannel::Tabs);
        let station = use_radio_station::<AppState, AppChannel>();
        let mut hovered = use_state(|| false);

        let is_open = tab.is_some();
        let is_active = tab.is_some_and(|t| t.active);
        let outputting = tab.is_some_and(|t| t.outputting);

        let background: Color = if is_active {
            (35, 35, 35).into()
        } else {
            (25, 25, 25).into()
        };
        let text_color: Color = if is_active {
            (230, 230, 230).into()
        } else if is_open {
            (170, 170, 170).into()
        } else {
            (110, 110, 110).into()
        };
        let icon = if self.is_main {
            lucide::house()
        } else {
            lucide::git_branch()
        };

        let on_press = {
            let path = worktree.path.clone();
            move |_: Event<PressEventData>| match tab {
                Some(tab) => {
                    radio.write_channel(AppChannel::Tabs).switch_to_tab(tab.id);
                }
                None => create_tab(station, Some(project_id), Some(path.clone()), None),
            }
        };

        let open_menu = {
            let worktree = worktree.clone();
            let is_main = self.is_main;
            move |_: Event<PressEventData>| {
                open_worktree_menu(radio, project_id, &worktree, is_main, tab_id);
            }
        };

        let content: Element = if self.compact {
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .center()
                .on_secondary_down(open_menu)
                .child(match tab {
                    Some(..) if outputting => loading_indicator(text_color),
                    Some(tab) => label()
                        .text(format!("{}", tab.index + 1))
                        .font_size(14.)
                        .into_element(),
                    None => svg(icon)
                        .width(Size::px(13.))
                        .height(Size::px(13.))
                        .stroke(text_color)
                        .into_element(),
                })
                .into_element()
        } else {
            let diff = self.worktree.diff.filter(|d| !d.is_clean());

            let name_block = rect()
                .width(Size::flex(1.))
                .height(Size::fill())
                .vertical()
                .main_align(Alignment::Center)
                .child(
                    OverflowedContent::new()
                        .width(Size::fill())
                        .height(Size::auto())
                        .child(label().text(self.worktree.name.clone()).max_lines(1)),
                )
                .map(diff, |el, diff| {
                    el.child(
                        rect()
                            .width(Size::fill())
                            .horizontal()
                            .spacing(6.)
                            .child(
                                label()
                                    .text(format!("+{}", diff.added))
                                    .font_size(11.)
                                    .color((108, 203, 129)),
                            )
                            .child(
                                label()
                                    .text(format!("−{}", diff.removed))
                                    .font_size(11.)
                                    .color((240, 113, 120)),
                            ),
                    )
                });

            // Fixed slot so the layout doesn't shift when a tab opens or closes.
            let trailing: Element = rect()
                .width(Size::px(20.))
                .height(Size::px(20.))
                .maybe_child(tab_id.map(|tab_id| {
                    if outputting && !*hovered.read() {
                        loading_indicator(text_color)
                    } else {
                        close_button(tab_id, radio, svg(lucide::moon()))
                    }
                }))
                .into_element();

            rect()
                .width(Size::fill())
                .height(Size::fill())
                .horizontal()
                .font_size(13.)
                .content(Content::flex())
                .cross_align(Alignment::Center)
                .spacing(6.)
                .padding((0., 0., 0., 10.))
                .on_pointer_over(move |_| hovered.set(true))
                .on_pointer_out(move |_| hovered.set(false))
                .on_secondary_down(open_menu)
                .child(
                    svg(icon)
                        .width(Size::px(13.))
                        .height(Size::px(13.))
                        .stroke(text_color),
                )
                .child(name_block)
                .child(trailing)
                .into_element()
        };

        Button::new()
            .width(Size::fill())
            .height(Size::px(if self.compact { 31. } else { 40. }))
            .flat()
            .rounded_lg()
            .background(background)
            .hover_background((45, 45, 45))
            .color(text_color)
            .on_press(on_press)
            .ripple()
            .color((230, 230, 230))
            .child(content)
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.worktree.path)
    }
}

/// The one style every sidebar action uses; icon-only when collapsed.
fn sidebar_action_button(
    icon: Svg,
    text: &'static str,
    collapsed: bool,
    on_press: impl FnMut(Event<PressEventData>) + 'static,
) -> impl IntoElement {
    Button::new()
        .flat()
        .width(Size::fill())
        .height(Size::px(31.))
        .rounded_lg()
        .hover_background((45, 45, 45))
        .on_press(on_press)
        .color((180, 180, 180))
        .child(if collapsed {
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .center()
                .child(
                    icon.width(Size::px(16.))
                        .height(Size::px(16.))
                        .stroke((200, 200, 200)),
                )
                .into_element()
        } else {
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .horizontal()
                .cross_align(Alignment::Center)
                .spacing(8.)
                .padding((0., 0., 0., 10.))
                .child(
                    icon.width(Size::px(16.))
                        .height(Size::px(16.))
                        .stroke((200, 200, 200)),
                )
                .child(label().text(text).font_size(14.))
                .into_element()
        })
}

/// New tab in `project`, or a loose tab when `None`.
fn new_tab_button(
    station: AppStation,
    project: Option<ProjectId>,
    collapsed: bool,
) -> impl IntoElement {
    sidebar_action_button(svg(lucide::circle_plus()), "New Tab", collapsed, move |_| {
        create_plain_tab(station, project);
    })
}

fn add_project_button(mut radio: AppRadio, collapsed: bool) -> impl IntoElement {
    sidebar_action_button(
        svg(lucide::folder_plus()),
        "Add Project",
        collapsed,
        move |_| {
            radio.write_channel(AppChannel::Tabs).modal = Some(Modal::AddProject);
        },
    )
}

fn close_button(tab_id: TabId, mut radio: AppRadio, icon: Svg) -> Element {
    Button::new()
        .flat()
        .width(Size::px(20.))
        .height(Size::px(20.))
        .compact()
        .rounded_full()
        .on_press(move |e: Event<PressEventData>| {
            e.stop_propagation();
            radio
                .write_channel(AppChannel::Tabs)
                .close_tab_by_id(tab_id);
        })
        .child(
            icon.width(Size::px(14.))
                .height(Size::px(14.))
                .stroke((200, 200, 200)),
        )
        .into_element()
}

fn loading_indicator(color: Color) -> Element {
    rect()
        .width(Size::px(20.))
        .height(Size::px(20.))
        .center()
        .child(CircularLoader::new().size(14.).primary_color(color))
        .into_element()
}

fn rename_input(
    rename_value: State<String>,
    input_a11y_id: AccessibilityId,
    tab_id: TabId,
    mut radio: AppRadio,
    mut editing: State<bool>,
    mut was_focused: State<bool>,
) -> Element {
    Input::new(rename_value)
        .flat()
        .compact()
        .width(Size::flex(1.))
        .auto_focus(true)
        .a11y_id(input_a11y_id)
        .background(Color::TRANSPARENT)
        .focus_background(Color::TRANSPARENT)
        .border_fill(Color::TRANSPARENT)
        .focus_border_fill(Color::TRANSPARENT)
        .inner_margin(Gaps::new(0., 0., 0., 0.))
        .on_submit(move |value: String| {
            radio
                .write_channel(AppChannel::Tabs)
                .rename_tab(tab_id, value);
            editing.set(false);
            was_focused.set(false);
        })
        .into_element()
}

fn tab_title(title: String) -> Element {
    OverflowedContent::new()
        .width(Size::flex(1.))
        .height(Size::auto())
        .child(label().text(title).max_lines(1))
        .into_element()
}

#[derive(PartialEq, Clone)]
struct TabButton {
    tab_id: TabId,
    index: usize,
    title: String,
    custom_title: String,
    is_active: bool,
    outputting: bool,
    collapsed: bool,
}

impl Component for TabButton {
    fn render(&self) -> impl IntoElement {
        let tab_id = self.tab_id;
        let custom_title = self.custom_title.clone();
        let is_active = self.is_active;
        let outputting = self.outputting;
        let mut radio = use_radio(AppChannel::Tabs);
        let mut hovered = use_state(|| false);
        let mut editing = use_state(|| false);
        let mut rename_value = use_state(String::new);

        let background: Color = if is_active {
            (35, 35, 35).into()
        } else {
            (25, 25, 25).into()
        };
        let text_color: Color = if is_active {
            (230, 230, 230).into()
        } else {
            (140, 140, 140).into()
        };

        // Track input focus to cancel editing on blur
        let input_a11y_id = use_a11y();
        let input_focus = use_focus(input_a11y_id);
        let mut was_focused = use_state(|| false);

        if *editing.read() {
            if input_focus().is_focused() {
                was_focused.set(true);
            } else if *was_focused.read() {
                editing.set(false);
                was_focused.set(false);
            }
        }

        let is_editing = *editing.read();
        let show_close = *hovered.read() || !outputting;

        let title_element = if is_editing {
            rename_input(
                rename_value,
                input_a11y_id,
                tab_id,
                radio,
                editing,
                was_focused,
            )
        } else {
            tab_title(self.title.clone())
        };

        let trailing = if show_close {
            close_button(tab_id, radio, svg(lucide::x()))
        } else {
            loading_indicator(text_color)
        };

        Button::new()
            .width(Size::fill())
            .height(Size::px(31.))
            .flat()
            .rounded_lg()
            .background(background)
            .hover_background((45, 45, 45))
            .color(text_color)
            .on_press(move |_: Event<PressEventData>| {
                if !is_editing {
                    radio.write_channel(AppChannel::Tabs).switch_to_tab(tab_id);
                }
            })
            .ripple()
            .color((230, 230, 230))
            .child(if self.collapsed {
                rect()
                    .width(Size::fill())
                    .height(Size::fill())
                    .center()
                    .child(if outputting {
                        loading_indicator(text_color)
                    } else {
                        label()
                            .text(format!("{}", self.index + 1))
                            .font_size(14.)
                            .into_element()
                    })
            } else {
                rect()
                    .width(Size::fill())
                    .height(Size::fill())
                    .horizontal()
                    .font_size(14.)
                    .content(Content::flex())
                    .cross_align(Alignment::Center)
                    .main_align(Alignment::SpaceBetween)
                    .on_pointer_over(move |_| hovered.set(true))
                    .on_pointer_out(move |_| hovered.set(false))
                    .on_secondary_down({
                        let custom_title = custom_title.clone();
                        move |_| {
                            let custom_title = custom_title.clone();
                            ContextMenu::open(
                                Menu::new()
                                    .child(
                                        MenuButton::new()
                                            .on_press(move |e: Event<PressEventData>| {
                                                e.stop_propagation();
                                                e.prevent_default();
                                                ContextMenu::close();
                                                was_focused.set(false);
                                                rename_value.set(custom_title.clone());
                                                editing.set(true);
                                            })
                                            .child("Rename"),
                                    )
                                    .child(
                                        MenuButton::new()
                                            .on_press(move |e: Event<PressEventData>| {
                                                e.stop_propagation();
                                                e.prevent_default();
                                                ContextMenu::close();
                                                radio
                                                    .write_channel(AppChannel::Tabs)
                                                    .close_tab_by_id(tab_id);
                                            })
                                            .child("Close"),
                                    ),
                            );
                        }
                    })
                    .child(title_element)
                    .child(trailing)
            })
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.tab_id.0)
    }
}

use freya::animation::*;
use freya::icons::lucide;
use freya::material_design::ButtonRippleExt;
use freya::prelude::*;
use freya::radio::*;

use async_io::Timer;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::components::titlebar::Titlebar;
use crate::git::Worktree;
use crate::state::{
    AppChannel, AppRadio, AppState, AppStation, Modal, ProjectId, TabId, WorktreeEntry,
};

/// Payload for every sidebar drag.
#[derive(Clone, PartialEq)]
enum DragPayload {
    Tab(TabId),
    Worktree(ProjectId, String),
    WorktreeGroup(ProjectId, String),
    TabGroup(Option<ProjectId>, String),
    Project(ProjectId),
}

/// What a [`GroupRow`] groups, and therefore which state ops it drives.
#[derive(PartialEq, Clone, Copy)]
enum GroupTarget {
    Worktrees(ProjectId),
    Tabs(Option<ProjectId>),
}

#[derive(PartialEq, Clone, Copy)]
pub struct TabBar;

#[derive(PartialEq, Clone)]
enum SidebarItem {
    Header(ProjectHeader),
    Group(GroupRow),
    Worktree(WorktreeRow),
    ArchivedFilter(ArchivedFilterRow),
    Tab(TabButton),
    Divider,
    LooseDrop,
}

impl SidebarItem {
    /// Item height plus the 4px gap below it.
    fn size(&self) -> f32 {
        let height = match self {
            SidebarItem::Header(_) => 28.,
            SidebarItem::Group(_) => 28.,
            SidebarItem::Worktree(row) => row.height(),
            SidebarItem::ArchivedFilter(_) => 34.,
            SidebarItem::Tab(tab) => {
                if tab.collapsed {
                    28.
                } else {
                    31.
                }
            }
            SidebarItem::Divider => 1.,
            SidebarItem::LooseDrop => 24.,
        };
        height + 4.
    }

    fn element(&self, mut radio: AppRadio) -> Element {
        match self {
            SidebarItem::Header(header) => {
                let group_id = header.id;
                let zone = DragZone::new(DragPayload::Project(group_id))
                    .child(header.clone())
                    .show_while_dragging(false)
                    .drag_element(drag_chip(
                        Some(SvgViewer::new(lucide::folder_git_2())),
                        header.name.clone(),
                    ));
                DropZone::new(move |payload: DragPayload| match payload {
                    DragPayload::Tab(dragged_id) => {
                        radio
                            .write_channel(AppChannel::Tabs)
                            .reparent_tab(dragged_id, Some(group_id));
                    }
                    DragPayload::Project(dragged_id) => {
                        radio
                            .write_channel(AppChannel::Tabs)
                            .move_project(dragged_id, group_id);
                    }
                    DragPayload::Worktree(..)
                    | DragPayload::WorktreeGroup(..)
                    | DragPayload::TabGroup(..) => {}
                })
                .child(zone)
                .into_element()
            }
            SidebarItem::Group(group) => {
                let target = group.target;
                let group_name = group.name.clone();
                let payload = match target {
                    GroupTarget::Worktrees(project_id) => {
                        DragPayload::WorktreeGroup(project_id, group.name.clone())
                    }
                    GroupTarget::Tabs(container) => {
                        DragPayload::TabGroup(container, group.name.clone())
                    }
                };
                let inner: Element = DragZone::new(payload)
                    .child(group.clone())
                    .show_while_dragging(false)
                    .drag_element(drag_chip(
                        Some(SvgViewer::new(lucide::folder())),
                        group.name.clone(),
                    ))
                    .into_element();
                let zone = DropZone::new(move |payload: DragPayload| {
                    let mut state = radio.write_channel(AppChannel::Tabs);
                    match (target, payload) {
                        (
                            GroupTarget::Worktrees(project_id),
                            DragPayload::Worktree(dragged_project, dragged_name),
                        ) if dragged_project == project_id => {
                            state.add_worktree_to_group(project_id, &group_name, &dragged_name);
                        }
                        (
                            GroupTarget::Worktrees(project_id),
                            DragPayload::WorktreeGroup(dragged_project, dragged_name),
                        ) if dragged_project == project_id && dragged_name != group_name => {
                            state.move_worktree_group_before(
                                project_id,
                                &dragged_name,
                                &group_name,
                            );
                        }
                        (GroupTarget::Tabs(container), DragPayload::Tab(dragged_id)) => {
                            state.append_tab_to_group(dragged_id, container, &group_name);
                        }
                        (
                            GroupTarget::Tabs(container),
                            DragPayload::TabGroup(dragged_container, dragged_name),
                        ) if !(dragged_container == container && dragged_name == group_name) => {
                            let first_member = state
                                .tabs
                                .iter()
                                .find(|tab| {
                                    tab.project == container
                                        && tab.group.as_deref() == Some(group_name.as_str())
                                })
                                .map(|tab| tab.id);
                            if let Some(first_member) = first_member {
                                state.move_tab_group(
                                    dragged_container,
                                    &dragged_name,
                                    first_member,
                                );
                            }
                        }
                        _ => {}
                    }
                })
                .child(inner);
                animated_portal(group.identity())
                    .key(group.identity())
                    .width(Size::fill())
                    .animation_dependency(group.index)
                    .child(zone)
                    .into_element()
            }
            SidebarItem::Worktree(row) => draggable_worktree_row(radio, row.clone()),
            SidebarItem::ArchivedFilter(row) => row.clone().into_element(),
            SidebarItem::Tab(tab) => draggable_tab(radio, tab.clone()),
            SidebarItem::Divider => rect()
                .width(Size::fill())
                .height(Size::px(1.))
                .background((70, 70, 70))
                .into_element(),
            SidebarItem::LooseDrop => DropZone::new(move |payload: DragPayload| {
                if let DragPayload::Tab(dragged_id) = payload {
                    radio
                        .write_channel(AppChannel::Tabs)
                        .reparent_tab(dragged_id, None);
                }
            })
            .child(rect().width(Size::fill()).height(Size::fill()))
            .into_element(),
        }
    }
}

impl Component for TabBar {
    fn render(&self) -> impl IntoElement {
        let radio = use_radio(AppChannel::Tabs);
        let station = use_radio_station::<AppState, AppChannel>();
        let mut tick = use_state(|| 0u32);

        // Re-render every minute so age labels roll over without state changes.
        use_hook(move || {
            spawn(async move {
                loop {
                    Timer::after(Duration::from_secs(60)).await;
                    *tick.write() += 1;
                }
            });
        });

        let (items, sidebar_collapsed) = {
            let _ = *tick.read();
            let state = radio.read();
            let index_of: HashMap<TabId, usize> = state
                .display_order()
                .into_iter()
                .enumerate()
                .map(|(i, id)| (id, i))
                .collect();
            let active_id = state.active_tab().map(|t| t.id);
            let tab_button = |tab: &crate::state::Tab, in_group: bool| TabButton {
                tab_id: tab.id,
                index: index_of.get(&tab.id).copied().unwrap_or(0),
                title: tab.display_title().to_string(),
                custom_title: tab.custom_title.clone().unwrap_or_default(),
                is_active: active_id == Some(tab.id),
                outputting: tab.outputting,
                collapsed: state.sidebar_collapsed,
                in_group,
            };

            // Tab rows for one container, group headers at their first member.
            let tab_section = |items: &mut Vec<SidebarItem>,
                               tabs: Vec<&crate::state::Tab>,
                               container: Option<ProjectId>| {
                let mut emitted: Vec<&str> = vec![];
                for (position, tab) in tabs.iter().enumerate() {
                    let Some(group) = &tab.group else {
                        items.push(SidebarItem::Tab(tab_button(tab, false)));
                        continue;
                    };
                    if emitted.iter().any(|name| name == group) {
                        continue;
                    }
                    emitted.push(group);
                    let members: Vec<&&crate::state::Tab> = tabs[position..]
                        .iter()
                        .filter(|member| member.group.as_deref() == Some(group.as_str()))
                        .collect();
                    let collapsed = state
                        .collapsed_tab_groups
                        .contains(&(container, group.clone()));
                    items.push(SidebarItem::Group(GroupRow {
                        target: GroupTarget::Tabs(container),
                        name: group.clone(),
                        index: index_of.get(&tab.id).copied().unwrap_or(0),
                        collapsed,
                        count: members.len(),
                        compact: state.sidebar_collapsed,
                    }));
                    if !collapsed {
                        items.extend(
                            members
                                .into_iter()
                                .map(|member| SidebarItem::Tab(tab_button(member, true))),
                        );
                    }
                }
            };

            let mut items: Vec<SidebarItem> = vec![];
            for project in &state.projects {
                let has_archived = !project.archived.is_empty();
                items.push(SidebarItem::Header(ProjectHeader {
                    id: project.id,
                    name: project.name.clone(),
                    collapsed: project.collapsed,
                    has_archived,
                    show_archived: project.show_archived,
                    compact: state.sidebar_collapsed,
                }));
                if project.collapsed {
                    continue;
                }
                let worktree_row = |entry: WorktreeEntry, index: usize, in_group: bool| {
                    let open_tab = entry
                        .tab
                        .and_then(|id| state.tabs.iter().find(|t| t.id == id));
                    SidebarItem::Worktree(WorktreeRow {
                        project_id: project.id,
                        index,
                        is_main: entry.worktree.is_main,
                        archived: entry.archived,
                        in_group,
                        worktree: entry.worktree,
                        tab: open_tab.map(|t| OpenTab {
                            id: t.id,
                            index: index_of.get(&t.id).copied().unwrap_or(0),
                            active: active_id == Some(t.id),
                            outputting: t.outputting,
                        }),
                        tab_title: open_tab
                            .map(|t| t.title.clone())
                            .filter(|title| !title.is_empty()),
                        age: open_tab.map(|t| format_age(t.last_output.elapsed())),
                        compact: state.sidebar_collapsed,
                    })
                };

                if project.show_archived && has_archived && !state.sidebar_collapsed {
                    items.push(SidebarItem::ArchivedFilter(ArchivedFilterRow {
                        project_id: project.id,
                    }));
                }
                let mut rows: Vec<WorktreeEntry> = Vec::new();
                let mut archived_rows: Vec<WorktreeEntry> = Vec::new();
                for entry in state.worktree_entries(project) {
                    if entry.archived {
                        archived_rows.push(entry);
                    } else {
                        rows.push(entry);
                    }
                }

                let group_of = |entry: &WorktreeEntry| {
                    (!entry.worktree.is_main)
                        .then(|| project.group_of(&entry.worktree.name))
                        .flatten()
                };
                let mut consumed = vec![false; rows.len()];
                let mut row_index = 0;
                for position in 0..rows.len() {
                    if consumed[position] {
                        continue;
                    }
                    let Some(group_index) = group_of(&rows[position]) else {
                        items.push(worktree_row(rows[position].clone(), row_index, false));
                        row_index += 1;
                        continue;
                    };
                    let group = &project.groups[group_index];
                    let member_indices: Vec<usize> = (position..rows.len())
                        .filter(|&candidate| {
                            !consumed[candidate] && group_of(&rows[candidate]) == Some(group_index)
                        })
                        .collect();
                    items.push(SidebarItem::Group(GroupRow {
                        target: GroupTarget::Worktrees(project.id),
                        name: group.name.clone(),
                        index: row_index,
                        collapsed: group.collapsed,
                        count: member_indices.len(),
                        compact: state.sidebar_collapsed,
                    }));
                    for member in member_indices {
                        consumed[member] = true;
                        if !group.collapsed {
                            items.push(worktree_row(rows[member].clone(), row_index, true));
                        }
                        row_index += 1;
                    }
                }
                for entry in archived_rows {
                    items.push(worktree_row(entry, row_index, false));
                    row_index += 1;
                }
                tab_section(
                    &mut items,
                    state
                        .tabs
                        .iter()
                        .filter(|t| t.project == Some(project.id) && t.worktree.is_none())
                        .collect(),
                    Some(project.id),
                );
            }
            let mut loose: Vec<SidebarItem> = vec![];
            tab_section(
                &mut loose,
                state.tabs.iter().filter(|t| t.project.is_none()).collect(),
                None,
            );
            if !state.projects.is_empty() && !loose.is_empty() {
                items.push(SidebarItem::Divider);
            }
            items.extend(loose);
            items.push(SidebarItem::LooseDrop);
            (items, state.sidebar_collapsed)
        };

        let sizes: Vec<f32> = items.iter().map(SidebarItem::size).collect();
        let length = items.len();

        rect()
            .expanded()
            .overflow(Overflow::Clip)
            .padding(if sidebar_collapsed {
                Gaps::new(6., 6., 6., 6.)
            } else {
                Gaps::new(6., 0., 6., 6.)
            })
            .spacing(4.)
            .direction(Direction::Vertical)
            .content(Content::flex())
            .child(Titlebar {
                compact: sidebar_collapsed,
            })
            .child(
                VirtualScrollView::new(move |item, _| {
                    rect()
                        .key(item.index)
                        .width(Size::fill())
                        .height(Size::px(item.size))
                        .padding((0., 0., 4., 0.))
                        .child(items[item.index].element(radio))
                        .into()
                })
                .length(length)
                .item_size(move |index: usize| sizes[index])
                .show_scrollbar(false)
                .width(Size::fill())
                .height(Size::flex(1.)),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(1.))
                    .background((70, 70, 70)),
            )
            .child(bottom_actions(radio, station, sidebar_collapsed))
            .into_element()
    }
}

fn pill_button(
    icon: SvgViewer,
    tooltip: &'static str,
    on_press: impl FnMut(Event<PressEventData>) + 'static,
) -> impl IntoElement {
    TooltipContainer::new(Tooltip::new_text(tooltip))
        .position(AttachedPosition::Top)
        .child(
            Button::new()
                .flat()
                .rounded_full()
                .expanded()
                .on_press(on_press)
                .color((200, 200, 200))
                .child(
                    icon.width(Size::px(17.))
                        .height(Size::px(17.))
                        .stroke((200, 200, 200)),
                ),
        )
}

fn bottom_actions(mut radio: AppRadio, station: AppStation, compact: bool) -> Element {
    if compact {
        rect()
            .width(Size::fill())
            .vertical()
            .spacing(4.)
            .child(sidebar_action_button(
                SvgViewer::new(lucide::panel_left_open()),
                "Expand Sidebar",
                true,
                move |_| {
                    radio.write_channel(AppChannel::Tabs).toggle_sidebar();
                },
            ))
            .child(sidebar_action_button(
                SvgViewer::new(lucide::circle_plus()),
                "New Tab",
                true,
                move |_| {
                    AppState::create_plain_tab(station, None);
                },
            ))
            .child(sidebar_action_button(
                SvgViewer::new(lucide::folder_plus()),
                "Add Project",
                true,
                move |_| {
                    radio.write_channel(AppChannel::Tabs).modal = Some(Modal::AddProject);
                },
            ))
            .child(sidebar_action_button(
                SvgViewer::new(lucide::info()),
                "About",
                true,
                move |_| {
                    radio.write_channel(AppChannel::Tabs).modal = Some(Modal::About);
                },
            ))
            .into_element()
    } else {
        rect()
            .width(Size::fill())
            .horizontal()
            .main_align(Alignment::End)
            .spacing(4.)
            .child(pill_button(
                SvgViewer::new(lucide::panel_left_close()),
                "Collapse Sidebar",
                move |_| {
                    radio.write_channel(AppChannel::Tabs).toggle_sidebar();
                },
            ))
            .child(pill_button(
                SvgViewer::new(lucide::circle_plus()),
                "New Tab",
                move |_| {
                    AppState::create_plain_tab(station, None);
                },
            ))
            .child(pill_button(
                SvgViewer::new(lucide::folder_plus()),
                "Add Project",
                move |_| {
                    radio.write_channel(AppChannel::Tabs).modal = Some(Modal::AddProject);
                },
            ))
            .child(pill_button(
                SvgViewer::new(lucide::info()),
                "About",
                move |_| {
                    radio.write_channel(AppChannel::Tabs).modal = Some(Modal::About);
                },
            ))
            .into_element()
    }
}

pub(crate) fn drag_preview(content: impl IntoElement) -> Rect {
    rect()
        .width(Size::px(260.))
        .background((62, 60, 66))
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

fn drag_chip(icon: Option<SvgViewer>, text: String) -> Rect {
    let mut row = rect()
        .horizontal()
        .spacing(6.)
        .cross_align(Alignment::Center);
    if let Some(icon) = icon {
        row = row.child(
            icon.width(Size::px(14.))
                .height(Size::px(14.))
                .stroke((230, 230, 230)),
        );
    }
    drag_preview(
        row.child(
            label()
                .text(text)
                .font_size(14.)
                .color((230, 230, 230))
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis),
        ),
    )
}

fn animated_portal<T>(id: T) -> Portal<T> {
    Portal::new(id)
        .function(Function::Expo)
        .ease(Ease::Out)
        .duration(Duration::from_millis(200))
}

fn draggable_tab(mut radio: AppRadio, tab: TabButton) -> Element {
    let drop_tab_id = tab.tab_id;
    let index = tab.index;
    let drag_title = tab.title.clone();
    let tooltip = tab.title.clone();
    let zone = DropZone::new(move |payload: DragPayload| match payload {
        DragPayload::Tab(dragged_id) => {
            radio
                .write_channel(AppChannel::Tabs)
                .move_tab(dragged_id, drop_tab_id);
        }
        DragPayload::TabGroup(dragged_container, dragged_name) => {
            radio.write_channel(AppChannel::Tabs).move_tab_group(
                dragged_container,
                &dragged_name,
                drop_tab_id,
            );
        }
        _ => {}
    })
    .child(
        DragZone::new(DragPayload::Tab(tab.tab_id))
            .child(tab)
            .show_while_dragging(false)
            .drag_element(
                animated_portal(drop_tab_id)
                    .animation_dependency(index)
                    .child(drag_chip(None, drag_title)),
            ),
    );
    animated_portal(drop_tab_id)
        .key(&("tab", drop_tab_id.0))
        .width(Size::fill())
        .animation_dependency(index)
        .child(
            TooltipContainer::new(Tooltip::new_text(tooltip))
                .position(AttachedPosition::Right)
                .child(zone),
        )
        .into_element()
}

/// Worktree rows drag to reorder within their own project.
fn draggable_worktree_row(mut radio: AppRadio, row: WorktreeRow) -> Element {
    let project_id = row.project_id;
    let index = row.index;
    let name = row.worktree.name.clone();
    let target_name = name.clone();
    let row_key = name.clone();
    let tooltip = name.clone();

    let inner: Element = if row.is_main {
        row.into_element()
    } else {
        let drag_title = name.clone();
        DragZone::new(DragPayload::Worktree(project_id, name))
            .child(row)
            .show_while_dragging(false)
            .drag_element(
                animated_portal((project_id, drag_title.clone()))
                    .animation_dependency(index)
                    .child(drag_chip(
                        Some(SvgViewer::new(lucide::git_branch())),
                        drag_title,
                    )),
            )
            .into_element()
    };

    let zone = DropZone::new(move |payload: DragPayload| match payload {
        DragPayload::Worktree(dragged_project, dragged_name) if dragged_project == project_id => {
            radio.write_channel(AppChannel::Tabs).reorder_worktree(
                project_id,
                &dragged_name,
                &target_name,
            );
        }
        DragPayload::WorktreeGroup(dragged_project, dragged_group)
            if dragged_project == project_id =>
        {
            radio.write_channel(AppChannel::Tabs).move_worktree_group(
                project_id,
                &dragged_group,
                &target_name,
            );
        }
        _ => {}
    })
    .child(inner);
    animated_portal((project_id, row_key.clone()))
        .key(&("worktree", project_id.0, row_key))
        .width(Size::fill())
        .animation_dependency(index)
        .child(
            TooltipContainer::new(Tooltip::new_text(tooltip))
                .position(AttachedPosition::Right)
                .child(zone),
        )
        .into_element()
}

pub(crate) fn menu_item(
    icon: SvgViewer,
    text: impl Into<String>,
    mut action: impl FnMut() + 'static,
) -> MenuButton {
    let text = text.into();
    MenuButton::new()
        .on_press(move |e: Event<PressEventData>| {
            e.stop_propagation();
            e.prevent_default();
            ContextMenu::close();
            action();
        })
        .child(
            rect()
                .horizontal()
                .spacing(8.)
                .cross_align(Alignment::Center)
                .child(
                    icon.width(Size::px(14.))
                        .height(Size::px(14.))
                        .stroke((180, 180, 180))
                        .stroke_width(2.5),
                )
                .child(label().text(text).font_size(14.)),
        )
}

pub(crate) fn copy_path_item(path: PathBuf) -> MenuButton {
    menu_item(SvgViewer::new(lucide::copy()), "Copy path", move || {
        let _ = Clipboard::set(path.display().to_string());
    })
}

fn open_project_menu(mut radio: AppRadio, station: AppStation, id: ProjectId) {
    let (root, has_archived) = match station.peek().project(id) {
        Some(project) => (Some(project.root.clone()), !project.archived.is_empty()),
        None => (None, false),
    };
    let menu = Menu::new()
        .map(root, |el, root| el.child(copy_path_item(root)))
        .child(menu_item(
            SvgViewer::new(lucide::refresh_cw()),
            "Refresh",
            move || {
                AppState::refresh_worktrees(station, id, true);
            },
        ))
        .child(menu_item(
            SvgViewer::new(lucide::moon()),
            "Sleep old worktrees",
            move || {
                radio
                    .write_channel(AppChannel::Tabs)
                    .sleep_old_worktrees(id);
            },
        ))
        .child(menu_item(
            SvgViewer::new(lucide::archive()),
            "Archive all worktrees",
            move || {
                radio.write_channel(AppChannel::Tabs).modal = Some(Modal::ConfirmArchiveAll(id));
            },
        ))
        .maybe(has_archived, |el| {
            el.child(menu_item(
                SvgViewer::new(lucide::archive_restore()),
                "Unarchive all worktrees",
                move || {
                    radio.write_channel(AppChannel::Tabs).modal =
                        Some(Modal::ConfirmUnarchiveAll(id));
                },
            ))
        })
        .child(menu_item(
            SvgViewer::new(lucide::x()),
            "Close project",
            move || {
                radio.write_channel(AppChannel::Tabs).modal = Some(Modal::ConfirmCloseProject(id));
            },
        ));
    ContextMenu::open_from_down(menu);
}

fn header_action(
    icon: SvgViewer,
    tooltip: &'static str,
    on_press: impl FnMut(Event<PressEventData>) + 'static,
) -> Element {
    TooltipContainer::new(Tooltip::new_text(tooltip))
        .position(AttachedPosition::Top)
        .child(
            Button::new()
                .flat()
                .width(Size::px(20.))
                .height(Size::px(20.))
                .compact()
                .rounded_full()
                .on_press(on_press)
                .child(
                    icon.width(Size::px(14.))
                        .height(Size::px(14.))
                        .stroke((150, 150, 150)),
                ),
        )
        .into_element()
}

#[derive(PartialEq, Clone)]
struct ProjectHeader {
    id: ProjectId,
    name: String,
    collapsed: bool,
    has_archived: bool,
    show_archived: bool,
    compact: bool,
}

impl Component for ProjectHeader {
    fn render(&self) -> impl IntoElement {
        let id = self.id;
        let has_archived = self.has_archived;
        let show_archived = self.show_archived;
        let mut radio = use_radio(AppChannel::Tabs);
        let station = use_radio_station::<AppState, AppChannel>();
        let mut hovered = use_state(|| false);
        let hovering = *hovered.read();

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
                .child(
                    SvgViewer::new(chevron)
                        .width(Size::px(14.))
                        .height(Size::px(14.))
                        .stroke((150, 150, 150)),
                )
                .into_element()
        } else {
            rect()
                .width(Size::fill())
                .horizontal()
                .content(Content::flex())
                .cross_align(Alignment::Center)
                .spacing(6.)
                .child(
                    SvgViewer::new(chevron)
                        .width(Size::px(14.))
                        .height(Size::px(14.))
                        .stroke((150, 150, 150)),
                )
                .child(
                    SvgViewer::new(lucide::folder_git_2())
                        .width(Size::px(14.))
                        .height(Size::px(14.))
                        .stroke((150, 150, 150)),
                )
                .child(
                    label()
                        .text(self.name.clone())
                        .width(Size::flex(1.))
                        .font_size(13.)
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                )
                .maybe(hovering, |el| {
                    el.maybe(has_archived, |el| {
                        let (icon, tooltip) = if show_archived {
                            (lucide::eye_off(), "Hide archived worktrees")
                        } else {
                            (lucide::eye(), "Show archived worktrees")
                        };
                        el.child(header_action(SvgViewer::new(icon), tooltip, move |e| {
                            e.stop_propagation();
                            radio
                                .write_channel(AppChannel::Tabs)
                                .toggle_show_archived(id);
                        }))
                    })
                    .child(header_action(
                        SvgViewer::new(lucide::circle_plus()),
                        "New tab",
                        move |e| {
                            e.stop_propagation();
                            AppState::create_plain_tab(station, Some(id));
                        },
                    ))
                    .child(header_action(
                        SvgViewer::new(lucide::arrow_down_up()),
                        "Sort worktrees",
                        move |e| {
                            e.stop_propagation();
                            radio.write_channel(AppChannel::Tabs).sort_worktrees(id);
                        },
                    ))
                })
                .into_element()
        };

        rect()
            .width(Size::fill())
            .on_pointer_over(move |_| hovered.set_if_modified(true))
            .on_pointer_out(move |_| hovered.set_if_modified(false))
            .child(
                Button::new()
                    .flat()
                    .width(Size::fill())
                    .height(Size::px(28.))
                    .compact()
                    .rounded_lg()
                    .hover_background(Color::from_argb(120, 80, 78, 86))
                    .on_pointer_down(move |e: Event<PointerEventData>| {
                        if let PointerEventData::Mouse(mouse) = e.data()
                            && mouse.button == Some(MouseButton::Right)
                        {
                            open_project_menu(radio, station, id)
                        }
                    })
                    .on_press(move |_| {
                        let mut state = radio.write_channel(AppChannel::Tabs);
                        if let Some(project) = state.project_mut(id) {
                            project.collapsed = !project.collapsed;
                        }
                    })
                    .color((200, 200, 200))
                    .child(content),
            )
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
    archived: bool,
    tab_id: Option<TabId>,
) {
    let in_group = radio
        .read()
        .project(project_id)
        .is_some_and(|project| project.group_of(&worktree.name).is_some());
    let menu = Menu::new()
        .child(copy_path_item(worktree.path.clone()))
        .map(tab_id, |el, tab_id| {
            el.child(menu_item(
                SvgViewer::new(lucide::moon()),
                "Sleep",
                move || {
                    radio
                        .write_channel(AppChannel::Tabs)
                        .close_tab_by_id(tab_id);
                },
            ))
        })
        .maybe(!is_main, |el| {
            let name = worktree.name.clone();
            let path = worktree.path.clone();
            let (icon, label) = if archived {
                (lucide::archive_restore(), "Unarchive")
            } else {
                (lucide::archive(), "Archive")
            };
            el.child(menu_item(SvgViewer::new(icon), label, move || {
                let mut state = radio.write_channel(AppChannel::Tabs);
                if archived {
                    state.unarchive_worktree(project_id, &name);
                } else {
                    let mut list = state
                        .project(project_id)
                        .map(|p| p.archived.clone())
                        .unwrap_or_default();
                    if !list.contains(&name) {
                        list.push(name.clone());
                    }
                    state.close_tabs_in_worktree(&path);
                    state.set_archived(project_id, list);
                }
            }))
        })
        .maybe(!is_main && !archived, |el| {
            el.maybe(in_group, |el| {
                let name = worktree.name.clone();
                el.child(menu_item(
                    SvgViewer::new(lucide::folder_minus()),
                    "Remove from group",
                    move || {
                        radio
                            .write_channel(AppChannel::Tabs)
                            .remove_worktree_from_group(project_id, &name);
                    },
                ))
            })
            .child(menu_item(
                SvgViewer::new(lucide::folder_plus()),
                "New group",
                {
                    let name = worktree.name.clone();
                    move || {
                        radio
                            .write_channel(AppChannel::Tabs)
                            .create_worktree_group(project_id, &name);
                    }
                },
            ))
        });
    ContextMenu::open_from_down(menu);
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
    index: usize,
    is_main: bool,
    archived: bool,
    worktree: Worktree,
    tab: Option<OpenTab>,
    tab_title: Option<String>,
    /// Time since the tab's last terminal output, like "1h".
    age: Option<String>,
    compact: bool,
    /// Member of a worktree group, indented under its header.
    in_group: bool,
}

impl WorktreeRow {
    /// Taller when the row has an open tab or uncommitted changes.
    fn height(&self) -> f32 {
        let dirty = self.worktree.diff.is_some_and(|diff| !diff.is_clean());
        if self.compact {
            28.
        } else if self.tab.is_some() {
            if dirty { 50. } else { 44. }
        } else if dirty {
            44.
        } else {
            28.
        }
    }
}

/// Branch name with the leading segment in bold, like "feat" in "feat/lalala".
fn worktree_name_label(worktree: &Worktree) -> Element {
    let name = &worktree.name;
    let prefix = worktree.branch_prefix();
    let at = if prefix.len() < name.len() {
        prefix.len()
    } else {
        0
    };
    paragraph()
        .width(Size::flex(1.))
        .max_lines(1)
        .text_overflow(TextOverflow::Ellipsis)
        .span(Span::new(name[..at].to_string()).font_weight(FontWeight::BOLD))
        .span(name[at..].to_string())
        .into_element()
}

/// Compact elapsed time like "now", "5m", "1h", "3d" or "2w".
fn format_age(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    if seconds < 60 {
        "now".to_string()
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else if seconds < 86400 {
        format!("{}h", seconds / 3600)
    } else if seconds < 604800 {
        format!("{}d", seconds / 86400)
    } else {
        format!("{}w", seconds / 604800)
    }
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
            Color::from_argb(160, 62, 60, 66)
        } else {
            Color::TRANSPARENT
        };
        let text_color: Color = if self.archived && !is_active {
            (110, 110, 110).into()
        } else if is_active {
            (230, 230, 230).into()
        } else if is_open {
            (170, 170, 170).into()
        } else {
            (110, 110, 110).into()
        };
        let icon = if self.archived {
            SvgViewer::new(lucide::archive()).stroke(text_color)
        } else if self.is_main {
            SvgViewer::new(lucide::house()).stroke(text_color)
        } else {
            SvgViewer::new(lucide::git_branch()).stroke(text_color)
        };

        let on_press = {
            let path = worktree.path.clone();
            let name = worktree.name.clone();
            let archived = self.archived;
            move |_: Event<PressEventData>| {
                if archived {
                    radio
                        .write_channel(AppChannel::Tabs)
                        .unarchive_worktree(project_id, &name);
                }
                match tab {
                    Some(tab) => {
                        radio.write_channel(AppChannel::Tabs).switch_to_tab(tab.id);
                    }
                    None => {
                        AppState::create_tab(station, Some(project_id), Some(path.clone()), None)
                    }
                }
            }
        };

        let open_menu = {
            let worktree = worktree.clone();
            let is_main = self.is_main;
            let archived = self.archived;
            move |e: Event<PointerEventData>| {
                if let PointerEventData::Mouse(mouse) = e.data()
                    && mouse.button == Some(MouseButton::Right)
                {
                    open_worktree_menu(radio, project_id, &worktree, is_main, archived, tab_id);
                }
            }
        };

        let content: Element = if self.compact {
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .center()
                .child(match tab {
                    Some(..) if outputting => loading_indicator(text_color),
                    Some(tab) => label()
                        .text(format!("{}", tab.index + 1))
                        .font_size(14.)
                        .max_lines(1)
                        .into_element(),
                    None => icon
                        .width(Size::px(14.))
                        .height(Size::px(14.))
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
                    rect()
                        .width(Size::fill())
                        .horizontal()
                        .content(Content::flex())
                        .cross_align(Alignment::Center)
                        .spacing(6.)
                        .child(worktree_name_label(&self.worktree))
                        .map(self.age.clone(), |el, age| {
                            el.child(label().text(age).font_size(11.).color((130, 130, 130)))
                        }),
                )
                .map(self.tab_title.clone(), |el, title| {
                    el.child(
                        label()
                            .text(title)
                            .width(Size::fill())
                            .color((130, 130, 130))
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis),
                    )
                })
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

            let leading = rect()
                .width(Size::px(28.))
                .height(Size::fill())
                .center()
                .child(match tab_id {
                    Some(tab_id) if *hovered.read() => {
                        close_button(tab_id, radio, SvgViewer::new(lucide::moon()))
                    }
                    Some(_) if outputting => loading_indicator(text_color),
                    _ => icon
                        .width(Size::px(14.))
                        .height(Size::px(14.))
                        .into_element(),
                });

            rect()
                .width(Size::fill())
                .height(Size::fill())
                .horizontal()
                .font_size(13.)
                .content(Content::flex())
                .cross_align(Alignment::Center)
                .spacing(6.)
                .child(leading)
                .child(name_block)
                .into_element()
        };

        let indent = if self.in_group && !self.compact {
            12.
        } else {
            0.
        };

        rect()
            .width(Size::fill())
            .height(Size::px(self.height()))
            .padding((0., 0., 0., indent))
            .on_pointer_over(move |_| hovered.set_if_modified(true))
            .on_pointer_out(move |_| hovered.set_if_modified(false))
            .child(
                Button::new()
                    .width(Size::fill())
                    .height(Size::fill())
                    .flat()
                    .rounded_lg()
                    .background(background)
                    .hover_background(Color::from_argb(120, 80, 78, 86))
                    .color(text_color)
                    .on_pointer_down(open_menu)
                    .on_press(on_press)
                    .ripple()
                    .color((230, 230, 230))
                    .child(content),
            )
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.worktree.path)
    }
}

/// Collapsible header for a named group of worktrees or tabs.
#[derive(PartialEq, Clone)]
struct GroupRow {
    target: GroupTarget,
    name: String,
    index: usize,
    collapsed: bool,
    count: usize,
    compact: bool,
}

impl GroupRow {
    /// Stable portal and diff identity for this group within its container.
    fn identity(&self) -> String {
        let container = match self.target {
            GroupTarget::Worktrees(project_id) => format!("worktrees-{}", project_id.0),
            GroupTarget::Tabs(Some(project_id)) => format!("tabs-{}", project_id.0),
            GroupTarget::Tabs(None) => "loose".to_string(),
        };
        format!("group-{container}-{}", self.name)
    }
}

impl Component for GroupRow {
    fn render(&self) -> impl IntoElement {
        let target = self.target;
        let name = self.name.clone();
        let collapsed = self.collapsed;
        let mut radio = use_radio(AppChannel::Tabs);
        let mut editing = use_state(|| false);
        let mut rename_value = use_state(String::new);
        let input_a11y_id = use_a11y();
        let input_focus = use_focus(input_a11y_id);
        let mut was_focused = use_state(|| false);
        let mut cancelled = use_state(|| false);

        // Blur commits the rename unless Escape cancelled it.
        use_side_effect({
            let name = name.clone();
            move || {
                if !*editing.read() {
                    return;
                }
                if input_focus().is_focused() {
                    was_focused.set(true);
                } else if *was_focused.read() {
                    if !*cancelled.read() {
                        let new_name = rename_value.peek().clone();
                        let mut state = radio.write_channel(AppChannel::Tabs);
                        match target {
                            GroupTarget::Worktrees(project_id) => {
                                state.rename_worktree_group(project_id, &name, new_name);
                            }
                            GroupTarget::Tabs(container) => {
                                state.rename_tab_group(container, &name, new_name);
                            }
                        }
                    }
                    editing.set(false);
                    was_focused.set(false);
                    cancelled.set(false);
                }
            }
        });

        let is_editing = *editing.read();
        let chevron = if collapsed {
            lucide::chevron_right()
        } else {
            lucide::chevron_down()
        };

        let open_menu = {
            let name = name.clone();
            move |event: Event<PointerEventData>| {
                let PointerEventData::Mouse(mouse) = event.data() else {
                    return;
                };
                if mouse.button != Some(MouseButton::Right) {
                    return;
                }
                let rename_target = name.clone();
                let dissolve_target = name.clone();
                ContextMenu::open_from_down(
                    Menu::new()
                        .child(menu_item(
                            SvgViewer::new(lucide::pencil()),
                            "Rename",
                            move || {
                                was_focused.set(false);
                                cancelled.set(false);
                                rename_value.set(rename_target.clone());
                                editing.set(true);
                            },
                        ))
                        .child(menu_item(
                            SvgViewer::new(lucide::folder_minus()),
                            "Ungroup",
                            move || {
                                let mut state = radio.write_channel(AppChannel::Tabs);
                                match target {
                                    GroupTarget::Worktrees(project_id) => {
                                        state.dissolve_worktree_group(project_id, &dissolve_target);
                                    }
                                    GroupTarget::Tabs(container) => {
                                        state.dissolve_tab_group(container, &dissolve_target);
                                    }
                                }
                            },
                        )),
                );
            }
        };

        let content: Element = if self.compact {
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .center()
                .child(
                    SvgViewer::new(lucide::folder())
                        .width(Size::px(14.))
                        .height(Size::px(14.))
                        .stroke((150, 150, 150)),
                )
                .into_element()
        } else {
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .horizontal()
                .font_size(13.)
                .content(Content::flex())
                .cross_align(Alignment::Center)
                .spacing(6.)
                .child(
                    rect()
                        .width(Size::px(28.))
                        .height(Size::fill())
                        .center()
                        .child(
                            SvgViewer::new(chevron)
                                .width(Size::px(14.))
                                .height(Size::px(14.))
                                .stroke((150, 150, 150)),
                        ),
                )
                .child(if is_editing {
                    rename_input(rename_value, input_a11y_id, cancelled)
                } else {
                    label()
                        .text(name.clone())
                        .width(Size::flex(1.))
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis)
                        .into_element()
                })
                .maybe(!is_editing, |el| {
                    el.child(
                        label()
                            .text(format!("{}", self.count))
                            .font_size(11.)
                            .color((110, 110, 110)),
                    )
                })
                .into_element()
        };

        Button::new()
            .width(Size::fill())
            .height(Size::px(28.))
            .flat()
            .rounded_lg()
            .hover_background(Color::from_argb(120, 80, 78, 86))
            .color((150, 150, 150))
            .on_pointer_down(open_menu)
            .on_press(move |_: Event<PressEventData>| {
                if *editing.peek() {
                    return;
                }
                let mut state = radio.write_channel(AppChannel::Tabs);
                match target {
                    GroupTarget::Worktrees(project_id) => {
                        state.toggle_worktree_group(project_id, &name);
                    }
                    GroupTarget::Tabs(container) => {
                        state.toggle_tab_group(container, &name);
                    }
                }
            })
            .child(content)
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.identity())
    }
}

/// Transparent name filter for all of a project's worktrees, shown at the top of the list.
#[derive(PartialEq, Clone)]
struct ArchivedFilterRow {
    project_id: ProjectId,
}

impl Component for ArchivedFilterRow {
    fn render(&self) -> impl IntoElement {
        let radio = use_radio(AppChannel::Tabs);
        let id = self.project_id;
        let value = radio
            .slice_mut(AppChannel::Tabs, move |state: &mut AppState| {
                state.archived_filters.entry(id).or_default()
            })
            .into_writable();

        rect()
            .width(Size::fill())
            .height(Size::px(34.))
            .font_size(13.)
            .child(
                Input::new(value)
                    .flat()
                    .width(Size::fill())
                    .placeholder("Filter worktrees")
                    .background(Color::TRANSPARENT)
                    .focus_background(Color::TRANSPARENT)
                    .border_fill(Color::TRANSPARENT)
                    .focus_border_fill(Color::TRANSPARENT)
                    .corner_radius(CornerRadius::new_all(8.))
                    .inner_margin(Gaps::new(8., 8., 8., 2.))
                    .leading(
                        rect()
                            .width(Size::px(36.))
                            .height(Size::fill())
                            .center()
                            .child(
                                SvgViewer::new(lucide::search())
                                    .width(Size::px(14.))
                                    .height(Size::px(14.))
                                    .stroke((110, 110, 110)),
                            )
                            .into_element(),
                    ),
            )
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&("archived-filter", self.project_id.0))
    }
}

/// Shared style for sidebar actions, icon-only when collapsed.
fn sidebar_action_button(
    icon: SvgViewer,
    text: &'static str,
    collapsed: bool,
    on_press: impl FnMut(Event<PressEventData>) + 'static,
) -> impl IntoElement {
    Button::new()
        .flat()
        .width(Size::fill())
        .height(Size::px(if collapsed { 31. } else { 34. }))
        .rounded_lg()
        .hover_background(Color::from_argb(120, 80, 78, 86))
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
                .spacing(6.)
                .child(
                    rect()
                        .width(Size::px(28.))
                        .height(Size::fill())
                        .center()
                        .child(
                            icon.width(Size::px(14.))
                                .height(Size::px(14.))
                                .stroke((200, 200, 200)),
                        ),
                )
                .child(label().text(text).font_size(13.))
                .into_element()
        })
}

fn close_button(tab_id: TabId, mut radio: AppRadio, icon: SvgViewer) -> Element {
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
    mut cancelled: State<bool>,
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
        .on_pre_key_down(move |e: Event<KeyboardEventData>| match &e.key {
            Key::Named(NamedKey::Enter) => {
                input_a11y_id.request_unfocus();
                false
            }
            Key::Named(NamedKey::Escape) => {
                cancelled.set(true);
                true
            }
            Key::Named(NamedKey::Tab) => false,
            _ => {
                e.stop_propagation();
                e.prevent_default();
                true
            }
        })
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
    /// Member of a tab group, indented under its header.
    in_group: bool,
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
            Color::from_argb(160, 62, 60, 66)
        } else {
            Color::TRANSPARENT
        };
        let text_color: Color = if is_active {
            (230, 230, 230).into()
        } else {
            (140, 140, 140).into()
        };

        let input_a11y_id = use_a11y();
        let input_focus = use_focus(input_a11y_id);
        let mut was_focused = use_state(|| false);
        let mut cancelled = use_state(|| false);

        // Blur commits the rename unless Escape cancelled it.
        use_side_effect(move || {
            if !*editing.read() {
                return;
            }
            if input_focus().is_focused() {
                was_focused.set(true);
            } else if *was_focused.read() {
                if !*cancelled.read() {
                    radio
                        .write_channel(AppChannel::Tabs)
                        .rename_tab(tab_id, rename_value.peek().clone());
                }
                editing.set(false);
                was_focused.set(false);
                cancelled.set(false);
            }
        });

        let is_editing = *editing.read();

        let title_element = if is_editing {
            rename_input(rename_value, input_a11y_id, cancelled)
        } else {
            label()
                .text(self.title.clone())
                .width(Size::flex(1.))
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis)
                .into_element()
        };

        let trailing: Element = if *hovered.read() {
            close_button(tab_id, radio, SvgViewer::new(lucide::x()))
        } else if outputting {
            loading_indicator(text_color)
        } else {
            rect().into_element()
        };

        let button = Button::new()
            .width(Size::fill())
            .height(Size::fill())
            .flat()
            .rounded_lg()
            .background(background)
            .hover_background(Color::from_argb(120, 80, 78, 86))
            .color(text_color)
            .on_pointer_down({
                let custom_title = custom_title.clone();
                move |e: Event<PointerEventData>| {
                    let PointerEventData::Mouse(mouse) = e.data() else {
                        return;
                    };
                    if mouse.button != Some(MouseButton::Right) {
                        return;
                    }
                    let custom_title = custom_title.clone();
                    let (in_group, path) =
                        match radio.read().tabs.iter().find(|tab| tab.id == tab_id) {
                            Some(tab) => (
                                tab.group.is_some(),
                                tab.panels
                                    .handle(tab.active_panel)
                                    .and_then(|handle| handle.cwd())
                                    .or_else(|| tab.worktree.clone()),
                            ),
                            None => (false, None),
                        };
                    ContextMenu::open_from_down(
                        Menu::new()
                            .map(path, |el, path| el.child(copy_path_item(path)))
                            .child(menu_item(
                                SvgViewer::new(lucide::pencil()),
                                "Rename",
                                move || {
                                    was_focused.set(false);
                                    cancelled.set(false);
                                    rename_value.set(custom_title.clone());
                                    editing.set(true);
                                },
                            ))
                            .child(menu_item(SvgViewer::new(lucide::x()), "Close", move || {
                                radio
                                    .write_channel(AppChannel::Tabs)
                                    .close_tab_by_id(tab_id);
                            }))
                            .maybe(in_group, |el| {
                                el.child(menu_item(
                                    SvgViewer::new(lucide::folder_minus()),
                                    "Remove from group",
                                    move || {
                                        radio
                                            .write_channel(AppChannel::Tabs)
                                            .set_tab_group(tab_id, None);
                                    },
                                ))
                            })
                            .child(menu_item(
                                SvgViewer::new(lucide::folder_plus()),
                                "New group",
                                move || {
                                    radio
                                        .write_channel(AppChannel::Tabs)
                                        .create_tab_group(tab_id);
                                },
                            )),
                    );
                }
            })
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
                            .max_lines(1)
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
                    .spacing(6.)
                    .child(title_element)
                    .child(trailing)
            });

        let indent = if self.in_group && !self.collapsed {
            12.
        } else {
            0.
        };

        rect()
            .width(Size::fill())
            .height(Size::px(if self.collapsed { 28. } else { 31. }))
            .padding((0., 0., 0., indent))
            .on_pointer_over(move |_| hovered.set_if_modified(true))
            .on_pointer_out(move |_| hovered.set_if_modified(false))
            .child(button)
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.tab_id.0)
    }
}

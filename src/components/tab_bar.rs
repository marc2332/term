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

/// Payload for every sidebar drag.
#[derive(Clone, PartialEq)]
enum DragPayload {
    Tab(TabId),
    Worktree(ProjectId, String),
}

#[derive(PartialEq, Clone, Copy)]
pub struct TabBar;

#[derive(PartialEq, Clone)]
enum SidebarItem {
    Header(ProjectHeader),
    Worktree(WorktreeRow),
    Tab(TabButton),
    Divider,
    LooseDrop,
}

fn worktree_row_height(row: &WorktreeRow) -> f32 {
    let dirty = row.worktree.diff.is_some_and(|d| !d.is_clean());
    if row.compact {
        31.
    } else if row.tab.is_some() {
        if dirty { 50. } else { 44. }
    } else if dirty {
        44.
    } else {
        34.
    }
}

/// Row height plus the 4px gap below it.
fn sidebar_item_size(item: &SidebarItem) -> f32 {
    let height = match item {
        SidebarItem::Header(_) => 28.,
        SidebarItem::Worktree(row) => worktree_row_height(row),
        SidebarItem::Tab(_) => 31.,
        SidebarItem::Divider => 1.,
        SidebarItem::LooseDrop => 24.,
    };
    height + 4.
}

fn sidebar_item_element(mut radio: AppRadio, item: &SidebarItem) -> Element {
    match item {
        SidebarItem::Header(header) => {
            let group_id = header.id;
            DropZone::new(header.clone(), move |payload: DragPayload| {
                if let DragPayload::Tab(dragged_id) = payload {
                    radio
                        .write_channel(AppChannel::Tabs)
                        .reparent_tab(dragged_id, Some(group_id));
                }
            })
            .into_element()
        }
        SidebarItem::Worktree(row) => draggable_worktree_row(radio, row.clone()),
        SidebarItem::Tab(tab) => draggable_tab(radio, tab.clone()),
        SidebarItem::Divider => rect()
            .width(Size::fill())
            .height(Size::px(1.))
            .background((45, 45, 45))
            .into_element(),
        SidebarItem::LooseDrop => DropZone::new(
            rect().width(Size::fill()).height(Size::fill()),
            move |payload: DragPayload| {
                if let DragPayload::Tab(dragged_id) = payload {
                    radio
                        .write_channel(AppChannel::Tabs)
                        .reparent_tab(dragged_id, None);
                }
            },
        )
        .into_element(),
    }
}

impl Component for TabBar {
    fn render(&self) -> impl IntoElement {
        let radio = use_radio(AppChannel::Tabs);
        let station = use_radio_station::<AppState, AppChannel>();

        let (items, sidebar_collapsed) = {
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

            let mut items: Vec<SidebarItem> = vec![];
            for project in &state.projects {
                items.push(SidebarItem::Header(ProjectHeader {
                    id: project.id,
                    name: project.name.clone(),
                    collapsed: project.collapsed,
                    has_archived: !project.archived.is_empty(),
                    show_archived: project.show_archived,
                    compact: state.sidebar_collapsed,
                }));
                if project.collapsed {
                    continue;
                }
                for entry in state.worktree_entries(project) {
                    let open_tab = entry
                        .tab
                        .and_then(|id| state.tabs.iter().find(|t| t.id == id));
                    items.push(SidebarItem::Worktree(WorktreeRow {
                        project_id: project.id,
                        is_main: entry.worktree.is_main,
                        archived: entry.archived,
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
                        compact: state.sidebar_collapsed,
                    }));
                }
                items.extend(
                    state
                        .tabs
                        .iter()
                        .filter(|t| t.project == Some(project.id) && t.worktree.is_none())
                        .map(|t| SidebarItem::Tab(tab_button(t))),
                );
            }
            let loose: Vec<SidebarItem> = state
                .tabs
                .iter()
                .filter(|t| t.project.is_none())
                .map(|t| SidebarItem::Tab(tab_button(t)))
                .collect();
            if !state.projects.is_empty() && !loose.is_empty() {
                items.push(SidebarItem::Divider);
            }
            items.extend(loose);
            items.push(SidebarItem::LooseDrop);
            (items, state.sidebar_collapsed)
        };

        let sizes: Vec<f32> = items.iter().map(sidebar_item_size).collect();
        let length = items.len();

        rect()
            .expanded()
            .background((26, 25, 28))
            .overflow(Overflow::Clip)
            .child(sidebar_backdrop())
            .padding(4.)
            .spacing(4.)
            .direction(Direction::Vertical)
            .content(Content::flex())
            .child(
                VirtualScrollView::new(move |item, _| {
                    rect()
                        .key(item.index)
                        .width(Size::fill())
                        .height(Size::px(item.size))
                        .padding((0., 0., 4., 0.))
                        .child(sidebar_item_element(radio, &items[item.index]))
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
                    .background((45, 45, 45)),
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
    rect().width(Size::flex(1.)).child(
        TooltipContainer::new(Tooltip::new(tooltip))
            .position(AttachedPosition::Top)
            .child(
                Button::new()
                    .flat()
                    .rounded_full()
                    .width(Size::fill())
                    .on_press(on_press)
                    .color((200, 200, 200))
                    .child(
                        rect().width(Size::fill()).center().child(
                            icon.width(Size::px(15.))
                                .height(Size::px(15.))
                                .stroke((200, 200, 200)),
                        ),
                    ),
            ),
    )
}

fn bottom_actions(mut radio: AppRadio, station: AppStation, compact: bool) -> Element {
    if compact {
        rect()
            .width(Size::fill())
            .vertical()
            .child(new_tab_button(station, None, true))
            .child(add_project_button(radio, true))
            .into_element()
    } else {
        rect()
            .width(Size::fill())
            .horizontal()
            .content(Content::flex())
            .spacing(8.)
            .child(pill_button(
                SvgViewer::new(lucide::circle_plus()),
                "New Tab",
                move |_| {
                    create_plain_tab(station, None);
                },
            ))
            .child(pill_button(
                SvgViewer::new(lucide::folder_plus()),
                "Add Project",
                move |_| {
                    radio.write_channel(AppChannel::Tabs).modal = Some(Modal::AddProject);
                },
            ))
            .into_element()
    }
}

/// Soft mesh backdrop made of radial gradient blobs fading to transparent.
fn sidebar_backdrop() -> Rect {
    let blob = |left: f32, top: f32, size: f32, r: u8, g: u8, b: u8| {
        rect()
            .position(Position::new_absolute().left(left).top(top))
            .interactive(false)
            .width(Size::px(size))
            .height(Size::px(size))
            .background(
                RadialGradient::new()
                    .stop((Color::from_argb(160, r, g, b), 0.))
                    .stop((Color::from_argb(110, r, g, b), 30.))
                    .stop((Color::from_argb(50, r, g, b), 62.))
                    .stop((Color::from_argb(0, r, g, b), 95.)),
            )
    };
    rect()
        .position(Position::new_absolute().left(0.).top(0.))
        .interactive(false)
        .width(Size::fill())
        .height(Size::fill())
        .child(blob(-200., -200., 560., 30, 46, 64))
        .child(blob(-40., 100., 620., 38, 66, 52))
        .child(blob(-220., 380., 660., 62, 54, 30))
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
    let tooltip = tab.title.clone();
    let zone = DropZone::new(
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
    );
    rect()
        .width(Size::fill())
        .child(
            TooltipContainer::new(Tooltip::new(tooltip))
                .position(AttachedPosition::Right)
                .child(zone),
        )
        .key(&("tab", drop_tab_id.0))
        .into_element()
}

/// Worktree rows drag to reorder within their own project.
fn draggable_worktree_row(mut radio: AppRadio, row: WorktreeRow) -> Element {
    let project_id = row.project_id;
    let name = row.worktree.name.clone();
    let target_name = name.clone();
    let row_key = name.clone();
    let tooltip = name.clone();

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
                        SvgViewer::new(lucide::git_branch())
                            .width(Size::px(14.))
                            .height(Size::px(14.))
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

    let zone = DropZone::new(inner, move |payload: DragPayload| {
        if let DragPayload::Worktree(dragged_project, dragged_name) = payload
            && dragged_project == project_id
        {
            radio.write_channel(AppChannel::Tabs).reorder_worktree(
                project_id,
                &dragged_name,
                &target_name,
            );
        }
    });
    rect()
        .width(Size::fill())
        .child(
            TooltipContainer::new(Tooltip::new(tooltip))
                .position(AttachedPosition::Right)
                .child(zone),
        )
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

fn open_project_menu(
    event: &Event<PressEventData>,
    mut radio: AppRadio,
    id: ProjectId,
    has_archived: bool,
    show_archived: bool,
) {
    let mut menu = Menu::new().child(menu_item("Archive All Worktrees", move || {
        let mut state = radio.write_channel(AppChannel::Tabs);
        let targets: Vec<(String, std::path::PathBuf)> = state
            .project(id)
            .map(|p| {
                p.worktrees
                    .iter()
                    .filter(|wt| !wt.is_main)
                    .map(|wt| (wt.name.clone(), wt.path.clone()))
                    .collect()
            })
            .unwrap_or_default();
        state.set_archived(id, targets.iter().map(|(name, _)| name.clone()).collect());
        for (_, path) in &targets {
            state.close_tabs_in_worktree(path);
        }
    }));
    if has_archived {
        let label = if show_archived {
            "Hide Archived Worktrees"
        } else {
            "Show Archived Worktrees"
        };
        menu = menu.child(menu_item(label, move || {
            radio
                .write_channel(AppChannel::Tabs)
                .toggle_show_archived(id);
        }));
        menu = menu.child(menu_item("Unarchive All Worktrees", move || {
            radio
                .write_channel(AppChannel::Tabs)
                .set_archived(id, vec![]);
        }));
    }
    menu = menu.child(menu_item("Close Project", move || {
        radio.write_channel(AppChannel::Tabs).remove_project(id);
    }));
    ContextMenu::open_from_event(event, menu);
}

fn header_action(
    icon: SvgViewer,
    on_press: impl FnMut(Event<PressEventData>) + 'static,
) -> Element {
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
                .child(header_action(
                    SvgViewer::new(lucide::circle_plus()),
                    move |e| {
                        e.stop_propagation();
                        create_plain_tab(station, Some(id));
                    },
                ))
                .child(header_action(
                    SvgViewer::new(lucide::arrow_down_up()),
                    move |e| {
                        e.stop_propagation();
                        radio.write_channel(AppChannel::Tabs).sort_worktrees(id);
                    },
                ))
                .into_element()
        };

        Button::new()
            .flat()
            .width(Size::fill())
            .height(Size::px(28.))
            .compact()
            .rounded_lg()
            .hover_background(Color::from_argb(120, 80, 78, 86))
            .on_secondary_down(move |e: Event<PressEventData>| {
                open_project_menu(&e, radio, id, has_archived, show_archived)
            })
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
    event: &Event<PressEventData>,
    mut radio: AppRadio,
    project_id: ProjectId,
    worktree: &Worktree,
    is_main: bool,
    archived: bool,
    tab_id: Option<TabId>,
) {
    let mut menu = Menu::new();
    if let Some(tab_id) = tab_id {
        menu = menu.child(menu_item("Close Tab", move || {
            radio
                .write_channel(AppChannel::Tabs)
                .close_tab_by_id(tab_id);
        }));
    }
    if !is_main {
        let name = worktree.name.clone();
        let path = worktree.path.clone();
        let label = if archived {
            "Unarchive Worktree"
        } else {
            "Archive Worktree"
        };
        menu = menu.child(menu_item(label, move || {
            let mut state = radio.write_channel(AppChannel::Tabs);
            let mut list = state
                .project(project_id)
                .map(|p| p.archived.clone())
                .unwrap_or_default();
            if archived {
                list.retain(|n| n != &name);
            } else {
                if !list.contains(&name) {
                    list.push(name.clone());
                }
                state.close_tabs_in_worktree(&path);
            }
            state.set_archived(project_id, list);
        }));
    }
    ContextMenu::open_from_event(event, menu);
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
    archived: bool,
    worktree: Worktree,
    tab: Option<OpenTab>,
    tab_title: Option<String>,
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
            lucide::archive()
        } else if self.is_main {
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
            let archived = self.archived;
            move |e: Event<PressEventData>| {
                open_worktree_menu(&e, radio, project_id, &worktree, is_main, archived, tab_id);
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
                    None => SvgViewer::new(icon)
                        .width(Size::px(14.))
                        .height(Size::px(14.))
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
                    label()
                        .text(self.worktree.name.clone())
                        .width(Size::fill())
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
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

            // Fixed slot, the layout must not shift when a tab opens or closes.
            let trailing: Element = rect()
                .width(Size::px(20.))
                .height(Size::px(20.))
                .maybe_child(tab_id.map(|tab_id| {
                    if outputting && !*hovered.read() {
                        loading_indicator(text_color)
                    } else {
                        close_button(tab_id, radio, SvgViewer::new(lucide::moon()))
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
                .on_pointer_over(move |_| hovered.set(true))
                .on_pointer_out(move |_| hovered.set(false))
                .child(
                    rect()
                        .width(Size::px(28.))
                        .height(Size::fill())
                        .center()
                        .child(
                            SvgViewer::new(icon)
                                .width(Size::px(14.))
                                .height(Size::px(14.))
                                .stroke(text_color),
                        ),
                )
                .child(name_block)
                .child(trailing)
                .into_element()
        };

        Button::new()
            .width(Size::fill())
            .height(Size::px(worktree_row_height(self)))
            .flat()
            .rounded_lg()
            .background(background)
            .hover_background(Color::from_argb(120, 80, 78, 86))
            .color(text_color)
            .on_secondary_down(open_menu)
            .on_press(on_press)
            .ripple()
            .color((230, 230, 230))
            .child(content)
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.worktree.path)
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

/// New tab in `project`, or a loose tab when `None`.
fn new_tab_button(
    station: AppStation,
    project: Option<ProjectId>,
    collapsed: bool,
) -> impl IntoElement {
    sidebar_action_button(
        SvgViewer::new(lucide::circle_plus()),
        "New Tab",
        collapsed,
        move |_| {
            create_plain_tab(station, project);
        },
    )
}

fn add_project_button(mut radio: AppRadio, collapsed: bool) -> impl IntoElement {
    sidebar_action_button(
        SvgViewer::new(lucide::folder_plus()),
        "Add Project",
        collapsed,
        move |_| {
            radio.write_channel(AppChannel::Tabs).modal = Some(Modal::AddProject);
        },
    )
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
    label()
        .text(title)
        .width(Size::flex(1.))
        .max_lines(1)
        .text_overflow(TextOverflow::Ellipsis)
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
            Color::from_argb(160, 62, 60, 66)
        } else {
            Color::TRANSPARENT
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
            close_button(tab_id, radio, SvgViewer::new(lucide::x()))
        } else {
            loading_indicator(text_color)
        };

        Button::new()
            .width(Size::fill())
            .height(Size::px(31.))
            .flat()
            .rounded_lg()
            .background(background)
            .hover_background(Color::from_argb(120, 80, 78, 86))
            .color(text_color)
            .on_secondary_down({
                let custom_title = custom_title.clone();
                move |e: Event<PressEventData>| {
                    let custom_title = custom_title.clone();
                    ContextMenu::open_from_event(
                        &e,
                        Menu::new()
                            .child(menu_item("Rename", move || {
                                was_focused.set(false);
                                rename_value.set(custom_title.clone());
                                editing.set(true);
                            }))
                            .child(menu_item("Close", move || {
                                radio
                                    .write_channel(AppChannel::Tabs)
                                    .close_tab_by_id(tab_id);
                            })),
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
                    .on_pointer_over(move |_| hovered.set(true))
                    .on_pointer_out(move |_| hovered.set(false))
                    .child(title_element)
                    .child(trailing)
            })
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.tab_id.0)
    }
}

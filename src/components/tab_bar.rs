use freya::icons::lucide;
use freya::material_design::ButtonRippleExt;
use freya::prelude::*;
use freya::radio::*;

use crate::state::{AppChannel, AppState, TabId};

type AppRadio = Radio<AppState, AppChannel>;

#[derive(PartialEq, Clone)]
pub struct TabBar;

impl Component for TabBar {
    fn render(&self) -> impl IntoElement {
        let mut radio = use_radio(AppChannel::Tabs);

        let (tabs, sidebar_collapsed): (Vec<TabButton>, bool) = {
            let state = radio.read();
            let tabs = state
                .tabs
                .iter()
                .enumerate()
                .map(|(i, t)| TabButton {
                    tab_id: t.id,
                    index: i,
                    title: t.display_title().to_string(),
                    custom_title: t.custom_title.clone().unwrap_or_default(),
                    is_active: i == state.active_tab,
                    outputting: t.outputting,
                    collapsed: state.sidebar_collapsed,
                })
                .collect();
            (tabs, state.sidebar_collapsed)
        };

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
                    .children(
                        tabs.into_iter()
                            .map(|tab| {
                                let drop_tab_id = tab.tab_id;
                                let drag_title = tab.title.clone();
                                DropZone::new(
                                    DragZone::new(tab.tab_id, tab)
                                        .show_while_dragging(false)
                                        .drag_element(
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
                                                .child(
                                                    label()
                                                        .text(drag_title)
                                                        .font_size(14.)
                                                        .color((230, 230, 230)),
                                                ),
                                        ),
                                    move |dragged_id: TabId| {
                                        radio
                                            .write_channel(AppChannel::Tabs)
                                            .move_tab(dragged_id, drop_tab_id);
                                    },
                                )
                                .key(drop_tab_id)
                                .into_element()
                            })
                            .chain(std::iter::once(
                                new_tab_button(radio, sidebar_collapsed).into_element(),
                            )),
                    ),
            )
    }
}

fn new_tab_button(mut radio: AppRadio, collapsed: bool) -> impl IntoElement {
    Button::new()
        .flat()
        .width(Size::fill())
        .rounded_lg()
        .hover_background((45, 45, 45))
        .on_press(move |_| {
            radio.write_channel(AppChannel::Tabs).new_tab();
        })
        .ripple()
        .color((230, 230, 230))
        .child(if collapsed {
            rect().width(Size::fill()).center().child(
                svg(lucide::circle_plus())
                    .width(Size::px(16.))
                    .height(Size::px(16.))
                    .stroke((200, 200, 200)),
            )
        } else {
            rect()
                .width(Size::fill())
                .horizontal()
                .cross_align(Alignment::Center)
                .spacing(4.)
                .child(
                    svg(lucide::circle_plus())
                        .width(Size::px(16.))
                        .height(Size::px(16.))
                        .stroke((200, 200, 200)),
                )
                .child(label().text("New Tab").font_size(14.))
        })
}

fn close_button(tab_id: TabId, mut radio: AppRadio) -> Element {
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
            svg(lucide::x())
                .width(Size::px(14.))
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
        .hover_background(Color::TRANSPARENT)
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
        let input_a11y_id = use_hook(|| Focus::new_id());
        let input_focus_status = use_focus_status(Focus::new_for_id(input_a11y_id));
        let mut was_focused = use_state(|| false);

        if *editing.read() {
            if input_focus_status() != FocusStatus::Not {
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
            close_button(tab_id, radio)
        } else {
            loading_indicator(text_color)
        };

        Button::new()
            .width(Size::fill())
            .height(Size::px(35.))
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
                rect().width(Size::fill()).center().child(if outputting {
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
                    .on_secondary_press({
                        let custom_title = custom_title.clone();
                        move |_| {
                            let custom_title = custom_title.clone();
                            ContextMenu::open(
                                Menu::new().child(
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

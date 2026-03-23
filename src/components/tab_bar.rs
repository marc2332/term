use freya::icons::lucide;
use freya::material_design::ButtonRippleExt;
use freya::prelude::*;
use freya::radio::*;

use crate::state::{AppChannel, TabId};

#[derive(PartialEq, Clone)]
pub struct TabBar;

impl Component for TabBar {
    fn render(&self) -> impl IntoElement {
        let mut radio = use_radio(AppChannel::Tabs);

        let (tabs, sidebar_collapsed): (Vec<(usize, TabId, String, bool, bool)>, bool) = {
            let state = radio.read();
            let tabs = state
                .tabs
                .iter()
                .enumerate()
                .map(|(i, t)| (i, t.id, t.title.clone(), i == state.active_tab, t.outputting))
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
                            .map(|(index, tab_id, title, is_active, outputting)| {
                                TabButton {
                                    tab_id,
                                    index,
                                    title,
                                    is_active,
                                    outputting,
                                    collapsed: sidebar_collapsed,
                                }
                                .into_element()
                            })
                            .chain(std::iter::once(
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
                                    .child(if sidebar_collapsed {
                                        rect()
                                            .width(Size::fill())
                                            .center()
                                            .child(
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
                                    .into_element(),
                            )),
                    ),
            )
    }
}

#[derive(PartialEq, Clone)]
struct TabButton {
    tab_id: TabId,
    index: usize,
    title: String,
    is_active: bool,
    outputting: bool,
    collapsed: bool,
}

impl Component for TabButton {
    fn render(&self) -> impl IntoElement {
        let tab_id = self.tab_id;
        let is_active = self.is_active;
        let outputting = self.outputting;
        let mut radio = use_radio(AppChannel::Tabs);
        let mut hovered = use_state(|| false);

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

        let show_close = *hovered.read() || !outputting;

        Button::new()
            .width(Size::fill())
            .height(Size::px(35.))
            .flat()
            .rounded_lg()
            .background(background)
            .hover_background((45, 45, 45))
            .color(text_color)
            .on_press(move |_: Event<PressEventData>| {
                radio.write_channel(AppChannel::Tabs).switch_to_tab(tab_id);
            })
            .ripple()
            .color((230, 230, 230))
            .child(if self.collapsed {
                rect()
                    .width(Size::fill())
                    .center()
                    .child(if outputting {
                        rect()
                            .width(Size::px(20.))
                            .height(Size::px(20.))
                            .center()
                            .child(CircularLoader::new().size(14.).primary_color(text_color))
                            .into_element()
                    } else {
                        label()
                            .text(format!("{}", self.index + 1))
                            .font_size(14.)
                            .into_element()
                    })
            } else {
                rect()
                    .width(Size::fill())
                    .horizontal()
                    .content(Content::flex())
                    .cross_align(Alignment::Center)
                    .main_align(Alignment::SpaceBetween)
                    .on_pointer_over(move |_| hovered.set(true))
                    .on_pointer_out(move |_| hovered.set(false))
                    .child(
                        OverflowedContent::new()
                            .width(Size::flex(1.))
                            .height(Size::auto())
                            .child(label().text(self.title.clone()).font_size(14.).max_lines(1)),
                    )
                    .child(if show_close {
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
                    } else {
                        rect()
                            .width(Size::px(20.))
                            .height(Size::px(20.))
                            .center()
                            .child(CircularLoader::new().size(14.).primary_color(text_color))
                            .into_element()
                    })
            })
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.tab_id.0)
    }
}

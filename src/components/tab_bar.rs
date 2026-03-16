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

        let tabs: Vec<(TabId, String, bool)> = {
            let state = radio.read();
            state
                .tabs
                .iter()
                .enumerate()
                .map(|(i, t)| (t.id, t.title.clone(), i == state.active_tab))
                .collect()
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
                    .children(tabs.into_iter().map(|(tab_id, title, is_active)| {
                        TabButton {
                            tab_id,
                            title,
                            is_active,
                        }
                        .into_element()
                    })),
            )
            .child(
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
                    .child(
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
                            .child(label().text("New Tab").font_size(14.)),
                    ),
            )
    }
}

#[derive(PartialEq, Clone)]
struct TabButton {
    tab_id: TabId,
    title: String,
    is_active: bool,
}

impl Component for TabButton {
    fn render(&self) -> impl IntoElement {
        let tab_id = self.tab_id;
        let is_active = self.is_active;
        let mut radio = use_radio(AppChannel::Tabs);

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

        Button::new()
            .width(Size::fill())
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
            .child(
                rect()
                    .width(Size::fill())
                    .horizontal()
                    .content(Content::flex())
                    .cross_align(Alignment::Center)
                    .main_align(Alignment::SpaceBetween)
                    .child(
                        OverflowedContent::new()
                            .width(Size::flex(1.))
                            .height(Size::auto())
                            .child(label().text(self.title.clone()).font_size(14.).max_lines(1)),
                    )
                    .child(
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
                            ),
                    ),
            )
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.tab_id.0)
    }
}

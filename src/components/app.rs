use freya::prelude::*;
use freya::radio::*;

use crate::{
    components::{tab_bar::TabBar, tab_content::TabContent},
    state::{AppChannel, AppState, NavDirection, watch_panel},
};

#[derive(PartialEq, Clone)]
pub struct App {
    pub font_size: f32,
    pub shell: String,
}

impl Component for App {
    fn render(&self) -> impl IntoElement {
        let font_size = self.font_size;
        let shell = self.shell.clone();

        use_init_root_theme(|| DARK_THEME);
        use_init_radio_station::<AppState, AppChannel>(move || {
            AppState::new(font_size, shell.clone())
        });

        let mut radio = use_radio(AppChannel::Tabs);

        // Create and watch the initial tab (runs once).
        use_hook(|| {
            let mut state = radio.write_channel(AppChannel::Tabs);
            let (tab_id, panel_id, handle) = state.new_tab();
            let task = watch_panel(radio, tab_id, panel_id, handle);
            state
                .active_tab_mut()
                .unwrap()
                .panels
                .set_task(panel_id, task);
        });

        rect()
            .expanded()
            .background((15, 15, 15))
            .color((220, 220, 220))
            .direction(Direction::Vertical)
            .on_key_down(move |e: Event<KeyboardEventData>| {
                let mods = e.modifiers;
                let ctrl = mods.contains(Modifiers::CONTROL);
                let ctrl_shift = mods.contains(Modifiers::CONTROL | Modifiers::SHIFT);
                let alt = mods.contains(Modifiers::ALT);

                match &e.key {
                    Key::Character(ch) if ctrl_shift && ch.eq_ignore_ascii_case("t") => {
                        let mut state = radio.write_channel(AppChannel::Tabs);
                        let (tab_id, panel_id, handle) = state.new_tab();
                        let task = watch_panel(radio, tab_id, panel_id, handle);
                        state
                            .active_tab_mut()
                            .unwrap()
                            .panels
                            .set_task(panel_id, task);
                    }
                    Key::Character(ch) if ctrl_shift && ch.eq_ignore_ascii_case("w") => {
                        radio.write_channel(AppChannel::Tabs).close_active_tab();
                    }
                    Key::Named(NamedKey::Tab) if ctrl && !mods.contains(Modifiers::SHIFT) => {
                        radio.write_channel(AppChannel::Tabs).next_tab();
                    }
                    Key::Named(NamedKey::Tab) if ctrl_shift => {
                        radio.write_channel(AppChannel::Tabs).prev_tab();
                    }
                    Key::Character(ch) if alt && ch.eq_ignore_ascii_case("p") => {
                        let mut state = radio.write_channel(AppChannel::Tabs);
                        if let Some((panel_id, handle)) = state.split_vertical() {
                            let tab_id = state.active_tab().unwrap().id;
                            let task = watch_panel(radio, tab_id, panel_id, handle);
                            state
                                .active_tab_mut()
                                .unwrap()
                                .panels
                                .set_task(panel_id, task);
                        }
                    }
                    Key::Character(ch) if alt && (ch == "+" || ch == "=") => {
                        let mut state = radio.write_channel(AppChannel::Tabs);
                        if let Some((panel_id, handle)) = state.split_horizontal() {
                            let tab_id = state.active_tab().unwrap().id;
                            let task = watch_panel(radio, tab_id, panel_id, handle);
                            state
                                .active_tab_mut()
                                .unwrap()
                                .panels
                                .set_task(panel_id, task);
                        }
                    }
                    Key::Character(ch) if alt && ch == "-" => {
                        radio.write_channel(AppChannel::Tabs).close_active_panel();
                    }
                    Key::Character(ch) if alt && ch == "1" => {
                        radio
                            .write_channel(AppChannel::Tabs)
                            .close_all_except_active();
                    }
                    Key::Character(ch) if alt && ch.eq_ignore_ascii_case("b") => {
                        radio.write_channel(AppChannel::Tabs).toggle_sidebar();
                    }
                    Key::Named(NamedKey::ArrowLeft) if alt => {
                        radio
                            .write_channel(AppChannel::Tabs)
                            .navigate(NavDirection::Left);
                    }
                    Key::Named(NamedKey::ArrowRight) if alt => {
                        radio
                            .write_channel(AppChannel::Tabs)
                            .navigate(NavDirection::Right);
                    }
                    Key::Named(NamedKey::ArrowUp) if alt => {
                        radio
                            .write_channel(AppChannel::Tabs)
                            .navigate(NavDirection::Up);
                    }
                    Key::Named(NamedKey::ArrowDown) if alt => {
                        radio
                            .write_channel(AppChannel::Tabs)
                            .navigate(NavDirection::Down);
                    }
                    Key::Character(ch) if ctrl && (ch == "+" || ch == "=") => {
                        radio.write_channel(AppChannel::Tabs).increase_font_size();
                    }
                    Key::Character(ch) if ctrl && ch == "-" => {
                        radio.write_channel(AppChannel::Tabs).decrease_font_size();
                    }
                    _ => {}
                }
            })
            .child(if radio.read().sidebar_collapsed {
                rect()
                    .expanded()
                    .horizontal()
                    .child(
                        rect()
                            .width(Size::px(40.))
                            .height(Size::fill())
                            .child(TabBar),
                    )
                    .child(
                        rect()
                            .width(Size::flex(1.))
                            .height(Size::fill())
                            .child(TabContent),
                    )
                    .into_element()
            } else {
                ResizableContainer::new()
                    .direction(Direction::Horizontal)
                    .panel(ResizablePanel::new(PanelSize::px(200.)).child(TabBar))
                    .panel(ResizablePanel::new(PanelSize::percent(100.)).child(TabContent))
                    .into_element()
            })
    }
}

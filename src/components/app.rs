use freya::prelude::*;
use freya::radio::*;
use freya::terminal::{TerminalHandle, TerminalId};

use crate::{
    components::{tab_bar::TabBar, tab_content::TabContent},
    state::{AppChannel, AppState, NavDirection, TabId},
};

enum TitleWatchResult {
    Changed(TabId, String),
    Closed(TerminalId),
}

async fn watch_handle(tab_id: TabId, handle: TerminalHandle) -> TitleWatchResult {
    let closed = futures::future::select(
        Box::pin(async {
            handle.clone().title_changed().await;
            false
        }),
        Box::pin(async {
            handle.clone().closed().await;
            true
        }),
    )
    .await;

    match closed {
        futures::future::Either::Left(_) => {
            TitleWatchResult::Changed(tab_id, handle.title().unwrap_or_default())
        }
        futures::future::Either::Right(_) => TitleWatchResult::Closed(handle.id()),
    }
}

#[derive(PartialEq, Clone)]
pub struct App {
    pub font_size: f32,
    pub shell: String,
}

impl Component for App {
    fn render(&self) -> impl IntoElement {
        let font_size = self.font_size;
        let shell = self.shell.clone();

        use_init_theme(|| DARK_THEME);
        use_init_radio_station::<AppState, AppChannel>(move || {
            AppState::new(font_size, shell.clone())
        });

        let mut radio = use_radio(AppChannel::Tabs);

        use_future(move || async move {
            let mut closed_ids = std::collections::HashSet::<TerminalId>::new();
            loop {
                let watchers = {
                    let state = radio.read();
                    state
                        .tabs
                        .iter()
                        .flat_map(|tab| {
                            let tab_id = tab.id;
                            tab.panels
                                .all_handles()
                                .into_iter()
                                .filter(|h| !closed_ids.contains(&h.id()))
                                .map(move |h| Box::pin(watch_handle(tab_id, h)))
                        })
                        .collect::<Vec<_>>()
                };

                match futures::future::select_all(watchers).await.0 {
                    TitleWatchResult::Changed(tab_id, title) if !title.is_empty() => {
                        if let Some(tab) = radio
                            .write_channel(AppChannel::Tabs)
                            .tabs
                            .iter_mut()
                            .find(|t| t.id == tab_id)
                        {
                            tab.title = title;
                        }
                    }
                    TitleWatchResult::Closed(terminal_id) => {
                        closed_ids.insert(terminal_id);
                    }
                    _ => {}
                }
            }
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
                        radio.write_channel(AppChannel::Tabs).new_tab();
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
                        radio.write_channel(AppChannel::Tabs).split_vertical();
                    }
                    Key::Character(ch) if alt && (ch == "+" || ch == "=") => {
                        radio.write_channel(AppChannel::Tabs).split_horizontal();
                    }
                    Key::Character(ch) if alt && ch == "-" => {
                        radio.write_channel(AppChannel::Tabs).close_active_panel();
                    }
                    Key::Character(ch) if alt && ch == "4" => {
                        radio.write_channel(AppChannel::Tabs).split_into_grid();
                    }
                    Key::Character(ch) if alt && ch == "1" => {
                        radio
                            .write_channel(AppChannel::Tabs)
                            .close_all_except_active();
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
            .child(
                ResizableContainer::new()
                    .direction(Direction::Horizontal)
                    .panel(ResizablePanel::new(15.).child(TabBar))
                    .panel(ResizablePanel::new(85.).child(TabContent)),
            )
    }
}

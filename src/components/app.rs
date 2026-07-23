use std::time::Duration;

use async_io::Timer;
use freya::prelude::*;
use freya::radio::*;

use crate::{
    components::{
        modals::ModalHost,
        tab_bar::TabBar,
        tab_content::TabContent,
        welcome::{Welcome, remove_button},
    },
    config::Startup,
    session,
    state::{
        AppChannel, AppState, Axis, Modal, NavDirection, create_context_tab, refresh_worktrees,
        restore_session, split_active_panel,
    },
};

#[derive(PartialEq, Clone)]
pub struct App {
    pub font_size: f32,
    pub shell: String,
    pub startup: Startup,
}

impl Component for App {
    fn render(&self) -> impl IntoElement {
        let font_size = self.font_size;
        let shell = self.shell.clone();
        let startup = self.startup;

        use_init_theme(dark_theme);
        let station = use_init_radio_station::<AppState, AppChannel>(move || {
            AppState::new(font_size, shell.clone())
        });

        let mut radio = use_radio(AppChannel::Tabs);

        use_hook(move || {
            match startup {
                Startup::Fresh => create_context_tab(station),
                Startup::RestoreLast => {
                    if let Some(saved) = session::load_sessions().first() {
                        restore_session(station, saved);
                    }
                }
                Startup::Welcome => {}
            }

            // Autosave: persist the session whenever its content changed.
            spawn(async move {
                let mut last: Option<session::Session> = None;
                loop {
                    Timer::after(Duration::from_secs(3)).await;
                    let snapshot = session::capture(&station.peek());
                    if snapshot.is_empty() {
                        continue;
                    }
                    if last.as_ref().is_none_or(|l| !l.content_eq(&snapshot)) {
                        session::update_current_session(&snapshot);
                        last = Some(snapshot);
                    }
                }
            });

            // Keep worktree lists and diff stats fresh.
            spawn(async move {
                loop {
                    Timer::after(Duration::from_secs(10)).await;
                    let project_ids: Vec<_> =
                        station.peek().projects.iter().map(|p| p.id).collect();
                    for project_id in project_ids {
                        refresh_worktrees(station, project_id);
                    }
                }
            });
        });

        let (show_welcome, notice) = {
            let state = radio.read();
            (
                state.tabs.is_empty() && state.projects.is_empty(),
                state.notice.clone(),
            )
        };

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
                        create_context_tab(station);
                    }
                    Key::Character(ch) if ctrl_shift && ch.eq_ignore_ascii_case("w") => {
                        radio.write_channel(AppChannel::Tabs).close_active_tab();
                    }
                    Key::Character(ch) if ctrl_shift && ch.eq_ignore_ascii_case("o") => {
                        radio.write_channel(AppChannel::Tabs).modal = Some(Modal::AddProject);
                    }
                    Key::Named(NamedKey::Tab) if ctrl && !mods.contains(Modifiers::SHIFT) => {
                        radio.write_channel(AppChannel::Tabs).next_tab();
                    }
                    Key::Named(NamedKey::Tab) if ctrl_shift => {
                        radio.write_channel(AppChannel::Tabs).prev_tab();
                    }
                    Key::Character(ch) if alt && ch.eq_ignore_ascii_case("p") => {
                        split_active_panel(station, Axis::Vertical);
                    }
                    Key::Character(ch) if alt && (ch == "+" || ch == "=") => {
                        split_active_panel(station, Axis::Horizontal);
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
            .child(ContextMenuViewer::new())
            .child(ModalHost)
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.))
                    .child(if show_welcome {
                        Welcome.into_element()
                    } else if radio.read().sidebar_collapsed {
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
                            .panel(
                                ResizablePanel::new(PanelSize::percent(100.)).child(TabContent),
                            )
                            .into_element()
                    }),
            )
            .map(notice, |el, notice| {
                el.child(
                    rect()
                        .width(Size::fill())
                        .horizontal()
                        .cross_align(Alignment::Center)
                        .spacing(8.)
                        .padding(8.)
                        .background((45, 30, 30))
                        .child(
                            label()
                                .text(notice)
                                .font_size(13.)
                                .color((235, 180, 180))
                                .max_lines(2),
                        )
                        .child(remove_button((200, 200, 200), move |_| {
                            radio.write_channel(AppChannel::Tabs).notice = None;
                        })),
                )
            })
    }
}

use std::time::Duration;

use async_io::Timer;
use freya::prelude::*;
use freya::radio::*;

use crate::{
    components::{
        modals::ModalHost,
        panel::AltHeld,
        resizing::{ResizeBands, use_edge_to_edge},
        tab_bar::TabBar,
        tab_content::TabContent,
        titlebar::Titlebar,
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
    pub font_family: Option<String>,
    pub shell: String,
    pub startup: Startup,
}

impl Component for App {
    fn render(&self) -> impl IntoElement {
        let font_size = self.font_size;
        let font_family = self.font_family.clone();
        let shell = self.shell.clone();
        let startup = self.startup;

        use_init_theme(|| {
            let mut theme = dark_theme();
            for key in ["button", "flat_button"] {
                theme.set(
                    key,
                    ButtonColorsThemePreference {
                        background: Preference::Specific(Color::TRANSPARENT),
                        hover_background: Preference::Specific(Color::from_argb(120, 80, 78, 86)),
                        border_fill: Preference::Specific(Color::TRANSPARENT),
                        focus_border_fill: Preference::Reference("border"),
                        color: Preference::Reference("text_primary"),
                    },
                );
            }
            theme.set(
                "menu_container",
                MenuContainerThemePreference {
                    background: Preference::Specific(Color::from_argb(242, 32, 31, 35)),
                    padding: Preference::Specific(Gaps::new_all(4.)),
                    shadow: Preference::Reference("shadow"),
                    border_fill: Preference::Specific(Color::from_rgb(58, 56, 62)),
                    corner_radius: Preference::Specific(CornerRadius::new_all(10.)),
                },
            );
            theme.set(
                "menu_item",
                MenuItemThemePreference {
                    background: Preference::Specific(Color::TRANSPARENT),
                    hover_background: Preference::Specific(Color::from_argb(120, 80, 78, 86)),
                    select_background: Preference::Specific(Color::from_argb(120, 80, 78, 86)),
                    border_fill: Preference::Specific(Color::TRANSPARENT),
                    select_border_fill: Preference::Reference("border_focus"),
                    corner_radius: Preference::Specific(CornerRadius::new_all(6.)),
                    color: Preference::Specific(Color::from_rgb(225, 225, 225)),
                },
            );
            theme.set(
                "tooltip",
                TooltipThemePreference {
                    background: Preference::Specific(Color::from_rgb(32, 31, 35)),
                    color: Preference::Specific(Color::from_rgb(225, 225, 225)),
                    border_fill: Preference::Specific(Color::from_rgb(58, 56, 62)),
                    font_size: Preference::Specific(13.),
                },
            );
            theme.set(
                "resizable_handle",
                ResizableHandleThemePreference {
                    background: Preference::Specific(Color::TRANSPARENT),
                    hover_background: Preference::Specific(Color::TRANSPARENT),
                    corner_radius: Preference::Specific(CornerRadius::new_all(0.)),
                },
            );
            theme
        });
        let station = use_init_radio_station::<AppState, AppChannel>(move || {
            AppState::new(font_size, font_family.clone(), shell.clone())
        });

        let mut radio = use_radio(AppChannel::Tabs);

        // Alt enables panel dragging.
        let mut alt_held = use_state(|| false);
        use_provide_context(move || AltHeld(alt_held));

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

        let edge_to_edge = use_edge_to_edge();

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
            .corner_radius(CornerRadius::new_all(if edge_to_edge() { 0. } else { 12. }))
            .overflow(Overflow::Clip)
            .direction(Direction::Vertical)
            .on_global_key_down(move |e: Event<KeyboardEventData>| {
                if e.key == Key::Named(NamedKey::Alt) {
                    alt_held.set_if_modified(true);
                }
            })
            .on_global_key_up(move |e: Event<KeyboardEventData>| {
                if e.key == Key::Named(NamedKey::Alt) {
                    alt_held.set_if_modified(false);
                }
            })
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
            .child(app_backdrop())
            .child(ContextMenuViewer::new())
            .child(ModalHost)
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.))
                    .child(if show_welcome {
                        rect()
                            .expanded()
                            .vertical()
                            .padding((4., 4., 0., 4.))
                            .child(Titlebar { compact: false })
                            .child(Welcome)
                            .into_element()
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
                            .panel(
                                ResizablePanel::new(PanelSize::px(240.))
                                    .min_size(138.)
                                    .child(TabBar),
                            )
                            .panel(ResizablePanel::new(PanelSize::percent(100.)).child(TabContent))
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
            .child(ResizeBands { thickness: 6. })
    }
}

/// Soft mesh backdrop made of radial gradient blobs fading to transparent.
fn app_backdrop() -> Rect {
    let blob = |position: Position, size: f32, r: u8, g: u8, b: u8| {
        rect()
            .position(position)
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
    let window = *Platform::get().root_size.read();
    let at = |left: f32, top: f32| Position::new_absolute().left(left).top(top);
    let from_right = |right: f32, top: f32| Position::new_absolute().right(right).top(top);
    let centered = |size: f32, top: f32| at(window.width / 2. - size / 2., top);

    rect()
        .position(at(0., 0.))
        .interactive(false)
        .width(Size::fill())
        .height(Size::fill())
        .child(blob(at(-200., -200.), 560., 30, 46, 64))
        .child(blob(at(-40., 100.), 620., 38, 66, 52))
        .child(blob(at(-220., 380.), 660., 62, 54, 30))
        .child(blob(centered(600., -260.), 600., 44, 38, 72))
        .child(blob(centered(640., window.height - 380.), 640., 28, 58, 62))
        .child(blob(from_right(-200., 120.), 600., 58, 36, 60))
}

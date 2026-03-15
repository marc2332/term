use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use async_io::Timer;
use freya::prelude::*;
use freya::radio::*;
use freya::terminal::{TerminalHandle, TerminalId};

use crate::{
    components::{tab_bar::TabBar, tab_content::TabContent},
    state::{AppChannel, AppState, NavDirection, TabId},
};

enum WatchResult {
    TitleChanged(TabId, AccessibilityId, String),
    Closed,
    OutputReceived(TabId),
}

async fn watch_handle(
    tab_id: TabId,
    panel_id: AccessibilityId,
    handle: TerminalHandle,
) -> WatchResult {
    let h1 = handle.clone();
    let h2 = handle.clone();
    futures::future::select_all([
        Box::pin(async move {
            h1.title_changed().await;
            WatchResult::TitleChanged(tab_id, panel_id, h1.title().unwrap_or_default())
        }) as std::pin::Pin<Box<dyn futures::Future<Output = WatchResult>>>,
        Box::pin(async move {
            h2.closed().await;
            WatchResult::Closed
        }),
        Box::pin(async move {
            handle.output_received().await;
            WatchResult::OutputReceived(tab_id)
        }),
    ])
    .await
    .0
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
        let watched = use_hook(|| Rc::new(RefCell::new(HashSet::<TerminalId>::new())));
        let last_output = use_hook(|| Rc::new(RefCell::new(HashMap::<TabId, Instant>::new())));

        let last_output_sweeper = last_output.clone();

        use_side_effect(move || {
            let state = radio.read();
            for tab in &state.tabs {
                let tab_id = tab.id;
                for (panel_id, handle) in tab.panels.all_panels() {
                    if !watched.borrow().contains(&handle.id()) {
                        watched.borrow_mut().insert(handle.id());
                        let watched = watched.clone();
                        let last_output = last_output.clone();
                        let handle_id = handle.id();
                        spawn(async move {
                            loop {
                                match watch_handle(tab_id, panel_id, handle.clone()).await {
                                    WatchResult::TitleChanged(tab_id, panel_id, title)
                                        if !title.is_empty() =>
                                    {
                                        let mut state = radio.write_channel(AppChannel::Tabs);
                                        if let Some(tab) =
                                            state.tabs.iter_mut().find(|t| t.id == tab_id)
                                        {
                                            if tab.active_panel == panel_id {
                                                tab.title = title;
                                            }
                                        }
                                    }
                                    WatchResult::OutputReceived(tab_id) => {
                                        last_output.borrow_mut().insert(tab_id, Instant::now());
                                        let state = radio.read();
                                        if state
                                            .tabs
                                            .iter()
                                            .any(|t| t.id == tab_id && !t.outputting)
                                        {
                                            drop(state);
                                            if let Some(tab) = radio
                                                .write_channel(AppChannel::Tabs)
                                                .tabs
                                                .iter_mut()
                                                .find(|t| t.id == tab_id)
                                            {
                                                tab.outputting = true;
                                            }
                                        }
                                    }
                                    WatchResult::Closed => {
                                        watched.borrow_mut().remove(&handle_id);
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                        });
                    }
                }
            }
        });

        // Periodically clear stale outputting flags.
        use_future(move || {
            let last_output = last_output_sweeper.clone();
            async move {
                let idle = Duration::from_secs(1);
                loop {
                    Timer::after(idle).await;
                    let now = Instant::now();
                    let lo = last_output.borrow();
                    let is_stale = |tab: &crate::state::Tab| {
                        tab.outputting
                            && lo
                                .get(&tab.id)
                                .map(|ts| now.duration_since(*ts) > idle)
                                .unwrap_or(true)
                    };
                    let state = radio.read();
                    if state.tabs.iter().any(|t| is_stale(t)) {
                        drop(lo);
                        drop(state);
                        let lo = last_output.borrow();
                        let mut state = radio.write_channel(AppChannel::Tabs);
                        for tab in &mut state.tabs {
                            if tab.outputting
                                && lo
                                    .get(&tab.id)
                                    .map(|ts| now.duration_since(*ts) > idle)
                                    .unwrap_or(true)
                            {
                                tab.outputting = false;
                            }
                        }
                    }
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

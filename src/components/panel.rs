use freya::radio::*;
use freya::{prelude::*, terminal::*};

use crate::state::{AppChannel, TabId};

#[derive(PartialEq, Clone)]
pub struct Panel {
    pub panel_id: AccessibilityId,
    pub tab_id: TabId,
    pub handle: TerminalHandle,
    pub font_size: f32,
}

impl Component for Panel {
    fn render(&self) -> impl IntoElement {
        let panel_id = self.panel_id;
        let font_size = self.font_size;
        let tab_id = self.tab_id;
        let handle = self.handle.clone();

        let mut radio = use_radio(AppChannel::Tabs);
        let focus = Focus::new_for_id(self.panel_id);

        let mut dimensions = use_state(|| (0.0, 0.0));

        let is_active = radio
            .read()
            .tabs
            .iter()
            .find(|t| t.id == self.tab_id)
            .unwrap()
            .active_panel
            == panel_id;
        let bg_color: Color = if is_active {
            (10, 10, 10).into()
        } else {
            (15, 15, 15).into()
        };

        rect()
            .expanded()
            .padding(8.)
            .background(bg_color)
            .a11y_id(focus.a11y_id())
            .a11y_auto_focus(is_active)
            .on_key_up({
                let handle = handle.clone();
                move |e: Event<KeyboardEventData>| {
                    if e.key == Key::Named(NamedKey::Shift) {
                        handle.shift_pressed(false);
                    }
                }
            })
            .on_key_down({
                let handle = handle.clone();
                move |e: Event<KeyboardEventData>| {
                    let mods = e.modifiers;
                    let ctrl_shift = mods.contains(Modifiers::CONTROL | Modifiers::SHIFT);
                    let ctrl = mods.contains(Modifiers::CONTROL);
                    let alt = mods.contains(Modifiers::ALT);

                    let is_shortcut = (ctrl_shift && matches!(&e.key, Key::Character(ch) if matches!(ch.to_lowercase().as_str(), "t" | "w")))
                        || (ctrl && matches!(&e.key, Key::Named(NamedKey::Tab)))
                        || (alt && matches!(&e.key, Key::Character(ch) if ch.eq_ignore_ascii_case("p") || ch == "4" || ch == "1"))
                        || (alt && matches!(&e.key, Key::Character(ch) if ch == "+" || ch == "=" || ch == "-"))
                        || (ctrl && matches!(&e.key, Key::Character(ch) if ch == "+" || ch == "=" || ch == "-"))
                        || (alt && matches!(&e.key, Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowRight | NamedKey::ArrowUp | NamedKey::ArrowDown)));

                    if is_shortcut {
                        return;
                    }

                    if matches!(&e.key, Key::Named(NamedKey::Tab)) {
                        e.prevent_default();
                        e.stop_propagation();
                    }

                   match &e.key {
                        Key::Character(ch)
                            if ctrl_shift && ch.eq_ignore_ascii_case("c") =>
                        {
                            if let Some(text) = handle.get_selected_text() {
                                let _ = Clipboard::set(text);
                            }
                        }
                        Key::Character(ch)
                            if ctrl_shift && ch.eq_ignore_ascii_case("v") =>
                        {
                            if let Ok(text) = Clipboard::get() {
                                let _ = handle.paste(&text);
                            }
                        }
                        _ => {
                            let _ = handle.write_key(&e.key, e.modifiers);
                        }
                    }
                }
            })
            .child(
                Terminal::new(handle.clone())
                .background(bg_color)
                    .font_size(font_size)
                    .on_measured(move |(char_width, line_height)| {
                        dimensions.set((char_width, line_height));
                    })
                    .on_mouse_down({
                        let handle = handle.clone();
                        move |e: Event<MouseEventData>| {
                            focus.request_focus();
                            radio.write_channel(AppChannel::Tabs).tabs.iter_mut().find(|t| t.id == tab_id).unwrap().active_panel = panel_id;
                            let (char_width, line_height) = *dimensions.read();
                            let col = (e.element_location.x / char_width as f64).floor() as usize;
                            let row = (e.element_location.y / line_height as f64).floor() as usize;
                            let button = match e.button {
                                Some(MouseButton::Middle) => TerminalMouseButton::Middle,
                                Some(MouseButton::Right) => TerminalMouseButton::Right,
                                _ => TerminalMouseButton::Left,
                            };
                            handle.mouse_down(row, col, button);
                        }
                    })
                    .on_mouse_move({
                        let handle = handle.clone();
                        move |e: Event<MouseEventData>| {
                            let (char_width, line_height) = *dimensions.read();
                            let col = (e.element_location.x / char_width as f64).floor() as usize;
                            let row = (e.element_location.y / line_height as f64).floor() as usize;
                            handle.mouse_move(row, col);
                        }
                    })
                    .on_mouse_up({
                        let handle = handle.clone();
                        move |e: Event<MouseEventData>| {
                            let (char_width, line_height) = *dimensions.read();
                            let col = (e.element_location.x / char_width as f64).floor() as usize;
                            let row = (e.element_location.y / line_height as f64).floor() as usize;
                            let button = match e.button {
                                Some(MouseButton::Middle) => TerminalMouseButton::Middle,
                                Some(MouseButton::Right) => TerminalMouseButton::Right,
                                _ => TerminalMouseButton::Left,
                            };
                            handle.mouse_up(row, col, button);
                        }
                    })
                    .on_wheel({
                        let handle = handle.clone();
                        move |e: Event<WheelEventData>| {
                            let (char_width, line_height) = *dimensions.read();
                            let (mouse_x, mouse_y) = e.element_location.to_tuple();
                            let col = (mouse_x / char_width as f64).floor() as usize;
                            let row = (mouse_y / line_height as f64).floor() as usize;
                            handle.wheel(e.delta_y, row, col);
                        }
                    }),
            )
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.panel_id.0)
    }
}

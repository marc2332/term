use freya::radio::*;
use freya::{prelude::*, terminal::*};

use crate::state::{AppChannel, PanelNode, TabId};

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

        let mut cell_size = use_state(Size2D::zero);
        let mut terminal_area = use_state(Area::zero);
        let mut is_pressed = use_state(|| false);

        let to_cell = move |global: CursorPoint| -> Option<(usize, usize)> {
            let cell = cell_size.read().to_f64();
            if cell.is_empty() {
                return None;
            }
            let area = terminal_area.read().to_f64();
            let local_x =
                (global.x - area.min_x()).clamp(0.0, (area.width() - cell.width).max(0.0));
            let local_y =
                (global.y - area.min_y()).clamp(0.0, (area.height() - cell.height).max(0.0));
            Some((
                (local_y / cell.height) as usize,
                (local_x / cell.width) as usize,
            ))
        };

        let to_button = |button: Option<MouseButton>| match button {
            Some(MouseButton::Middle) => TerminalMouseButton::Middle,
            Some(MouseButton::Right) => TerminalMouseButton::Right,
            _ => TerminalMouseButton::Left,
        };

        let (is_active, has_multiple_panels) = {
            let state = radio.read();
            let tab = state.tabs.iter().find(|t| t.id == self.tab_id).unwrap();
            (
                tab.active_panel == panel_id,
                !matches!(tab.panels, PanelNode::Leaf(..)),
            )
        };

        let bg_color: Color = if is_active {
            (10, 10, 10).into()
        } else {
            (15, 15, 15).into()
        };
        let border = if has_multiple_panels {
            let border_color: Color = if is_active {
                (120, 120, 120).into()
            } else {
                (40, 40, 40).into()
            };
            Some(Border::new().fill(border_color).width(1.0))
        } else {
            None
        };

        rect()
            .expanded()
            .padding(8.)
            .background(bg_color)
            .border(border)
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
                        || (alt && matches!(&e.key, Key::Character(ch) if ch.eq_ignore_ascii_case("p") || ch.eq_ignore_ascii_case("b") || ch == "1"))
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
                    .on_measured(move |(char_width, line_height)| cell_size.set(Size2D::new(char_width, line_height)))
                    .on_sized(move |event: Event<SizedEventData>| terminal_area.set(event.area))
                    .on_mouse_down({
                        let handle = handle.clone();
                        move |event: Event<MouseEventData>| {
                            focus.request_focus();
                            radio.write_channel(AppChannel::Tabs).tabs.iter_mut()
                                .find(|tab| tab.id == tab_id).unwrap().active_panel = panel_id;
                            if let Some((row, col)) = to_cell(event.global_location) {
                                is_pressed.set(true);
                                handle.mouse_down(row, col, to_button(event.button));
                            }
                        }
                    })
                    .on_global_pointer_move({
                        let handle = handle.clone();
                        move |event: Event<PointerEventData>| {
                            let global = event.global_location();
                            if !terminal_area.read().to_f64().contains(global) && !*is_pressed.read() {
                                return;
                            }
                            if let Some((row, col)) = to_cell(global) {
                                handle.mouse_move(row, col);
                            }
                        }
                    })
                    .on_global_pointer_press({
                        let handle = handle.clone();
                        move |event: Event<PointerEventData>| {
                            if !*is_pressed.read() {
                                return;
                            }
                            is_pressed.set(false);
                            match to_cell(event.global_location()) {
                                Some((row, col)) => handle.mouse_up(row, col, to_button(event.button())),
                                None => handle.release(),
                            }
                        }
                    })
                    .on_wheel({
                        let handle = handle.clone();
                        move |event: Event<WheelEventData>| {
                            if let Some((row, col)) = to_cell(event.global_location) {
                                handle.wheel(event.delta_y, row, col);
                            }
                        }
                    }),
            )
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.panel_id.0)
    }
}

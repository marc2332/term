use freya::icons::lucide;
use freya::radio::*;
use freya::{prelude::*, terminal::*};

use crate::components::tab_bar::{drag_preview, menu_item};
use crate::shortcuts::{self, Shortcut};
use crate::state::{AppChannel, AppRadio, AppState, Axis, PanelNode, TabId};

#[derive(PartialEq, Clone)]
pub struct Panel {
    pub panel_id: AccessibilityId,
    pub tab_id: TabId,
    pub handle: TerminalHandle,
    pub font_size: f32,
    pub font_family: Option<String>,
}

/// Whether Alt is held, enabling panel dragging. Provided by the app root.
#[derive(Clone, Copy, PartialEq)]
pub struct AltHeld(pub State<bool>);

/// Payload for alt-dragging a panel onto another to swap them.
#[derive(PartialEq, Clone)]
struct PanelDrag(AccessibilityId);

impl Component for Panel {
    fn render(&self) -> impl IntoElement {
        let panel_id = self.panel_id;
        let font_size = self.font_size;
        let font_family = self.font_family.clone();
        let tab_id = self.tab_id;
        let handle = self.handle.clone();

        let mut radio = use_radio(AppChannel::Tabs);
        let station = use_radio_station::<AppState, AppChannel>();

        let mut cell_size = use_state(Size2D::zero);
        let mut terminal_area = use_state(Area::zero);
        let mut is_pressed = use_state(|| false);
        let mut click_origin = use_state(|| None::<(usize, usize)>);
        let mut drop_hover = use_state(|| false);
        let drags = use_drag::<PanelDrag>();
        let alt_held = *use_consume::<AltHeld>().0.read();
        let drag_active = move || alt_held || drags.read().is_some();

        // Global cursor point to terminal cell, clamped to the terminal area.
        let to_cell = move |global: CursorPoint| -> Option<(f32, f32)> {
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
                (local_y / cell.height) as f32,
                (local_x / cell.width) as f32,
            ))
        };

        let to_button = |button: Option<MouseButton>| match button {
            Some(MouseButton::Middle) => TerminalMouseButton::Middle,
            Some(MouseButton::Right) => TerminalMouseButton::Right,
            _ => TerminalMouseButton::Left,
        };

        let (is_active, has_multiple_panels, shell_exited) = {
            let state = radio.read();
            let tab = state.tab(self.tab_id).unwrap();
            (
                tab.active_panel == panel_id,
                !matches!(tab.panels, PanelNode::Leaf(..)),
                state.exited_panels.contains(&panel_id),
            )
        };
        let drop_hovered = *drop_hover.read();

        let bg_color: Color = if drop_hovered {
            (28, 28, 28).into()
        } else if is_active {
            (10, 10, 10).into()
        } else {
            (15, 15, 15).into()
        };
        let border = if drop_hovered {
            Some(Border::new().fill((160, 160, 160)).width(2.0))
        } else if has_multiple_panels && is_active {
            Some(Border::new().fill((70, 70, 70)).width(2.0))
        } else {
            Some(Border::new().fill(Color::TRANSPARENT).width(2.0))
        };

        let title = self
            .handle
            .title()
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "terminal".into());

        let panel = rect()
            .expanded()
            .layer(Layer::OverlayLevel(1))
            .padding(8.)
            .corner_radius(12.)
            .overflow(Overflow::Clip)
            .shadow((0., 2., 8., 0., Color::from_argb(60, 0, 0, 0)))
            .background(bg_color)
            .border(border)
            .vertical()
            .content(Content::flex())
            .spacing(6.)
            .a11y_id(panel_id)
            .a11y_auto_focus(is_active)
            .on_secondary_down(move |_: Event<PressEventData>| {
                ContextMenu::open_from_down(
                    Menu::new()
                        .child(menu_item(
                            SvgViewer::new(lucide::square_split_vertical()),
                            "Split vertically",
                            move || AppState::split_active_panel(station, Axis::Vertical),
                        ))
                        .child(menu_item(
                            SvgViewer::new(lucide::square_split_horizontal()),
                            "Split horizontally",
                            move || AppState::split_active_panel(station, Axis::Horizontal),
                        ))
                        .child(menu_item(SvgViewer::new(lucide::x()), "Close", move || {
                            radio
                                .write_channel(AppChannel::Tabs)
                                .close_panel(tab_id, panel_id);
                        })),
                )
            })
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
                    match shortcuts::resolve(&e) {
                        Some(Shortcut::Copy) => {
                            if let Some(text) = handle.get_selected_text() {
                                let _ = Clipboard::set(text);
                            }
                        }
                        Some(Shortcut::Paste) => {
                            if let Ok(text) = Clipboard::get() {
                                let _ = handle.paste(&text);
                            }
                        }
                        Some(_) => {}
                        None => {
                            if matches!(&e.key, Key::Named(NamedKey::Tab)) {
                                e.prevent_default();
                                e.stop_propagation();
                            }
                            // Cmd combos never reach the shell on macOS.
                            if !shortcuts::reserved_for_app(e.modifiers) {
                                let _ = handle.write_key(&e.key, e.modifiers);
                            }
                        }
                    }
                }
            })
            .child({
                let terminal = Terminal::new(handle.clone())
                    .background(bg_color)
                    .font_size(font_size);
                let terminal = match font_family {
                    Some(font_family) => terminal.font_family(font_family),
                    None => terminal,
                };
                let terminal = terminal
                    .on_measured(move |(char_width, line_height)| {
                        cell_size.set(Size2D::new(char_width, line_height))
                    })
                    .on_sized(move |event: Event<SizedEventData>| terminal_area.set(event.area))
                    .on_mouse_down({
                        let handle = handle.clone();
                        move |event: Event<MouseEventData>| {
                            radio
                                .write_channel(AppChannel::Tabs)
                                .tabs
                                .iter_mut()
                                .find(|tab| tab.id == tab_id)
                                .unwrap()
                                .activate_panel(panel_id);
                            if drag_active() {
                                return;
                            }
                            if let Some((row, col)) = to_cell(event.global_location) {
                                is_pressed.set(true);
                                click_origin.set(Some((row as usize, col as usize)));
                                let selection_type =
                                    match EventsCombos::pressed(event.element_location) {
                                        PressEventType::Double => SelectionType::Semantic,
                                        PressEventType::Triple => SelectionType::Lines,
                                        _ => SelectionType::Simple,
                                    };
                                handle.mouse_down(
                                    row,
                                    col,
                                    to_button(event.button),
                                    selection_type,
                                );
                            }
                        }
                    })
                    .on_global_pointer_move({
                        let handle = handle.clone();
                        move |event: Event<PointerEventData>| {
                            if drag_active() {
                                return;
                            }
                            let global = event.global_location();
                            if !terminal_area.read().to_f64().contains(global)
                                && !*is_pressed.read()
                            {
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
                            let origin = *click_origin.read();
                            click_origin.set(None);
                            match to_cell(event.global_location()) {
                                Some((row, col)) => {
                                    let button = to_button(event.button());
                                    handle.mouse_up(row, col, button);
                                    if button == TerminalMouseButton::Left
                                        && origin == Some((row as usize, col as usize))
                                        && let Some(url) = handle.hyperlink_at(row, col)
                                    {
                                        let _ = open::that(url);
                                    }
                                }
                                None => handle.release(),
                            }
                        }
                    })
                    .on_wheel({
                        let handle = handle.clone();
                        move |event: Event<WheelEventData>| {
                            if drag_active() {
                                return;
                            }
                            if let Some((row, col)) = to_cell(event.global_location) {
                                handle.wheel(event.delta_y, row, col);
                            }
                        }
                    });
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.))
                    .maybe(shell_exited, |el| {
                        el.child(shell_exited_overlay(radio, tab_id, panel_id))
                    })
                    .child(terminal)
            });

        DragZone::new(PanelDrag(panel_id))
            .child(
                DropZone::new(move |drag: PanelDrag| {
                    radio
                        .write_channel(AppChannel::Tabs)
                        .swap_panels(tab_id, drag.0, panel_id);
                })
                .on_drag_over(move |over: bool| drop_hover.set_if_modified(over))
                .child(panel),
            )
            .drag_threshold(if alt_held { 4. } else { f64::INFINITY })
            .maybe(alt_held, |el| {
                el.drag_element(
                    drag_preview(
                        label()
                            .text(title)
                            .font_size(13.)
                            .color((230, 230, 230))
                            .max_lines(1),
                    )
                    .width(Size::auto())
                    .rounded_full()
                    .padding((6., 14.)),
                )
            })
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.panel_id.0)
    }
}

/// Dim scrim with a centered card, shown when the panel's shell has exited.
fn shell_exited_overlay(mut radio: AppRadio, tab_id: TabId, panel_id: AccessibilityId) -> Rect {
    rect()
        .position(Position::new_absolute().left(0.).top(0.))
        .layer(Layer::Relative(1))
        .expanded()
        .center()
        .corner_radius(6.)
        .background(Color::from_argb(150, 5, 5, 5))
        .child(
            rect()
                .vertical()
                .cross_align(Alignment::Center)
                .spacing(12.)
                .padding((22., 30.))
                .corner_radius(14.)
                .background((24, 24, 24))
                .border(Border::new().fill((70, 70, 70)).width(1.))
                .shadow((0., 4., 18., 0., Color::from_argb(140, 0, 0, 0)))
                .child(
                    SvgViewer::new(lucide::unplug())
                        .width(Size::px(28.))
                        .height(Size::px(28.))
                        .stroke((220, 220, 220)),
                )
                .child(
                    label()
                        .text("The shell exited")
                        .font_size(16.)
                        .color((235, 235, 235)),
                )
                .child(
                    Button::new()
                        .on_press(move |_| {
                            let mut state = radio.write_channel(AppChannel::Tabs);
                            let is_last_panel = state
                                .tabs
                                .iter()
                                .find(|tab| tab.id == tab_id)
                                .is_some_and(|tab| matches!(tab.panels, PanelNode::Leaf(..)));
                            if is_last_panel {
                                state.close_tab_by_id(tab_id);
                            } else {
                                state.close_panel(tab_id, panel_id);
                            }
                        })
                        .color((230, 230, 230))
                        .child(label().text("Close panel").font_size(13.)),
                ),
        )
}

use freya::icons::lucide;
use freya::prelude::*;

/// Translucent pill shared by the window controls and the drag handle.
fn pill_fill(hovering: bool) -> Color {
    if hovering {
        Color::from_argb(56, 255, 255, 255)
    } else {
        Color::from_argb(26, 255, 255, 255)
    }
}

#[derive(PartialEq)]
pub struct Titlebar {
    pub compact: bool,
}

impl Component for Titlebar {
    fn render(&self) -> impl IntoElement {
        let mut maximized = use_state(|| false);
        let window_id = Platform::window_id();

        use_side_effect(move || {
            let _ = Platform::get().root_size.read();
            Platform::get().with_window(window_id, move |window| {
                if let Some(mut maximized) = maximized.try_write() {
                    *maximized = window.is_maximized();
                }
            });
        });

        if self.compact {
            return rect()
                .width(Size::fill())
                .height(Size::px(35.))
                .padding((2., 0., 0., 0.))
                .child(DragHandle { compact: true })
                .into_element();
        }

        let minimize = move |_| {
            Platform::get().with_window(window_id, |window| {
                window.set_minimized(true);
            });
        };

        let toggle_maximize = move |_| {
            Platform::get().with_window(window_id, |window| {
                window.set_maximized(!window.is_maximized());
            });
        };

        let close = move |_| {
            Platform::get().close_window(window_id);
        };

        rect()
            .width(Size::fill())
            .height(Size::px(28.))
            .horizontal()
            .content(Content::flex())
            .cross_align(Alignment::Center)
            .padding(2.)
            .spacing(6.)
            .child(ControlButton {
                icon: lucide::x(),
                icon_size: 14.,
                on_press: close.into(),
            })
            .child(ControlButton {
                icon: lucide::minus(),
                icon_size: 14.,
                on_press: minimize.into(),
            })
            .child(ControlButton {
                icon: if maximized() {
                    lucide::copy()
                } else {
                    lucide::square()
                },
                icon_size: 12.,
                on_press: toggle_maximize.into(),
            })
            .child(DragHandle { compact: false })
            .into_element()
    }
}

#[derive(PartialEq)]
struct ControlButton {
    icon: Bytes,
    icon_size: f32,
    on_press: EventHandler<Event<PressEventData>>,
}

impl Component for ControlButton {
    fn render(&self) -> impl IntoElement {
        let mut hovering = use_state(|| false);
        let on_press = self.on_press.clone();

        rect()
            .width(Size::px(24.))
            .height(Size::px(24.))
            .corner_radius(CornerRadius::new_all(12.))
            .background(pill_fill(hovering()))
            .center()
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .on_press(move |e| on_press.call(e))
            .child(
                SvgViewer::new(self.icon.clone())
                    .width(Size::px(self.icon_size))
                    .height(Size::px(self.icon_size))
                    .stroke((238, 238, 238)),
            )
    }
}

#[derive(PartialEq)]
struct DragHandle {
    compact: bool,
}

impl Component for DragHandle {
    fn render(&self) -> impl IntoElement {
        let mut hovering = use_state(|| false);
        let (height, radius) = if self.compact { (31., 8.) } else { (24., 12.) };

        rect()
            .window_drag()
            .width(Size::flex(1.))
            .height(Size::px(height))
            .corner_radius(CornerRadius::new_all(radius))
            .background(pill_fill(hovering()))
            .center()
            .cursor(CursorIcon::Grab)
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .child(
                SvgViewer::new(lucide::grip_horizontal())
                    .width(Size::px(15.))
                    .height(Size::px(15.))
                    .stroke((190, 190, 190)),
            )
    }
}

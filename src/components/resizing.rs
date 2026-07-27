use freya::prelude::*;
use freya::winit::window::ResizeDirection;

const DIRECTIONS: [ResizeDirection; 8] = [
    ResizeDirection::North,
    ResizeDirection::South,
    ResizeDirection::West,
    ResizeDirection::East,
    ResizeDirection::NorthWest,
    ResizeDirection::NorthEast,
    ResizeDirection::SouthWest,
    ResizeDirection::SouthEast,
];

/// Invisible bands along the window borders that drive a native resize.
///
/// Undecorated windows lose the resize borders the compositor would otherwise
/// draw, so they are recreated here on top of everything else. Skipped while
/// fullscreen, where the borders are not reachable anyway.
#[derive(PartialEq)]
pub struct ResizeBands {
    /// How far into the window each band reaches. Corners use twice this.
    pub thickness: f32,
}

/// Whether the window borders sit flush with the screen, either fullscreen or
/// maximized. Refreshed whenever the window is resized.
pub fn use_edge_to_edge() -> State<bool> {
    let mut edge_to_edge = use_state(|| false);

    use_side_effect(move || {
        let _ = Platform::get().root_size.read();
        Platform::get().with_window(None, move |window| {
            edge_to_edge.set(window.fullscreen().is_some() || window.is_maximized())
        });
    });

    edge_to_edge
}

impl Component for ResizeBands {
    fn render(&self) -> impl IntoElement {
        let edge_to_edge = use_edge_to_edge();

        if edge_to_edge() {
            return rect().into_element();
        }

        let size = *Platform::get().root_size.read();
        let thickness = self.thickness;

        rect()
            .layer(Layer::Overlay)
            .width(Size::px(0.))
            .height(Size::px(0.))
            .children(
                DIRECTIONS
                    .iter()
                    .map(|direction| band(*direction, size, thickness)),
            )
            .into_element()
    }
}

fn band(direction: ResizeDirection, size: Size2D, thickness: f32) -> Element {
    let (left, top, width, height) = geometry(direction, size, thickness);

    rect()
        .position(Position::new_global().top(top).left(left))
        .width(Size::px(width))
        .height(Size::px(height))
        .on_pointer_enter(move |_| Cursor::set(cursor(direction)))
        .on_pointer_leave(move |_| Cursor::set(CursorIcon::Default))
        .on_pointer_down(move |_| {
            Platform::get().with_window(None, move |window| {
                let _ = window.drag_resize_window(direction);
            });
        })
        .into_element()
}

/// Band placement as `(left, top, width, height)`, clamped for tiny windows.
fn geometry(direction: ResizeDirection, size: Size2D, thickness: f32) -> (f32, f32, f32, f32) {
    let corner = thickness * 2.;
    let span_x = (size.width - corner).max(0.);
    let span_y = (size.height - corner).max(0.);
    let far_x = (size.width - thickness).max(0.);
    let far_y = (size.height - thickness).max(0.);

    match direction {
        ResizeDirection::North => (thickness, 0., span_x, thickness),
        ResizeDirection::South => (thickness, far_y, span_x, thickness),
        ResizeDirection::West => (0., thickness, thickness, span_y),
        ResizeDirection::East => (far_x, thickness, thickness, span_y),
        ResizeDirection::NorthWest => (0., 0., corner, corner),
        ResizeDirection::NorthEast => (span_x, 0., corner, corner),
        ResizeDirection::SouthWest => (0., span_y, corner, corner),
        ResizeDirection::SouthEast => (span_x, span_y, corner, corner),
    }
}

fn cursor(direction: ResizeDirection) -> CursorIcon {
    match direction {
        ResizeDirection::North => CursorIcon::NResize,
        ResizeDirection::South => CursorIcon::SResize,
        ResizeDirection::West => CursorIcon::WResize,
        ResizeDirection::East => CursorIcon::EResize,
        ResizeDirection::NorthWest => CursorIcon::NwResize,
        ResizeDirection::NorthEast => CursorIcon::NeResize,
        ResizeDirection::SouthWest => CursorIcon::SwResize,
        ResizeDirection::SouthEast => CursorIcon::SeResize,
    }
}

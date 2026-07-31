use std::time::Duration;

use freya::animation::*;
use freya::prelude::*;
use freya::radio::*;

use crate::{
    components::panel::Panel,
    state::{AppChannel, PanelNode, TabId},
};

#[derive(PartialEq, Clone, Copy)]
pub struct TabContent;

impl Component for TabContent {
    fn render(&self) -> impl IntoElement {
        let radio = use_radio(AppChannel::Tabs);
        let state = radio.read();
        let font_size = state.font_size;
        let font_family = state.font_family.as_deref();

        if let Some(tab) = state.tabs.get(state.active_tab) {
            rect()
                .expanded()
                .background((45, 45, 45))
                .padding(4.)
                .corner_radius(10.)
                .child(render_node(
                    &tab.panels,
                    font_size,
                    font_family,
                    &tab.id,
                    &tab.panels.leaves(),
                ))
                .into_element()
        } else {
            rect().expanded().into_element()
        }
    }
}

fn render_node(
    node: &PanelNode,
    font_size: f32,
    font_family: Option<&str>,
    tab_id: &TabId,
    leaves: &[AccessibilityId],
) -> impl IntoElement {
    match node {
        PanelNode::Leaf(panel_id, handle, _) => Portal::new(handle.id())
            .key(handle.id().0)
            .width(Size::fill())
            .height(Size::fill())
            .function(Function::Expo)
            .ease(Ease::Out)
            .duration(Duration::from_millis(200))
            .animation_dependency(leaves.iter().position(|id| id == panel_id))
            .child(Panel {
                panel_id: *panel_id,
                handle: handle.clone(),
                font_size,
                font_family: font_family.map(str::to_string),
                tab_id: *tab_id,
            })
            .into_element(),
        PanelNode::Horizontal(left, right) => ResizableContainer::new()
            .direction(Direction::Horizontal)
            .panel(
                ResizablePanel::new(PanelSize::percent(50.)).child(render_node(
                    left,
                    font_size,
                    font_family,
                    tab_id,
                    leaves,
                )),
            )
            .panel(
                ResizablePanel::new(PanelSize::percent(50.)).child(render_node(
                    right,
                    font_size,
                    font_family,
                    tab_id,
                    leaves,
                )),
            )
            .into_element(),
        PanelNode::Vertical(top, bottom) => ResizableContainer::new()
            .direction(Direction::Vertical)
            .panel(
                ResizablePanel::new(PanelSize::percent(50.)).child(render_node(
                    top,
                    font_size,
                    font_family,
                    tab_id,
                    leaves,
                )),
            )
            .panel(
                ResizablePanel::new(PanelSize::percent(50.)).child(render_node(
                    bottom,
                    font_size,
                    font_family,
                    tab_id,
                    leaves,
                )),
            )
            .into_element(),
    }
}

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

        if let Some(tab) = state.tabs.get(state.active_tab) {
            render_node(&tab.panels, font_size, &tab.id).into_element()
        } else {
            rect().expanded().into_element()
        }
    }
}

fn render_node(node: &PanelNode, font_size: f32, tab_id: &TabId) -> impl IntoElement {
    match node {
        PanelNode::Leaf(panel_id, handle, _) => Panel {
            panel_id: *panel_id,
            handle: handle.clone(),
            font_size,
            tab_id: *tab_id,
        }
        .into_element(),
        PanelNode::Horizontal(left, right) => ResizableContainer::new()
            .direction(Direction::Horizontal)
            .panel(
                ResizablePanel::new(PanelSize::percent(50.))
                    .child(render_node(left, font_size, tab_id)),
            )
            .panel(
                ResizablePanel::new(PanelSize::percent(50.))
                    .child(render_node(right, font_size, tab_id)),
            )
            .into_element(),
        PanelNode::Vertical(top, bottom) => ResizableContainer::new()
            .direction(Direction::Vertical)
            .panel(
                ResizablePanel::new(PanelSize::percent(50.))
                    .child(render_node(top, font_size, tab_id)),
            )
            .panel(
                ResizablePanel::new(PanelSize::percent(50.))
                    .child(render_node(bottom, font_size, tab_id)),
            )
            .into_element(),
    }
}

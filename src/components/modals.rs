use std::path::PathBuf;

use freya::prelude::*;
use freya::radio::*;

use crate::git;
use crate::state::{AppChannel, AppState, Modal, open_project};

type AppRadio = Radio<AppState, AppChannel>;

#[derive(PartialEq, Clone, Copy)]
pub struct ModalHost;

impl Component for ModalHost {
    fn render(&self) -> impl IntoElement {
        let radio = use_radio(AppChannel::Tabs);
        match radio.read().modal.clone() {
            None => rect().into_element(),
            Some(Modal::AddProject) => AddProjectModal.into_element(),
        }
    }
}

fn close_modal(mut radio: AppRadio) {
    let mut state = radio.write_channel(AppChannel::Tabs);
    state.modal = None;
    state.focus_active_panel();
}

fn error_label(error: &str) -> Element {
    label()
        .text(error.to_string())
        .font_size(13.)
        .color((235, 100, 100))
        .max_lines(3)
        .into_element()
}

fn expand_home(path: &str) -> PathBuf {
    if path == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));
    }
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    PathBuf::from(path)
}

#[derive(PartialEq, Clone, Copy)]
struct AddProjectModal;

impl Component for AddProjectModal {
    fn render(&self) -> impl IntoElement {
        let radio = use_radio(AppChannel::Tabs);
        let station = use_radio_station::<AppState, AppChannel>();
        let path = use_state(String::new);
        let mut error = use_state(|| None::<String>);

        let submit = move |value: String| {
            let value = value.trim().to_string();
            if value.is_empty() {
                return;
            }
            spawn(async move {
                match git::run_async(move || git::detect_project(&expand_home(&value))).await {
                    Ok(info) => {
                        close_modal(radio);
                        open_project(station, info);
                    }
                    Err(e) => error.set(Some(e)),
                }
            });
        };

        Popup::new()
            .on_close_request(move |_| close_modal(radio))
            .child(PopupTitle::new("Add Project".to_string()))
            .child(
                PopupContent::new()
                    .child(
                        rect()
                            .width(Size::fill())
                            .vertical()
                            .spacing(12.)
                            .child(
                                label()
                                    .text("Path to a git repository (any of its worktrees works).")
                                    .font_size(13.)
                                    .color((150, 150, 150)),
                            )
                            .child(
                                Input::new(path)
                                    .flat()
                                    .layout_variant(InputLayoutVariant::Expanded)
                                    .width(Size::fill())
                                    .placeholder("~/Projects/myproject")
                                    .auto_focus(true)
                                    .on_submit(submit),
                            ),
                    )
                    .maybe_child(error.read().as_deref().map(error_label)),
            )
    }
}

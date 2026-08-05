use std::path::PathBuf;

use freya::icons::lucide;
use freya::prelude::*;
use freya::radio::*;

use crate::config::Config;
use crate::git;
use crate::state::{AppChannel, AppState, Modal, ProjectId};

type AppRadio = Radio<AppState, AppChannel>;

#[derive(PartialEq, Clone, Copy)]
pub struct ModalHost;

impl Component for ModalHost {
    fn render(&self) -> impl IntoElement {
        let mut radio = use_radio(AppChannel::Tabs);
        let modal = radio.read().modal.clone();
        let project_name = |id: ProjectId| {
            radio
                .read()
                .project(id)
                .map(|p| p.name.clone())
                .unwrap_or_default()
        };
        match modal {
            None => rect().into_element(),
            Some(Modal::About) => AboutModal.into_element(),
            Some(Modal::AddProject) => AddProjectModal.into_element(),
            Some(Modal::ConfirmArchiveAll(id)) => ConfirmModal {
                title: "Archive all worktrees",
                message: format!(
                    "Archive all worktrees in {}? Their open tabs will be closed.",
                    project_name(id)
                ),
                confirm: "Archive",
                on_confirm: (move |()| {
                    radio
                        .write_channel(AppChannel::Tabs)
                        .archive_all_worktrees(id);
                })
                .into(),
            }
            .into_element(),
            Some(Modal::ConfirmUnarchiveAll(id)) => ConfirmModal {
                title: "Unarchive all worktrees",
                message: format!("Unarchive all archived worktrees in {}?", project_name(id)),
                confirm: "Unarchive",
                on_confirm: (move |()| {
                    radio
                        .write_channel(AppChannel::Tabs)
                        .set_archived(id, vec![]);
                })
                .into(),
            }
            .into_element(),
            Some(Modal::ConfirmCloseProject(id)) => ConfirmModal {
                title: "Close project",
                message: format!(
                    "Close {} and all of its tabs? Nothing on disk is affected.",
                    project_name(id)
                ),
                confirm: "Close",
                on_confirm: (move |()| {
                    radio.write_channel(AppChannel::Tabs).remove_project(id);
                })
                .into(),
            }
            .into_element(),
        }
    }
}

#[derive(PartialEq, Clone, Copy)]
struct AboutModal;

impl Component for AboutModal {
    fn render(&self) -> impl IntoElement {
        let radio = use_radio(AppChannel::Tabs);
        let mut copied = use_state(|| false);
        Popup::new()
            .width(Size::px(300.))
            .on_close_request(move |_| close_modal(radio))
            .child(
                PopupContent::new().child(
                    rect()
                        .width(Size::fill())
                        .vertical()
                        .cross_align(Alignment::Center)
                        .spacing(8.)
                        .padding((16., 0., 16., 0.))
                        .child(
                            ImageViewer::new(("marcterm-icon", include_bytes!("../../icon.png")))
                                .width(Size::px(128.))
                                .height(Size::px(128.)),
                        )
                        .child(label().text(env!("CARGO_PKG_NAME")).font_size(18.))
                        .child(
                            label()
                                .text(format!("Version {}", env!("CARGO_PKG_VERSION")))
                                .font_size(13.)
                                .color((150, 150, 150)),
                        )
                        .child(
                            rect()
                                .horizontal()
                                .spacing(16.)
                                .child(link("Website", "https://term.mespin.me"))
                                .child(link("Source code", "https://github.com/marc2332/term")),
                        )
                        .child(
                            Button::new()
                                .rounded_full()
                                .on_press(move |_| {
                                    if let Ok(path) = Config::ensure_path() {
                                        let _ = Clipboard::set(path.display().to_string());
                                        copied.set(true);
                                    }
                                })
                                .child(
                                    label()
                                        .text(if *copied.read() {
                                            "Copied!"
                                        } else {
                                            "Copy config path"
                                        })
                                        .font_size(13.),
                                ),
                        ),
                ),
            )
            .child(
                PopupButtons::new().child(
                    Button::new()
                        .expanded()
                        .filled()
                        .rounded_full()
                        .on_press(move |_| close_modal(radio))
                        .child("Accept"),
                ),
            )
    }
}

fn link(text: &'static str, url: &'static str) -> Link {
    Link::new(url).child(label().text(text).font_size(13.).color((100, 145, 235)))
}

#[derive(PartialEq, Clone)]
struct ConfirmModal {
    title: &'static str,
    message: String,
    confirm: &'static str,
    on_confirm: EventHandler<()>,
}

impl ComponentOwned for ConfirmModal {
    fn render(self) -> impl IntoElement {
        let radio = use_radio(AppChannel::Tabs);
        let on_confirm = self.on_confirm;
        Popup::new()
            .on_close_request(move |_| close_modal(radio))
            .child(PopupTitle::new(self.title.to_string()))
            .child(
                PopupContent::new().child(
                    label()
                        .text(self.message)
                        .font_size(13.)
                        .color((150, 150, 150))
                        .max_lines(3),
                ),
            )
            .child(
                PopupButtons::new()
                    .child(
                        Button::new()
                            .expanded()
                            .rounded_full()
                            .on_press(move |_| close_modal(radio))
                            .child("Cancel"),
                    )
                    .child(
                        Button::new()
                            .expanded()
                            .filled()
                            .rounded_full()
                            .on_press(move |_| {
                                on_confirm.call(());
                                close_modal(radio);
                            })
                            .child(self.confirm),
                    ),
            )
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
        let mut path = use_state(String::new);
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
                        AppState::open_project(station, info);
                    }
                    Err(e) => error.set(Some(e)),
                }
            });
        };

        let pick_folder = move |_| {
            spawn(async move {
                if let Some(folder) = rfd::AsyncFileDialog::new().pick_folder().await {
                    path.set(folder.path().display().to_string());
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
                                rect()
                                    .width(Size::fill())
                                    .horizontal()
                                    .content(Content::flex())
                                    .cross_align(Alignment::Center)
                                    .spacing(8.)
                                    .child(
                                        Input::new(path)
                                            .flat()
                                            .layout_variant(InputLayoutVariant::Expanded)
                                            .width(Size::flex(1.))
                                            .placeholder("~/Projects/myproject")
                                            .auto_focus(true)
                                            .on_submit(submit),
                                    )
                                    .child(
                                        Button::new()
                                            .flat()
                                            .rounded_full()
                                            .on_press(pick_folder)
                                            .child(
                                                SvgViewer::new(lucide::folder_open())
                                                    .width(Size::px(15.))
                                                    .height(Size::px(15.))
                                                    .stroke((200, 200, 200)),
                                            ),
                                    ),
                            ),
                    )
                    .maybe_child(error.read().as_deref().map(error_label)),
            )
    }
}

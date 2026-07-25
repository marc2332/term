use freya::icons::lucide;
use freya::material_design::ButtonRippleExt;
use freya::prelude::*;
use freya::radio::*;

use std::path::PathBuf;

use crate::git;
use crate::session::{self, Session};
use crate::state::{AppChannel, AppState, Modal, create_tab, open_project, restore_session};

#[derive(PartialEq, Clone, Copy)]
pub struct Welcome;

impl Component for Welcome {
    fn render(&self) -> impl IntoElement {
        let mut radio = use_radio(AppChannel::Tabs);
        let station = use_radio_station::<AppState, AppChannel>();
        let recent = use_state(|| {
            session::load_recent_projects()
                .into_iter()
                .map(|p| {
                    let exists = git::project_exists(&p.root);
                    (p.root, exists)
                })
                .collect::<Vec<(PathBuf, bool)>>()
        });
        let sessions = use_state(session::load_sessions);

        rect()
            .expanded()
            .center()
            .child(
                rect()
                    .width(Size::px(520.))
                    .vertical()
                    .spacing(20.)
                    .child(
                        label()
                            .text("marcterm")
                            .font_size(26.)
                            .color((230, 230, 230)),
                    )
                    .child(
                        rect()
                            .width(Size::fill())
                            .horizontal()
                            .spacing(8.)
                            .child(welcome_button(
                                SvgViewer::new(lucide::folder_plus()),
                                "Open Project",
                                false,
                                move |_| {
                                    radio.write_channel(AppChannel::Tabs).modal =
                                        Some(Modal::AddProject);
                                },
                            ))
                            .child(welcome_button(
                                SvgViewer::new(lucide::terminal()),
                                "New Terminal",
                                true,
                                move |_| create_tab(station, None, None, None),
                            )),
                    )
                    .maybe(!recent.read().is_empty(), |el| {
                        el.child(section_title("Recent projects")).child(
                            rect().width(Size::fill()).vertical().spacing(2.).children(
                                recent
                                    .read()
                                    .iter()
                                    .map(|(root, exists)| RecentProjectRow {
                                        root: root.clone(),
                                        exists: *exists,
                                        recent,
                                    })
                                    .map(IntoElement::into_element),
                            ),
                        )
                    })
                    .maybe(!sessions.read().is_empty(), |el| {
                        el.child(section_title("Recent sessions")).child(
                            rect().width(Size::fill()).vertical().spacing(2.).children(
                                sessions
                                    .read()
                                    .iter()
                                    .map(|s| SessionRow {
                                        session: s.clone(),
                                        sessions,
                                    })
                                    .map(IntoElement::into_element),
                            ),
                        )
                    }),
            )
    }
}

fn welcome_button(
    icon: SvgViewer,
    text: &'static str,
    flat: bool,
    on_press: impl FnMut(Event<PressEventData>) + 'static,
) -> impl IntoElement {
    Button::new()
        .maybe(flat, |el| el.flat())
        .on_press(on_press)
        .ripple()
        .color((230, 230, 230))
        .child(
            rect()
                .horizontal()
                .spacing(6.)
                .cross_align(Alignment::Center)
                .child(
                    icon.width(Size::px(16.))
                        .height(Size::px(16.))
                        .stroke((220, 220, 220)),
                )
                .child(label().text(text).font_size(14.)),
        )
}

fn section_title(text: &'static str) -> Element {
    label()
        .text(text)
        .font_size(12.)
        .color((120, 120, 120))
        .into_element()
}

pub fn remove_button(
    stroke: impl Into<Color>,
    on_press: impl FnMut(Event<PressEventData>) + 'static,
) -> Element {
    let stroke = stroke.into();
    Button::new()
        .flat()
        .width(Size::px(22.))
        .height(Size::px(22.))
        .compact()
        .rounded_full()
        .on_press(on_press)
        .child(
            SvgViewer::new(lucide::x())
                .width(Size::px(14.))
                .height(Size::px(14.))
                .stroke(stroke),
        )
        .into_element()
}

#[derive(PartialEq, Clone)]
struct RecentProjectRow {
    root: PathBuf,
    exists: bool,
    recent: State<Vec<(PathBuf, bool)>>,
}

impl Component for RecentProjectRow {
    fn render(&self) -> impl IntoElement {
        let mut station = use_radio_station::<AppState, AppChannel>();
        let root = self.root.clone();
        let mut recent = self.recent;
        let exists = self.exists;
        let name = git::dir_name(&root);

        let open = {
            let root = root.clone();
            move |_: Event<PressEventData>| {
                let root = root.clone();
                spawn(async move {
                    match git::run_async(move || git::detect_project(&root)).await {
                        Ok(info) => open_project(station, info),
                        Err(e) => station.write_channel(AppChannel::Tabs).notice = Some(e),
                    }
                });
            }
        };

        let remove = {
            let root = root.clone();
            move |e: Event<PressEventData>| {
                e.stop_propagation();
                session::remove_recent_project(&root);
                recent.write().retain(|(r, _)| r != &root);
            }
        };

        Button::new()
            .flat()
            .width(Size::fill())
            .rounded_lg()
            .hover_background((35, 35, 35))
            .on_press(open)
            .color((200, 200, 200))
            .child(
                rect()
                    .width(Size::fill())
                    .horizontal()
                    .spacing(8.)
                    .content(Content::flex())
                    .cross_align(Alignment::Center)
                    .child(
                        SvgViewer::new(lucide::folder_git_2())
                            .width(Size::px(14.))
                            .height(Size::px(14.))
                            .stroke(if exists {
                                Color::from((150, 150, 150))
                            } else {
                                Color::from((90, 90, 90))
                            }),
                    )
                    .child(
                        label()
                            .text(if exists {
                                name
                            } else {
                                format!("{name} (missing)")
                            })
                            .font_size(14.)
                            .color(if exists {
                                Color::from((220, 220, 220))
                            } else {
                                Color::from((120, 120, 120))
                            }),
                    )
                    .child(
                        OverflowedContent::new()
                            .width(Size::flex(1.))
                            .height(Size::auto())
                            .child(
                                label()
                                    .text(self.root.display().to_string())
                                    .font_size(12.)
                                    .color((120, 120, 120))
                                    .max_lines(1),
                            ),
                    )
                    .child(remove_button((150, 150, 150), remove)),
            )
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.root)
    }
}

#[derive(PartialEq, Clone)]
struct SessionRow {
    session: Session,
    sessions: State<Vec<Session>>,
}

impl Component for SessionRow {
    fn render(&self) -> impl IntoElement {
        let station = use_radio_station::<AppState, AppChannel>();
        let mut sessions = self.sessions;
        let started_at = self.session.started_at;

        let restore = {
            let session_data = self.session.clone();
            move |_: Event<PressEventData>| {
                restore_session(station, &session_data);
            }
        };

        let remove = move |e: Event<PressEventData>| {
            e.stop_propagation();
            session::remove_session(started_at);
            sessions.write().retain(|s| s.started_at != started_at);
        };

        Button::new()
            .flat()
            .width(Size::fill())
            .rounded_lg()
            .hover_background((35, 35, 35))
            .on_press(restore)
            .color((200, 200, 200))
            .child(
                rect()
                    .width(Size::fill())
                    .horizontal()
                    .spacing(8.)
                    .content(Content::flex())
                    .cross_align(Alignment::Center)
                    .child(
                        SvgViewer::new(lucide::history())
                            .width(Size::px(14.))
                            .height(Size::px(14.))
                            .stroke((150, 150, 150)),
                    )
                    .child(
                        OverflowedContent::new()
                            .width(Size::flex(1.))
                            .height(Size::auto())
                            .child(
                                label()
                                    .text(self.session.summary())
                                    .font_size(14.)
                                    .max_lines(1),
                            ),
                    )
                    .child(
                        label()
                            .text(session::time_ago(self.session.saved_at))
                            .font_size(12.)
                            .color((120, 120, 120)),
                    )
                    .child(remove_button((150, 150, 150), remove)),
            )
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.session.started_at)
    }
}

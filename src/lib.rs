// SPDX-License-Identifier: GPL-3.0-only

mod downloads;
mod icons;
mod localize;

use crate::downloads::Entry;
use cosmic::{
    Element, Task, app,
    applet::{
        cosmic_panel_config::PanelAnchor,
        menu_button,
        token::subscription::{TokenRequest, TokenUpdate, activation_token_subscription},
    },
    cctk::sctk::reexports::calloop,
    iced::{
        self, Alignment, Background, Border, Length, Limits, Padding, Subscription,
        widget::{column, row},
        window,
    },
    surface, theme,
    widget::{self, icon, text},
};
use cosmic::iced::core::text::{Ellipsize, EllipsizeHeightLimit, Wrapping};
use std::{borrow::Cow, path::PathBuf};

/// How many recent items the stack shows.
const MAX_ITEMS: usize = 10;
/// Widest a label may get (px). Longer names are shortened in the middle, so the extension
/// stays visible.
const LABEL_MAX_WIDTH: f32 = 400.0;
const ICON_SIZE: u16 = 32;
/// How far (px) the fan bends sideways at its far end.
const ARC_DEPTH: f32 = 40.0;
/// The popup fits the fan up to this width. A full-width label, its icon and the arc need ~540.
const POPUP_MAX_WIDTH: f32 = 600.0;

// The fan is drawn tighter than a regular applet menu. COSMIC's spacing tokens are menu-sized
// (and grow with the interface density setting), so the fan keeps its own measurements.
/// Space around the whole fan.
const FAN_PADDING: u16 = 8;
/// Between rows.
const ROW_SPACING: u16 = 2;
/// Above and below each row.
const ROW_PADDING: f32 = 2.0;
/// Left and right of each row, before the arc is added.
const ROW_EDGE: f32 = 8.0;
/// Between the label and its icon.
const LABEL_GAP: u16 = 10;
/// Inside the label pill: vertical, horizontal.
const PILL_PADDING: [u16; 2] = [4, 10];

pub fn run() -> cosmic::iced::Result {
    icons::add_nixos_profile_dirs();
    localize::localize();

    cosmic::applet::run::<DownloadsStack>(())
}

struct DownloadsStack {
    core: cosmic::app::Core,
    popup: Option<window::Id>,
    dir: Option<PathBuf>,
    /// Newest first, with the icon resolved once per scan rather than on every redraw.
    entries: Vec<(Entry, icon::Handle)>,
    loaded: bool,
    token_tx: Option<calloop::channel::Sender<TokenRequest>>,
    /// Waiting for an activation token before it's opened.
    pending_open: Option<PathBuf>,
}

#[derive(Debug, Clone)]
enum Message {
    TogglePopup,
    Closed(window::Id),
    Loaded(Option<PathBuf>, Vec<Entry>),
    Open(PathBuf),
    OpenFolder,
    Token(TokenUpdate),
    Surface(surface::Action<Message>),
}

impl cosmic::Application for DownloadsStack {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = "io.github.KemonoElite.CosmicAppletDownloadsStack";

    fn core(&self) -> &cosmic::app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::app::Core {
        &mut self.core
    }

    fn init(core: cosmic::app::Core, _flags: ()) -> (Self, app::Task<Self::Message>) {
        (
            Self {
                core,
                popup: None,
                dir: downloads::dir(),
                entries: Vec::new(),
                loaded: false,
                token_tx: None,
                pending_open: None,
            },
            Task::none(),
        )
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::Closed(id))
    }

    fn update(&mut self, message: Self::Message) -> app::Task<Self::Message> {
        match message {
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    return cosmic::surface::surface_task(cosmic::surface::action::destroy_popup(
                        p,
                    ));
                }
                // Scan the folder every time the stack opens, so it's never stale.
                return Task::batch([
                    cosmic::surface::surface_task(cosmic::surface::action::app_popup(
                        |_| Default::default(),
                        |app: &mut DownloadsStack| {
                            let new_id = window::Id::unique();
                            app.popup.replace(new_id);

                            let mut popup_settings = app.core.applet.get_popup_settings(
                                app.core.main_window_id().unwrap(),
                                new_id,
                                None,
                                None,
                                None,
                            );
                            popup_settings.positioner.size_limits = Limits::NONE
                                .max_width(POPUP_MAX_WIDTH)
                                .min_width(1.)
                                .max_height(1080.)
                                .min_height(1.);
                            popup_settings
                        },
                        None,
                    )),
                    scan(),
                ]);
            }
            Message::Closed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                }
            }
            Message::Loaded(dir, entries) => {
                self.dir = dir;
                self.entries = entries
                    .into_iter()
                    .map(|entry| {
                        let handle = file_icon(&entry.icon_names);
                        (entry, handle)
                    })
                    .collect();
                self.loaded = true;
            }
            Message::Open(path) => {
                self.request_open(path);
                return self.close_popup();
            }
            Message::OpenFolder => {
                if let Some(dir) = self.dir.clone() {
                    self.request_open(dir);
                }
                return self.close_popup();
            }
            Message::Token(u) => match u {
                TokenUpdate::Init(tx) => {
                    self.token_tx = Some(tx);
                }
                TokenUpdate::Finished => {
                    self.token_tx = None;
                }
                TokenUpdate::ActivationToken { token, .. } => {
                    if let Some(path) = self.pending_open.take() {
                        open(path, token);
                    }
                }
            },
            Message::Surface(a) => {
                return cosmic::task::message(cosmic::Action::Surface(a));
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let button = self
            .core
            .applet
            .icon_button("folder-download-symbolic")
            .on_press_down(Message::TogglePopup);

        self.core
            .applet
            .applet_tooltip(
                button,
                fl!("downloads"),
                self.popup.is_some(),
                Message::Surface,
                None,
            )
            .into()
    }

    fn view_window(&self, id: window::Id) -> Element<'_, Message> {
        if !matches!(self.popup, Some(p) if p == id) {
            return widget::text("").into();
        }

        let content = if self.dir.is_none() {
            column![message_row(fl!("no-downloads-folder"))]
        } else {
            // Built nearest-to-the-panel first, so `d` is the distance along the fan.
            let mut rows: Vec<Element<'_, Message>> = self
                .entries
                .iter()
                .enumerate()
                .map(|(d, (entry, handle))| {
                    fan_row(
                        entry.name.clone(),
                        handle.clone(),
                        d,
                        Message::Open(entry.path.clone()),
                    )
                })
                .collect();

            if self.loaded && self.entries.is_empty() {
                rows.push(message_row(fl!("no-downloads")));
            }

            rows.push(fan_row(
                fl!("open-in-files"),
                icon::from_name("folder-open-symbolic").size(ICON_SIZE).handle(),
                self.entries.len(),
                Message::OpenFolder,
            ));

            // With a bottom panel the newest file sits at the bottom, like the macOS Dock.
            if matches!(self.core.applet.anchor, PanelAnchor::Bottom) {
                rows.reverse();
            }

            iced::widget::Column::with_children(rows)
        };

        let content = content
            .spacing(ROW_SPACING)
            // Rows are only as wide as their content and line up on the right, so a long name
            // grows the fan to the left and the icons stay put.
            .align_x(Alignment::End)
            .padding(FAN_PADDING);

        // popup_container is a fixed 360 px wide; let it fit the fan instead.
        self.core
            .applet
            .popup_container(content)
            .limits(
                Limits::NONE
                    .min_width(1.)
                    .max_width(POPUP_MAX_WIDTH)
                    .min_height(1.)
                    .max_height(1000.),
            )
            .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        activation_token_subscription(0).map(Message::Token)
    }

    fn style(&self) -> Option<iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}

impl DownloadsStack {
    fn close_popup(&mut self) -> app::Task<Message> {
        match self.popup.take() {
            Some(p) => cosmic::surface::surface_task(cosmic::surface::action::destroy_popup(p)),
            None => Task::none(),
        }
    }

    /// Opens `path` with its default app, asking the compositor for an activation token first
    /// so the app gets focus when it appears.
    fn request_open(&mut self, path: PathBuf) {
        let Some(tx) = self.token_tx.as_ref() else {
            open(path, None);
            return;
        };

        self.pending_open = Some(path);
        if tx
            .send(TokenRequest {
                app_id: <Self as cosmic::Application>::APP_ID.to_string(),
                exec: "xdg-open".to_string(),
            })
            .is_err()
        {
            tracing::error!("activation token channel closed");
            if let Some(path) = self.pending_open.take() {
                open(path, None);
            }
        }
    }
}

/// Reads the Downloads folder on a blocking thread. The folder is looked up again each time, so
/// one created (or moved) after login is picked up.
fn scan() -> app::Task<Message> {
    cosmic::task::future(async {
        let (dir, entries) = tokio::task::spawn_blocking(|| {
            let dir = downloads::dir();
            let entries = dir
                .as_deref()
                .map(|dir| downloads::recent(dir, MAX_ITEMS))
                .unwrap_or_default();
            (dir, entries)
        })
        .await
        .unwrap_or_default();
        Message::Loaded(dir, entries)
    })
}

fn open(path: PathBuf, token: Option<String>) {
    let mut cmd = std::process::Command::new("xdg-open");
    cmd.arg(path);
    if let Some(token) = token {
        cmd.env("XDG_ACTIVATION_TOKEN", &token);
        cmd.env("DESKTOP_STARTUP_ID", &token);
    }
    tokio::spawn(cosmic_launch::process::spawn(cmd));
}

/// The first of `names` the icon theme has. The handle keeps the names, so it follows icon theme
/// changes while the applet runs.
///
/// If the theme has none of them, a symbolic generic icon is used instead of an empty image.
fn file_icon(names: &[String]) -> icon::Handle {
    let named = || {
        let (first, rest) = names.split_first()?;
        Some(
            icon::from_name(first.clone())
                .fallback(Some(icon::IconFallback::Names(
                    rest.iter().cloned().map(Cow::Owned).collect(),
                )))
                .size(ICON_SIZE),
        )
    };

    match named() {
        Some(found) if found.clone().path().is_some() => found.handle(),
        _ => icon::from_name("text-x-generic-symbolic")
            .size(ICON_SIZE)
            .handle(),
    }
}

/// One row of the fan: the label on a pill, with its icon to the right.
/// `d` is the distance from the panel and drives the sideways arc.
fn fan_row<'a>(label: String, handle: icon::Handle, d: usize, msg: Message) -> Element<'a, Message> {
    let t = (d as f32 / MAX_ITEMS as f32).min(1.0);

    // One line; a name too long for the label is shortened in the middle to fit.
    let label = text::body(label)
        .wrapping(Wrapping::None)
        .ellipsize(Ellipsize::Middle(EllipsizeHeightLimit::Lines(1)));

    let pill = widget::container(label)
        .max_width(LABEL_MAX_WIDTH)
        .padding(PILL_PADDING)
        .class(theme::Container::custom(pill_style));

    let content = row![
        pill,
        icon::icon(handle).size(ICON_SIZE),
    ]
    .spacing(LABEL_GAP)
    .align_y(Alignment::Center);

    menu_button(content)
        .width(Length::Shrink)
        .on_press(msg)
        .padding(Padding {
            top: ROW_PADDING,
            right: ROW_EDGE + ARC_DEPTH * (1.0 - t * t),
            bottom: ROW_PADDING,
            left: ROW_EDGE,
        })
        .into()
}

/// A line of text in place of the fan, sized to its content.
fn message_row<'a>(message: String) -> Element<'a, Message> {
    widget::container(text::body(message))
        .padding(Padding::from(PILL_PADDING))
        .into()
}

fn pill_style(theme: &cosmic::Theme) -> widget::container::Style {
    let cosmic = theme.cosmic();
    let component = &cosmic.primary(theme.transparent).component;
    widget::container::Style {
        text_color: Some(component.on.into()),
        background: Some(Background::Color(component.base.into())),
        border: Border {
            radius: cosmic.corner_radii.radius_s.into(),
            width: 1.0,
            color: component.divider.into(),
        },
        ..Default::default()
    }
}

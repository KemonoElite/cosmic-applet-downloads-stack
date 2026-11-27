// SPDX-License-Identifier: GPL-3.0-only

mod chromium;
mod downloads;
mod icons;
mod localize;

use crate::downloads::{Entry, Progress, Scan, State};
use cosmic::iced::core::text::{Ellipsize, EllipsizeHeightLimit, Wrapping};
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
    widget::{self, icon, progress_bar, space, text},
};
use std::{borrow::Cow, collections::HashMap, path::PathBuf, time::Duration};

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
/// Downloads in progress get their own card between the fan and the panel.
/// At most this many are listed.
const MAX_DOWNLOADS: usize = 5;
/// The card is at least this wide, even when the fan above it is narrower.
const CARD_MIN_WIDTH: f32 = 300.0;
/// Between the fan and the card.
const CARD_GAP: u16 = 8;
/// Inside the card, and between its heading and downloads.
const CARD_PADDING: u16 = 8;
const CARD_SPACING: u16 = 4;
/// Inside one download: vertical, horizontal.
const DOWNLOAD_PADDING: [u16; 2] = [4, 8];
/// Between a download's name line and its bar.
const PROGRESS_SPACING: u16 = 4;
const BAR_GIRTH: f32 = 4.0;
/// How often the stack re-reads the folder while something is downloading.
const PROGRESS_REFRESH: Duration = Duration::from_secs(1);

pub fn run() -> cosmic::iced::Result {
    icons::add_nixos_profile_dirs();
    localize::localize();

    cosmic::applet::run::<DownloadsStack>(())
}

struct DownloadsStack {
    core: cosmic::app::Core,
    popup: Option<window::Id>,
    dir: Option<PathBuf>,
    /// Finished files, newest first, with the icon resolved once per scan rather than on every
    /// redraw.
    files: Vec<(Entry, icon::Handle)>,
    /// Downloads in progress, newest first.
    downloading: Vec<Entry>,
    /// Icons by name list, so refreshing a download's progress doesn't look them up again.
    icon_cache: HashMap<Vec<String>, icon::Handle>,
    loaded: bool,
    /// A scan is running; refresh ticks are skipped until it's done.
    scanning: bool,
    token_tx: Option<calloop::channel::Sender<TokenRequest>>,
    /// Waiting for an activation token before it's opened.
    pending_open: Option<PathBuf>,
}

#[derive(Debug, Clone)]
enum Message {
    TogglePopup,
    Closed(window::Id),
    Loaded(Option<PathBuf>, Scan),
    /// Re-read the folder to update downloads in progress.
    Refresh,
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
                files: Vec::new(),
                downloading: Vec::new(),
                icon_cache: HashMap::new(),
                loaded: false,
                scanning: false,
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
                self.scanning = true;
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
            Message::Loaded(dir, scan) => {
                self.dir = dir;
                let cache = &mut self.icon_cache;
                self.downloading = scan.downloading;
                self.downloading.truncate(MAX_DOWNLOADS);
                self.files = scan
                    .files
                    .into_iter()
                    .map(|entry| {
                        let handle = cache
                            .entry(entry.icon_names.clone())
                            .or_insert_with(|| file_icon(&entry.icon_names))
                            .clone();
                        (entry, handle)
                    })
                    .collect();
                self.loaded = true;
                self.scanning = false;
            }
            Message::Refresh => {
                if self.popup.is_some() && !self.scanning {
                    self.scanning = true;
                    return scan();
                }
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
            column![message_row(fl!("no-downloads-folder"))].padding(FAN_PADDING)
        } else {
            // Built nearest-to-the-panel first, so `d` is the distance along the fan.
            let mut rows: Vec<Element<'_, Message>> = self
                .files
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

            if self.loaded && self.files.is_empty() && self.downloading.is_empty() {
                rows.push(message_row(fl!("no-downloads")));
            }

            rows.push(fan_row(
                fl!("open-in-files"),
                icon::from_name("folder-open-symbolic")
                    .size(ICON_SIZE)
                    .handle(),
                self.files.len(),
                Message::OpenFolder,
            ));

            // Keeps the card below from getting cramped under a narrow fan.
            if !self.downloading.is_empty() {
                rows.push(space::horizontal().width(CARD_MIN_WIDTH).into());
            }

            let panel_at_bottom = matches!(self.core.applet.anchor, PanelAnchor::Bottom);
            // With a bottom panel the newest file sits at the bottom, like the macOS Dock.
            if panel_at_bottom {
                rows.reverse();
            }

            // Rows are only as wide as their content and line up on the right, so a long name
            // grows the fan to the left and the icons stay put.
            let fan = iced::widget::Column::with_children(rows)
                .spacing(ROW_SPACING)
                .align_x(Alignment::End);

            // The Downloads card goes between the fan and the panel.
            let mut parts: Vec<Element<'_, Message>> = vec![fan.into()];
            if !self.downloading.is_empty() {
                let card = downloads_card(&self.downloading);
                if matches!(self.core.applet.anchor, PanelAnchor::Top) {
                    parts.insert(0, card);
                } else {
                    parts.push(card);
                }
            }

            iced::widget::Column::with_children(parts)
                .spacing(CARD_GAP)
                .align_x(Alignment::End)
                .padding(FAN_PADDING)
                // The card asks for the full width; with the column set to shrink, "full" is
                // the fan's width rather than the popup's maximum.
                .width(Length::Shrink)
        };

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
        let token = activation_token_subscription(0).map(Message::Token);

        if self.popup.is_some() && !self.downloading.is_empty() {
            Subscription::batch([
                token,
                iced::time::every(PROGRESS_REFRESH).map(|_| Message::Refresh),
            ])
        } else {
            token
        }
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
        let (dir, scan) = tokio::task::spawn_blocking(|| {
            let dir = downloads::dir();
            let scan = dir
                .as_deref()
                .map(|dir| downloads::scan(dir, MAX_ITEMS))
                .unwrap_or_default();
            (dir, scan)
        })
        .await
        .unwrap_or_default();
        Message::Loaded(dir, scan)
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
fn fan_row<'a>(
    label: String,
    handle: icon::Handle,
    d: usize,
    msg: Message,
) -> Element<'a, Message> {
    let t = (d as f32 / MAX_ITEMS as f32).min(1.0);

    // One line; a name too long for the label is shortened in the middle to fit.
    let label = text::body(label)
        .wrapping(Wrapping::None)
        .ellipsize(Ellipsize::Middle(EllipsizeHeightLimit::Lines(1)));

    let pill = widget::container(label)
        .max_width(LABEL_MAX_WIDTH)
        .padding(PILL_PADDING)
        .class(theme::Container::custom(pill_style));

    let content = row![pill, icon::icon(handle).size(ICON_SIZE)]
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

/// The card listing downloads in progress: a centered heading, then each download with its name,
/// size and a bar across the card's full width. As wide as the fan, no icons.
fn downloads_card<'a>(downloading: &[Entry]) -> Element<'a, Message> {
    let heading = widget::container(text::heading(fl!("downloads")))
        .center_x(Length::Fill)
        .padding([0, 0, CARD_SPACING, 0]);

    let mut list = column![heading].spacing(CARD_SPACING).width(Length::Fill);
    for entry in downloading {
        if let Some(progress) = entry.progress {
            list = list.push(download_row(entry.name.clone(), progress));
        }
    }

    widget::container(list)
        .width(Length::Fill)
        .padding(CARD_PADDING)
        .class(theme::Container::custom(card_style))
        .into()
}

/// One download: name on the left, size on the right, and the bar underneath.
///
/// The bar fills up when the size is known (Chromium-based browsers). Otherwise it animates
/// while data arrives, and the size counts up. Clicking opens the Downloads folder, since the
/// file can't be opened yet.
fn download_row<'a>(name: String, progress: Progress) -> Element<'a, Message> {
    let name = text::body(name)
        .wrapping(Wrapping::None)
        .ellipsize(Ellipsize::Middle(EllipsizeHeightLimit::Lines(1)))
        .width(Length::Fill);
    let caption = text::caption(progress_caption(progress)).wrapping(Wrapping::None);

    let bar: Element<'a, Message> = match (progress.state, progress.fraction()) {
        (State::Active, None) => progress_bar::indeterminate_linear()
            .width(Length::Fill)
            .girth(BAR_GIRTH)
            .into(),
        (_, fraction) => progress_bar::determinate_linear(fraction.unwrap_or(0.0))
            .width(Length::Fill)
            .girth(BAR_GIRTH)
            .into(),
    };

    let content = column![
        row![name, caption]
            .spacing(PROGRESS_SPACING)
            .align_y(Alignment::Center),
        bar,
    ]
    .spacing(PROGRESS_SPACING);

    menu_button(content)
        .padding(DOWNLOAD_PADDING)
        .on_press(Message::OpenFolder)
        .into()
}

/// "4.5 MB of 12 MB", "4.5 MB" when the total isn't known, with "Stalled" or "Interrupted" in
/// front when the download isn't moving.
fn progress_caption(progress: Progress) -> String {
    let received = downloads::format_size(progress.received);
    let size = match progress.total {
        Some(total) => fl!(
            "progress-of",
            received = received,
            total = downloads::format_size(total)
        ),
        None => received,
    };
    match progress.state {
        State::Active => size,
        State::Stalled => fl!("progress-stalled", size = size),
        State::Interrupted => fl!("progress-interrupted", size = size),
    }
}

/// A line of text in place of the fan, sized to its content.
fn message_row<'a>(message: String) -> Element<'a, Message> {
    widget::container(text::body(message))
        .padding(Padding::from(PILL_PADDING))
        .into()
}

fn card_style(theme: &cosmic::Theme) -> widget::container::Style {
    let cosmic = theme.cosmic();
    let container = &cosmic.primary(theme.transparent);
    widget::container::Style {
        text_color: Some(container.on.into()),
        background: Some(Background::Color(container.base.into())),
        border: Border {
            radius: cosmic.corner_radii.radius_s.into(),
            width: 1.0,
            color: container.divider.into(),
        },
        ..Default::default()
    }
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

#[cfg(test)]
mod tests {
    use cosmic::iced::{
        Length, Size,
        advanced::{Widget, layout::Limits, widget::Tree},
        widget::{Column, Container, Space},
    };

    /// The Downloads card asks for the full width, inside a column set to shrink. It must come out
    /// as wide as the fan beside it, not as wide as the popup allows.
    #[test]
    fn card_is_as_wide_as_the_fan() {
        let fan = Space::new().width(250.0).height(40.0);
        let card: Container<'_, (), cosmic::Theme, ()> = Container::new(Space::new().height(20.0))
            .width(Length::Fill)
            .padding(8);

        let column: Column<'_, (), cosmic::Theme, ()> =
            Column::with_children([fan.into(), card.into()])
                .spacing(8)
                .padding(8)
                .width(Length::Shrink);

        // Wrapped the way `popup_container` wraps it.
        let mut popup: Container<'_, (), cosmic::Theme, ()> =
            Container::new(Container::new(column)).height(Length::Shrink);

        let mut tree = Tree::new(&popup as &dyn Widget<(), cosmic::Theme, ()>);
        let limits = Limits::new(Size::new(1.0, 1.0), Size::new(600.0, 1000.0));
        let node = popup.layout(&mut tree, &(), &limits);

        // 250 fan + 2×8 padding.
        assert_eq!(node.size().width, 266.0);
        let column = &node.children()[0].children()[0];
        let card = column.children()[1].bounds();
        assert_eq!(card.width, 250.0);
    }
}

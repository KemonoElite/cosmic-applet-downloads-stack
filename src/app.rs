// SPDX-License-Identifier: GPL-3.0-only

use crate::downloads::{self, Entry};
use cosmic::applet::cosmic_panel_config::PanelAnchor;
use cosmic::iced::platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::{Alignment, Background, Border, Length, Padding, window::Id};
use cosmic::prelude::*;
use cosmic::widget::{self, icon};
use std::path::PathBuf;
use std::process::Stdio;

/// How many recent items the fan shows.
const MAX_ITEMS: usize = 10;
/// Longer file names are shortened in the middle.
const MAX_NAME_CHARS: usize = 30;
const ICON_SIZE: u16 = 32;
/// How far (px) the fan bends sideways at its far end.
const ARC_DEPTH: f32 = 40.0;

#[derive(Default)]
pub struct AppModel {
    core: cosmic::Core,
    popup: Option<Id>,
    dir: Option<PathBuf>,
    entries: Vec<Entry>,
    loaded: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    Loaded(Vec<Entry>),
    Open(PathBuf),
    OpenFolder,
    Opened,
}

impl cosmic::Application for AppModel {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;

    /// Must match the .desktop file name in resources/.
    const APP_ID: &'static str = "io.github.yourname.DownloadsStack";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(
        core: cosmic::Core,
        _flags: Self::Flags,
    ) -> (Self, Task<cosmic::Action<Self::Message>>) {
        let app = AppModel {
            core,
            dir: downloads::downloads_dir(),
            ..Default::default()
        };
        (app, Task::none())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    /// The button shown in the panel.
    fn view(&self) -> Element<'_, Self::Message> {
        let size = self.core.applet.suggested_size(true).0;
        let handle: icon::Handle = icon::from_name("folder-download-symbolic")
            .fallback(Some(icon::IconFallback::Names(vec![
                "folder-symbolic".into(),
            ])))
            .symbolic(true)
            .size(size)
            .into();

        self.core
            .applet
            .icon_button_from_handle(handle)
            .on_press(Message::TogglePopup)
            .into()
    }

    /// The fan popup.
    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        let mut column = widget::Column::new().spacing(2.0).padding(8);

        if self.dir.is_none() {
            column = column.push(widget::text::body("Couldn't find your Downloads folder"));
            return self.core.applet.popup_container(column).into();
        }

        // Built nearest-to-the-panel first, so `d` is the distance along the fan.
        let mut rows: Vec<Element<'_, Message>> = self
            .entries
            .iter()
            .enumerate()
            .map(|(d, entry)| {
                fan_row(
                    downloads::shorten(&entry.name, MAX_NAME_CHARS),
                    file_icon(&entry.icon_names),
                    d,
                    Message::Open(entry.path.clone()),
                )
            })
            .collect();

        if self.loaded && self.entries.is_empty() {
            rows.push(widget::text::body("No downloads yet").into());
        }

        rows.push(fan_row(
            "Open in Files".to_owned(),
            file_icon(&["folder-open-symbolic".to_owned(), "folder".to_owned()]),
            self.entries.len(),
            Message::OpenFolder,
        ));

        // With a bottom panel the newest file sits at the bottom, like the macOS Dock.
        if matches!(self.core.applet.anchor, PanelAnchor::Bottom) {
            rows.reverse();
        }

        for row in rows {
            column = column.push(row);
        }

        self.core.applet.popup_container(column).into()
    }

    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    return destroy_popup(p);
                }
                let new_id = Id::unique();
                self.popup = Some(new_id);
                let settings = self.core.applet.get_popup_settings(
                    self.core.main_window_id().unwrap(),
                    new_id,
                    None,
                    None,
                    None,
                );
                // Re-scan the folder every time the popup opens.
                return Task::batch([get_popup(settings), self.reload()]);
            }
            Message::PopupClosed(id) => {
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                }
            }
            Message::Loaded(entries) => {
                self.entries = entries;
                self.loaded = true;
            }
            Message::Open(path) => {
                return Task::batch([self.close_popup(), open_path(path)]);
            }
            Message::OpenFolder => {
                if let Some(dir) = self.dir.clone() {
                    return Task::batch([self.close_popup(), open_path(dir)]);
                }
            }
            Message::Opened => {}
        }
        Task::none()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}

impl AppModel {
    fn close_popup(&mut self) -> Task<cosmic::Action<Message>> {
        match self.popup.take() {
            Some(p) => destroy_popup(p),
            None => Task::none(),
        }
    }

    fn reload(&self) -> Task<cosmic::Action<Message>> {
        let Some(dir) = self.dir.clone() else {
            return Task::none();
        };
        cosmic::task::future(async move {
            let entries =
                tokio::task::spawn_blocking(move || downloads::recent(&dir, MAX_ITEMS))
                    .await
                    .unwrap_or_default();
            Message::Loaded(entries)
        })
    }
}

/// Opens a file or folder with the default application.
fn open_path(path: PathBuf) -> Task<cosmic::Action<Message>> {
    cosmic::task::future(async move {
        let _ = tokio::process::Command::new("xdg-open")
            .arg(&path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
        Message::Opened
    })
}

/// Icon with fallbacks: the first name is tried first, the rest if it is missing.
fn file_icon(names: &[String]) -> icon::Icon {
    let (first, rest) = names.split_first().expect("icon names are never empty");
    icon::from_name(first.clone())
        .fallback(Some(icon::IconFallback::Names(
            rest.iter().cloned().map(Into::into).collect(),
        )))
        .size(ICON_SIZE)
        .icon()
        .size(ICON_SIZE)
}

/// One row of the fan: a label pill with its icon on the right.
/// `d` is the distance from the panel and drives the sideways arc.
fn fan_row<'a>(label: String, icon: icon::Icon, d: usize, msg: Message) -> Element<'a, Message> {
    let t = (d as f32 / MAX_ITEMS as f32).min(1.0);
    let right_padding = 8.0 + ARC_DEPTH * (1.0 - t * t);

    let pill = widget::container(widget::text::body(label))
        .padding([4, 10])
        .style(pill_style);

    let content = widget::Row::new()
        .push(widget::space::horizontal())
        .push(pill)
        .push(icon)
        .spacing(10.0)
        .align_y(Alignment::Center);

    widget::button::custom(content)
        .on_press(msg)
        .width(Length::Fill)
        .padding(Padding {
            top: 2.0,
            right: right_padding,
            bottom: 2.0,
            left: 8.0,
        })
        .class(cosmic::theme::Button::MenuItem)
        .into()
}

fn pill_style(theme: &cosmic::Theme) -> cosmic::iced::widget::container::Style {
    let cosmic = theme.cosmic();
    let component = &cosmic.primary.component;
    cosmic::iced::widget::container::Style {
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

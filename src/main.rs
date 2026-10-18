// SPDX-License-Identifier: GPL-3.0-only

mod app;
mod downloads;
mod icons;

fn main() -> cosmic::iced::Result {
    icons::expose_host_icon_themes();
    cosmic::applet::run::<app::AppModel>(())
}

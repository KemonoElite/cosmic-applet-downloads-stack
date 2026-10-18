// SPDX-License-Identifier: GPL-3.0-only

mod app;
mod downloads;
mod icons;

fn main() -> cosmic::iced::Result {
    icons::expose_host_icon_themes();
    icons::apply_system_icon_theme();
    cosmic::applet::run::<app::AppModel>(())
}

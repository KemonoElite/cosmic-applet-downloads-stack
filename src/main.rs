// SPDX-License-Identifier: GPL-3.0-only

mod app;
mod downloads;
mod icons;

fn main() -> cosmic::iced::Result {
    icons::init();
    cosmic::applet::run::<app::AppModel>(())
}

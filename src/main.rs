// SPDX-License-Identifier: GPL-3.0-only

mod app;
mod downloads;

fn main() -> cosmic::iced::Result {
    cosmic::applet::run::<app::AppModel>(())
}

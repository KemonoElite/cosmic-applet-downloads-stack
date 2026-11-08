// SPDX-License-Identifier: GPL-3.0-only

mod app;
mod downloads;
mod icons;

fn main() -> cosmic::iced::Result {
    // Same as the stock applets: without this, errors from libcosmic/iced are dropped.
    tracing_subscriber::fmt::init();
    let _ = tracing_log::LogTracer::init();

    icons::init();
    cosmic::applet::run::<app::AppModel>(())
}

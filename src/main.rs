// SPDX-License-Identifier: GPL-3.0-only

mod app;
mod downloads;
mod icons;

fn main() -> cosmic::iced::Result {
    // Same as the stock applets: without this, errors from libcosmic/iced are dropped.
    // Errors from everything, plus this applet's own info messages. RUST_LOG overrides.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("error,cosmic_applet_downloads_stack=info")
            }),
        )
        .init();
    let _ = tracing_log::LogTracer::init();

    icons::init();
    cosmic::applet::run::<app::AppModel>(())
}

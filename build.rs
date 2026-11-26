// SPDX-License-Identifier: GPL-3.0-only

//! Writes the translated desktop entry to `target/xdgen/`, like pop-os/cosmic-applets does.
//! Name, Comment and Keywords come from the `.ftl` files in `i18n/`.

use std::{fs, path::PathBuf};
use xdgen::{App, Context, FluentString};

const APP_ID: &str = "io.github.KemonoElite.CosmicAppletDownloadsStack";
const NAME: &str = "cosmic-applet-downloads-stack";

fn main() {
    println!("cargo::rerun-if-changed=i18n");
    println!("cargo::rerun-if-changed=data/{APP_ID}.desktop");
    println!("cargo::rerun-if-env-changed=CARGO_TARGET_DIR");

    let ctx = Context::new("i18n/", NAME).unwrap();

    let app = App::new(FluentString(NAME))
        .comment(FluentString("cosmic-applet-downloads-stack-comment"))
        .keywords(FluentString("cosmic-applet-downloads-stack-keywords"));

    let contents = app
        .expand_desktop(format!("data/{APP_ID}.desktop"), &ctx)
        .unwrap();

    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target"));
    let parent = target_dir.join("xdgen");
    fs::create_dir_all(&parent).unwrap();
    fs::write(parent.join(format!("{APP_ID}.desktop")), contents).unwrap();
}

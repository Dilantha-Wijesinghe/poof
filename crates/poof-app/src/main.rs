//! Poof — find out what's eating your Mac's storage, and get it back.
//!
//! Running the binary opens the app. `poof --check` performs one background
//! free-space check (used by the optional `LaunchAgent`) and exits.

mod app;
mod font;
mod icon;
mod screen;
mod style;
mod task;
mod trash;
mod updater;
mod widget;

use poof_core::monitor;

fn main() -> iced::Result {
    if std::env::args().any(|a| a == monitor::CHECK_FLAG) {
        if let Err(e) = monitor::check() {
            eprintln!("poof: {e}");
            std::process::exit(1);
        }
        return Ok(());
    }

    let mut application = iced::application(app::App::new, app::App::update, app::App::view)
        .title(poof_core::APP_NAME)
        .theme(app::App::theme)
        .subscription(app::App::subscription)
        .window(app::window_settings())
        .default_font(font::REGULAR)
        .antialiasing(true);
    for file in font::FILES {
        application = application.font(file);
    }
    application.run()
}

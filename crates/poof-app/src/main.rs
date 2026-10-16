//! Poof — find out what's eating your Mac's storage, and get it back.
//!
//! Running the binary opens the app. `poof --check` performs one background
//! free-space check (used by the optional `LaunchAgent`) and exits.
//! `poof --finish-update <pid> <bundle>` sees an update through its first
//! launch (see `poof_core::update::finish_update`).

mod app;
mod font;
mod icon;
mod screen;
mod style;
mod task;
mod trash;
mod updater;
mod widget;

use std::path::Path;

use poof_core::{monitor, update};

fn main() -> iced::Result {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == monitor::CHECK_FLAG) {
        exit_with(monitor::check().map(drop));
    }
    if let [_, flag, pid, bundle] = args.as_slice()
        && flag == update::FINISH_FLAG
        && let Ok(pid) = pid.parse()
    {
        exit_with(update::finish_update(pid, Path::new(bundle)));
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

/// Ends a command-line run, reporting `result`'s error if there is one.
fn exit_with(result: poof_core::Result<()>) -> ! {
    if let Err(e) = &result {
        eprintln!("poof: {e}");
    }
    std::process::exit(i32::from(result.is_err()))
}

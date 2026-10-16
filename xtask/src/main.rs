//! Development tasks for Poof.
//!
//! ```sh
//! cargo xtask icon   # packaging/macos/icon.svg → packaging/macos/AppIcon.icns
//! ```

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use resvg::{tiny_skia, usvg};

/// (file name inside the .iconset, pixel size) as required by `iconutil`.
const ICONSET: [(&str, u32); 10] = [
    ("icon_16x16.png", 16),
    ("icon_16x16@2x.png", 32),
    ("icon_32x32.png", 32),
    ("icon_32x32@2x.png", 64),
    ("icon_128x128.png", 128),
    ("icon_128x128@2x.png", 256),
    ("icon_256x256.png", 256),
    ("icon_256x256@2x.png", 512),
    ("icon_512x512.png", 512),
    ("icon_512x512@2x.png", 1024),
];

fn main() -> ExitCode {
    let task = std::env::args().nth(1);
    let result = match task.as_deref() {
        Some("icon") => icon(),
        _ => Err("usage: cargo xtask icon".to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace root").to_path_buf()
}

/// Renders the app icon at every size macOS needs and packs it with `iconutil`.
fn icon() -> Result<(), String> {
    let packaging = root().join("packaging/macos");
    let source = packaging.join("icon.svg");
    let data = std::fs::read(&source).map_err(|e| format!("{}: {e}", source.display()))?;
    let options = usvg::Options { resources_dir: Some(packaging.clone()), ..usvg::Options::default() };
    let tree = usvg::Tree::from_data(&data, &options).map_err(|e| e.to_string())?;

    let iconset = std::env::temp_dir().join("Poof.iconset");
    let _ = std::fs::remove_dir_all(&iconset);
    std::fs::create_dir_all(&iconset).map_err(|e| e.to_string())?;
    for (name, size) in ICONSET {
        let mut pixmap = tiny_skia::Pixmap::new(size, size).ok_or("invalid icon size")?;
        let scale = size as f32 / tree.size().width();
        resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
        pixmap.save_png(iconset.join(name)).map_err(|e| e.to_string())?;
    }
    // Also keep a 1024 px PNG for the README and release notes.
    std::fs::copy(iconset.join("icon_512x512@2x.png"), root().join("docs/images/icon.png"))
        .map_err(|e| e.to_string())?;

    let output = packaging.join("AppIcon.icns");
    let status = Command::new("iconutil")
        .args(["--convert", "icns", "--output"])
        .arg(&output)
        .arg(&iconset)
        .status()
        .map_err(|e| format!("iconutil: {e}"))?;
    if !status.success() {
        return Err(format!("iconutil exited with {status}"));
    }
    println!("wrote {}", output.display());
    Ok(())
}

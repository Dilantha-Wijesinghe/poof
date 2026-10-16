//! Development tasks for Poof.
//!
//! ```sh
//! cargo xtask icon          # packaging/macos/icon.svg → packaging/macos/AppIcon.icns
//! cargo xtask keygen        # create the update-signing key pair (once per project)
//! cargo xtask sign <file>   # write <file>.minisig using $POOF_UPDATE_SIGNING_KEY
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
        Some("keygen") => keygen(),
        Some("sign") => {
            std::env::args().nth(2).ok_or_else(|| "usage: cargo xtask sign <file>".to_owned()).and_then(sign)
        }
        _ => Err("usage: cargo xtask <icon | keygen | sign FILE>".to_owned()),
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

/// Where `keygen` keeps the secret key: outside the repository, private to the user.
fn secret_key_path() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    home.join(".poof-release/update-signing.key")
}

/// Creates the minisign key pair that signs update archives. The public key
/// is committed (the app embeds it); the secret key goes to
/// `~/.poof-release/` and must be added to the `POOF_UPDATE_SIGNING_KEY`
/// GitHub secret.
fn keygen() -> Result<(), String> {
    let secret_path = secret_key_path();
    if secret_path.exists() {
        return Err(format!("{} already exists; refusing to overwrite it", secret_path.display()));
    }
    let pair = minisign::KeyPair::generate_unencrypted_keypair().map_err(|e| e.to_string())?;
    let public = pair.pk.to_box().map_err(|e| e.to_string())?.into_string();
    let secret = pair.sk.to_box(Some("Poof update signing key")).map_err(|e| e.to_string())?.into_string();

    let dir = secret_path.parent().ok_or("invalid key path")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    write_private(&secret_path, &secret)?;
    std::fs::write(root().join("crates/poof-core/update-key.pub"), public).map_err(|e| e.to_string())?;
    println!("public key  → crates/poof-core/update-key.pub (commit this)");
    println!(
        "secret key  → {} (back it up; add it as the POOF_UPDATE_SIGNING_KEY secret)",
        secret_path.display()
    );
    Ok(())
}

fn write_private(path: &Path, contents: &str) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .and_then(|mut f| f.write_all(contents.as_bytes()))
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Signs a release archive. The key comes from `$POOF_UPDATE_SIGNING_KEY`
/// (as in CI) or, locally, from `~/.poof-release/update-signing.key`.
fn sign(file: String) -> Result<(), String> {
    let secret = match std::env::var("POOF_UPDATE_SIGNING_KEY") {
        Ok(key) if !key.trim().is_empty() => key,
        _ => std::fs::read_to_string(secret_key_path()).map_err(|_| {
            "no signing key: set POOF_UPDATE_SIGNING_KEY or run `cargo xtask keygen`".to_owned()
        })?,
    };
    let key = load_secret_key(&secret)?;
    let data = std::fs::File::open(&file).map_err(|e| format!("{file}: {e}"))?;
    let name = Path::new(&file).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let trusted = format!("file:{name}");
    let signature = minisign::sign(None, &key, data, Some(&trusted), None).map_err(|e| e.to_string())?;
    let out = format!("{file}.minisig");
    std::fs::write(&out, signature.into_string()).map_err(|e| e.to_string())?;
    verify_with_shipped_key(&file, &out)?;
    println!("wrote {out} (verified against crates/poof-core/update-key.pub)");
    Ok(())
}

/// Checks a signature against the public key compiled into the app, so a
/// release can never ship updates that installed copies would reject.
fn verify_with_shipped_key(file: &str, signature: &str) -> Result<(), String> {
    let public =
        std::fs::read_to_string(root().join("crates/poof-core/update-key.pub")).map_err(|e| e.to_string())?;
    let public = minisign::PublicKeyBox::from_string(&public)
        .and_then(minisign::PublicKey::from_box)
        .map_err(|e| e.to_string())?;
    let signature = std::fs::read_to_string(signature).map_err(|e| e.to_string())?;
    let signature = minisign::SignatureBox::from_string(&signature).map_err(|e| e.to_string())?;
    let data = std::fs::File::open(file).map_err(|e| e.to_string())?;
    minisign::verify(&public, &signature, data, true, false, false)
        .map_err(|_| "the signing key does not match crates/poof-core/update-key.pub".to_owned())
}

/// Parses a secret key as written by `keygen` (minisign format, unencrypted).
fn load_secret_key(text: &str) -> Result<minisign::SecretKey, String> {
    minisign::SecretKeyBox::from_string(text)
        .and_then(minisign::SecretKeyBox::into_unencrypted_secret_key)
        .map_err(|e| format!("invalid signing key: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact text `keygen` saves must load and produce verifiable signatures.
    #[test]
    fn saved_keys_sign_verifiably() {
        let pair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let saved = pair.sk.to_box(Some("test")).unwrap().into_string();
        let key = load_secret_key(&saved).unwrap();

        let data = b"Poof-0.1.0.zip contents";
        let signature = minisign::sign(None, &key, &data[..], None, None).unwrap();
        let signature = minisign::SignatureBox::from_string(&signature.into_string()).unwrap();
        minisign::verify(&pair.pk, &signature, std::io::Cursor::new(&data[..]), true, false, false).unwrap();
    }
}

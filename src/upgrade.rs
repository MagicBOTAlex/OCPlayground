//! Self-update support: `ocplay --upgrade` downloads the latest release build
//! and replaces the running executable and its bundled system files.
//!
//! Nix-managed installs are refused, since Nix owns their store paths.

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;

const REPO: &str = "MagicBOTAlex/OCPlayground";

pub fn upgrade() -> Result<i32> {
    let exe = std::env::current_exe().context("cannot determine the current executable")?;
    let exe = fs::canonicalize(&exe).unwrap_or(exe);

    if is_nix_store(&exe) {
        eprintln!("ocplay was installed with Nix, so it updates itself with Nix:");
        eprintln!("  nix profile upgrade ocplay");
        eprintln!("  # or, from a checkout: nix profile install .#ocplay");
        return Ok(1);
    }

    let url = release_url()?;
    eprintln!("Downloading {url}");
    let bytes = download(&url)?;

    let install_dir = exe
        .parent()
        .context("executable has no parent directory")?
        .to_path_buf();

    let staging = tempfile::Builder::new()
        .prefix("ocplay-upgrade-")
        .tempdir()
        .context("failed to create a staging directory")?;
    extract(&bytes, staging.path())?;

    let new_binary = staging.path().join("ocplay/ocplay");
    if !new_binary.is_file() {
        bail!("downloaded archive does not contain ocplay/ocplay");
    }

    let new_bytes = fs::read(&new_binary).context("failed to read the downloaded binary")?;
    if fs::read(&exe).ok().as_deref() == Some(new_bytes.as_slice()) {
        eprintln!("ocplay is already up to date.");
        return Ok(0);
    }

    replace_file(&install_dir, &new_binary, &exe)?;
    replace_assets(&install_dir, &staging.path().join("ocplay/assets"))?;

    eprintln!("Upgraded ocplay. Restart it to use the new version.");
    Ok(0)
}

fn is_nix_store(path: &Path) -> bool {
    path.starts_with("/nix/store")
}

fn release_url() -> Result<String> {
    let asset = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "ocplay-x86_64-unknown-linux-gnu.tar.gz",
        ("linux", "aarch64") => "ocplay-aarch64-unknown-linux-gnu.tar.gz",
        (os, arch) => bail!("no prebuilt ocplay for {os}/{arch}; build from source instead"),
    };
    Ok(format!(
        "https://github.com/{REPO}/releases/latest/download/{asset}"
    ))
}

fn download(url: &str) -> Result<Vec<u8>> {
    let mut response = ureq::get(url)
        .call()
        .map_err(|e| anyhow::anyhow!("download failed: {e}"))?;
    response
        .body_mut()
        .read_to_vec()
        .map_err(|e| anyhow::anyhow!("failed to read the download: {e}"))
}

fn extract(bytes: &[u8], dest: &Path) -> Result<()> {
    let decoder = flate2::read::GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(dest)
        .context("failed to extract the downloaded archive")
}

/// Replace the running executable without disturbing symlinks pointing at it:
/// write the new binary next to it, then rename into place.
fn replace_file(dir: &Path, source: &Path, target: &Path) -> Result<()> {
    let staged = dir.join(".ocplay-upgrade");
    fs::copy(source, &staged).context("failed to stage the new binary")?;
    set_executable(&staged)?;
    fs::rename(&staged, target).context("failed to replace the binary")?;
    Ok(())
}

fn replace_assets(dir: &Path, source: &Path) -> Result<()> {
    if !source.is_dir() {
        return Ok(());
    }
    let target = dir.join("assets");
    let backup = dir.join("assets.old");
    let _ = fs::remove_dir_all(&backup);
    if target.exists() {
        fs::rename(&target, &backup).context("failed to move the old assets aside")?;
    }
    copy_dir(source, &target)?;
    let _ = fs::remove_dir_all(&backup);
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let target = to.join(entry.file_name());
        if source.is_dir() {
            copy_dir(&source, &target)?;
        } else {
            fs::copy(&source, &target)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_nix_store_paths() {
        assert!(is_nix_store(Path::new(
            "/nix/store/abc-ocplay-0.1.0/bin/ocplay"
        )));
        assert!(!is_nix_store(Path::new("/home/user/.local/share/ocplay/ocplay")));
    }

    #[test]
    fn release_url_points_at_the_latest_asset() {
        let url = release_url().unwrap();
        assert!(url.starts_with("https://github.com/MagicBOTAlex/OCPlayground/releases/latest/download/"));
        assert!(url.ends_with(".tar.gz"));
    }
}

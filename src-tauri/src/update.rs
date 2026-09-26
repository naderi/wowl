//! Self-update from GitHub Releases (Windows).
//!
//! `check()` asks the GitHub API for the latest *published* release and compares
//! it with the running version. `prepare()` downloads the matching asset and its
//! minisign signature (`<asset>.sig`, produced by `tauri signer sign`) and only
//! keeps the file if the signature verifies against the public key embedded in
//! this build (`updater.pub`). `install()` then applies it:
//!
//!   * portable  — the running `Wowl.exe` is renamed to `Wowl.exe.old`, the new
//!                 file takes its name, and a detached helper starts it once this
//!                 process is gone. Windows allows renaming a running exe; the
//!                 `wowl.toml` and `history/` next to it are untouched.
//!   * installed — the verified NSIS setup is launched and Wowl exits.
//!   * MSI / dev builds are only *reported*: their installer (or the build
//!     tree) owns the files, so Wowl must not rewrite them.
//!
//! On other platforms the feature is reported as unsupported.

use serde::Serialize;

#[cfg_attr(not(windows), allow(dead_code))]
pub const PROGRESS_EVENT: &str = "update-progress";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    /// "portable" | "installed" | "unsupported"
    pub mode: String,
    pub release_url: String,
    pub notes: String,
    /// A signed asset for this install exists and Wowl may apply it.
    pub can_download: bool,
    pub asset_size: Option<u64>,
    /// Why `can_download` is false, for the UI ("" when it is true).
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedUpdate {
    pub version: String,
    pub mode: String,
}

#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Clone, Serialize)]
struct Progress {
    downloaded: u64,
    total: u64,
}

#[cfg(windows)]
mod imp {
    use std::fs;
    use std::io::Write;
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::{Duration, Instant};

    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use minisign_verify::{PublicKey, Signature};
    use reqwest::Client;
    use semver::Version;
    use serde::Deserialize;
    use tauri::{AppHandle, Emitter};

    use super::{PreparedUpdate, Progress, UpdateInfo, PROGRESS_EVENT};

    const RELEASES_URL: &str = "https://api.github.com/repos/naderi/wowl/releases/latest";
    const PUBLIC_KEY: &str = include_str!("../updater.pub");
    const PORTABLE_ASSET: &str = "Wowl.exe";
    const MAX_DOWNLOAD: u64 = 256 * 1024 * 1024;
    const TIMEOUT: Duration = Duration::from_secs(120);
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    #[derive(Debug, Deserialize)]
    struct Asset {
        name: String,
        browser_download_url: String,
        size: u64,
    }

    #[derive(Debug, Deserialize)]
    struct Release {
        tag_name: String,
        html_url: String,
        body: Option<String>,
        assets: Vec<Asset>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Mode {
        Portable,
        Installed,
        Unsupported,
    }

    impl Mode {
        fn name(self) -> &'static str {
            match self {
                Mode::Portable => "portable",
                Mode::Installed => "installed",
                Mode::Unsupported => "unsupported",
            }
        }
    }

    // --- what kind of install is this? ---------------------------------------

    /// Classify an executable path. Only a plain folder (portable) or a Tauri
    /// NSIS install directory (has `uninstall.exe`) may be rewritten by Wowl.
    fn detect_mode(exe: &Path) -> Mode {
        let p = exe.to_string_lossy().to_ascii_lowercase().replace('/', "\\");
        if p.contains("\\target\\debug\\")
            || p.contains("\\target\\release\\")
            || p.contains("\\program files")
        {
            return Mode::Unsupported; // dev build tree, or an MSI (admin-owned) install
        }
        if exe.parent().is_some_and(|d| d.join("uninstall.exe").is_file()) {
            return Mode::Installed;
        }
        Mode::Portable
    }

    fn arch_label() -> Option<&'static str> {
        match std::env::consts::ARCH {
            "x86_64" => Some("x64"),
            "aarch64" => Some("arm64"),
            _ => None,
        }
    }

    /// (asset, signature asset) that updates `mode` on this CPU, if any.
    fn asset_names(mode: Mode, version: &str) -> Option<(String, String)> {
        let asset = match mode {
            // the portable exe in the releases is an x64 build
            Mode::Portable if arch_label() == Some("x64") => PORTABLE_ASSET.to_string(),
            Mode::Installed => format!("Wowl_{version}_{}-setup.exe", arch_label()?),
            _ => return None,
        };
        let sig = format!("{asset}.sig");
        Some((asset, sig))
    }

    fn dir_writable(dir: &Path) -> bool {
        let probe = dir.join(".wowl-update-probe");
        let ok = fs::File::create(&probe).is_ok();
        let _ = fs::remove_file(&probe);
        ok
    }

    // --- network ---------------------------------------------------------------

    fn releases_url() -> String {
        // Debug builds can be pointed at a local fake release server (tests).
        #[cfg(debug_assertions)]
        if let Ok(url) = std::env::var("WOWL_UPDATE_API") {
            return url;
        }
        RELEASES_URL.to_string()
    }

    async fn fetch_release(client: &Client) -> Result<Release, String> {
        client
            .get(releases_url())
            .header("Accept", "application/vnd.github+json")
            .timeout(TIMEOUT)
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| format!("could not reach GitHub: {e}"))?
            .json::<Release>()
            .await
            .map_err(|e| format!("unexpected answer from GitHub: {e}"))
    }

    fn parse_version(tag: &str) -> Result<Version, String> {
        Version::parse(tag.trim().trim_start_matches('v'))
            .map_err(|e| format!("release tag '{tag}' is not a version: {e}"))
    }

    fn current_version() -> Version {
        Version::parse(env!("CARGO_PKG_VERSION")).expect("crate version is semver")
    }

    pub async fn check(client: &Client) -> Result<UpdateInfo, String> {
        let release = fetch_release(client).await?;
        let latest = parse_version(&release.tag_name)?;
        let current = current_version();
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let mode = detect_mode(&exe);

        let mut info = UpdateInfo {
            current_version: current.to_string(),
            latest_version: latest.to_string(),
            update_available: latest > current,
            mode: mode.name().to_string(),
            release_url: release.html_url.clone(),
            notes: release.body.clone().unwrap_or_default().chars().take(1500).collect(),
            can_download: false,
            asset_size: None,
            reason: String::new(),
        };
        if !info.update_available {
            return Ok(info);
        }

        info.reason = match asset_names(mode, &latest.to_string()) {
            None => match mode {
                Mode::Unsupported => "managed",
                _ => "arch",
            }
            .to_string(),
            Some((asset, sig)) => {
                let has = |n: &str| release.assets.iter().find(|a| a.name == n);
                match (has(&asset), has(&sig)) {
                    (Some(a), Some(_)) if mode == Mode::Portable && !exe.parent().is_some_and(dir_writable) => {
                        info.asset_size = Some(a.size);
                        "readonly".to_string()
                    }
                    (Some(a), Some(_)) => {
                        info.asset_size = Some(a.size);
                        info.can_download = true;
                        String::new()
                    }
                    _ => "unsigned".to_string(), // asset or its signature isn't published (yet)
                }
            }
        };
        Ok(info)
    }

    // --- verification -----------------------------------------------------------

    fn decode_text(b64: &str, what: &str) -> Result<String, String> {
        let bytes = STANDARD
            .decode(b64.trim())
            .map_err(|e| format!("bad {what} encoding: {e}"))?;
        String::from_utf8(bytes).map_err(|e| format!("bad {what} text: {e}"))
    }

    /// Both arguments are the base64 text `tauri signer` writes to its files.
    pub(super) fn verify(data: &[u8], signature_b64: &str, public_key_b64: &str) -> Result<(), String> {
        let key = PublicKey::decode(&decode_text(public_key_b64, "public key")?)
            .map_err(|e| format!("bad public key: {e}"))?;
        let sig = Signature::decode(&decode_text(signature_b64, "signature")?)
            .map_err(|e| format!("bad signature: {e}"))?;
        key.verify(data, &sig, false)
            .map_err(|_| "signature does not match — the download was rejected".to_string())
    }

    fn verify_release_file(data: &[u8], signature_b64: &str) -> Result<(), String> {
        verify(data, signature_b64, PUBLIC_KEY)
    }

    // --- download ----------------------------------------------------------------

    async fn download(
        client: &Client,
        url: &str,
        expected: u64,
        mut on_progress: impl FnMut(u64, u64),
    ) -> Result<Vec<u8>, String> {
        let mut resp = client
            .get(url)
            .timeout(Duration::from_secs(600))
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| format!("download failed: {e}"))?;
        let total = resp.content_length().unwrap_or(expected);
        let mut data = Vec::with_capacity(total.min(MAX_DOWNLOAD) as usize);
        let mut last = Instant::now();
        while let Some(chunk) = resp.chunk().await.map_err(|e| format!("download interrupted: {e}"))? {
            data.extend_from_slice(&chunk);
            if data.len() as u64 > MAX_DOWNLOAD {
                return Err("download is larger than expected — rejected".to_string());
            }
            if last.elapsed() >= Duration::from_millis(80) {
                on_progress(data.len() as u64, total);
                last = Instant::now();
            }
        }
        on_progress(data.len() as u64, total);
        Ok(data)
    }

    fn write_atomic(path: &Path, data: &[u8]) -> Result<(), String> {
        let tmp = path.with_extension("part");
        let mut f = fs::File::create(&tmp).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
        f.write_all(data).and_then(|_| f.sync_all()).map_err(|e| e.to_string())?;
        drop(f);
        fs::rename(&tmp, path).map_err(|e| format!("cannot write {}: {e}", path.display()))
    }

    /// Where a downloaded update of `mode` is kept, plus the file for its signature.
    fn staged_paths(mode: Mode, exe: &Path, asset: &str) -> Result<(PathBuf, PathBuf), String> {
        let file = match mode {
            Mode::Portable => {
                let name = exe.file_name().and_then(|n| n.to_str()).ok_or("executable has no name")?;
                exe.with_file_name(format!("{name}.new"))
            }
            _ => {
                let dir = std::env::temp_dir().join("Wowl-update");
                fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                dir.join(asset)
            }
        };
        let sig = PathBuf::from(format!("{}.sig", file.display()));
        Ok((file, sig))
    }

    pub async fn prepare(app: &AppHandle, client: &Client, expected: &str) -> Result<PreparedUpdate, String> {
        let release = fetch_release(client).await?;
        let latest = parse_version(&release.tag_name)?;
        if latest.to_string() != expected {
            return Err(format!("the latest release is now {latest}, not {expected} — check again"));
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let mode = detect_mode(&exe);
        let (asset_name, sig_name) =
            asset_names(mode, &latest.to_string()).ok_or("this installation cannot be updated automatically")?;
        let find = |n: &str| {
            release.assets.iter().find(|a| a.name == n).ok_or_else(|| format!("release has no {n}"))
        };
        let (asset, sig_asset) = (find(&asset_name)?, find(&sig_name)?);

        let signature = client
            .get(&sig_asset.browser_download_url)
            .timeout(TIMEOUT)
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| format!("could not fetch the signature: {e}"))?
            .text()
            .await
            .map_err(|e| e.to_string())?;

        let data = download(client, &asset.browser_download_url, asset.size, |done, total| {
            let _ = app.emit(PROGRESS_EVENT, Progress { downloaded: done, total });
        })
        .await?;
        verify_release_file(&data, &signature)?;

        let (file, sig_file) = staged_paths(mode, &exe, &asset_name)?;
        write_atomic(&file, &data)?;
        write_atomic(&sig_file, signature.trim().as_bytes())?;
        Ok(PreparedUpdate { version: latest.to_string(), mode: mode.name().to_string() })
    }

    // --- applying ------------------------------------------------------------------

    /// Re-check a staged file against its signature right before using it.
    fn verified_staged(mode: Mode, exe: &Path, version: &str) -> Result<PathBuf, String> {
        let (asset, _) = asset_names(mode, version).ok_or("this installation cannot be updated automatically")?;
        let (file, sig_file) = staged_paths(mode, exe, &asset)?;
        let data = fs::read(&file).map_err(|_| "no downloaded update found — download it again".to_string())?;
        let sig = fs::read_to_string(&sig_file).map_err(|_| "the downloaded update has no signature".to_string())?;
        verify_release_file(&data, &sig)?;
        Ok(file)
    }

    /// `current` → `current.old`, `new` → `current`. Rolls back if the second
    /// step fails. Windows lets a running exe be renamed, so this is safe while
    /// Wowl is still running.
    fn swap_files(current: &Path, new: &Path) -> Result<PathBuf, String> {
        let name = current.file_name().and_then(|n| n.to_str()).ok_or("executable has no name")?;
        let old = current.with_file_name(format!("{name}.old"));
        let _ = fs::remove_file(&old);
        fs::rename(current, &old).map_err(|e| format!("cannot move the running program aside: {e}"))?;
        if let Err(e) = fs::rename(new, current) {
            let _ = fs::rename(&old, current);
            return Err(format!("cannot put the new version in place: {e}"));
        }
        Ok(old)
    }

    /// Start `exe` a moment from now, from a process that outlives this one, so
    /// the new version never runs alongside the one that is shutting down.
    fn relaunch_later(exe: &Path) -> Result<(), String> {
        Command::new("cmd")
            .raw_arg(format!(
                r#"/C "ping 127.0.0.1 -n 3 >nul & start "" "{}"""#,
                exe.display()
            ))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("cannot restart: {e}"))
    }

    /// Apply a prepared update. On success the caller must exit the app.
    pub fn install(version: &str) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let mode = detect_mode(&exe);
        let staged = verified_staged(mode, &exe, version)?;
        match mode {
            Mode::Portable => {
                swap_files(&exe, &staged)?;
                let _ = fs::remove_file(format!("{}.sig", staged.display()));
                relaunch_later(&exe)
            }
            Mode::Installed => Command::new(&staged)
                .spawn()
                .map(|_| ())
                .map_err(|e| format!("cannot start the installer: {e}")),
            _ => Err("this installation cannot be updated automatically".to_string()),
        }
    }

    /// Remove the previous version left behind by the last update.
    pub fn cleanup_old() {
        let Ok(exe) = std::env::current_exe() else { return };
        if let Some(name) = exe.file_name().and_then(|n| n.to_str()) {
            let _ = fs::remove_file(exe.with_file_name(format!("{name}.old")));
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        // A throwaway key pair, made only for these tests: `tauri signer sign`
        // over the bytes of PAYLOAD.
        const PAYLOAD: &[u8] = b"mowl update test payload\n";
        const TEST_PUB: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDYzRkE2Nzk1RUYyMzc5RDcKUldUWGVTUHZsV2Y2WTdxWVRoWmlsWEMrdjdxcDRSallZQlBJelo1VThPTGF2VjNseXhSQWI3WWUK";
        const TEST_SIG: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVUWGVTUHZsV2Y2WTVpUDdybTI5OWVoYmc0Z1JWcURhYldhUVJPQkp4am92cTZtZDVuTFZ5aXhlbmtsV0RVOUIwNWdpaUJ6RHJOdVN3MzVZdlZvSDZ3SWNGS2JBbmpWaEFFPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzg5OTkzNTg4CWZpbGU6cGF5bG9hZC5iaW4KSmFpLzdnU2tQZE9zRkR3Ny9HRXRxcFhkSHlYbXBnbFo0TTNvYmRXUWNJWW4wUkVqMVorTFNVT01pajNzUGl6ZXVPLzI4UGEvMzc0dGZVTWhDWWVpQnc9PQo=";

        #[test]
        fn accepts_a_genuine_signature() {
            assert!(verify(PAYLOAD, TEST_SIG, TEST_PUB).is_ok());
            // the trailing newline tauri writes must not matter
            assert!(verify(PAYLOAD, &format!("{TEST_SIG}\n"), TEST_PUB).is_ok());
        }

        #[test]
        fn rejects_tampered_data() {
            assert!(verify(b"mowl update test payload!\n", TEST_SIG, TEST_PUB).is_err());
        }

        #[test]
        fn rejects_a_signature_from_another_key() {
            // the real embedded key did not sign the throwaway payload
            assert!(verify(PAYLOAD, TEST_SIG, PUBLIC_KEY).is_err());
        }

        #[test]
        fn rejects_garbage() {
            assert!(verify(PAYLOAD, "not base64!", TEST_PUB).is_err());
            assert!(verify(PAYLOAD, TEST_SIG, "AAAA").is_err());
        }

        #[test]
        fn embedded_public_key_parses() {
            assert!(PublicKey::decode(&decode_text(PUBLIC_KEY, "key").unwrap()).is_ok());
        }

        #[test]
        fn classifies_install_locations() {
            let p = |s: &str| detect_mode(Path::new(s));
            assert_eq!(p(r"D:\src\Wowl\src-tauri\target\debug\wowl.exe"), Mode::Unsupported);
            assert_eq!(p(r"C:\Program Files\Wowl\wowl.exe"), Mode::Unsupported);
            assert_eq!(p(r"D:\Tools\Wowl\Wowl.exe"), Mode::Portable);
        }

        #[test]
        fn picks_the_right_asset() {
            if arch_label() == Some("x64") {
                assert_eq!(asset_names(Mode::Portable, "1.2.0").unwrap().0, "Wowl.exe");
                assert_eq!(asset_names(Mode::Installed, "1.2.0").unwrap().0, "Wowl_1.2.0_x64-setup.exe");
                assert_eq!(asset_names(Mode::Installed, "1.2.0").unwrap().1, "Wowl_1.2.0_x64-setup.exe.sig");
            }
            assert!(asset_names(Mode::Unsupported, "1.2.0").is_none());
        }

        #[test]
        fn compares_versions_semantically() {
            let v = |s: &str| parse_version(s).unwrap();
            assert!(v("v1.10.0") > v("1.9.9"));
            assert!(v("99.0.0") > current_version());
            assert!(parse_version("nightly").is_err());
        }

        #[test]
        fn swap_keeps_the_old_file_and_rolls_back_on_failure() {
            let dir = std::env::temp_dir().join(format!("wowl-swap-{}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            let cur = dir.join("Wowl.exe");
            let new = dir.join("Wowl.exe.new");
            fs::write(&cur, b"old").unwrap();
            fs::write(&new, b"new").unwrap();

            let old = swap_files(&cur, &new).unwrap();
            assert_eq!(fs::read(&cur).unwrap(), b"new");
            assert_eq!(fs::read(&old).unwrap(), b"old");
            assert!(!new.exists());

            // no staged file → nothing may be lost
            assert!(swap_files(&cur, &dir.join("missing")).is_err());
            assert_eq!(fs::read(&cur).unwrap(), b"new");
            let _ = fs::remove_dir_all(&dir);
        }
    }
}

#[cfg(windows)]
pub use imp::{check, cleanup_old, install, prepare};

/// Whether this build can check for and apply updates.
pub const fn supported() -> bool {
    cfg!(windows)
}

#[cfg(not(windows))]
mod other {
    use super::{PreparedUpdate, UpdateInfo};
    use reqwest::Client;
    use tauri::AppHandle;

    const NO: &str = "updates are only available on Windows";
    pub async fn check(_: &Client) -> Result<UpdateInfo, String> {
        Err(NO.into())
    }
    pub async fn prepare(_: &AppHandle, _: &Client, _: &str) -> Result<PreparedUpdate, String> {
        Err(NO.into())
    }
    pub fn install(_: &str) -> Result<(), String> {
        Err(NO.into())
    }
    pub fn cleanup_old() {}
}
#[cfg(not(windows))]
pub use other::{check, cleanup_old, install, prepare};

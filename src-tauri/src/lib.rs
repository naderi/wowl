mod config;
mod history;
mod providers;

use std::fs;
use std::sync::Mutex;

use base64::Engine;
use reqwest::Client;
use serde::Serialize;

use config::Settings;
use history::HistoryEntry;
use providers::{Photo, Provider};

const USER_AGENT: &str = concat!("Wowl/", env!("CARGO_PKG_VERSION"));

/// The image currently shown in the UI — kept so it can be set as wallpaper /
/// saved without re-downloading.
struct CurrentImage {
    photo: Photo,
    bytes: Vec<u8>,
    mime: String,
}

struct AppState {
    client: Client,
    current: Mutex<Option<CurrentImage>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PhotoResult {
    photo: Photo,
    /// Base64 `data:` URL of the downloaded full image.
    data_url: String,
    supports_search: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderInfo {
    id: &'static str,
    label: &'static str,
    supports_search: bool,
    needs_key: bool,
}

#[tauri::command]
fn load_settings() -> Settings {
    Settings::load()
}

#[tauri::command]
fn save_settings(settings: Settings) -> Result<(), String> {
    settings.save().map_err(|e| e.to_string())
}

#[tauri::command]
fn config_location() -> String {
    config::config_path().display().to_string()
}

#[tauri::command]
fn list_providers() -> Vec<ProviderInfo> {
    vec![
        ProviderInfo {
            id: "picsum",
            label: "Lorem Picsum",
            supports_search: false,
            needs_key: false,
        },
        ProviderInfo {
            id: "unsplash",
            label: "Unsplash",
            supports_search: true,
            needs_key: true,
        },
        ProviderInfo {
            id: "bing",
            label: "Bing",
            supports_search: false,
            needs_key: false,
        },
        ProviderInfo {
            id: "pixabay",
            label: "Pixabay",
            supports_search: true,
            needs_key: true,
        },
        ProviderInfo {
            id: "wallhaven",
            label: "Wallhaven",
            supports_search: true,
            needs_key: false,
        },
    ]
}

#[tauri::command]
async fn next_photo(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<PhotoResult, String> {
    let settings = Settings::load();
    let provider = Provider::from_id(&settings.provider);
    let target = target_resolution(&app);

    let photo = provider
        .fetch(&state.client, &settings, target)
        .await
        .map_err(|e| e.to_string())?;

    let (bytes, mime) = download_bytes(&state.client, &photo.image_url)
        .await
        .map_err(|e| e.to_string())?;

    // Unsplash ToS: ping the download endpoint once the image is fetched.
    if let Some(trigger) = &photo.download_trigger {
        let mut req = state.client.get(trigger);
        if provider == Provider::Unsplash && !settings.unsplash_key.trim().is_empty() {
            req = req.header(
                "Authorization",
                format!("Client-ID {}", settings.unsplash_key.trim()),
            );
        }
        let _ = req.send().await;
    }

    if settings.history_mode == "loaded" {
        if let Err(e) = history::add(&photo, &bytes, &mime, settings.history_size as usize) {
            eprintln!("wowl: could not add to history: {e}");
        }
    }

    let data_url = format!(
        "data:{};base64,{}",
        mime,
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    );

    *state.current.lock().unwrap() = Some(CurrentImage {
        photo: photo.clone(),
        bytes,
        mime,
    });

    Ok(PhotoResult {
        photo,
        data_url,
        supports_search: provider.supports_search(),
    })
}

#[tauri::command]
fn get_history() -> Vec<HistoryEntry> {
    history::list()
}

#[tauri::command]
fn history_image(id: String) -> Result<tauri::ipc::Response, String> {
    let path = history::file_path(&id).ok_or_else(|| "image not in history".to_string())?;
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// Load a history image, make it the active image, and return its bytes.
#[tauri::command]
fn select_history_image(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<tauri::ipc::Response, String> {
    let entry = history::list()
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| "image not in history".to_string())?;
    let path = config::history_dir().join(&entry.file);
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    let mime = format!(
        "image/{}",
        path.extension().and_then(|e| e.to_str()).unwrap_or("jpeg")
    );

    *state.current.lock().unwrap() = Some(CurrentImage {
        photo: Photo {
            id: entry.id,
            provider: entry.provider,
            width: 0,
            height: 0,
            photographer: entry.photographer,
            photographer_url: entry.photographer_url,
            source_url: entry.source_url,
            image_url: entry.image_url,
            download_trigger: None,
        },
        bytes: bytes.clone(),
        mime,
    });
    Ok(tauri::ipc::Response::new(bytes))
}

/// Mirror the active image left-to-right and return its new `data:` URL.
#[tauri::command]
fn flip_current(state: tauri::State<'_, AppState>) -> Result<String, String> {
    use image::ImageEncoder;

    let mut guard = state.current.lock().unwrap();
    let cur = guard.as_mut().ok_or_else(|| "no image loaded".to_string())?;

    let flipped = image::load_from_memory(&cur.bytes)
        .map_err(|e| e.to_string())?
        .fliph()
        .into_rgb8();
    let (w, h) = flipped.dimensions();

    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90)
        .write_image(flipped.as_raw(), w, h, image::ExtendedColorType::Rgb8)
        .map_err(|e| e.to_string())?;

    cur.bytes = out;
    cur.mime = "image/jpeg".into();
    Ok(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&cur.bytes)
    ))
}

/// When `history_mode == "applied"`, an image enters the history the moment it
/// is used as wallpaper / lock screen.
fn add_to_history_if_applied(cur: &CurrentImage) {
    let settings = Settings::load();
    if settings.history_mode == "applied" {
        if let Err(e) = history::add(
            &cur.photo,
            &cur.bytes,
            &cur.mime,
            settings.history_size as usize,
        ) {
            eprintln!("wowl: could not add to history: {e}");
        }
    }
}

#[tauri::command]
fn set_wallpaper(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let guard = state.current.lock().unwrap();
    let cur = guard.as_ref().ok_or_else(|| "no image loaded".to_string())?;

    let dir = config::cache_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // Fresh file name per image so the OS doesn't reuse a cached wallpaper.
    let name = format!(
        "wp-{}.{}",
        history::sanitize(&cur.photo.id),
        history::ext_for(&cur.mime)
    );
    for old in fs::read_dir(&dir).into_iter().flatten().flatten() {
        let p = old.path();
        match p.file_name().and_then(|n| n.to_str()) {
            Some(n) if n.starts_with("wp-") && n != name => {
                let _ = fs::remove_file(&p);
            }
            _ => {}
        }
    }
    let path = dir.join(&name);
    fs::write(&path, &cur.bytes).map_err(|e| e.to_string())?;

    let _ = wallpaper::set_mode(wallpaper::Mode::Crop);
    wallpaper::set_from_path(path.to_str().ok_or_else(|| "invalid path".to_string())?)
        .map_err(|e| e.to_string())?;

    add_to_history_if_applied(cur);
    Ok(())
}

/// Set the Windows lock-screen image via the PersonalizationCSP registry keys.
/// Needs one elevation (UAC) and Windows Pro/Enterprise.
#[cfg(windows)]
#[tauri::command]
fn set_lockscreen(state: tauri::State<'_, AppState>) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    let guard = state.current.lock().unwrap();
    let cur = guard.as_ref().ok_or_else(|| "no image loaded".to_string())?;

    let program_data =
        std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".to_string());
    let dir = std::path::Path::new(&program_data).join("Wowl");
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;

    let img_path = dir.join("lockscreen.jpg");
    fs::write(&img_path, &cur.bytes).map_err(|e| e.to_string())?;

    // .reg file with escaped backslashes.
    let img_escaped = img_path.to_string_lossy().replace('\\', "\\\\");
    let reg_path = dir.join("lockscreen.reg");
    let reg_body = format!(
        "Windows Registry Editor Version 5.00\r\n\r\n\
[HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\PersonalizationCSP]\r\n\
\"LockScreenImagePath\"=\"{img}\"\r\n\
\"LockScreenImageUrl\"=\"{img}\"\r\n\
\"LockScreenImageStatus\"=dword:00000001\r\n",
        img = img_escaped
    );
    fs::write(&reg_path, reg_body).map_err(|e| e.to_string())?;

    // Import the .reg elevated — a single UAC prompt.
    let ps = format!(
        "try {{ $p = Start-Process reg -Verb RunAs -WindowStyle Hidden -PassThru -Wait \
         -ArgumentList 'import','\"{}\"'; exit $p.ExitCode }} catch {{ exit 1223 }}",
        reg_path.display()
    );
    let status = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &ps])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .status()
        .map_err(|e| e.to_string())?;

    match status.code() {
        Some(0) => {
            add_to_history_if_applied(cur);
            Ok(())
        }
        Some(1223) => Err("the elevation prompt was cancelled".to_string()),
        _ => Err("the registry import failed (Windows Pro/Enterprise required)".to_string()),
    }
}

#[cfg(not(windows))]
#[tauri::command]
fn set_lockscreen(_state: tauri::State<'_, AppState>) -> Result<(), String> {
    Err("Setting the lock screen is only supported on Windows".to_string())
}

/// Remove the PersonalizationCSP keys so the Windows lock-screen setting applies
/// again. One UAC prompt.
#[cfg(windows)]
#[tauri::command]
fn reset_lockscreen() -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    let ps = "try { $p = Start-Process reg -Verb RunAs -WindowStyle Hidden -PassThru -Wait \
        -ArgumentList 'delete','\"HKLM\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\PersonalizationCSP\"','/f'; \
        exit $p.ExitCode } catch { exit 1223 }";
    let status = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", ps])
        .creation_flags(0x0800_0000)
        .status()
        .map_err(|e| e.to_string())?;

    match status.code() {
        // 0 = deleted, 1 = key was not there (already reset)
        Some(0) | Some(1) => Ok(()),
        Some(1223) => Err("the elevation prompt was cancelled".to_string()),
        _ => Err("could not reset the lock screen".to_string()),
    }
}

#[cfg(not(windows))]
#[tauri::command]
fn reset_lockscreen() -> Result<(), String> {
    Err("Only supported on Windows".to_string())
}

/// Write the active image to `path`, remember the folder, return that folder.
#[tauri::command]
fn save_current_image(path: String, state: tauri::State<'_, AppState>) -> Result<String, String> {
    let guard = state.current.lock().unwrap();
    let cur = guard.as_ref().ok_or_else(|| "no image loaded".to_string())?;

    let path = std::path::PathBuf::from(&path);
    fs::write(&path, &cur.bytes).map_err(|e| e.to_string())?;

    let dir = path
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut settings = Settings::load();
    settings.last_save_dir = dir.clone();
    settings.save().map_err(|e| e.to_string())?;
    Ok(dir)
}

#[tauri::command]
fn delete_history(ids: Vec<String>) -> Result<Vec<HistoryEntry>, String> {
    history::delete(&ids).map_err(|e| e.to_string())?;
    Ok(history::list())
}

#[tauri::command]
fn clear_history() -> Result<(), String> {
    history::clear().map_err(|e| e.to_string())
}

fn target_resolution(app: &tauri::AppHandle) -> (u32, u32) {
    if let Ok(Some(monitor)) = app.primary_monitor() {
        let size = monitor.size();
        if size.width > 0 && size.height > 0 {
            return (size.width, size.height);
        }
    }
    (2560, 1440)
}

async fn download_bytes(client: &Client, url: &str) -> anyhow::Result<(Vec<u8>, String)> {
    let resp = client.get(url).send().await?.error_for_status()?;
    let mime = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).trim().to_string())
        .filter(|s| s.starts_with("image/"))
        .unwrap_or_else(|| "image/jpeg".to_string());
    let bytes = resp.bytes().await?.to_vec();
    Ok((bytes, mime))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let client = Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .expect("failed to build HTTP client");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            client,
            current: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            load_settings,
            save_settings,
            config_location,
            list_providers,
            next_photo,
            get_history,
            history_image,
            select_history_image,
            delete_history,
            clear_history,
            set_wallpaper,
            set_lockscreen,
            reset_lockscreen,
            save_current_image,
            flip_current,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

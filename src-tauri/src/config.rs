use std::fs;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

/// Persisted user settings. Stored as `wowl.toml` next to the executable when
/// that directory is writable (portable mode), otherwise in the OS config dir.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    /// Active image provider id: `picsum` | `unsplash` | `bing`.
    pub provider: String,
    /// Raw search-term text as typed by the user, separated by `,` or `;`.
    pub search_terms: String,
    /// How many images to keep cached in the history folder.
    pub history_size: u32,
    /// When an image enters the history: `loaded` (every image shown) or
    /// `applied` (only images set as wallpaper / lock screen).
    pub history_mode: String,
    /// UI theme: `system` | `light` | `dark`.
    pub theme: String,
    /// UI language: `de` | `en`.
    pub language: String,
    /// User-supplied Unsplash access key.
    pub unsplash_key: String,
    /// Last folder used in the "save as" dialog.
    pub last_save_dir: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            provider: "picsum".into(),
            search_terms: String::new(),
            history_size: 20,
            history_mode: "loaded".into(),
            theme: "system".into(),
            language: "en".into(),
            unsplash_key: String::new(),
            last_save_dir: String::new(),
        }
    }
}

impl Settings {
    /// Split the raw search-term text into trimmed, non-empty terms.
    pub fn terms(&self) -> Vec<String> {
        self.search_terms
            .split(|c| c == ',' || c == ';')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect()
    }

    pub fn load() -> Self {
        let path = config_path();
        match fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                eprintln!("wowl: invalid config at {}: {e}", path.display());
                Settings::default()
            }),
            Err(_) => Settings::default(),
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)?;
        fs::write(&path, text)?;
        Ok(())
    }
}

/// Base directory for all persisted data (config file + history cache).
pub fn base_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if dir_writable(dir) || dir.join("wowl.toml").exists() {
                return dir.to_path_buf();
            }
        }
    }
    if let Some(pd) = ProjectDirs::from("com", "alinaderi", "Wowl") {
        let dir = pd.config_dir().to_path_buf();
        let _ = fs::create_dir_all(&dir);
        return dir;
    }
    PathBuf::from(".")
}

pub fn config_path() -> PathBuf {
    base_dir().join("wowl.toml")
}

pub fn history_dir() -> PathBuf {
    base_dir().join("history")
}

/// Scratch directory for files handed to the OS (e.g. the wallpaper image).
pub fn cache_dir() -> PathBuf {
    base_dir().join("cache")
}

fn dir_writable(dir: &Path) -> bool {
    let probe = dir.join(".wowl-write-test");
    match fs::write(&probe, b"") {
        Ok(_) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::history_dir;
use crate::providers::Photo;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub provider: String,
    pub photographer: String,
    pub photographer_url: Option<String>,
    pub source_url: Option<String>,
    pub image_url: String,
    /// File name within the history directory.
    pub file: String,
    /// RFC 3339 timestamp of when the image was added.
    pub saved_at: String,
}

fn index_path() -> PathBuf {
    history_dir().join("index.json")
}

/// Newest first.
pub fn list() -> Vec<HistoryEntry> {
    match fs::read_to_string(index_path()) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Absolute path of the cached image file for a history id, if it exists.
pub fn file_path(id: &str) -> Option<PathBuf> {
    list()
        .into_iter()
        .find(|e| e.id == id)
        .map(|e| history_dir().join(e.file))
}

fn save_index(entries: &[HistoryEntry]) -> anyhow::Result<()> {
    fs::create_dir_all(history_dir())?;
    fs::write(index_path(), serde_json::to_string_pretty(entries)?)?;
    Ok(())
}

pub fn sanitize(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

pub fn ext_for(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/avif" => "avif",
        _ => "jpg",
    }
}

/// Add an image to the history, then prune to `max` entries (FIFO). `max == 0`
/// disables the history entirely.
pub fn add(photo: &Photo, bytes: &[u8], mime: &str, max: usize) -> anyhow::Result<()> {
    let dir = history_dir();
    let mut entries = list();

    // Never store an image that is already in the history.
    if entries.iter().any(|e| e.id == photo.id) {
        return Ok(());
    }
    if max == 0 {
        return Ok(());
    }

    fs::create_dir_all(&dir)?;
    let file = format!("{}.{}", sanitize(&photo.id), ext_for(mime));
    fs::write(dir.join(&file), bytes)?;

    entries.insert(
        0,
        HistoryEntry {
            id: photo.id.clone(),
            provider: photo.provider.clone(),
            photographer: photo.photographer.clone(),
            photographer_url: photo.photographer_url.clone(),
            source_url: photo.source_url.clone(),
            image_url: photo.image_url.clone(),
            file,
            saved_at: chrono::Utc::now().to_rfc3339(),
        },
    );

    if entries.len() > max {
        for stale in entries.drain(max..) {
            let _ = fs::remove_file(dir.join(&stale.file));
        }
    }

    save_index(&entries)?;
    Ok(())
}

/// Remove the given ids from the history (and delete their cached files).
pub fn delete(ids: &[String]) -> anyhow::Result<()> {
    let dir = history_dir();
    let set: std::collections::HashSet<&str> = ids.iter().map(String::as_str).collect();
    let mut entries = list();
    entries.retain(|e| {
        if set.contains(e.id.as_str()) {
            let _ = fs::remove_file(dir.join(&e.file));
            false
        } else {
            true
        }
    });
    save_index(&entries)?;
    Ok(())
}

pub fn clear() -> anyhow::Result<()> {
    let dir = history_dir();
    for e in list() {
        let _ = fs::remove_file(dir.join(&e.file));
    }
    let _ = fs::remove_file(index_path());
    Ok(())
}

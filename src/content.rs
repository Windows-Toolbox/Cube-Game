//! The game's data files (`content\` in this repo: ships, weapons, enemies, levels, …) – numbers the game reads
//! instead of having them in the code, so they can be changed without compiling.
//!
//! On the PC the game was built on, the files are read from the repo's `content\` folder every time the game menu
//! opens: change, save, open the game menu, try it. Everywhere else (a release, a copy) the files compiled into the
//! exe are used. Errors in a file are shown in red in the game menu; the game then uses the built-in value.

use std::path::PathBuf;

include!(concat!(env!("OUT_DIR"), "/content_files.rs"));

/// The repo's `content\` folder, if the game runs on the PC it was built on.
fn folder() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("content");
    if cfg!(test) {
        return None; // tests use what was compiled in, so they do not depend on half-edited files
    }
    dir.is_dir().then_some(dir)
}

/// The text of a data file, e.g. `"ships.json"` or `"difficulties/a1.json"`.
pub fn read(name: &str) -> Result<String, String> {
    if let Some(dir) = folder() {
        return std::fs::read_to_string(dir.join(name)).map_err(|e| e.to_string());
    }
    FILES.iter().find(|(n, _)| *n == name).map(|(_, text)| text.to_string()).ok_or_else(|| "no such file".to_string())
}

/// The file names in a folder of `content\` (e.g. `"difficulties"`) that end in `.json`, sorted.
pub fn list(folder_name: &str) -> Vec<String> {
    let mut names: Vec<String> = if let Some(dir) = folder() {
        std::fs::read_dir(dir.join(folder_name))
            .map(|d| d.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.ends_with(".json")).collect())
            .unwrap_or_default()
    } else {
        let prefix = format!("{folder_name}/");
        FILES.iter().filter_map(|(n, _)| n.strip_prefix(&prefix)).filter(|n| n.ends_with(".json") && !n.contains('/')).map(String::from).collect()
    };
    names.sort();
    names
}

/// A JSON data file, read: its top level must be `{ … }`. The error is the text the game menu shows.
pub fn read_object(name: &str) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let text = read(name)?;
    match serde_json::from_str::<serde_json::Value>(text.trim_start_matches('\u{feff}')) {
        Ok(serde_json::Value::Object(map)) => Ok(map),
        Ok(_) => Err("should be { … }".to_string()),
        Err(e) => Err(e.to_string()),
    }
}

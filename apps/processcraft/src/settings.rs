//! Per-user workbench preferences (window size, panels, theme, recent files).
//! Stored as JSON next to the recovery folder; a missing or unreadable file means defaults.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const MAX_RECENT: usize = 8;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub window: Option<[f32; 2]>,
    pub left_width: Option<f32>,
    pub right_width: Option<f32>,
    pub show_left: Option<bool>,
    pub show_right: Option<bool>,
    pub dark: bool,
    pub recent: Vec<PathBuf>,
}

impl Settings {
    fn location() -> PathBuf {
        #[cfg(test)]
        {
            std::env::temp_dir().join(format!(
                "processcraft-unit-settings-{}.json",
                std::process::id()
            ))
        }
        #[cfg(not(test))]
        {
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
                .join("ProcessCraft")
                .join("settings.json")
        }
    }
    pub fn load() -> Self {
        std::fs::read(Self::location())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }
    /// Best effort: preferences are a convenience, never a reason to interrupt the user.
    pub fn save(&self) {
        let path = Self::location();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(bytes) = serde_json::to_vec_pretty(self) {
            let temp = path.with_extension("json.tmp");
            if std::fs::write(&temp, bytes).is_ok() {
                let _ = std::fs::rename(&temp, &path);
            }
        }
    }
    /// Moves `path` to the front of the recent list.
    pub fn remember(&mut self, path: &Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(MAX_RECENT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recent_files_are_unique_newest_first_and_bounded() {
        let mut settings = Settings::default();
        for i in 0..10 {
            settings.remember(Path::new(&format!("{i}.processcraft")));
        }
        settings.remember(Path::new("5.processcraft"));
        assert_eq!(settings.recent.len(), MAX_RECENT);
        assert_eq!(settings.recent[0], Path::new("5.processcraft"));
        assert_eq!(
            settings
                .recent
                .iter()
                .filter(|p| p.ends_with("5.processcraft"))
                .count(),
            1
        );
        let json = serde_json::to_vec(&settings).unwrap();
        assert_eq!(serde_json::from_slice::<Settings>(&json).unwrap(), settings);
        assert_eq!(
            serde_json::from_slice::<Settings>(b"{}").unwrap(),
            Settings::default()
        );
    }
}

//! What the explorer and `rs2ool cache caption` remember between runs:
//! `settings.json` in your platform's config directory. Both read and
//! write the same file, so a change made in one applies to the other.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The saved settings. A key the file doesn't have takes its default, so
/// an older file still loads.
#[cfg_attr(feature = "explorer", derive(bevy::prelude::Resource))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Where the explorer's actions write; `None` until you choose one.
    pub out_dir: Option<PathBuf>,
    /// Captioning sequences with a vision model.
    pub caption: Caption,
}

/// How to reach the vision model that captions sequences. Off by default:
/// nothing is contacted until `enabled` is set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Caption {
    /// Whether captioning may contact `endpoint` at all.
    pub enabled: bool,
    /// Which API the server speaks.
    pub backend: Backend,
    /// The server's base URL, without the API path.
    pub endpoint: String,
    /// The model to ask, as the server names it. Empty until you choose
    /// one.
    pub model: String,
    /// The environment variable holding an API key, if the server wants
    /// one. The key itself is never stored.
    pub api_key_env: Option<String>,
    /// How long to wait for one reply, in seconds.
    pub timeout_secs: u64,
    /// How many frames of a sequence each image shows.
    pub frames: u32,
    /// The question asked with each image, or `None` for the built-in one.
    /// `{classes}` is replaced with the class list.
    pub prompt: Option<String>,
}

impl Default for Caption {
    fn default() -> Self {
        Self {
            enabled: false,
            backend: Backend::Ollama,
            endpoint: "http://127.0.0.1:11434".into(),
            model: String::new(),
            api_key_env: None,
            timeout_secs: 120,
            frames: 6,
            prompt: None,
        }
    }
}

/// The API a model server speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    /// Ollama's own API (`/api/chat`).
    Ollama,
    /// The OpenAI-compatible chat API (`/v1/chat/completions`), as served
    /// by llama.cpp's server, LM Studio, vLLM and Ollama too.
    #[value(name = "openai")]
    #[serde(rename = "openai")]
    OpenAi,
}

impl Backend {
    /// Its name in the settings file and on the command line.
    pub fn name(self) -> &'static str {
        match self {
            Backend::Ollama => "ollama",
            Backend::OpenAi => "openai",
        }
    }
}

fn dirs() -> Option<directories::ProjectDirs> {
    directories::ProjectDirs::from("", "", "rs2ool")
}

/// `settings.json` in the platform config directory, or `None` if the
/// platform doesn't have one.
pub fn settings_path() -> Option<PathBuf> {
    dirs().map(|d| d.config_dir().join("settings.json"))
}

/// The platform data directory for files the tools keep, such as
/// captions, or `None` if the platform doesn't have one.
pub fn data_dir() -> Option<PathBuf> {
    dirs().map(|d| d.data_dir().to_path_buf())
}

impl Settings {
    /// Reads the settings, falling back to the defaults if the file's
    /// missing or won't parse.
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// Reads the settings from [`settings_path`], or the defaults.
    pub fn load_default() -> Self {
        settings_path().map(|p| Self::load(&p)).unwrap_or_default()
    }

    /// Writes the settings as pretty JSON, making the directory if needed.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let dir = std::env::temp_dir().join(format!("rs2ool-settings-{}", std::process::id()));
        let p = dir.join("settings.json");
        let s = Settings {
            out_dir: Some(PathBuf::from("/tmp/out")),
            caption: Caption {
                enabled: true,
                backend: Backend::OpenAi,
                model: "m".into(),
                ..Caption::default()
            },
        };
        s.save(&p).unwrap();
        assert_eq!(Settings::load(&p), s);
        assert_eq!(
            Settings::load(&dir.join("missing.json")),
            Settings::default()
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn older_files_still_load() {
        let s: Settings = serde_json::from_str(r#"{"out_dir": "/o"}"#).unwrap();
        assert_eq!(s.out_dir, Some(PathBuf::from("/o")));
        assert_eq!(s.caption, Caption::default());
        assert!(!s.caption.enabled);
    }
}

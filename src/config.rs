use serde::Deserialize;

/// Configuration loaded from `marcterm.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Shell binary to launch (e.g. "bash", "zsh", "/bin/fish").
    #[serde(default = "default_shell")]
    pub shell: String,

    /// Initial font size in logical pixels.
    #[serde(default = "default_font_size")]
    pub font_size: f32,
}

fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "bash".to_string())
}

fn default_font_size() -> f32 {
    14.0
}

impl Default for Config {
    fn default() -> Self {
        Self {
            shell: default_shell(),
            font_size: default_font_size(),
        }
    }
}

impl Config {
    pub fn path() -> std::path::PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("marcterm.toml")
    }

    /// Load config from [`Config::path`], falling back to defaults on any error.
    pub fn load() -> Self {
        let contents = std::fs::read_to_string(Self::path()).unwrap_or_default();
        toml::from_str(&contents).unwrap_or_default()
    }
}

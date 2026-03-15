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
        // Inside Flatpak, XDG_CONFIG_HOME points to ~/.var/app/<id>/config/
        // but we want the host's ~/.config/ since marcterm integrates with the host.
        if std::env::var("FLATPAK_ID").is_ok() {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            return std::path::PathBuf::from(home)
                .join(".config")
                .join("marcterm.toml");
        }
        dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("marcterm.toml")
    }

    /// Load config from [`Config::path`], falling back to defaults on any error.
    pub fn load() -> Self {
        let path = Self::path();
        let contents = match std::fs::read_to_string(&path) {
            Ok(c) => {
                eprintln!("Loaded config from {}", path.display());
                c
            }
            Err(e) => {
                eprintln!("Could not read config from {}: {e}", path.display());
                return Self::default();
            }
        };
        match toml::from_str(&contents) {
            Ok(config) => config,
            Err(e) => {
                eprintln!("Failed to parse {}: {e}", path.display());
                Self::default()
            }
        }
    }
}

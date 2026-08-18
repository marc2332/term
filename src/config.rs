use serde::Deserialize;

use crate::git::is_flatpak;

/// What to show when the app starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Startup {
    /// Welcome screen with recent projects and sessions.
    #[default]
    Welcome,
    /// Restore the most recent session directly.
    RestoreLast,
    /// A single plain terminal, like before projects existed.
    Fresh,
}

/// Configuration loaded from `marcterm.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Shell binary to launch (e.g. "bash", "zsh", "/bin/fish").
    #[serde(default = "default_shell")]
    pub shell: String,

    /// Initial font size in logical pixels.
    #[serde(default = "default_font_size")]
    pub font_size: f32,

    /// Font family used by the terminal. Uses freya's default when not set.
    #[serde(default)]
    pub font_family: Option<String>,

    /// What to show on launch: "welcome", "restore-last" or "fresh".
    #[serde(default)]
    pub startup: Startup,
}

fn default_shell() -> String {
    let shell = if is_flatpak() {
        host_login_shell()
    } else {
        std::env::var("SHELL")
            .ok()
            .filter(|shell| !shell.is_empty())
            .or_else(login_shell)
    };
    shell.unwrap_or_else(|| "bash".to_string())
}

/// The host's login shell of record, as the sandbox misreports `/bin/sh` in `$SHELL` and passwd.
fn host_login_shell() -> Option<String> {
    let user = std::env::var("USER").ok()?;
    let output = std::process::Command::new("flatpak-spawn")
        .args(["--host", "getent", "passwd", &user])
        .output()
        .ok()?;
    let passwd = String::from_utf8(output.stdout).ok()?;
    let shell = passwd.trim().rsplit(':').next()?;
    (!shell.is_empty()).then(|| shell.to_string())
}

/// The user's login shell.
fn login_shell() -> Option<String> {
    let passwd = unsafe { libc::getpwuid(libc::getuid()) };
    if passwd.is_null() {
        return None;
    }
    let shell = unsafe { std::ffi::CStr::from_ptr((*passwd).pw_shell) };
    let shell = shell.to_str().ok()?;
    (!shell.is_empty()).then(|| shell.to_string())
}

fn default_font_size() -> f32 {
    14.0
}

impl Default for Config {
    fn default() -> Self {
        Self {
            shell: default_shell(),
            font_size: default_font_size(),
            font_family: None,
            startup: Startup::default(),
        }
    }
}

impl Config {
    pub fn path() -> std::path::PathBuf {
        // Use the host's ~/.config, not the Flatpak sandbox's XDG_CONFIG_HOME.
        if is_flatpak() {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            return std::path::PathBuf::from(home)
                .join(".config")
                .join("marcterm.toml");
        }
        dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("marcterm.toml")
    }

    /// Path to the config file, creating it empty if it does not exist yet.
    pub fn ensure_path() -> std::io::Result<std::path::PathBuf> {
        let path = Self::path();
        if !path.exists() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, "")?;
        }
        Ok(path)
    }

    /// Load config from [`Config::path`], falling back to defaults on any error.
    pub fn load() -> Self {
        let path = Self::path();
        let contents = match std::fs::read_to_string(&path) {
            Ok(c) => {
                tracing::info!("Loaded config from {}", path.display());
                c
            }
            Err(e) => {
                tracing::warn!("Could not read config from {}: {e}", path.display());
                return Self::default();
            }
        };
        match toml::from_str(&contents) {
            Ok(config) => config,
            Err(e) => {
                tracing::error!("Failed to parse {}: {e}", path.display());
                Self::default()
            }
        }
    }
}

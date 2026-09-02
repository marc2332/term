mod components;
mod config;
mod git;
mod logging;
mod session;
mod shortcuts;
mod state;

use clap::Parser;
use components::app::App;
use config::Config;
use freya::borderless::BorderlessPlugin;
use freya::prelude::*;

/// Fall back to Adwaita when the host cursor theme isn't reachable in Flatpak.
#[cfg(target_os = "linux")]
fn fix_flatpak_cursor_theme() {
    if !crate::git::is_flatpak() {
        return;
    }

    let theme_name = std::env::var("XCURSOR_THEME").unwrap_or_else(|_| "default".into());

    if xcursor::CursorTheme::load(&theme_name)
        .load_icon("left_ptr")
        .is_none()
    {
        // Safe as no other thread has been spawned yet.
        unsafe {
            std::env::set_var("XCURSOR_THEME", "Adwaita");
        }
    }

    if std::env::var("XCURSOR_SIZE").is_err() {
        // Safe as no other thread has been spawned yet.
        unsafe {
            std::env::set_var("XCURSOR_SIZE", "24");
        }
    }
}

#[derive(Parser)]
#[command(name = "marcterm", about, version)]
struct Cli {
    /// Enable performance overlay
    #[arg(long)]
    fps: bool,

    /// Open the terminal in this directory
    directory: Option<std::path::PathBuf>,
}

fn main() {
    logging::init();

    #[cfg(target_os = "linux")]
    fix_flatpak_cursor_theme();

    let cli = Cli::parse();
    let config = Config::load();

    let startup_dir = cli.directory.filter(|dir| {
        let exists = dir.is_dir();
        if !exists {
            tracing::warn!("Ignoring {}: not an existing directory", dir.display());
        }
        exists
    });

    let mut launch_config = LaunchConfig::new()
        .with_plugin(BorderlessPlugin::new().with_corner_radius(12.))
        .with_window(
            WindowConfig::new(move || App {
                font_size: config.font_size,
                font_family: config.font_family.clone(),
                shell: config.shell.clone(),
                startup: config.startup,
                startup_dir: startup_dir.clone(),
            })
            .with_title("marcterm")
            .with_decorations(false)
            .with_transparency(true)
            .with_background(Color::TRANSPARENT)
            .with_app_id("io.marc.term")
            .with_size(1024., 768.)
            .with_min_size(400., 250.)
            .with_icon(LaunchConfig::window_icon(include_bytes!("../icon.png"))),
        );

    if cli.fps {
        launch_config = launch_config
            .with_plugin(freya_performance_plugin::PerformanceOverlayPlugin::default());
    }

    launch(launch_config);
}

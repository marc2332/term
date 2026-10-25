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
        launch_config = launch_config.with_plugin(
            freya_metrics_plugin::MetricsPlugin::default().with_visible_performance(true),
        );
    }

    launch(launch_config);
}

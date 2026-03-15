mod components;
mod config;
mod state;

use clap::Parser;
use components::app::App;
use config::Config;
use freya::prelude::*;

#[derive(Parser)]
#[command(name = "marcterm", about, version)]
struct Cli {
    /// Enable performance overlay
    #[arg(long)]
    fps: bool,
}

fn main() {
    let cli = Cli::parse();
    let config = Config::load();

    let mut launch_config = LaunchConfig::new().with_window(
        WindowConfig::new(move || App {
            font_size: config.font_size,
            shell: config.shell.clone(),
        })
        .with_title("marcterm")
        .with_size(1024., 768.)
        .with_icon(LaunchConfig::window_icon(include_bytes!("../icon.png"))),
    );

    if cli.fps {
        launch_config = launch_config
            .with_plugin(freya_performance_plugin::PerformanceOverlayPlugin::default());
    }

    launch(launch_config);
}

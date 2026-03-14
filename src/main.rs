mod components;
mod config;
mod state;

use components::app::App;
use config::Config;
use freya::prelude::*;

fn main() {
    let config = Config::load();

    launch(
        LaunchConfig::new().with_window(
            WindowConfig::new(move || App {
                font_size: config.font_size,
                shell: config.shell.clone(),
            })
            .with_title("marcterm")
            .with_size(1024., 768.)
            .with_icon(LaunchConfig::window_icon(include_bytes!("../icon.png"))),
        ),
    );
}

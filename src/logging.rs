use std::{
    fs::File,
    sync::Mutex,
};

use tracing_subscriber::EnvFilter;

use crate::session::state_dir;

/// Print logs to the terminal on debug builds, write them to the state dir on release ones.
pub fn init() {
    let filter = EnvFilter::try_from_env("MARCTERM_LOG")
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new("marcterm=info,freya_terminal=info"));

    if cfg!(debug_assertions) {
        tracing_subscriber::fmt().with_env_filter(filter).init();
        return;
    }

    let dir = state_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }

    let path = dir.join("marcterm.log");
    let _ = std::fs::rename(&path, dir.join("marcterm.log.old"));
    let Ok(file) = File::create(&path) else {
        return;
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_writer(Mutex::new(file))
        .init();
}

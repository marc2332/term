use std::process::{Command, Stdio};
use std::sync::OnceLock;

pub fn is_flatpak() -> bool {
    std::env::var("FLATPAK_ID").is_ok()
}

/// Whether the host can wrap flatpak-spawned shells in transient systemd scopes, probed once.
pub fn host_has_systemd_run() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        Command::new("flatpak-spawn")
            .args(["--host", "systemd-run", "--version"])
            .stdin(Stdio::null())
            .output()
            .is_ok_and(|out| out.status.success())
    })
}

f:
    taplo fmt
    cargo +nightly-2026-03-15 fmt --all -- --error-on-unformatted --unstable-features

# Build a native Debian package
deb:
    cargo deb

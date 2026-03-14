# marcterm 🖥️

Terminal emulator built with [Freya](https://github.com/marc2332/freya) and Rust 🦀.

---

![marcterm screenshot](demo.png)

---

## 📦 Installation

### Flatpak (Linux)

```sh
flatpak remote-add --if-not-exists marcterm https://marc2332.github.io/marcterm --no-gpg-verify
flatpak install marcterm io.marc.term
```

### Cargo

```sh
cargo install marcterm
```

## ✨ Features

- 🗂️ **Tabs** — open and manage multiple terminal sessions
- ➗ **Panel splitting** — split any panel horizontally, vertically, or into a 2x2 grid
- ↔️ **Resizable panes** — drag to resize split panels
- 🔡 **Adjustable font size** — change at runtime with a keyboard shortcut

## ⌨️ Keybindings

### Tabs

| Shortcut | Action |
|---|---|
| `Ctrl+Shift+T` | New tab |
| `Ctrl+Shift+W` | Close active tab |
| `Ctrl+Tab` | Next tab |
| `Ctrl+Shift+Tab` | Previous tab |

### Panels

| Shortcut | Action |
|---|---|
| `Alt+P` | Split panel vertically (top/bottom) |
| `Alt++ / Alt+=` | Split panel horizontally (left/right) |
| `Alt+4` | Split panel into 2x2 grid |
| `Alt+-` | Close active panel |
| `Alt+1` | Close all panels except active |
| `Alt+←` | Focus panel to the left |
| `Alt+→` | Focus panel to the right |
| `Alt+↑` | Focus panel above |
| `Alt+↓` | Focus panel below |

### General

| Shortcut | Action |
|---|---|
| `Ctrl++ / Ctrl+=` | Increase font size |
| `Ctrl+-` | Decrease font size |
| `Ctrl+Shift+C` | Copy selected text |
| `Ctrl+Shift+V` | Paste from clipboard |

## ⚙️ Configuration

marcterm reads its config from `~/.config/marcterm.toml`.

```toml
# Shell binary to launch.
shell = "bash"

# Font size in logical pixels.
font_size = 14.0
```

Copy the bundled `marcterm.demo.toml` as a starting point:

```sh
cp marcterm.demo.toml ~/.config/marcterm.toml
```

## 🔨 Building from source

```sh
cargo build --release
```

The compiled binary will be at `target/release/marcterm`.

## 📄 License

This project is open source. See [LICENSE](LICENSE) for details.

# rambo

A lightweight Rust daemon that monitors RAM usage and automatically kills a
configurable list of applications when memory gets critically low — before the
system freezes.

Works on **KDE Plasma** and **GNOME** (any desktop with a StatusNotifierItem
system tray and libnotify notifications).

## How it works

1. Polls free RAM every few seconds
2. When free RAM drops below the threshold (default: 500 MB), it finds the
   first running app from your priority kill list
3. Sends a desktop notification: *"Will kill Firefox in 30s — [Don't Kill]"*
4. If you click **Don't Kill**, it snoozes for 5 minutes
5. If the countdown expires, it kills the process and notifies you
6. A system tray icon shows live free RAM (colour-coded bar gauge) and lets you
   open the config or change the threshold on the fly

## Requirements

- Linux (KDE or GNOME)
- Rust toolchain (`curl https://sh.rustup.rs | sh`)
- `libdbus-1-dev` (for system tray / notifications)

```bash
sudo apt install libdbus-1-dev   # Debian/Ubuntu
sudo dnf install dbus-devel       # Fedora
```

## Install

```bash
git clone https://github.com/yourname/rambo
cd rambo
./install.sh
```

This builds the binary, installs it to `~/.local/bin/`, and registers a
**systemd user service** that auto-starts on login.

## Uninstall

```bash
./uninstall.sh
```

## Configuration

The config file is created automatically on first run:

```
~/.config/rambo/config.toml
```

You can also open it via the system tray icon → **Open Config…**

The tray icon also has a **Set Threshold** submenu with presets (256 / 512 / 1024 / 2048 / 4096 MB).
Selecting a preset updates and saves the config immediately — no restart required.

### Kill Priority

The tray **Kill Priority…** submenu lists all configured apps in order. For each app you can:
- **Move Up ↑** / **Move Down ↓** — reorder without editing the file
- **Enable / Disable** — temporarily exclude an app without removing it

Changes are saved to `config.toml` immediately.

### Example config

```toml
threshold_mb = 500
countdown_seconds = 30
check_interval_seconds = 5
snooze_seconds = 300

[[killable_apps]]
name = "slack"
display_name = "Slack"
enabled = true

[[killable_apps]]
name = "discord"
display_name = "Discord"
enabled = true

[[killable_apps]]
name = "firefox"
display_name = "Firefox"
enabled = true
```

| Field | Description |
|-------|-------------|
| `threshold_mb` | Kill when free RAM is below this (MB) |
| `countdown_seconds` | Grace period before killing |
| `check_interval_seconds` | How often to check RAM |
| `snooze_seconds` | Snooze duration after "Don't Kill" |
| `killable_apps` | Ordered list — first enabled running match is targeted |

The `name` field does a **case-insensitive substring match** on the process
name, so `"firefox"` matches `firefox`, `firefox-bin`, etc.

## CLI flags

All flags override the config file value for that run only:

```
rambo [OPTIONS]

Options:
  -t, --threshold <MB>    Kill when free RAM drops below this (default: from config)
  -c, --countdown <SECS>  Grace period before killing (default: from config)
  -i, --interval <SECS>   Memory poll interval (default: from config)
  -h, --help              Print help
  -V, --version           Print version
```

**Testing example** — trigger immediately with a high threshold and short countdown:

```bash
rambo --threshold 90000 --countdown 5 --interval 2
```

## Tray icon

The icon is a live RAM bar gauge:

| Colour | Meaning |
|--------|---------|
| 🟢 Green | Free RAM is healthy (> 2× threshold) |
| 🟡 Amber | Free RAM is low (between 1× and 2× threshold) |
| 🔴 Red   | Free RAM is critical (below threshold) |

Right-clicking the icon shows the current free RAM, a **Set Threshold** submenu,
**Open Config…**, and **Quit**.

## Useful commands

```bash
systemctl --user status rambo    # check if running
journalctl --user -u rambo -f    # live logs
systemctl --user restart rambo   # restart after config change
```

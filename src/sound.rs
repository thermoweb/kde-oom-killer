//! Notification sounds, embedded in the binary so the install stays a single
//! self-contained executable. On first use each clip is written to a cache file
//! and played by spawning an audio player.
//!
//! We play the clip directly rather than via the freedesktop `sound-file`
//! notification hint: KDE Plasma ignores that hint (it sources notification
//! sounds from its own per-event config), so the hint is silent there. Playing
//! it ourselves works the same across KDE/GNOME and avoids double playback.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::thread;

const WARNING_OGA: &[u8] = include_bytes!("../sounds/warning.oga");
const KILL_OGA: &[u8] = include_bytes!("../sounds/kill.oga");

/// Play the warning alert clip at `volume_pct` (0–100), non-blocking. Silent
/// no-op if it can't play.
pub fn play_warning(volume_pct: u8) {
    static PATH: OnceLock<Option<String>> = OnceLock::new();
    if let Some(p) = PATH.get_or_init(|| extract("warning.oga", WARNING_OGA)) {
        play(p, volume_pct);
    }
}

/// Play the gunshot kill clip at `volume_pct` (0–100), non-blocking. Silent
/// no-op if it can't play.
pub fn play_kill(volume_pct: u8) {
    static PATH: OnceLock<Option<String>> = OnceLock::new();
    if let Some(p) = PATH.get_or_init(|| extract("kill.oga", KILL_OGA)) {
        play(p, volume_pct);
    }
}

/// Play `base_path` at `volume_pct` (0–100), detached.
///
/// We can't rely on the player's own volume flag: `pw-play --volume` (the player
/// present on a PipeWire desktop) only applies at 1.0 and mutes anything below.
/// So for any volume under 100% we bake the gain into a cached scaled copy with
/// ffmpeg and play that at the player's native level; 100% plays the original.
fn play(base_path: &str, volume_pct: u8) {
    let base_path = base_path.to_owned();
    // Play on a detached thread so the caller never blocks. The thread waits on
    // the child to reap it (we'd otherwise leak a zombie per sound in a
    // long-running daemon).
    thread::spawn(move || {
        let pct = volume_pct.min(100);
        let path = if pct >= 100 {
            base_path.clone()
        } else {
            // Fall back to the full-volume clip if scaling fails — quieter is
            // nice-to-have, but silence would hide that a kill happened.
            scaled_clip(&base_path, pct).unwrap_or_else(|| base_path.clone())
        };

        // Ogg-capable players tried in rough order of how commonly they're
        // already running on a modern Linux desktop. The path is passed last.
        const PLAYERS: &[(&str, &[&str])] = &[
            ("pw-play", &[]),
            ("paplay", &[]),
            ("canberra-gtk-play", &["-f"]),
            ("ogg123", &["-q"]),
            ("ffplay", &["-nodisp", "-autoexit", "-loglevel", "quiet"]),
            ("mpv", &["--no-video", "--really-quiet"]),
        ];
        for (bin, args) in PLAYERS {
            let child = Command::new(bin)
                .args(*args)
                .arg(&path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            if let Ok(mut child) = child {
                let _ = child.wait();
                return;
            }
        }
        tracing::warn!("no audio player found to play notification sound");
    });
}

/// Return a path to `base_path` scaled to `pct`% volume, baking the gain in with
/// ffmpeg. The result is cached next to the original as `<stem>.v<pct>.oga`, so
/// each volume level is encoded at most once. `None` if ffmpeg is unavailable or
/// the encode fails.
fn scaled_clip(base_path: &str, pct: u8) -> Option<String> {
    let base = Path::new(base_path);
    let stem = base.file_stem()?.to_str()?;
    let out = base.with_file_name(format!("{stem}.v{pct}.oga"));
    if out.exists() {
        return Some(out.to_string_lossy().into_owned());
    }
    let linear = pct as f32 / 100.0;
    let status = Command::new("ffmpeg")
        .args(["-y", "-i", base_path, "-filter:a"])
        .arg(format!("volume={linear:.3}"))
        .args(["-c:a", "libvorbis", "-q:a", "4"])
        .arg(&out)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match status {
        Ok(s) if s.success() => Some(out.to_string_lossy().into_owned()),
        _ => {
            tracing::warn!("ffmpeg volume scaling failed; playing at full volume");
            None
        }
    }
}

/// Write `bytes` to `<cache>/rambo/<name>` (only if missing or stale) and return
/// the path. Uses XDG_RUNTIME_DIR when available (tmpfs, cleared on reboot),
/// falling back to the system temp dir.
fn extract(name: &str, bytes: &[u8]) -> Option<String> {
    let dir = runtime_dir().join("rambo");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!("could not create sound dir {}: {e}", dir.display());
        return None;
    }
    let path = dir.join(name);
    // Rewrite only when the on-disk size differs — cheap idempotent refresh.
    let stale = std::fs::metadata(&path)
        .map(|m| m.len() != bytes.len() as u64)
        .unwrap_or(true);
    if stale {
        if let Err(e) = std::fs::write(&path, bytes) {
            tracing::warn!("could not write sound {}: {e}", path.display());
            return None;
        }
    }
    Some(path.to_string_lossy().into_owned())
}

fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

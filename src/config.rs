use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Largest threshold the settings UI allows; also the upper bound used to
/// detect and recover a config poisoned with a sentinel value.
pub const MAX_THRESHOLD_MB: u64 = 65536;
/// Threshold used for new installs and when recovering an out-of-range value.
pub const DEFAULT_THRESHOLD_MB: u64 = 2048;

#[derive(Debug, Serialize, Deserialize, Clone, Hash)]
pub struct KillableApp {
    /// Process name to match (substring match, case-insensitive)
    pub name: String,
    /// Human-friendly display name shown in notifications
    pub display_name: Option<String>,
    /// Whether this app participates in the priority list
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

fn default_pressure_threshold() -> f64 {
    25.0
}

fn default_sound_volume() -> u8 {
    100
}

impl KillableApp {
    pub fn label(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.name)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    /// Kill when free RAM drops below this value (MB)
    pub threshold_mb: u64,
    /// Grace period in seconds before killing after the warning notification
    pub countdown_seconds: u64,
    /// How often to poll memory (seconds)
    pub check_interval_seconds: u64,
    /// After user clicks "Don't Kill", snooze warnings for this many seconds
    pub snooze_seconds: u64,
    /// Ordered list of apps to kill (first enabled match wins)
    pub killable_apps: Vec<KillableApp>,
    #[serde(default = "default_true")]
    pub use_memory_pressure: bool,
    #[serde(default = "default_pressure_threshold")]
    pub pressure_threshold_pct: f64,
    /// Play the alert sound with the warning notification
    #[serde(default = "default_true")]
    pub warning_sound_enabled: bool,
    /// Play the gunshot sound when a target is killed
    #[serde(default = "default_true")]
    pub kill_sound_enabled: bool,
    /// Playback volume for both sounds, 0–100%
    #[serde(default = "default_sound_volume")]
    pub sound_volume_pct: u8,
    /// Test Mode: forces the kill logic to trigger regardless of free RAM.
    /// Never persisted — it's a transient override that resets on restart, so
    /// it can't poison the saved threshold.
    #[serde(skip)]
    pub test_override: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            threshold_mb: DEFAULT_THRESHOLD_MB,
            countdown_seconds: 30,
            check_interval_seconds: 5,
            snooze_seconds: 300,
            killable_apps: vec![
                KillableApp { name: "slack".to_string(),    display_name: Some("Slack".to_string()),    enabled: true },
                KillableApp { name: "discord".to_string(),  display_name: Some("Discord".to_string()),  enabled: true },
                KillableApp { name: "firefox".to_string(),  display_name: Some("Firefox".to_string()),  enabled: true },
                KillableApp { name: "chromium".to_string(), display_name: Some("Chromium".to_string()), enabled: true },
                KillableApp { name: "chrome".to_string(),   display_name: Some("Chrome".to_string()),   enabled: true },
                KillableApp { name: "code".to_string(),     display_name: Some("VS Code".to_string()),  enabled: true },
                KillableApp { name: "gimp".to_string(),     display_name: Some("GIMP".to_string()),     enabled: true },
            ],
            use_memory_pressure: true,
            pressure_threshold_pct: 25.0,
            warning_sound_enabled: true,
            kill_sound_enabled: true,
            sound_volume_pct: 100,
            test_override: false,
        }
    }
}

impl Config {
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()))
            .join("rambo")
            .join("config.toml")
    }

    /// Path of the legacy JSON config, used for one-time migration.
    fn legacy_json_path() -> PathBuf {
        Self::config_path().with_extension("json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();

        // One-time migration: JSON → TOML
        let json_path = Self::legacy_json_path();
        if json_path.exists() && !path.exists() {
            if let Ok(content) = std::fs::read_to_string(&json_path) {
                if let Ok(mut cfg) = serde_json::from_str::<Config>(&content) {
                    for app in &mut cfg.killable_apps {
                        app.enabled = true;
                    }
                    if cfg.save().is_ok() {
                        let _ = std::fs::remove_file(&json_path);
                        tracing::info!(path = %path.display(), "Migrated config from JSON to TOML");
                    }
                    return cfg;
                }
            }
        }

        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => match toml::from_str::<Config>(&content) {
                    Ok(mut config) => {
                        // Repair an out-of-range value and rewrite it so the fix
                        // survives, rather than re-reading the poisoned file next start.
                        if config.sanitize() {
                            let _ = config.save();
                        }
                        return config;
                    }
                    Err(e) => tracing::warn!(error = %e, "Failed to parse config"),
                },
                Err(e) => tracing::warn!(error = %e, "Failed to read config"),
            }
        }
        let default = Config::default();
        if let Err(e) = default.save() {
            tracing::warn!(error = %e, "Failed to write default config");
        } else {
            tracing::info!(path = %path.display(), "Created default config");
        }
        default
    }

    /// Repair values that are out of the range the UI permits. In particular a
    /// `threshold_mb` of `u64::MAX` (left behind by an older Test Mode that
    /// mutated the persisted threshold) is reset to a sane default. Returns
    /// `true` if anything was changed.
    fn sanitize(&mut self) -> bool {
        if self.threshold_mb > MAX_THRESHOLD_MB {
            tracing::warn!(
                threshold = self.threshold_mb,
                "threshold out of range; resetting to {DEFAULT_THRESHOLD_MB}"
            );
            self.threshold_mb = DEFAULT_THRESHOLD_MB;
            return true;
        }
        false
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let toml_str = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(&path, toml_str)
    }
}


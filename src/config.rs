use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
}

impl Default for Config {
    fn default() -> Self {
        Config {
            threshold_mb: 500,
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
                        println!("[rambo] Migrated config from JSON to TOML at {}", path.display());
                    }
                    return cfg;
                }
            }
        }

        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => match toml::from_str::<Config>(&content) {
                    Ok(config) => return config,
                    Err(e) => eprintln!("[rambo] Failed to parse config: {e}"),
                },
                Err(e) => eprintln!("[rambo] Failed to read config: {e}"),
            }
        }
        let default = Config::default();
        if let Err(e) = default.save() {
            eprintln!("[rambo] Failed to write default config: {e}");
        } else {
            println!("[rambo] Created default config at {}", path.display());
        }
        default
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


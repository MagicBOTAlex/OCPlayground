//! The `computer.yaml` configuration schema and loader.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_name")]
    pub name: String,
    /// KiB of RAM.
    #[serde(default = "default_memory")]
    pub memory: u32,
    #[serde(default)]
    pub cpu: CpuConfig,
    #[serde(default)]
    pub gpu: GpuConfig,
    #[serde(default)]
    pub eeprom: EepromConfig,
    #[serde(default)]
    pub filesystems: Vec<FilesystemConfig>,
    #[serde(default)]
    pub components: ComponentsConfig,
    #[serde(default)]
    pub internet: InternetConfig,
    #[serde(default)]
    pub run: RunConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CpuConfig {
    #[serde(default = "default_cpu_tier")]
    pub tier: u8,
}

impl Default for CpuConfig {
    fn default() -> Self {
        CpuConfig {
            tier: default_cpu_tier(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GpuConfig {
    #[serde(default = "default_gpu_tier")]
    pub tier: u8,
    #[serde(default)]
    pub screen: ScreenConfig,
}

impl Default for GpuConfig {
    fn default() -> Self {
        GpuConfig {
            tier: default_gpu_tier(),
            screen: ScreenConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreenConfig {
    #[serde(default = "default_screen_width")]
    pub width: i32,
    #[serde(default = "default_screen_height")]
    pub height: i32,
    /// Maximum color depth in bits: 1, 4 or 8.
    #[serde(default = "default_screen_depth", rename = "maxDepth")]
    pub max_depth: u8,
}

impl Default for ScreenConfig {
    fn default() -> Self {
        ScreenConfig {
            width: default_screen_width(),
            height: default_screen_height(),
            max_depth: default_screen_depth(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EepromConfig {
    #[serde(default = "default_bios")]
    pub bios: String,
    #[serde(default = "default_boot")]
    pub boot: String,
}

impl Default for EepromConfig {
    fn default() -> Self {
        EepromConfig {
            bios: default_bios(),
            boot: default_boot(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilesystemConfig {
    pub label: String,
    #[serde(default = "default_fs_path")]
    pub path: String,
    #[serde(default = "default_mount")]
    pub mount: String,
    #[serde(default)]
    pub readonly: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentsConfig {
    #[serde(default = "default_true")]
    pub keyboard: bool,
    /// Capture terminal mouse events and deliver them as screen touch signals.
    #[serde(default = "default_true")]
    pub mouse: bool,
}

impl Default for ComponentsConfig {
    fn default() -> Self {
        ComponentsConfig {
            keyboard: true,
            mouse: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InternetConfig {
    /// Whether the internet card is present and HTTP requests are allowed.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// HTTP request timeout in seconds; `0` means no timeout.
    #[serde(default)]
    pub timeout: f64,
    /// Whether TCP connections are permitted (not implemented yet).
    #[serde(default)]
    pub tcp: bool,
    /// Value sent as the default `User-Agent` request header.
    #[serde(default = "default_user_agent", rename = "userAgent")]
    pub user_agent: String,
}

impl Default for InternetConfig {
    fn default() -> Self {
        InternetConfig {
            enabled: true,
            timeout: 0.0,
            tcp: false,
            user_agent: default_user_agent(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunConfig {
    #[serde(default)]
    pub script: Option<String>,
    #[serde(default)]
    pub timeout: f64,
    #[serde(default)]
    pub interactive: bool,
    /// Seconds to keep the final screen visible after the machine terminates.
    #[serde(default = "default_terminate_delay", rename = "terminateDelay")]
    pub terminate_delay: f64,
}

impl Default for RunConfig {
    fn default() -> Self {
        RunConfig {
            script: None,
            timeout: 0.0,
            interactive: false,
            terminate_delay: default_terminate_delay(),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Config> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        let mut config: Config = serde_yaml::from_str(&text)
            .with_context(|| format!("failed to parse config {}", path.display()))?;
        config
            .filesystems
            .retain(|fs| !fs.mount.trim().is_empty());
        Ok(config)
    }

    /// Resolve the server filesystem root when a filesystem uses `builtin`.
    pub fn builtin_system_dir(&self) -> Result<PathBuf> {
        system_dir()
    }
}

/// Locate the vendored OpenComputers system files.
pub fn system_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("OCPLAY_SYSTEM") {
        let path = PathBuf::from(dir);
        if path.is_dir() {
            return Ok(path);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("assets/system");
            if candidate.is_dir() {
                return Ok(candidate);
            }
        }
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/system");
    if manifest.is_dir() {
        return Ok(manifest);
    }
    anyhow::bail!(
        "could not locate vendored system files; set OCPLAY_SYSTEM or run from the project directory"
    )
}

fn default_name() -> String {
    "computer".into()
}
fn default_memory() -> u32 {
    512
}
fn default_cpu_tier() -> u8 {
    2
}
fn default_gpu_tier() -> u8 {
    3
}
fn default_screen_width() -> i32 {
    80
}
fn default_screen_height() -> i32 {
    25
}
fn default_screen_depth() -> u8 {
    8
}
fn default_bios() -> String {
    "builtin".into()
}
fn default_boot() -> String {
    "auto".into()
}
fn default_fs_path() -> String {
    "builtin".into()
}
fn default_mount() -> String {
    "/".into()
}
fn default_terminate_delay() -> f64 {
    5.0
}
fn default_true() -> bool {
    true
}
fn default_user_agent() -> String {
    "opencomputers/ocplay".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_uses_defaults() {
        let config: Config = serde_yaml::from_str("{}").unwrap();
        assert_eq!(config.name, "computer");
        assert_eq!(config.memory, 512);
        assert_eq!(config.gpu.screen.width, 80);
        assert_eq!(config.gpu.screen.max_depth, 8);
        assert!(config.internet.enabled);
        assert_eq!(config.internet.user_agent, "opencomputers/ocplay");
        assert_eq!(config.run.terminate_delay, 5.0);
        assert!(config.components.keyboard);
        assert!(config.components.mouse);
    }

    #[test]
    fn parses_internet_and_run_settings() {
        let yaml = r#"
internet:
  enabled: false
  timeout: 12.5
  tcp: true
  userAgent: custom/1.0
components:
  mouse: false
run:
  terminateDelay: 0
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(!config.internet.enabled);
        assert_eq!(config.internet.timeout, 12.5);
        assert!(config.internet.tcp);
        assert_eq!(config.internet.user_agent, "custom/1.0");
        assert!(!config.components.mouse);
        assert_eq!(config.run.terminate_delay, 0.0);
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let result: Result<Config, _> = serde_yaml::from_str("totallyUnknown: true");
        assert!(result.is_err());
    }
}

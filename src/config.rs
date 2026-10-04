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

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentsConfig {
    #[serde(default = "default_true")]
    pub keyboard: bool,
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
}

impl Default for RunConfig {
    fn default() -> Self {
        RunConfig {
            script: None,
            timeout: 0.0,
            interactive: false,
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
fn default_true() -> bool {
    true
}

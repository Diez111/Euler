//! Euler installer — lógica compartida UI/daemon.

pub mod progress;
pub mod theme;

use euler_core::install::InstallPlan;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct InstallRequest {
    pub device: String,
    pub hostname: String,
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default = "default_encrypt")]
    pub encrypt: bool,
    #[serde(default)]
    pub hw_profile: Option<String>,
    #[serde(default)]
    pub codecs: Vec<String>,
    #[serde(default)]
    pub enable_bluetooth: bool,
    #[serde(default)]
    pub enable_printer: bool,
}

fn default_encrypt() -> bool {
    true
}

impl std::fmt::Debug for InstallRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstallRequest")
            .field("device", &self.device)
            .field("hostname", &self.hostname)
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .field("encrypt", &self.encrypt)
            .field("hw_profile", &self.hw_profile)
            .field("codecs", &self.codecs)
            .field("enable_bluetooth", &self.enable_bluetooth)
            .field("enable_printer", &self.enable_printer)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum InstallStatus {
    Idle,
    Running {
        step: usize,
        total: usize,
        message: String,
    },
    Success,
    Failed(String),
}

pub fn build_plan(req: &InstallRequest) -> anyhow::Result<InstallPlan> {
    euler_core::validate::validate_device(&req.device)?;
    euler_core::validate::validate_hostname(&req.hostname)?;
    euler_core::validate::validate_username(&req.username)?;
    euler_core::validate::validate_password(&req.password)?;
    if let Some(ref hw) = req.hw_profile {
        let lower = hw.trim().to_ascii_lowercase();
        const ALLOWED: &[&str] = &["auto", "intel", "amd", "generic", "minimal"];
        if !ALLOWED.contains(&lower.as_str()) {
            anyhow::bail!(
                "hw_profile inválido: '{}' (permitidos: auto, intel, amd, generic, minimal)",
                hw
            );
        }
    }
    for codec in &req.codecs {
        if !euler_core::codecs::validate_codec_id(codec) {
            anyhow::bail!("codec inválido: '{}'", codec);
        }
    }
    let hw_opt = match req
        .hw_profile
        .as_deref()
        .map(|s| s.trim().to_ascii_lowercase())
    {
        None => None,
        Some(s) if s == "auto" => Some(euler_core::hw::HwProfile::detect()),
        Some(s) if s == "intel" => Some(euler_core::hw::HwProfile {
            gpu: euler_core::hw::GpuKind::Intel,
            wifi: euler_core::hw::WifiKind::None,
            has_bluetooth: req.enable_bluetooth,
            has_printer: req.enable_printer,
            has_nvme: true,
            ram_mb: 0,
            cpu_vendor: "GenuineIntel".to_string(),
        }),
        Some(s) if s == "amd" => Some(euler_core::hw::HwProfile {
            gpu: euler_core::hw::GpuKind::Amd,
            wifi: euler_core::hw::WifiKind::None,
            has_bluetooth: req.enable_bluetooth,
            has_printer: req.enable_printer,
            has_nvme: true,
            ram_mb: 0,
            cpu_vendor: "AuthenticAMD".to_string(),
        }),
        Some(s) if s == "generic" => Some(euler_core::hw::HwProfile {
            gpu: euler_core::hw::GpuKind::Unknown,
            wifi: euler_core::hw::WifiKind::None,
            has_bluetooth: req.enable_bluetooth,
            has_printer: req.enable_printer,
            has_nvme: false,
            ram_mb: 0,
            cpu_vendor: "Unknown".to_string(),
        }),
        Some(s) if s == "minimal" => None,
        _ => None,
    };
    InstallPlan::new_with_hw(
        &req.device,
        &req.hostname,
        &req.username,
        req.encrypt,
        hw_opt,
        &req.codecs,
        req.enable_bluetooth,
        req.enable_printer,
    )
}

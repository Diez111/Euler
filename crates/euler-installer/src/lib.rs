//! Euler installer — lógica compartida UI/daemon.

pub mod progress;

use euler_core::install::InstallPlan;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallRequest {
    pub device: String,
    pub hostname: String,
    pub username: String,
    pub password: String,
    pub encrypt: bool,
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
    InstallPlan::new(&req.device, &req.hostname, &req.username)
}

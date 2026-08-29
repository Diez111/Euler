//! Particionado GPT profesional para Euler.
//! Esquema: p1 512M EFI FAT32 (EF00), p2 resto LUKS2 -> BTRFS.

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const EFI_SIZE_MB: u64 = 512;
pub const EFI_LABEL: &str = "EFI";
pub const EULER_LABEL: &str = "EULER";
pub const SECTOR_SIZE: u64 = 512;
pub const ALIGN_SECTORS: u64 = 2048; // 1M alineado

#[derive(Debug, Error)]
pub enum DiskError {
    #[error("disco demasiado pequeño: {0} bytes, mínimo {1} bytes")]
    TooSmall(u64, u64),
    #[error("dispositivo no es disco válido: {0}")]
    InvalidDevice(String),
    #[error("sfdisk falló: {0}")]
    SfdiskFailed(String),
    #[error("mkfs falló: {0}")]
    MkfsFailed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartitionSpec {
    pub number: u32,
    pub start_mb: u64,
    pub size_mb: Option<u64>, // None = resto
    pub type_guid: String,    // EF00 / 8309
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskLayout {
    pub device: String,
    pub efi_size_mb: u64,
    pub partitions: Vec<PartitionSpec>,
}

impl DiskLayout {
    /// Crea layout GPT estándar Euler para `device` (ej. /dev/nvme0n1, /dev/sda).
    pub fn euler_default(device: &str) -> Result<Self, DiskError> {
        if device.is_empty() || !device.starts_with("/dev/") {
            return Err(DiskError::InvalidDevice(device.to_string()));
        }
        Ok(Self {
            device: device.to_string(),
            efi_size_mb: EFI_SIZE_MB,
            partitions: vec![
                PartitionSpec {
                    number: 1,
                    start_mb: 1,
                    size_mb: Some(EFI_SIZE_MB),
                    type_guid: "EF00".to_string(),
                    label: EFI_LABEL.to_string(),
                },
                PartitionSpec {
                    number: 2,
                    start_mb: 1 + EFI_SIZE_MB,
                    size_mb: None,
                    type_guid: "8309".to_string(),
                    label: EULER_LABEL.to_string(),
                },
            ],
        })
    }

    /// Genera script sfdisk (formato dump) para `sgdisk`/`sfdisk`.
    pub fn sfdisk_script(&self) -> String {
        let mut out = String::new();
        out.push_str("label: gpt\n");
        out.push_str("unit: sectors\n");
        out.push_str("first-lba: 2048\n");
        for p in &self.partitions {
            let size = match p.size_mb {
                Some(mb) => format!(" size={} ", mb * 2048),
                None => " size= ".to_string(),
            };
            out.push_str(&format!(
                "{} : start={} ,{}type={}, name=\"{}\"\n",
                self.device,
                p.start_mb * 2048,
                size,
                p.type_guid,
                p.label
            ));
        }
        out
    }

    /// Comandos sgdisk equivalentes (alternativa a sfdisk).
    /// 8309 = Linux LUKS (correcto para p2 LUKS2); 8300 es Linux filesystem genérico.
    pub fn sgdisk_commands(&self) -> Vec<String> {
        vec![
            format!("sgdisk --zap-all {}", self.device),
            format!(
                "sgdisk -n 1:0:+{}M -t 1:ef00 -c 1:{} {}",
                self.efi_size_mb, EFI_LABEL, self.device
            ),
            format!(
                "sgdisk -n 2:0:0 -t 2:8309 -c 2:{} {}",
                EULER_LABEL, self.device
            ),
        ]
    }

    /// Partición EFI path (maneja nvme p1 vs sda1).
    pub fn efi_partition(&self) -> String {
        partition_path(&self.device, 1)
    }

    /// Partición LUKS path.
    pub fn luks_partition(&self) -> String {
        partition_path(&self.device, 2)
    }
}

/// Resuelve /dev/nvme0n1 + 1 -> /dev/nvme0n1p1, /dev/sda +1 -> /dev/sda1
pub fn partition_path(device: &str, num: u32) -> String {
    let last = device.chars().last().unwrap_or('a');
    if last.is_ascii_digit() {
        format!("{}p{}", device, num)
    } else {
        format!("{}{}", device, num)
    }
}

/// Valida que disco tenga tamaño mínimo (10G para Euler).
pub fn validate_disk_size(bytes: u64) -> Result<(), DiskError> {
    const MIN_BYTES: u64 = 10 * 1024 * 1024 * 1024;
    if bytes < MIN_BYTES {
        return Err(DiskError::TooSmall(bytes, MIN_BYTES));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn euler_default_nvme() {
        let l = DiskLayout::euler_default("/dev/nvme0n1").unwrap();
        assert_eq!(l.efi_partition(), "/dev/nvme0n1p1");
        assert_eq!(l.luks_partition(), "/dev/nvme0n1p2");
        assert_eq!(l.partitions.len(), 2);
        assert_eq!(l.partitions[0].type_guid, "EF00");
    }

    #[test]
    fn euler_default_sda() {
        let l = DiskLayout::euler_default("/dev/sda").unwrap();
        assert_eq!(l.efi_partition(), "/dev/sda1");
        assert_eq!(l.luks_partition(), "/dev/sda2");
    }

    #[test]
    fn invalid_device() {
        assert!(DiskLayout::euler_default("nvme0n1").is_err());
        assert!(DiskLayout::euler_default("").is_err());
    }

    #[test]
    fn sfdisk_script_contains_gpt() {
        let l = DiskLayout::euler_default("/dev/sda").unwrap();
        let s = l.sfdisk_script();
        assert!(s.contains("label: gpt"));
        assert!(s.contains("EF00"));
        assert!(s.contains("EULER"));
    }

    #[test]
    fn validate_too_small() {
        assert!(validate_disk_size(5 * 1024 * 1024 * 1024).is_err());
        assert!(validate_disk_size(20 * 1024 * 1024 * 1024).is_ok());
    }

    #[test]
    fn sgdisk_commands_zap() {
        let l = DiskLayout::euler_default("/dev/nvme0n1").unwrap();
        let cmds = l.sgdisk_commands();
        assert!(cmds[0].contains("--zap-all"));
        assert!(cmds[1].contains("ef00"));
    }
}

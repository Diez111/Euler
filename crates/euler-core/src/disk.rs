//! Particionado GPT profesional para Euler.
//! Esquema: p1 512M EFI FAT32 (EF00), p2 resto LUKS2 -> BTRFS.

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const EFI_SIZE_MB: u64 = 512;
pub const EFI_LABEL: &str = "EFI";
pub const EULER_LABEL: &str = "EULER";
pub const SECTOR_SIZE: u64 = 512;
pub const ALIGN_SECTORS: u64 = 2048; // 1M alineado (2048*512)
pub const MIN_DISK_BYTES: u64 = 10 * 1024 * 1024 * 1024;

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
    #[error("overflow en cálculo de sectores: {0} * {1} excede u64")]
    Overflow(u64, u64),
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
    /// Valida con `crate::validate::validate_device` para rechazar inyección.
    pub fn euler_default(device: &str) -> Result<Self, DiskError> {
        crate::validate::validate_device(device)
            .map_err(|e| DiskError::InvalidDevice(e.to_string()))?;
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

    /// Genera script sfdisk (formato dump) para `sfdisk`.
    /// Formato esperado por `sfdisk /dev/sda < script`:
    ///   label: gpt
    ///   /dev/sda1 : start=2048, size=1048576, type=EF00, name="EFI"
    /// Wrapper backwards-compat: asume `encrypt = true` (LUKS2 -> 8309).
    pub fn sfdisk_script(&self) -> String {
        self.sfdisk_script_encrypt(true)
    }

    /// Variante encrypt-aware de `sfdisk_script`.
    /// Si `encrypt` es true => p2 type 8309 (Linux LUKS), si false => 8300 (Linux filesystem).
    pub fn sfdisk_script_encrypt(&self, encrypt: bool) -> String {
        use std::fmt::Write as _;
        // Script típico ~180-220 bytes; 256 evita re-alloc en caso común (2 particiones).
        let mut out = String::with_capacity(256);
        out.push_str("label: gpt\n");
        out.push_str("unit: sectors\n");
        let _ = writeln!(out, "first-lba: {}", ALIGN_SECTORS);
        for p in &self.partitions {
            let part_dev = partition_path(&self.device, p.number);
            let type_guid = if !encrypt && p.type_guid == "8309" {
                "8300"
            } else {
                p.type_guid.as_str()
            };
            let start = p.start_mb.checked_mul(2048).unwrap_or_else(|| {
                eprintln!(
                    "[warn] overflow start_mb {} *2048, usando u64::MAX",
                    p.start_mb
                );
                u64::MAX
            });
            if let Some(mb) = p.size_mb {
                let sectors = mb.checked_mul(2048).unwrap_or_else(|| {
                    eprintln!("[warn] overflow size_mb {} *2048, usando u64::MAX", mb);
                    u64::MAX
                });
                let _ = writeln!(
                    out,
                    "{} : start={}, size={}, type={}, name=\"{}\"",
                    part_dev, start, sectors, type_guid, p.label
                );
            } else {
                let _ = writeln!(
                    out,
                    "{} : start={}, size=, type={}, name=\"{}\"",
                    part_dev, start, type_guid, p.label
                );
            }
        }
        out
    }

    /// Variante fallible que valida overflow vía `checked_mul` y retorna `DiskError::Overflow`.
    pub fn try_sfdisk_script_encrypt(&self, encrypt: bool) -> Result<String, DiskError> {
        use std::fmt::Write as _;
        let mut out = String::with_capacity(256);
        out.push_str("label: gpt\n");
        out.push_str("unit: sectors\n");
        let _ = writeln!(out, "first-lba: {}", ALIGN_SECTORS);
        for p in &self.partitions {
            let part_dev = partition_path(&self.device, p.number);
            let type_guid = if !encrypt && p.type_guid == "8309" {
                "8300"
            } else {
                p.type_guid.as_str()
            };
            let start = p
                .start_mb
                .checked_mul(2048)
                .ok_or(DiskError::Overflow(p.start_mb, 2048))?;
            if let Some(mb) = p.size_mb {
                let sectors = mb.checked_mul(2048).ok_or(DiskError::Overflow(mb, 2048))?;
                let _ = writeln!(
                    out,
                    "{} : start={}, size={}, type={}, name=\"{}\"",
                    part_dev, start, sectors, type_guid, p.label
                );
            } else {
                let _ = writeln!(
                    out,
                    "{} : start={}, size=, type={}, name=\"{}\"",
                    part_dev, start, type_guid, p.label
                );
            }
        }
        Ok(out)
    }

    /// Comandos sgdisk equivalentes (alternativa a sfdisk) — argv vectors, sin shell.
    /// 8309 = Linux LUKS (correcto para p2 LUKS2); 8300 es Linux filesystem genérico.
    pub fn sgdisk_commands(&self) -> Vec<String> {
        // Compat: string version (legacy, no usar para ejecución)
        self.sgdisk_argv().iter().map(|v| v.join(" ")).collect()
    }

    /// Argv vectors listos para `Command::new(argv[0]).args(&argv[1..])` sin `sh -c`.
    /// Wrapper backwards-compat: asume `encrypt = true` (LUKS -> 8309).
    pub fn sgdisk_argv(&self) -> Vec<Vec<String>> {
        self.sgdisk_argv_encrypt(true)
    }

    /// Variante encrypt-aware: `encrypt == true` => p2 type `8309` (Linux LUKS),
    /// `encrypt == false` => `8300` (Linux filesystem genérico).
    /// Single `sgdisk` invocation atómico: --zap-all + ambas particiones en un fork+exec.
    pub fn sgdisk_argv_encrypt(&self, encrypt: bool) -> Vec<Vec<String>> {
        vec![self.sgdisk_single_argv_encrypt(encrypt)]
    }

    /// Single atomic argv para `sgdisk`: --zap-all + -n/-t/-c de p1 y p2 en una sola invocación.
    /// Evita estado intermedio inconsistente si fallaba entre comandos separados.
    pub fn sgdisk_single_argv_encrypt(&self, encrypt: bool) -> Vec<String> {
        let p2_type = if encrypt { "2:8309" } else { "2:8300" };
        vec![
            "sgdisk".to_string(),
            "--zap-all".to_string(),
            "-n".to_string(),
            format!("1:0:+{}M", self.efi_size_mb),
            "-t".to_string(),
            "1:ef00".to_string(),
            "-c".to_string(),
            format!("1:{}", EFI_LABEL),
            "-n".to_string(),
            "2:0:0".to_string(),
            "-t".to_string(),
            p2_type.to_string(),
            "-c".to_string(),
            format!("2:{}", EULER_LABEL),
            self.device.clone(),
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

/// Resuelve `/dev/nvme0n1` + 1 -> `/dev/nvme0n1p1`, `/dev/sda` + 1 -> `/dev/sda1`.
///
/// Asume un device directo tipo `/dev/sd*`, `/dev/nvme*`, `/dev/vda`, `/dev/mmcblk*`, etc.
/// No maneja rutas `/dev/disk/by-*` (ej. `by-id`, `by-uuid`, `by-path`); dichas rutas
/// son rechazadas por `crate::validate::validate_device` / `is_safe_device` antes de
/// llegar aquí. Si se pasa una ruta `by-*` el resultado sería incorrecto (heuristic
/// `last.is_ascii_digit()` no aplica) — pero la validación previa lo previene.
/// El sufijo `p` se añade solo si el último char es dígito (nvme/mmcblk/loop).
#[inline]
pub fn partition_path(device: &str, num: u32) -> String {
    let last = device.chars().last().unwrap_or('a');
    if last.is_ascii_digit() {
        format!("{}p{}", device, num)
    } else {
        format!("{}{}", device, num)
    }
}

/// Valida que disco tenga tamaño mínimo (10G para Euler).
/// Usa `MIN_DISK_BYTES` (10 GiB) como umbral.
#[inline]
pub fn validate_disk_size(bytes: u64) -> Result<(), DiskError> {
    validate_disk_size_or_err(bytes)
}

/// Alias `_or_err` para APIs que prefieren nombre estilo Result.
/// Valida contra `MIN_DISK_BYTES`; útil como helper público para callers externos.
pub fn validate_disk_size_or_err(bytes: u64) -> Result<(), DiskError> {
    if bytes < MIN_DISK_BYTES {
        return Err(DiskError::TooSmall(bytes, MIN_DISK_BYTES));
    }
    Ok(())
}

/// Verifica que un device sea seguro para pasar a `sgdisk`/`sfdisk` sin `sh -c`.
/// Reusa la validación estricta de `crate::validate`.
#[inline]
pub fn is_safe_device(device: &str) -> bool {
    crate::validate::is_valid_device_path(device)
}

/// Precheck temprano de tamaño vía /sys/block/<basename>/size * 512.
/// Si el sysfs existe (live ISO), valida contra MIN_DISK_BYTES.
/// Si no existe o no se puede leer, ignora silenciosamente (ej. tests, contenedores).
pub fn precheck_disk_size_for_device(device: &str) -> Result<(), DiskError> {
    // Extrae basename tras último '/'
    let basename = device.rsplit('/').next().unwrap_or(device);
    if basename.is_empty() {
        return Ok(());
    }
    let sys_path = format!("/sys/block/{}/size", basename);
    let content = match std::fs::read_to_string(&sys_path) {
        Ok(c) => c,
        Err(_) => return Ok(()), // no sysfs -> skip (tests / live sin disco)
    };
    let sectors: u64 = match content.trim().parse() {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let bytes = sectors.saturating_mul(SECTOR_SIZE);
    validate_disk_size_or_err(bytes)
}

impl DiskLayout {
    /// Wrapper de instancia para precheck vía sysfs.
    pub fn precheck_disk_size_available(&self) -> Result<(), DiskError> {
        precheck_disk_size_for_device(&self.device)
    }
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
        // atomic single invocation: one sgdisk string with --zap-all + both partitions
        assert_eq!(
            cmds.len(),
            1,
            "sgdisk debe ser atómico en una sola invocación"
        );
        assert!(cmds[0].contains("--zap-all"));
        assert!(cmds[0].contains("ef00"));
        assert!(cmds[0].contains("EULER"));
    }

    #[test]
    fn sgdisk_argv_encrypt_true_is_8309() {
        let l = DiskLayout::euler_default("/dev/sda").unwrap();
        let argv = l.sgdisk_argv_encrypt(true);
        assert_eq!(argv.len(), 1, "atómico: un solo argv");
        assert!(argv[0].iter().any(|a| a == "2:8309"));
        assert!(!argv[0].iter().any(|a| a == "2:8300"));
        assert!(argv[0].iter().any(|a| a == "--zap-all"));
        // wrapper default must match encrypt=true
        assert_eq!(l.sgdisk_argv(), argv);
        // single helper consistency
        assert_eq!(l.sgdisk_single_argv_encrypt(true), argv[0]);
    }

    #[test]
    fn sgdisk_argv_encrypt_false_is_8300() {
        let l = DiskLayout::euler_default("/dev/sda").unwrap();
        let argv = l.sgdisk_argv_encrypt(false);
        assert_eq!(argv.len(), 1);
        assert!(argv[0].iter().any(|a| a == "2:8300"));
        assert!(!argv[0].iter().any(|a| a == "2:8309"));
        // ensure wrapper (default) differs
        assert_ne!(l.sgdisk_argv(), argv);
        assert_eq!(l.sgdisk_single_argv_encrypt(false), argv[0]);
    }

    #[test]
    fn sfdisk_script_encrypt_variants() {
        let l = DiskLayout::euler_default("/dev/sda").unwrap();
        let script_enc = l.sfdisk_script_encrypt(true);
        assert!(script_enc.contains("type=8309"));
        assert!(!script_enc.contains("type=8300"));
        // default wrapper == encrypt true
        assert_eq!(l.sfdisk_script(), script_enc);

        let script_noenc = l.sfdisk_script_encrypt(false);
        assert!(script_noenc.contains("type=8300"));
        assert!(!script_noenc.contains("type=8309"));
        // still contains EFI and label
        assert!(script_noenc.contains("EF00"));
        assert!(script_noenc.contains("EULER"));
    }

    #[test]
    fn validate_disk_size_or_err_alias() {
        assert!(validate_disk_size_or_err(5 * 1024 * 1024 * 1024).is_err());
        assert!(validate_disk_size_or_err(20 * 1024 * 1024 * 1024).is_ok());
        // alias must match original
        assert_eq!(
            validate_disk_size(5 * 1024 * 1024 * 1024).is_err(),
            validate_disk_size_or_err(5 * 1024 * 1024 * 1024).is_err()
        );
    }

    #[test]
    fn partition_path_heuristic() {
        assert_eq!(partition_path("/dev/sda", 1), "/dev/sda1");
        assert_eq!(partition_path("/dev/nvme0n1", 1), "/dev/nvme0n1p1");
        assert_eq!(partition_path("/dev/mmcblk0", 2), "/dev/mmcblk0p2");
        assert_eq!(partition_path("/dev/vda", 1), "/dev/vda1");
        assert_eq!(partition_path("/dev/loop0", 1), "/dev/loop0p1");
    }
}

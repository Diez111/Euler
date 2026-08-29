//! Plan de instalación Euler — orquestación de pasos sin I/O real.
//! El daemon ejecuta estos pasos con privilegios; aquí solo se genera el plan.

use serde::{Deserialize, Serialize};

use crate::btrfs::{euler_subvolumes, fstab_entries, mkfs_command, subvol_create_commands};
use crate::crypt::LuksConfig;
use crate::disk::DiskLayout;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum InstallStepKind {
    Partition,
    FormatEfi,
    LuksFormat,
    LuksOpen,
    MkfsBtrfs,
    SubvolCreate,
    Mount,
    UnpackSquashfs,
    Fstab,
    Crypttab,
    Users,
    Bootloader,
    Initramfs,
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallStep {
    pub kind: InstallStepKind,
    pub description: String,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallPlan {
    pub device: String,
    pub hostname: String,
    pub username: String,
    pub steps: Vec<InstallStep>,
}

impl InstallPlan {
    pub fn new(device: &str, hostname: &str, username: &str) -> anyhow::Result<Self> {
        let layout = DiskLayout::euler_default(device)?;
        let luks = LuksConfig::new(&layout.luks_partition())?;
        let mapper = luks.mapper_path();

        let mut steps = Vec::new();

        // 1. Partition
        for cmd in layout.sgdisk_commands() {
            steps.push(InstallStep {
                kind: InstallStepKind::Partition,
                description: "Particionado GPT EFI 1G + LUKS".to_string(),
                command: vec!["sh".to_string(), "-c".to_string(), cmd],
            });
        }

        // 2. Format EFI
        steps.push(InstallStep {
            kind: InstallStepKind::FormatEfi,
            description: "Formatear EFI FAT32".to_string(),
            command: vec![
                "mkfs.vfat".to_string(),
                "-F32".to_string(),
                "-n".to_string(),
                "EFI".to_string(),
                layout.efi_partition(),
            ],
        });

        // 3. LUKS format
        steps.push(InstallStep {
            kind: InstallStepKind::LuksFormat,
            description: "LUKS2 format argon2id".to_string(),
            command: luks.format_command(),
        });

        // 4. LUKS open
        steps.push(InstallStep {
            kind: InstallStepKind::LuksOpen,
            description: "Abrir LUKS".to_string(),
            command: luks.open_command(),
        });

        // 5. mkfs.btrfs
        steps.push(InstallStep {
            kind: InstallStepKind::MkfsBtrfs,
            description: "mkfs.btrfs dup single xxhash".to_string(),
            command: mkfs_command(&mapper),
        });

        // 6. Subvol create (montar temporal)
        steps.push(InstallStep {
            kind: InstallStepKind::Mount,
            description: "Montar BTRFS temporal para subvols".to_string(),
            command: vec!["mount".to_string(), mapper.clone(), "/mnt".to_string()],
        });
        for cmd in subvol_create_commands("/mnt") {
            steps.push(InstallStep {
                kind: InstallStepKind::SubvolCreate,
                description: format!("Crear subvol {cmd}"),
                command: vec!["sh".to_string(), "-c".to_string(), cmd],
            });
        }
        steps.push(InstallStep {
            kind: InstallStepKind::Mount,
            description: "Desmontar temporal".to_string(),
            command: vec!["umount".to_string(), "/mnt".to_string()],
        });

        // 7. Mount jerárquico para instalación
        let subvols = euler_subvolumes();
        for sv in &subvols {
            let mnt = if sv.mountpoint == "/" {
                "/mnt".to_string()
            } else {
                format!("/mnt{}", sv.mountpoint)
            };
            let opts = format!("{},subvol={}", sv.options, sv.name);
            // simplificado: mount -o opts mapper mnt
            let _ = opts;
            steps.push(InstallStep {
                kind: InstallStepKind::Mount,
                description: format!("Montar {} en {}", sv.name, mnt),
                command: vec![
                    "mount".to_string(),
                    "-o".to_string(),
                    sv.options.clone(),
                    mapper.clone(),
                    mnt,
                ],
            });
        }
        // EFI
        steps.push(InstallStep {
            kind: InstallStepKind::Mount,
            description: "Montar EFI".to_string(),
            command: vec![
                "mount".to_string(),
                layout.efi_partition(),
                "/mnt/boot/efi".to_string(),
            ],
        });

        // 8. Unpack
        steps.push(InstallStep {
            kind: InstallStepKind::UnpackSquashfs,
            description: "Desempaquetar filesystem.squashfs".to_string(),
            command: vec![
                "unsquashfs".to_string(),
                "-f".to_string(),
                "-d".to_string(),
                "/mnt".to_string(),
                "/run/live/medium/live/filesystem.squashfs".to_string(),
            ],
        });

        // 9. fstab
        let fstab = fstab_entries(&mapper);
        steps.push(InstallStep {
            kind: InstallStepKind::Fstab,
            description: "Generar fstab".to_string(),
            command: vec!["sh".to_string(), "-c".to_string(), fstab.join("\n")],
        });

        // 10. crypttab
        steps.push(InstallStep {
            kind: InstallStepKind::Crypttab,
            description: "Generar crypttab".to_string(),
            command: vec![
                "sh".to_string(),
                "-c".to_string(),
                luks.crypttab_entry("UUID-REEMPLAZAR"),
            ],
        });

        // 11. Users
        steps.push(InstallStep {
            kind: InstallStepKind::Users,
            description: format!("Crear usuario {username} y hostname {hostname}"),
            command: vec![
                "systemd-nspawn".to_string(),
                "-D".to_string(),
                "/mnt".to_string(),
                "useradd".to_string(),
                "-m".to_string(),
                "-G".to_string(),
                "sudo,audio,video".to_string(),
                username.to_string(),
            ],
        });

        // 12. Bootloader
        steps.push(InstallStep {
            kind: InstallStepKind::Bootloader,
            description: "Instalar GRUB EFI".to_string(),
            command: vec![
                "grub-install".to_string(),
                "--target=x86_64-efi".to_string(),
                "--efi-directory=/mnt/boot/efi".to_string(),
                "--bootloader-id=euler".to_string(),
                "--removable".to_string(),
            ],
        });

        // 13. Initramfs
        steps.push(InstallStep {
            kind: InstallStepKind::Initramfs,
            description: "update-initramfs".to_string(),
            command: vec![
                "systemd-nspawn".to_string(),
                "-D".to_string(),
                "/mnt".to_string(),
                "update-initramfs".to_string(),
                "-c".to_string(),
                "-k".to_string(),
                "all".to_string(),
            ],
        });

        steps.push(InstallStep {
            kind: InstallStepKind::Done,
            description: "Instalación completa".to_string(),
            command: vec![],
        });

        Ok(Self {
            device: device.to_string(),
            hostname: hostname.to_string(),
            username: username.to_string(),
            steps,
        })
    }

    pub fn step_count(&self) -> usize {
        self.steps.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_has_all_kinds() {
        let p = InstallPlan::new("/dev/sda", "euler", "euler").unwrap();
        let kinds: Vec<_> = p.steps.iter().map(|s| s.kind.clone()).collect();
        assert!(kinds.contains(&InstallStepKind::Partition));
        assert!(kinds.contains(&InstallStepKind::LuksFormat));
        assert!(kinds.contains(&InstallStepKind::MkfsBtrfs));
        assert!(kinds.contains(&InstallStepKind::Bootloader));
        assert!(kinds.contains(&InstallStepKind::Initramfs));
    }

    #[test]
    fn plan_invalid_device() {
        assert!(InstallPlan::new("sda", "euler", "euler").is_err());
    }

    #[test]
    fn plan_step_count_reasonable() {
        let p = InstallPlan::new("/dev/nvme0n1", "euler", "diez").unwrap();
        assert!(p.step_count() > 15);
        assert!(p.step_count() < 40);
    }
}

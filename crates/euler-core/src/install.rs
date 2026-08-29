//! Plan de instalación Euler — orquestación de pasos sin I/O real.
//! El daemon ejecuta estos pasos con privilegios; aquí solo se genera el plan.
//! Refactorizado para complejidad cognitiva ≤15: flujo alto nivel + helpers enfocados.

use serde::{Deserialize, Serialize};

use crate::btrfs::{euler_subvolumes, fstab_entries_for_hw, mkfs_command, subvol_create_argv};
use crate::crypt::LuksConfig;
use crate::disk::DiskLayout;
use crate::hw::HwProfile;

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
    HwDetect,
    HwPackages,
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
        Self::new_inner(device, hostname, username, true)
    }

    pub fn new_no_encrypt(device: &str, hostname: &str, username: &str) -> anyhow::Result<Self> {
        Self::new_inner(device, hostname, username, false)
    }

    /// Variante con perfil HW, codecs, bluetooth y printer — inserta pasos HwDetect/HwPackages.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_hw(
        device: &str,
        hostname: &str,
        username: &str,
        encrypt: bool,
        hw: Option<HwProfile>,
        codecs: &[String],
        enable_bluetooth: bool,
        enable_printer: bool,
    ) -> anyhow::Result<Self> {
        Self::new_inner_with_hw(
            device,
            hostname,
            username,
            encrypt,
            hw,
            codecs,
            enable_bluetooth,
            enable_printer,
        )
    }

    /// Flujo alto nivel — complejidad reducida a orquestación.
    fn new_inner(
        device: &str,
        hostname: &str,
        username: &str,
        encrypt: bool,
    ) -> anyhow::Result<Self> {
        Self::new_inner_with_hw(device, hostname, username, encrypt, None, &[], false, false)
    }

    #[allow(clippy::too_many_arguments)]
    fn new_inner_with_hw(
        device: &str,
        hostname: &str,
        username: &str,
        encrypt: bool,
        hw: Option<HwProfile>,
        codecs: &[String],
        enable_bluetooth: bool,
        enable_printer: bool,
    ) -> anyhow::Result<Self> {
        Self::validate_inputs(device, hostname, username)?;
        let (layout, luks, btrfs_device) = Self::prepare_storage(device, encrypt)?;
        let mut steps = Vec::with_capacity(Self::estimated_steps(encrypt));
        Self::push_disk_size_check(&mut steps, &layout);
        Self::push_partitioning(&mut steps, &layout, encrypt);
        Self::push_efi_format(&mut steps, &layout);
        if encrypt {
            Self::push_luks_steps(&mut steps, &luks);
        }
        Self::push_btrfs_format(&mut steps, &btrfs_device);
        Self::push_subvol_creation(&mut steps, &btrfs_device);
        Self::push_mount_hierarchy(&mut steps, &btrfs_device, &layout);
        Self::push_unpack(&mut steps);
        Self::push_hw_detect(&mut steps, &hw);
        Self::push_hw_packages(&mut steps, &hw, codecs, enable_bluetooth, enable_printer);
        Self::push_fstab_entries(&mut steps, &btrfs_device, &layout, &hw);
        if encrypt {
            Self::push_crypttab(&mut steps, &luks, &layout);
        }
        Self::push_users_and_hostname(&mut steps, hostname, username);
        Self::push_bootloader(&mut steps);
        Self::push_initramfs_and_done(&mut steps, encrypt);
        Ok(Self {
            device: device.to_string(),
            hostname: hostname.to_string(),
            username: username.to_string(),
            steps,
        })
    }

    // ——— helpers de validación ———

    fn validate_inputs(device: &str, hostname: &str, username: &str) -> anyhow::Result<()> {
        crate::validate::validate_device(device)?;
        crate::validate::validate_hostname(hostname)?;
        crate::validate::validate_username(username)?;
        Self::validate_disk_size_precheck(device)?;
        Ok(())
    }

    /// Precheck temprano vía sysfs (/sys/block/*/size) antes de sgdisk.
    /// Si sysfs no existe (tests/contenedor) ignora; si existe y es demasiado pequeño, falla temprano.
    fn validate_disk_size_precheck(device: &str) -> anyhow::Result<()> {
        if let Err(e) = crate::disk::precheck_disk_size_for_device(device) {
            anyhow::bail!(e.to_string());
        }
        Ok(())
    }

    fn prepare_storage(
        device: &str,
        encrypt: bool,
    ) -> anyhow::Result<(DiskLayout, LuksConfig, String)> {
        let layout = DiskLayout::euler_default(device)?;
        let luks = LuksConfig::new(&layout.luks_partition())?;
        let btrfs_device = if encrypt {
            luks.mapper_path()
        } else {
            layout.luks_partition()
        };
        Ok((layout, luks, btrfs_device))
    }

    const fn estimated_steps(encrypt: bool) -> usize {
        if encrypt {
            36
        } else {
            32
        }
    }

    // ——— helpers de steps ———

    fn push_disk_size_check(steps: &mut Vec<InstallStep>, layout: &DiskLayout) {
        steps.push(InstallStep {
            kind: InstallStepKind::Partition,
            description: "Validar tamaño disco >=10G".to_string(),
            command: vec![
                "sh".to_string(),
                "-c".to_string(),
                format!(
                    "SIZE=$(blockdev --getsize64 {} 2>/dev/null || echo 0); if [ \"$SIZE\" -lt {} ]; then echo \"disco demasiado pequeño: $SIZE bytes, mínimo {} bytes\" >&2; exit 1; fi; echo \"disk size $SIZE ok\"",
                    layout.device, crate::disk::MIN_DISK_BYTES, crate::disk::MIN_DISK_BYTES
                ),
            ],
        });
    }

    fn push_partitioning(steps: &mut Vec<InstallStep>, layout: &DiskLayout, encrypt: bool) {
        for argv in layout.sgdisk_argv_encrypt(encrypt) {
            steps.push(InstallStep {
                kind: InstallStepKind::Partition,
                description: format!(
                    "Particionado GPT EFI {}M + {}",
                    layout.efi_size_mb,
                    if encrypt { "LUKS" } else { "BTRFS" }
                ),
                command: argv,
            });
        }
        steps.push(InstallStep {
            kind: InstallStepKind::Partition,
            description: "Releer tabla de particiones".to_string(),
            command: vec!["partprobe".to_string(), layout.device.clone()],
        });
    }

    fn push_efi_format(steps: &mut Vec<InstallStep>, layout: &DiskLayout) {
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
    }

    fn push_luks_steps(steps: &mut Vec<InstallStep>, luks: &LuksConfig) {
        steps.push(InstallStep {
            kind: InstallStepKind::LuksFormat,
            description: "LUKS2 format argon2id (passphrase via stdin)".to_string(),
            command: luks.format_command(),
        });
        steps.push(InstallStep {
            kind: InstallStepKind::LuksOpen,
            description: "Abrir LUKS".to_string(),
            command: luks.open_command(),
        });
    }

    fn push_btrfs_format(steps: &mut Vec<InstallStep>, btrfs_device: &str) {
        steps.push(InstallStep {
            kind: InstallStepKind::MkfsBtrfs,
            description: "mkfs.btrfs -f dup single xxhash".to_string(),
            command: mkfs_command(btrfs_device),
        });
    }

    fn push_subvol_creation(steps: &mut Vec<InstallStep>, btrfs_device: &str) {
        steps.push(InstallStep {
            kind: InstallStepKind::Mount,
            description: "Montar BTRFS temporal para subvols".to_string(),
            command: vec![
                "mount".to_string(),
                btrfs_device.to_string(),
                "/mnt".to_string(),
            ],
        });
        for argv in subvol_create_argv("/mnt") {
            let desc = argv.last().cloned().unwrap_or_default();
            steps.push(InstallStep {
                kind: InstallStepKind::SubvolCreate,
                description: format!("Crear subvol {}", desc),
                command: argv,
            });
        }
        steps.push(InstallStep {
            kind: InstallStepKind::Mount,
            description: "Desmontar temporal".to_string(),
            command: vec!["umount".to_string(), "/mnt".to_string()],
        });
    }

    fn push_mount_hierarchy(steps: &mut Vec<InstallStep>, btrfs_device: &str, layout: &DiskLayout) {
        for sv in &euler_subvolumes() {
            let mnt = Self::mountpoint_for(&sv.mountpoint);
            if mnt != "/mnt" {
                steps.push(InstallStep {
                    kind: InstallStepKind::Mount,
                    description: format!("Crear directorio {}", mnt),
                    command: vec!["mkdir".to_string(), "-p".to_string(), mnt.clone()],
                });
            }
            steps.push(InstallStep {
                kind: InstallStepKind::Mount,
                description: format!("Montar {} en {}", sv.name, mnt),
                command: vec![
                    "mount".to_string(),
                    "-o".to_string(),
                    sv.options.clone(),
                    btrfs_device.to_string(),
                    mnt,
                ],
            });
        }
        steps.push(InstallStep {
            kind: InstallStepKind::Mount,
            description: "Crear directorio /mnt/boot/efi".to_string(),
            command: vec![
                "mkdir".to_string(),
                "-p".to_string(),
                "/mnt/boot/efi".to_string(),
            ],
        });
        steps.push(InstallStep {
            kind: InstallStepKind::Mount,
            description: "Montar EFI".to_string(),
            command: vec![
                "mount".to_string(),
                layout.efi_partition(),
                "/mnt/boot/efi".to_string(),
            ],
        });
    }

    fn mountpoint_for(sv_mount: &str) -> String {
        if sv_mount == "/" {
            "/mnt".to_string()
        } else {
            // optimizado: with_capacity evita realloc de format! (4 + len)
            let mut s = String::with_capacity(4 + sv_mount.len());
            s.push_str("/mnt");
            s.push_str(sv_mount);
            s
        }
    }

    fn push_unpack(steps: &mut Vec<InstallStep>) {
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
    }

    fn push_hw_detect(steps: &mut Vec<InstallStep>, hw: &Option<HwProfile>) {
        let cmd = if let Some(p) = hw {
            format!(
                "echo 'HwDetect gpu={} wifi={} bluetooth={} nvme={} ram={}MB cpu={}' ; lspci 2>/dev/null | head -n 20 || true; lsusb 2>/dev/null | head -n 20 || true",
                p.gpu, p.wifi, p.has_bluetooth, p.has_nvme, p.ram_mb, p.cpu_vendor
            )
        } else {
            "echo 'detecting hardware...' ; lspci 2>/dev/null | head -n 20 || true; lsusb 2>/dev/null | head -n 20 || true; cat /proc/cpuinfo 2>/dev/null | head -n 5 || true".to_string()
        };
        steps.push(InstallStep {
            kind: InstallStepKind::HwDetect,
            description: "Detectar hardware".to_string(),
            command: vec!["sh".to_string(), "-c".to_string(), cmd],
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn push_hw_packages(
        steps: &mut Vec<InstallStep>,
        profile: &Option<HwProfile>,
        codecs: &[String],
        enable_bluetooth: bool,
        enable_printer: bool,
    ) {
        let mut packages: Vec<String> = Vec::new();
        for id in codecs {
            if let Some(opt) = crate::codecs::find_codec(id) {
                for &pkg in opt.packages {
                    let pkg_s = pkg.to_string();
                    if !packages.contains(&pkg_s) {
                        packages.push(pkg_s);
                    }
                }
            }
        }
        if let Some(p) = profile {
            for pkg in p.extra_packages() {
                let pkg_s = pkg.to_string();
                if !packages.contains(&pkg_s) {
                    packages.push(pkg_s);
                }
            }
        }
        if enable_bluetooth {
            for pkg in ["bluez", "bluez-firmware"] {
                let pkg_s = pkg.to_string();
                if !packages.contains(&pkg_s) {
                    packages.push(pkg_s);
                }
            }
        }
        let printer_needed =
            enable_printer || profile.as_ref().map(|p| p.has_printer).unwrap_or(false);
        if printer_needed {
            for pkg in crate::peripherals::PRINTER_PACKAGES {
                let pkg_s = pkg.to_string();
                if !packages.contains(&pkg_s) {
                    packages.push(pkg_s);
                }
            }
        }

        let mut script_parts: Vec<String> = Vec::new();
        if !packages.is_empty() {
            script_parts.push(format!(
                "systemd-nspawn -D /mnt apt-get update && systemd-nspawn -D /mnt apt-get install -y {}",
                packages.join(" ")
            ));
        } else {
            script_parts.push("echo 'no extra hw packages required'".to_string());
        }
        let bluetooth_needed =
            enable_bluetooth || profile.as_ref().map(|p| p.has_bluetooth).unwrap_or(false);
        if bluetooth_needed {
            script_parts.push(
                "systemd-nspawn -D /mnt systemctl enable bluetooth 2>/dev/null || true".to_string(),
            );
        }
        if printer_needed {
            script_parts.push(
                "systemd-nspawn -D /mnt systemctl enable cups 2>/dev/null || true".to_string(),
            );
            script_parts.push(
                "systemd-nspawn -D /mnt systemctl enable cups-browsed 2>/dev/null || true"
                    .to_string(),
            );
        }
        if let Some(p) = profile {
            let additions = p.kernel_additions();
            if !additions.is_empty() {
                script_parts.push(format!(
                    "sed -i 's/GRUB_CMDLINE_LINUX_DEFAULT=\"/GRUB_CMDLINE_LINUX_DEFAULT=\"{} /' /mnt/etc/default/grub 2>/dev/null || echo 'GRUB_CMDLINE_LINUX_DEFAULT=\"{}\"' >> /mnt/etc/default/grub; systemd-nspawn -D /mnt update-grub 2>/dev/null || true",
                    additions, additions
                ));
            }
        }
        let full_script = script_parts.join(" ; ");
        steps.push(InstallStep {
            kind: InstallStepKind::HwPackages,
            description: "Instalar paquetes HW / codecs / bluetooth / printer / kernel cmdline"
                .to_string(),
            command: vec!["sh".to_string(), "-c".to_string(), full_script],
        });
    }

    fn push_fstab_entries(
        steps: &mut Vec<InstallStep>,
        btrfs_device: &str,
        layout: &DiskLayout,
        hw: &Option<HwProfile>,
    ) {
        let fstab_lines = fstab_entries_for_hw(btrfs_device, hw.clone());
        let fstab_content = fstab_lines.join("\n");
        steps.push(InstallStep {
            kind: InstallStepKind::Fstab,
            description: "Generar /mnt/etc/fstab".to_string(),
            command: vec![
                "sh".to_string(),
                "-c".to_string(),
                format!(
                    "mkdir -p /mnt/etc && cat > /mnt/etc/fstab <<'EULERFSTAB'\n{}\nEULERFSTAB",
                    fstab_content
                ),
            ],
        });
        steps.push(InstallStep {
            kind: InstallStepKind::Fstab,
            description: "Añadir EFI a fstab".to_string(),
            command: vec![
                "sh".to_string(),
                "-c".to_string(),
                format!(
                    "echo '{}  /boot/efi  vfat  umask=0077,shortname=winnt 0 1' >> /mnt/etc/fstab",
                    layout.efi_partition()
                ),
            ],
        });
    }

    fn push_crypttab(steps: &mut Vec<InstallStep>, luks: &LuksConfig, layout: &DiskLayout) {
        let crypt_entry = luks.crypttab_entry("UUID-REEMPLAZAR");
        let luks_part = layout.luks_partition();
        steps.push(InstallStep {
            kind: InstallStepKind::Crypttab,
            description: "Generar /mnt/etc/crypttab (con UUID real)".to_string(),
            command: vec![
                "sh".to_string(),
                "-c".to_string(),
                format!(
                    "mkdir -p /mnt/etc && cat > /mnt/etc/crypttab <<'EULERCRYPT'\n{}\nEULERCRYPT\n\
                     udevadm settle 2>/dev/null || true; partprobe {} 2>/dev/null || true; sleep 1\n\
                     for i in 1 2 3; do BLKID=$(blkid -p -s UUID -o value {} 2>/dev/null || true); [ -n \"$BLKID\" ] && break; sleep 1; done\n\
                     if [ -z \"$BLKID\" ]; then echo \"error: blkid no devolvió UUID para {}\" >&2; exit 1; fi\n\
                     if ! echo \"$BLKID\" | grep -Eq '^[0-9a-fA-F-]{{36}}$'; then echo \"UUID inválido: $BLKID\" >&2; exit 1; fi\n\
                     sed -i \"s/UUID-REEMPLAZAR/$BLKID/\" /mnt/etc/crypttab\n\
                     echo \"crypttab UUID=$BLKID\"",
                    crypt_entry, luks_part, luks_part, luks_part
                ),
            ],
        });
    }

    fn push_users_and_hostname(steps: &mut Vec<InstallStep>, hostname: &str, username: &str) {
        steps.push(InstallStep {
            kind: InstallStepKind::Users,
            description: format!("Configurar hostname {}", hostname),
            command: vec![
                "sh".to_string(),
                "-c".to_string(),
                format!(
                    "echo '{}' > /mnt/etc/hostname && sed -i 's/127.0.1.1.*/127.0.1.1\t{}/' /mnt/etc/hosts 2>/dev/null || echo '127.0.1.1\t{}' >> /mnt/etc/hosts",
                    hostname, hostname, hostname
                ),
            ],
        });
        steps.push(InstallStep {
            kind: InstallStepKind::Users,
            description: format!("Crear usuario {}", username),
            command: vec![
                "systemd-nspawn".to_string(),
                "-D".to_string(),
                "/mnt".to_string(),
                "useradd".to_string(),
                "-m".to_string(),
                "-G".to_string(),
                "sudo,audio,video,plugdev,netdev".to_string(),
                "-s".to_string(),
                "/bin/bash".to_string(),
                username.to_string(),
            ],
        });
        steps.push(InstallStep {
            kind: InstallStepKind::Users,
            description: format!("Establecer password para {}", username),
            command: vec![
                "systemd-nspawn".to_string(),
                "-D".to_string(),
                "/mnt".to_string(),
                "chpasswd".to_string(),
            ],
        });
        steps.push(InstallStep {
            kind: InstallStepKind::Users,
            description: "Configurar sudoers".to_string(),
            command: vec![
                "sh".to_string(),
                "-c".to_string(),
                format!(
                    "echo '{} ALL=(ALL) ALL' > /mnt/etc/sudoers.d/{} && chmod 440 /mnt/etc/sudoers.d/{}",
                    username, username, username
                ),
            ],
        });
    }

    fn push_bootloader(steps: &mut Vec<InstallStep>) {
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
        steps.push(InstallStep {
            kind: InstallStepKind::Bootloader,
            description: "Generar grub.cfg".to_string(),
            command: vec![
                "systemd-nspawn".to_string(),
                "-D".to_string(),
                "/mnt".to_string(),
                "update-grub".to_string(),
            ],
        });
    }

    fn push_initramfs_and_done(steps: &mut Vec<InstallStep>, encrypt: bool) {
        steps.push(InstallStep {
            kind: InstallStepKind::Initramfs,
            description: "update-initramfs -c -k all".to_string(),
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
            description: "Instalación completa — desmontar y cerrar LUKS".to_string(),
            command: vec![
                "sh".to_string(),
                "-c".to_string(),
                if encrypt {
                    "if findmnt /mnt >/dev/null 2>&1; then umount -R /mnt 2>/dev/null || true; fi; cryptsetup close luks-euler 2>/dev/null || true".to_string()
                } else {
                    "if findmnt /mnt >/dev/null 2>&1; then umount -R /mnt 2>/dev/null || true; fi".to_string()
                },
            ],
        });
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
    fn plan_no_encrypt_skips_luks() {
        let p = InstallPlan::new_no_encrypt("/dev/sda", "euler", "euler").unwrap();
        let kinds: Vec<_> = p.steps.iter().map(|s| s.kind.clone()).collect();
        assert!(!kinds.contains(&InstallStepKind::LuksFormat));
        assert!(!kinds.contains(&InstallStepKind::LuksOpen));
        assert!(!kinds.contains(&InstallStepKind::Crypttab));
        assert!(kinds.contains(&InstallStepKind::MkfsBtrfs));
    }

    #[test]
    fn plan_invalid_device() {
        assert!(InstallPlan::new("sda", "euler", "euler").is_err());
        assert!(InstallPlan::new("/dev/sda; rm -rf", "euler", "euler").is_err());
    }

    #[test]
    fn plan_step_count_reasonable() {
        let p = InstallPlan::new("/dev/nvme0n1", "euler", "diez").unwrap();
        assert!(p.step_count() > 20);
        assert!(p.step_count() < 60);
    }

    #[test]
    fn plan_fstab_writes_file() {
        let p = InstallPlan::new("/dev/sda", "euler", "euler").unwrap();
        let fstab = p
            .steps
            .iter()
            .find(|s| s.kind == InstallStepKind::Fstab)
            .unwrap();
        assert!(fstab.command.join(" ").contains("/mnt/etc/fstab"));
        assert!(fstab.command.join(" ").contains("EULERFSTAB"));
    }

    #[test]
    fn plan_mount_has_mkdir() {
        let p = InstallPlan::new("/dev/sda", "euler", "euler").unwrap();
        let has_mkdir = p
            .steps
            .iter()
            .any(|s| s.command.contains(&"mkdir".to_string()));
        assert!(has_mkdir, "debe crear directorios con mkdir -p");
    }

    #[test]
    fn plan_no_sh_injection_device() {
        let p = InstallPlan::new("/dev/sda", "euler", "euler").unwrap();
        for step in &p.steps {
            if step.kind == InstallStepKind::Partition {
                if step.description.contains("Validar tamaño") {
                    assert_eq!(step.command[0], "sh");
                    assert!(step.command.join(" ").contains("blockdev"));
                    continue;
                }
                assert!(
                    step.command[0] == "sgdisk" || step.command[0] == "partprobe",
                    "partition step debe ser sgdisk/partprobe, fue {:?}",
                    step.command
                );
                assert_ne!(step.command[0], "sh");
            }
        }
    }
}

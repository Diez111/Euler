//! BTRFS layout profesional Euler.
//! Subvols flat (@siblings) + mount opts SSD + compresión.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Subvolume {
    pub name: String,
    pub mountpoint: String,
    pub options: String,
}

/// Layout BTRFS Euler — flat siblings en top-level id=5
pub fn euler_subvolumes() -> Vec<Subvolume> {
    vec![
        Subvolume {
            name: "@".to_string(),
            mountpoint: "/".to_string(),
            options: "subvol=@,compress=zstd:1,noatime,ssd,discard=async,space_cache=v2,commit=30"
                .to_string(),
        },
        Subvolume {
            name: "@home".to_string(),
            mountpoint: "/home".to_string(),
            options: "subvol=@home,compress=zstd:1,noatime,ssd,discard=async,space_cache=v2"
                .to_string(),
        },
        Subvolume {
            name: "@snapshots".to_string(),
            mountpoint: "/.snapshots".to_string(),
            options: "subvol=@snapshots,noatime,ssd,discard=async,space_cache=v2".to_string(),
        },
        Subvolume {
            name: "@var_log".to_string(),
            mountpoint: "/var/log".to_string(),
            options: "subvol=@var_log,compress=zstd:1,noatime,ssd,discard=async,space_cache=v2"
                .to_string(),
        },
        Subvolume {
            name: "@var_cache".to_string(),
            mountpoint: "/var/cache".to_string(),
            options: "subvol=@var_cache,compress=zstd:1,noatime,ssd,discard=async,space_cache=v2"
                .to_string(),
        },
    ]
}

/// Comandos btrfs subvol create
pub fn subvol_create_commands(mnt: &str) -> Vec<String> {
    euler_subvolumes()
        .iter()
        .map(|s| format!("btrfs subvol create {}/{}", mnt, s.name))
        .collect()
}

/// fstab entries para UUID/mapper
pub fn fstab_entries(device: &str) -> Vec<String> {
    let header = "# Euler BTRFS — generado por instalador\n".to_string();
    let mut out = vec![header];
    // EFI se añade aparte por instalador
    for sv in euler_subvolumes() {
        let mount = if sv.mountpoint == "/" {
            "/"
        } else {
            &sv.mountpoint
        };
        let fsck = "0 0";
        // device es /dev/mapper/luks-euler o UUID=
        out.push(format!(
            "{}  {}  btrfs  {}  {}",
            device, mount, sv.options, fsck
        ));
    }
    out.push("tmpfs  /tmp  tmpfs  defaults,noatime,mode=1777  0  0".to_string());
    out
}

/// mkfs.btrfs comando profesional
pub fn mkfs_command(device: &str) -> Vec<String> {
    vec![
        "mkfs.btrfs".to_string(),
        "-L".to_string(),
        "EULER".to_string(),
        "--csum".to_string(),
        "xxhash".to_string(),
        "-m".to_string(),
        "dup".to_string(),
        "-d".to_string(),
        "single".to_string(),
        device.to_string(),
    ]
}

/// Validación mount options contienen requeridos
pub fn validate_mount_options(opts: &str) -> bool {
    opts.contains("compress=zstd:1") && opts.contains("ssd") && opts.contains("space_cache=v2")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subvolumes_flat() {
        let svs = euler_subvolumes();
        assert_eq!(svs.len(), 5);
        assert!(svs.iter().any(|s| s.name == "@"));
        assert!(svs.iter().any(|s| s.name == "@home"));
        assert!(!svs.iter().any(|s| s.name.contains('/')));
    }

    #[test]
    fn fstab_contains_compress() {
        let entries = fstab_entries("/dev/mapper/luks-euler");
        let root = entries.iter().find(|e| e.contains(" /  btrfs")).unwrap();
        assert!(root.contains("compress=zstd:1"));
        assert!(root.contains("discard=async"));
    }

    #[test]
    fn mkfs_dup_single() {
        let cmd = mkfs_command("/dev/mapper/luks-euler");
        assert!(cmd.contains(&"dup".to_string()));
        assert!(cmd.contains(&"single".to_string()));
        assert!(cmd.contains(&"xxhash".to_string()));
    }

    #[test]
    fn validate_opts() {
        assert!(validate_mount_options("compress=zstd:1,ssd,space_cache=v2"));
        assert!(!validate_mount_options("compress=zstd:3,ssd"));
    }

    #[test]
    fn subvol_commands_count() {
        assert_eq!(subvol_create_commands("/mnt").len(), 5);
    }
}

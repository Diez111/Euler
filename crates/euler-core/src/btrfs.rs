//! BTRFS layout profesional Euler.
//! Subvols flat (@siblings) + mount opts SSD + compresión.
//!
//! Optimización alloc zero-copy:
//! - `EULER_SUBVOLS_DATA` es `&'static [(&'static str, ...)]` — 0 heap, backing store const
//! - `euler_subvolumes_static()` expone vista zero-copy para hot paths (install, fstab)
//! - `euler_subvolumes()` mantiene compat API `Vec<Subvolume>` clonando desde const con `Vec::with_capacity`
//! - `subvol_create_argv` evita `Path::join` + `to_string_lossy().to_string()` (2 allocs) via `String::with_capacity` + single `push`
//! - `fstab_entries` pre-reserva `Vec::with_capacity(1+len)` y construye líneas con `with_capacity` en lugar de `format!` sin size hint

use crate::hw::HwProfile;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Subvolume {
    pub name: String,
    pub mountpoint: String,
    pub options: String,
}

/// Backing store estático zero-copy — sin heap, inline &'static str
/// Usar `euler_subvolumes_static()` para 0 alloc; `euler_subvolumes()` clona a owned si se necesita `Vec<Subvolume>`.
pub const EULER_SUBVOLS_DATA: &[(&str, &str, &str)] = &[
    (
        "@",
        "/",
        "subvol=@,compress=zstd:1,noatime,ssd,discard=async,space_cache=v2,commit=30",
    ),
    (
        "@home",
        "/home",
        "subvol=@home,compress=zstd:1,noatime,ssd,discard=async,space_cache=v2",
    ),
    (
        "@snapshots",
        "/.snapshots",
        "subvol=@snapshots,noatime,ssd,discard=async,space_cache=v2",
    ),
    (
        "@var_log",
        "/var/log",
        "subvol=@var_log,compress=zstd:1,noatime,ssd,discard=async,space_cache=v2",
    ),
    (
        "@var_cache",
        "/var/cache",
        "subvol=@var_cache,compress=zstd:1,noatime,ssd,discard=async,space_cache=v2",
    ),
];

/// Número de subvolúmenes (const para pre-alloc)
pub const EULER_SUBVOLUME_COUNT: usize = 5;

/// Vista estática zero-copy — 0 heap allocation.
/// Retorna `&'static [(&'static str, &'static str, &'static str)]` donde tuple = (name, mountpoint, options).
#[inline]
pub fn euler_subvolumes_static() -> &'static [(&'static str, &'static str, &'static str)] {
    EULER_SUBVOLS_DATA
}

/// Layout BTRFS Euler — flat siblings en top-level id=5
/// Compat: retorna `Vec<Subvolume>` owned para `install.rs`. Internamente clona desde `EULER_SUBVOLS_DATA` con capacidad exacta.
#[allow(clippy::unnecessary_to_owned)]
pub fn euler_subvolumes() -> Vec<Subvolume> {
    let mut out = Vec::with_capacity(EULER_SUBVOLS_DATA.len());
    for &(name, mountpoint, options) in EULER_SUBVOLS_DATA {
        out.push(Subvolume {
            name: name.to_string(),
            mountpoint: mountpoint.to_string(),
            options: options.to_string(),
        });
    }
    out
}

/// Comandos btrfs subvol create (strings legacy, `join(" ")` para log/display)
/// Nota: legacy helper — preferir `subvol_create_argv` para ejecución sin shell.
pub fn subvol_create_commands(mnt: &str) -> Vec<String> {
    let argv = subvol_create_argv(mnt);
    let mut out = Vec::with_capacity(argv.len());
    for v in &argv {
        out.push(v.join(" "));
    }
    out
}

/// Argv vectors sin shell para subvol create — zero-copy optimizado.
/// Antes: `euler_subvolumes()` (15 allocs) + `Path::join` (PathBuf alloc) + `to_string_lossy().to_string()` (2ª alloc) por subvol.
/// Ahora: itera `EULER_SUBVOLS_DATA` (0 alloc), construye path con `String::with_capacity(mnt.len()+1+name.len())` (1 alloc/path).
#[allow(clippy::vec_init_then_push)]
pub fn subvol_create_argv(mnt: &str) -> Vec<Vec<String>> {
    let data = euler_subvolumes_static();
    let mut out = Vec::with_capacity(data.len());
    let mnt_trimmed = mnt.trim_end_matches('/');
    for &(name, _, _) in data {
        let path = if mnt_trimmed.is_empty() {
            // mnt == "/" o "" -> "/@name"
            let mut p = String::with_capacity(1 + name.len());
            p.push('/');
            p.push_str(name);
            p
        } else {
            let mut p = String::with_capacity(mnt_trimmed.len() + 1 + name.len());
            p.push_str(mnt_trimmed);
            p.push('/');
            p.push_str(name);
            p
        };
        let v = vec![
            "btrfs".to_string(),
            "subvol".to_string(),
            "create".to_string(),
            path,
        ];
        out.push(v);
    }
    out
}

/// fstab entries para UUID/mapper — zero-copy optimizado
/// Nota: no incluye tmpfs para /tmp — se usa zram1 ext2 de zram-generator (ver zram-generator.conf).
/// Si zram no está disponible, el instalador añade tmpfs como fallback.
/// Optimizado: `Vec::with_capacity(1+len)` + `String::with_capacity(device+mount+options)` por línea (1 alloc/línea vs format! sin hint)
/// Compat: mantiene firma original; para variante sensible a HW usar `fstab_entries_for_hw`.
pub fn fstab_entries(device: &str) -> Vec<String> {
    fstab_entries_for_hw(device, None)
}

/// fstab responsive a hardware — alterna `ssd`/`discard` según `HwProfile::has_nvme`.
/// - Si `hw` es `Some` y `!has_nvme` (HDD / eMMC / generic) elimina `ssd,` y `discard=async` para evitar penalización.
/// - Si `hw` es `None` o `has_nvme=true` (NVMe/SSD) conserva opciones SSD-optimizadas.
///
/// Mantiene `fstab_entries` para compat; esta variante es la que debe usar el instalador cuando dispone de `HwProfile`.
pub fn fstab_entries_for_hw(device: &str, hw: Option<HwProfile>) -> Vec<String> {
    let has_nvme = hw.as_ref().map(|h| h.has_nvme).unwrap_or(true);
    let data = euler_subvolumes_static();
    let mut out = Vec::with_capacity(1 + data.len());
    out.push("# Euler BTRFS — generado por instalador\n".to_string());
    for &(_, mountpoint, options) in data {
        let opts = if has_nvme {
            options.to_string()
        } else {
            // HDD: quitar ssd y discard=async (sin coma colgante)
            let mut s = options.replace("ssd,", "");
            s = s.replace(",ssd", "");
            s = s.replace("ssd", "");
            s = s.replace("discard=async,", "");
            s = s.replace(",discard=async", "");
            s = s.replace("discard=async", "");
            // limpiar ",," y ", " residuales
            while s.contains(",,") {
                s = s.replace(",,", ",");
            }
            s = s.replace(", ", ",");
            // evitar ",," al final/inicio por reemplazos
            s.trim_matches(',').to_string()
        };
        let mut line =
            String::with_capacity(device.len() + 2 + mountpoint.len() + 9 + opts.len() + 4);
        line.push_str(device);
        line.push_str("  ");
        line.push_str(mountpoint);
        line.push_str("  btrfs  ");
        line.push_str(&opts);
        line.push_str("  0 0");
        out.push(line);
    }
    out
}

/// fstab entries fallback con tmpfs (usar si zram deshabilitado) — pre-reserva para evitar realloc
pub fn fstab_entries_with_tmpfs(device: &str) -> Vec<String> {
    fstab_entries_with_tmpfs_for_hw(device, None)
}

/// variante HW-aware de `fstab_entries_with_tmpfs`
pub fn fstab_entries_with_tmpfs_for_hw(device: &str, hw: Option<HwProfile>) -> Vec<String> {
    let mut out = fstab_entries_for_hw(device, hw);
    // fstab_entries_for_hw ya reserva 1+len, necesitamos 1 más para tmpfs
    out.reserve(1);
    out.push("tmpfs  /tmp  tmpfs  defaults,noatime,mode=1777  0  0".to_string());
    out
}

/// mkfs.btrfs comando profesional — incluye -f para sobreescribir
pub fn mkfs_command(device: &str) -> Vec<String> {
    vec![
        "mkfs.btrfs".to_string(),
        "-f".to_string(),
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

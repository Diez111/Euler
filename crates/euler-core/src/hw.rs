//! Euler hw detect — std-only, sin unwrap, fallback a lspci/lsusb.
use std::fmt;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuKind {
    Intel,
    Amd,
    Nvidia,
    VmVirtio,
    Unknown,
}
impl fmt::Display for GpuKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Intel => "Intel",
                Self::Amd => "Amd",
                Self::Nvidia => "Nvidia",
                Self::VmVirtio => "VmVirtio",
                Self::Unknown => "Unknown",
            }
        )
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WifiKind {
    Intel,
    Realtek,
    Atheros,
    Broadcom,
    None,
}
impl fmt::Display for WifiKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Intel => "Intel",
                Self::Realtek => "Realtek",
                Self::Atheros => "Atheros",
                Self::Broadcom => "Broadcom",
                Self::None => "None",
            }
        )
    }
}
#[derive(Debug, Clone)]
pub struct HwProfile {
    pub gpu: GpuKind,
    pub wifi: WifiKind,
    pub has_bluetooth: bool,
    pub has_printer: bool,
    pub has_nvme: bool,
    pub ram_mb: u32,
    pub cpu_vendor: String,
}
fn vendor_to_gpu(v: &str) -> GpuKind {
    match v.trim().to_ascii_lowercase().as_str() {
        "0x8086" => GpuKind::Intel,
        "0x1002" => GpuKind::Amd,
        "0x10de" => GpuKind::Nvidia,
        "0x1af4" => GpuKind::VmVirtio,
        _ => GpuKind::Unknown,
    }
}
fn detect_gpu() -> GpuKind {
    if let Ok(c) = fs::read_to_string("/sys/class/drm/card0/device/vendor") {
        let k = vendor_to_gpu(c.trim());
        if k != GpuKind::Unknown {
            return k;
        }
    }
    if let Ok(ents) = fs::read_dir("/sys/bus/pci/devices") {
        for ent in ents.flatten() {
            if let Ok(cls) = fs::read_to_string(ent.path().join("class")) {
                if cls.trim() == "0x030000" {
                    if let Ok(v) = fs::read_to_string(ent.path().join("vendor")) {
                        let k = vendor_to_gpu(v.trim());
                        if k != GpuKind::Unknown {
                            return k;
                        }
                    }
                }
            }
        }
    }
    if let Ok(out) = std::process::Command::new("lspci").output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).to_ascii_lowercase();
            if s.contains("intel") && (s.contains("vga") || s.contains("graphics")) {
                return GpuKind::Intel;
            }
            if s.contains("amd") || s.contains("radeon") || s.contains("ati") {
                return GpuKind::Amd;
            }
            if s.contains("nvidia") {
                return GpuKind::Nvidia;
            }
            if s.contains("virtio") || s.contains("vmware") || s.contains("qxl") {
                return GpuKind::VmVirtio;
            }
        }
    }
    GpuKind::Unknown
}
fn detect_cpu_vendor() -> String {
    if let Ok(c) = fs::read_to_string("/proc/cpuinfo") {
        for line in c.lines() {
            if let Some(rest) = line.strip_prefix("vendor_id") {
                if let Some(v) = rest.split(':').nth(1) {
                    let v = v.trim().to_string();
                    if !v.is_empty() {
                        return v;
                    }
                }
            }
        }
    }
    "Unknown".to_string()
}
fn detect_bluetooth() -> bool {
    if Path::new("/sys/class/bluetooth").exists() {
        return true;
    }
    if let Ok(ents) = fs::read_dir("/sys/bus/usb/devices") {
        for ent in ents.flatten() {
            for name in ["bDeviceClass", "bInterfaceClass"] {
                if let Ok(c) = fs::read_to_string(ent.path().join(name)) {
                    let t = c.trim();
                    if t == "e0" || t == "0xe0" || t == "224" {
                        return true;
                    }
                }
            }
        }
    }
    if let Ok(out) = std::process::Command::new("lsusb").output() {
        if out.status.success()
            && String::from_utf8_lossy(&out.stdout)
                .to_ascii_lowercase()
                .contains("bluetooth")
        {
            return true;
        }
    }
    false
}
fn detect_printer() -> bool {
    // Check USB device/interface class 7 (printer) via sysfs.
    // Sysfs values may be "07", "7", "0x07", etc.; normalize by stripping 0x and leading zeros.
    let is_printer_class = |s: &str| {
        let t = s.trim().to_ascii_lowercase();
        let hex = t.strip_prefix("0x").unwrap_or(&t);
        let stripped = hex.trim_start_matches('0');
        // after stripping "0", "07" -> "7", "00" -> "" (not printer)
        stripped == "7"
    };
    if let Ok(ents) = fs::read_dir("/sys/bus/usb/devices") {
        for ent in ents.flatten() {
            for name in ["bDeviceClass", "bInterfaceClass"] {
                if let Ok(c) = fs::read_to_string(ent.path().join(name)) {
                    if is_printer_class(c.trim()) {
                        return true;
                    }
                }
            }
            // Some devices expose interface class deeper: .../ :1.0/bInterfaceClass
            // Check one level of subdirectories for interfaces if not already found.
            if let Ok(subs) = fs::read_dir(ent.path()) {
                for sub in subs.flatten() {
                    // only inspect entries like "1-1:1.0"
                    if let Some(fname) = sub.file_name().to_str() {
                        if fname.contains(':') {
                            if let Ok(c) = fs::read_to_string(sub.path().join("bInterfaceClass")) {
                                if is_printer_class(c.trim()) {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    // Check for /dev/usb/lp* character devices
    if let Ok(ents) = fs::read_dir("/dev/usb") {
        for ent in ents.flatten() {
            if let Some(n) = ent.file_name().to_str() {
                if n.starts_with("lp") {
                    return true;
                }
            }
        }
    }
    // Fallback: direct lp nodes at /dev (some systems)
    if Path::new("/dev/usb/lp0").exists()
        || Path::new("/dev/usb/lp1").exists()
        || Path::new("/dev/lp0").exists()
    {
        return true;
    }
    if let Ok(ents) = fs::read_dir("/dev") {
        for ent in ents.flatten() {
            if let Some(n) = ent.file_name().to_str() {
                if n.starts_with("lp") || (n.starts_with("usb") && n.contains("lp")) {
                    // heuristic for /dev/usb/lp* already covered, but also /dev/lp*
                    if n.starts_with("lp") {
                        return true;
                    }
                }
            }
        }
    }
    if let Ok(out) = std::process::Command::new("lsusb").output() {
        if out.status.success()
            && String::from_utf8_lossy(&out.stdout)
                .to_ascii_lowercase()
                .contains("printer")
        {
            return true;
        }
    }
    false
}
fn detect_wifi() -> WifiKind {
    if let Ok(ents) = fs::read_dir("/sys/class/net") {
        for ent in ents.flatten() {
            if let Some(n) = ent.file_name().to_str() {
                if !n.starts_with("wlan") {
                    continue;
                }
            } else {
                continue;
            }
            if let Ok(v) = fs::read_to_string(ent.path().join("device/vendor")) {
                match v.trim().to_ascii_lowercase().as_str() {
                    "0x8086" => return WifiKind::Intel,
                    "0x10ec" => return WifiKind::Realtek,
                    "0x168c" => return WifiKind::Atheros,
                    "0x14e4" => return WifiKind::Broadcom,
                    _ => {}
                }
            }
        }
    }
    if let Ok(out) = std::process::Command::new("lspci").output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).to_ascii_lowercase();
            if s.contains("intel") && s.contains("wireless") {
                return WifiKind::Intel;
            }
            if s.contains("realtek") {
                return WifiKind::Realtek;
            }
            if s.contains("atheros") || s.contains("qualcomm") {
                return WifiKind::Atheros;
            }
            if s.contains("broadcom") {
                return WifiKind::Broadcom;
            }
        }
    }
    if let Ok(out) = std::process::Command::new("lsusb").output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).to_ascii_lowercase();
            if s.contains("realtek") {
                return WifiKind::Realtek;
            }
            if s.contains("intel") {
                return WifiKind::Intel;
            }
        }
    }
    WifiKind::None
}
fn detect_ram_mb() -> u32 {
    if let Ok(c) = fs::read_to_string("/proc/meminfo") {
        for line in c.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                if let Some(kb_str) = rest.split_whitespace().next() {
                    if let Ok(kb) = kb_str.parse::<u32>() {
                        return kb / 1024;
                    }
                }
            }
        }
    }
    0
}
fn detect_nvme() -> bool {
    if let Ok(ents) = fs::read_dir("/sys/block") {
        for ent in ents.flatten() {
            if let Some(n) = ent.file_name().to_str() {
                if n.starts_with("nvme") {
                    return true;
                }
            }
        }
    }
    false
}
impl HwProfile {
    /// Detecta perfil actual sin panic (fallbacks Unknown/None/0).
    pub fn detect() -> Self {
        Self {
            gpu: detect_gpu(),
            wifi: detect_wifi(),
            has_bluetooth: detect_bluetooth(),
            has_printer: detect_printer(),
            has_nvme: detect_nvme(),
            ram_mb: detect_ram_mb(),
            cpu_vendor: detect_cpu_vendor(),
        }
    }
    /// Extra packages not in minbase (microcodes already in minbase, so omitted).
    pub fn extra_packages(&self) -> Vec<&'static str> {
        let mut v = Vec::new();
        if self.has_bluetooth {
            v.push("bluez");
            v.push("bluez-firmware");
        }
        v
    }
    /// Kernel cmdline additions per GPU.
    pub fn kernel_additions(&self) -> &'static str {
        match self.gpu {
            GpuKind::Intel => "i915.enable_guc=3 i915.enable_fbc=1",
            GpuKind::Amd => "amdgpu.ppfeaturemask=0xffffffff amd_pstate=active",
            _ => "",
        }
    }
    pub fn is_intel(&self) -> bool {
        self.gpu == GpuKind::Intel
    }
    pub fn is_amd(&self) -> bool {
        self.gpu == GpuKind::Amd
    }
    pub fn is_nvidia(&self) -> bool {
        self.gpu == GpuKind::Nvidia
    }
    pub fn is_vm(&self) -> bool {
        self.gpu == GpuKind::VmVirtio
    }
    pub fn is_printer(&self) -> bool {
        self.has_printer
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detect_no_panic() {
        let p = HwProfile::detect();
        assert!(matches!(
            p.gpu,
            GpuKind::Intel | GpuKind::Amd | GpuKind::Nvidia | GpuKind::VmVirtio | GpuKind::Unknown
        ));
    }
    #[test]
    fn detect_ram_cpu_no_panic() {
        let p = HwProfile::detect();
        let _ = p.ram_mb;
        assert!(!p.cpu_vendor.is_empty());
        let _ = p.extra_packages();
        let _ = p.kernel_additions();
    }
    #[test]
    fn kernel_and_helpers() {
        let mut p = HwProfile::detect();
        p.gpu = GpuKind::Intel;
        assert!(p.kernel_additions().contains("i915"));
        assert!(p.is_intel());
        p.gpu = GpuKind::Amd;
        assert!(p.kernel_additions().contains("amdgpu"));
        assert!(p.is_amd());
        p.gpu = GpuKind::Unknown;
        assert_eq!(p.kernel_additions(), "");
    }
    #[test]
    fn printer_detection_no_panic() {
        // Direct detector and helper must not panic, even without hardware.
        let has = detect_printer();
        let _ = has;
        let mut p = HwProfile::detect();
        let _ = p.has_printer;
        let _ = p.is_printer();
        // toggling should be consistent
        p.has_printer = true;
        assert!(p.is_printer());
        p.has_printer = false;
        assert!(!p.is_printer());
    }
}

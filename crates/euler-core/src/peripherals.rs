//! Periféricos Euler — detección impresora / escáner vía USB.
//! std-only, sin unwrap, fallbacks silenciosos.

use std::fmt;
use std::fs;
use std::path::Path;

/// Tipo de periférico detectado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeriphKind {
    Printer,
    Scanner,
    None,
}

impl fmt::Display for PeriphKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Printer => "Printer",
                Self::Scanner => "Scanner",
                Self::None => "None",
            }
        )
    }
}

/// Perfil de periféricos USB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeriphProfile {
    pub has_printer_usb: bool,
    pub has_scanner: bool,
}

impl PeriphProfile {
    /// Detecta impresora/escáner USB sin panic.
    /// - /sys/bus/usb/devices: bInterfaceClass 07 = printer, 06 = still image (scanner)
    /// - fallback: lsusb output contains "printer"/"scanner"
    pub fn detect() -> Self {
        Self {
            has_printer_usb: detect_printer_usb(),
            has_scanner: detect_scanner_usb(),
        }
    }

    /// Clasificación principal.
    pub fn kind(&self) -> PeriphKind {
        if self.has_printer_usb {
            PeriphKind::Printer
        } else if self.has_scanner {
            PeriphKind::Scanner
        } else {
            PeriphKind::None
        }
    }

    /// ¿Algún periférico presente?
    pub fn has_any(&self) -> bool {
        self.has_printer_usb || self.has_scanner
    }
}

/// Paquetes Debian para impresión (CUPS + drivers + GUI config).
pub const PRINTER_PACKAGES: &[&str] = &[
    "cups",
    "cups-browsed",
    "printer-driver-gutenprint",
    "system-config-printer",
];

/// Tamaño estimado en MiB de los paquetes de impresión.
#[inline]
pub fn printer_size_mb() -> u32 {
    45
}

/// Validación de perfil periférico.
/// Actualmente siempre Ok — reservado para futuras reglas (ej. impresora requiere cups).
/// Retorna Ok(()) si el perfil es consistente, Err(msg) si no.
pub fn validate_periph(profile: &PeriphProfile) -> Result<(), String> {
    // Placeholder: ningún perfil es inválido hoy. Mantener API estable.
    // Ejemplo futuro: si has_printer_usb y PRINTER_PACKAGES vacío -> Err.
    let _ = profile;
    if PRINTER_PACKAGES.is_empty() {
        return Err("PRINTER_PACKAGES vacío".to_string());
    }
    Ok(())
}

/// Alias sin argumentos para compatibilidad con spec mínima: valida perfil vacío.
pub fn validate_periph_default() -> Result<(), String> {
    validate_periph(&PeriphProfile {
        has_printer_usb: false,
        has_scanner: false,
    })
}

// ——— detección interna ———

fn detect_printer_usb() -> bool {
    // 1) sysfs: bInterfaceClass == 07 (printer) o 07/01
    if let Ok(ents) = fs::read_dir("/sys/bus/usb/devices") {
        for ent in ents.flatten() {
            let base = ent.path();
            // cada device puede tener múltiples interfaces: usbX:Y.Z
            // chequeo directo de bInterfaceClass / bDeviceClass
            for name in ["bInterfaceClass", "bDeviceClass"] {
                if let Ok(c) = fs::read_to_string(base.join(name)) {
                    let t = c.trim().to_ascii_lowercase();
                    // 07 es printer (decimal 7 o hex 07)
                    if t == "07" || t == "7" || t == "0x07" || t == "0x7" {
                        return true;
                    }
                }
            }
            // también revisar subdirectorios interfaz (ej. 1-2:1.0/bInterfaceClass)
            if let Ok(subs) = fs::read_dir(&base) {
                for sub in subs.flatten() {
                    if let Ok(c) = fs::read_to_string(sub.path().join("bInterfaceClass")) {
                        let t = c.trim().to_ascii_lowercase();
                        if t == "07" || t == "7" || t == "0x07" || t == "0x7" {
                            return true;
                        }
                    }
                }
            }
        }
    }
    // 2) lsusb fallback: buscar "printer"
    if let Ok(out) = std::process::Command::new("lsusb").output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).to_ascii_lowercase();
            if s.contains("printer") || s.contains("cups") {
                return true;
            }
        }
    }
    // 3) Check CUPS config already present implies printer support wanted — not detection, skip.
    // 4) legacy: /dev/usb/lp* exists
    if Path::new("/dev/usb").exists() {
        if let Ok(ents) = fs::read_dir("/dev/usb") {
            for e in ents.flatten() {
                if let Some(n) = e.file_name().to_str() {
                    if n.starts_with("lp") {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn detect_scanner_usb() -> bool {
    // sysfs: bInterfaceClass 06 = still image (scanner) o 255 vendor-specific con scanner
    if let Ok(ents) = fs::read_dir("/sys/bus/usb/devices") {
        for ent in ents.flatten() {
            let base = ent.path();
            for name in ["bInterfaceClass"] {
                if let Ok(c) = fs::read_to_string(base.join(name)) {
                    let t = c.trim().to_ascii_lowercase();
                    if t == "06" || t == "6" || t == "0x06" {
                        return true;
                    }
                }
            }
            if let Ok(subs) = fs::read_dir(&base) {
                for sub in subs.flatten() {
                    if let Ok(c) = fs::read_to_string(sub.path().join("bInterfaceClass")) {
                        let t = c.trim().to_ascii_lowercase();
                        if t == "06" || t == "6" || t == "0x06" {
                            return true;
                        }
                    }
                }
            }
        }
    }
    if let Ok(out) = std::process::Command::new("lsusb").output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).to_ascii_lowercase();
            if s.contains("scanner") || s.contains("epson") && s.contains("scanner") {
                return true;
            }
            // Many scanners report as "Canon", "Epson", "HP" without "scanner" keyword.
            // We avoid false positives: only trigger on explicit scanner word or SANE known path.
        }
    }
    // SANE config dir presence not reliable — skip.
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_no_panic_and_validate() {
        let p = PeriphProfile::detect();
        // no panic, fields are bool
        let _ = p.has_printer_usb;
        let _ = p.has_scanner;
        let _ = p.kind();
        let _ = p.has_any();
        // validate always Ok today
        assert!(validate_periph(&p).is_ok());
        assert!(validate_periph_default().is_ok());
        // PeriphKind display
        assert_eq!(PeriphKind::Printer.to_string(), "Printer");
        assert_eq!(PeriphKind::Scanner.to_string(), "Scanner");
        assert_eq!(PeriphKind::None.to_string(), "None");
    }

    #[test]
    fn printer_packages_and_size() {
        assert_eq!(printer_size_mb(), 45);
        assert!(PRINTER_PACKAGES.contains(&"cups"));
        assert!(PRINTER_PACKAGES.contains(&"cups-browsed"));
        assert!(PRINTER_PACKAGES.contains(&"printer-driver-gutenprint"));
        assert!(PRINTER_PACKAGES.contains(&"system-config-printer"));
        assert_eq!(PRINTER_PACKAGES.len(), 4);
        // validate_periph checks packages non-empty
        let empty_profile = PeriphProfile {
            has_printer_usb: false,
            has_scanner: false,
        };
        assert_eq!(empty_profile.kind(), PeriphKind::None);
        let printer_profile = PeriphProfile {
            has_printer_usb: true,
            has_scanner: false,
        };
        assert_eq!(printer_profile.kind(), PeriphKind::Printer);
        assert!(printer_profile.has_any());
        let scanner_profile = PeriphProfile {
            has_printer_usb: false,
            has_scanner: true,
        };
        assert_eq!(scanner_profile.kind(), PeriphKind::Scanner);
    }
}

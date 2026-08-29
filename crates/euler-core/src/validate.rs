//! Validación de entrada para instalador Euler.
//! Refactorizado: `validate_device` complejidad ≤10 via helpers enfocados + tabla lookup O(n).

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ValidateError {
    #[error("username inválido: {0}")]
    Username(String),
    #[error("hostname inválido: {0}")]
    Hostname(String),
    #[error("password inválido: {0}")]
    Password(String),
    #[error("device inválido: {0}")]
    Device(String),
}

// Nombres reservados del sistema (no permitidos como usuario regular).
const RESERVED_USERNAMES: &[&str] = &[
    "root",
    "daemon",
    "bin",
    "sys",
    "sync",
    "games",
    "man",
    "lp",
    "mail",
    "news",
    "uucp",
    "proxy",
    "www-data",
    "backup",
    "list",
    "irc",
    "gnats",
    "nobody",
    "systemd-network",
    "systemd-resolve",
    "systemd-timesync",
    "messagebus",
    "syslog",
    "_apt",
    "tss",
    "uuidd",
    "tcpdump",
    "avahi",
    "avahi-autoipd",
    "ssl-cert",
    "ssh",
    "sshd",
    "polkitd",
    "rtkit",
    "colord",
    "pulse",
    "pipewire",
    "gdm",
    "lightdm",
    "saned",
    "usbmux",
    "nogroup",
    "nologin",
];

#[inline]
fn is_reserved_username(name: &str) -> bool {
    if RESERVED_USERNAMES.contains(&name) {
        return true;
    }
    name.starts_with("avahi")
}

pub fn validate_username(name: &str) -> Result<(), ValidateError> {
    validate_username_len(name)?;
    validate_username_first_char(name)?;
    validate_username_charset(name)?;
    validate_username_not_reserved(name)
}

fn validate_username_len(name: &str) -> Result<(), ValidateError> {
    if name.is_empty() || name.len() > 32 {
        return Err(ValidateError::Username(format!(
            "'{name}' longitud inválida (1-32)"
        )));
    }
    Ok(())
}

fn validate_username_first_char(name: &str) -> Result<(), ValidateError> {
    let Some(first) = name.chars().next() else {
        return Err(ValidateError::Username(
            "'<vacío>' debe empezar con [a-z_], no con dígito o '-'".to_string(),
        ));
    };
    if !(first.is_ascii_lowercase() || first == '_') {
        return Err(ValidateError::Username(format!(
            "'{name}' debe empezar con [a-z_], no con dígito o '-'"
        )));
    }
    Ok(())
}

fn validate_username_charset(name: &str) -> Result<(), ValidateError> {
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err(ValidateError::Username(format!(
            "'{name}' solo permite [a-z0-9_-] en minúsculas"
        )));
    }
    Ok(())
}

fn validate_username_not_reserved(name: &str) -> Result<(), ValidateError> {
    if is_reserved_username(name) {
        return Err(ValidateError::Username(format!(
            "'{name}' es un nombre reservado del sistema"
        )));
    }
    Ok(())
}

pub fn validate_hostname(name: &str) -> Result<(), ValidateError> {
    validate_hostname_len(name)?;
    validate_hostname_charset(name)?;
    validate_hostname_dash(name)?;
    validate_hostname_not_numeric(name)
}

fn validate_hostname_len(name: &str) -> Result<(), ValidateError> {
    if name.is_empty() || name.len() > 63 {
        return Err(ValidateError::Hostname(format!(
            "'{name}' longitud inválida (1-63)"
        )));
    }
    Ok(())
}

fn validate_hostname_charset(name: &str) -> Result<(), ValidateError> {
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(ValidateError::Hostname(format!(
            "'{name}' solo permite alfanumérico y '-'"
        )));
    }
    Ok(())
}

fn validate_hostname_dash(name: &str) -> Result<(), ValidateError> {
    if name.starts_with('-') || name.ends_with('-') {
        return Err(ValidateError::Hostname(format!(
            "'{name}' no puede empezar o terminar con '-'"
        )));
    }
    Ok(())
}

fn validate_hostname_not_numeric(name: &str) -> Result<(), ValidateError> {
    if name.chars().all(|c| c.is_ascii_digit()) {
        return Err(ValidateError::Hostname(format!(
            "'{name}' no puede ser puramente numérico"
        )));
    }
    Ok(())
}

pub fn validate_password(pw: &str) -> Result<(), ValidateError> {
    validate_password_len(pw)?;
    validate_password_no_whitespace(pw)?;
    validate_password_no_control(pw)?;
    validate_password_no_forbidden(pw)
}

fn validate_password_len(pw: &str) -> Result<(), ValidateError> {
    if pw.len() < 8 {
        return Err(ValidateError::Password(
            "demasiado corto (mínimo 8 caracteres)".to_string(),
        ));
    }
    if pw.len() > 128 {
        return Err(ValidateError::Password(
            "demasiado largo (máximo 128 caracteres)".to_string(),
        ));
    }
    Ok(())
}

fn validate_password_no_whitespace(pw: &str) -> Result<(), ValidateError> {
    if pw.chars().any(|c| c.is_whitespace()) {
        return Err(ValidateError::Password(
            "no debe contener espacios ni caracteres de control".to_string(),
        ));
    }
    Ok(())
}

fn validate_password_no_control(pw: &str) -> Result<(), ValidateError> {
    if let Some(c) = pw.chars().find(|c| c.is_control()) {
        return Err(ValidateError::Password(format!(
            "contiene carácter de control '{}' (U+{:04X})",
            c.escape_debug(),
            c as u32
        )));
    }
    Ok(())
}

fn validate_password_no_forbidden(pw: &str) -> Result<(), ValidateError> {
    const FORBIDDEN_PW: &str = ":\"'`$\\;|&*?()[]{}<>";
    if let Some(c) = pw.chars().find(|c| FORBIDDEN_PW.contains(*c)) {
        return Err(ValidateError::Password(format!(
            "contiene carácter prohibido '{c}'"
        )));
    }
    Ok(())
}

/// Verifica si un path de device es válido sin devolver error detallado.
pub fn is_valid_device_path(dev: &str) -> bool {
    validate_device(dev).is_ok()
}

/// Helpers `validate_device` — flujo alto nivel ≤10 complejidad.
pub fn validate_device(dev: &str) -> Result<(), ValidateError> {
    validate_device_not_empty(dev)?;
    validate_device_prefix(dev)?;
    let suffix = extract_device_suffix(dev)?;
    validate_device_no_disk(dev)?;
    validate_device_no_dotdot(dev)?;
    validate_device_no_doubleslash(suffix, dev)?;
    validate_device_forbidden(dev)?;
    validate_device_charset(suffix, dev)?;
    Ok(())
}

fn validate_device_not_empty(dev: &str) -> Result<(), ValidateError> {
    if dev.is_empty() {
        return Err(ValidateError::Device("vacío".to_string()));
    }
    if dev.len() < 6 {
        return Err(ValidateError::Device(format!(
            "'{dev}' demasiado corto (mínimo 6, ej. /dev/sda)"
        )));
    }
    Ok(())
}

fn validate_device_prefix(dev: &str) -> Result<(), ValidateError> {
    if !dev.starts_with("/dev/") {
        return Err(ValidateError::Device(format!(
            "'{dev}' debe empezar con /dev/"
        )));
    }
    Ok(())
}

fn extract_device_suffix(dev: &str) -> Result<&str, ValidateError> {
    let suffix = &dev[5..];
    if suffix.is_empty() || suffix.starts_with('/') {
        return Err(ValidateError::Device(format!(
            "'{dev}' ruta inválida después de /dev/"
        )));
    }
    Ok(suffix)
}

fn validate_device_no_disk(dev: &str) -> Result<(), ValidateError> {
    if dev.contains("/disk/") {
        return Err(ValidateError::Device(format!(
            "'{dev}' no se permiten rutas /dev/disk/by-* usa /dev/sd* o /dev/nvme* directo"
        )));
    }
    Ok(())
}

fn validate_device_no_dotdot(dev: &str) -> Result<(), ValidateError> {
    if dev.contains("..") {
        return Err(ValidateError::Device(format!(
            "'{dev}' no puede contener '..'"
        )));
    }
    Ok(())
}

fn validate_device_no_doubleslash(suffix: &str, dev: &str) -> Result<(), ValidateError> {
    if suffix.contains("//") {
        return Err(ValidateError::Device(format!(
            "'{dev}' no puede contener '//'"
        )));
    }
    Ok(())
}

// ——— forbidden handling con lookup O(n) ———

#[inline]
fn is_forbidden_byte(b: u8) -> bool {
    matches!(
        b,
        b';' | b'&'
            | b'|'
            | b'$'
            | b'`'
            | b'('
            | b')'
            | b'>'
            | b'<'
            | b'\n'
            | b'\r'
            | b' '
            | b'\t'
            | b'*'
            | b'?'
            | b'"'
            | b'\''
            | b'\\'
    )
}

fn forbidden_display(b: u8) -> String {
    match b {
        b'\n' => "\\n".to_string(),
        b'\r' => "\\r".to_string(),
        b'\t' => "\\t".to_string(),
        b' ' => "espacio".to_string(),
        _ => format!("'{}'", b as char),
    }
}

fn validate_device_forbidden(dev: &str) -> Result<(), ValidateError> {
    for &b in dev.as_bytes() {
        if is_forbidden_byte(b) {
            let display = forbidden_display(b);
            return Err(ValidateError::Device(format!(
                "'{dev}' contiene carácter prohibido {display}"
            )));
        }
    }
    Ok(())
}

fn validate_device_charset(suffix: &str, dev: &str) -> Result<(), ValidateError> {
    // bytes path para SIMD auto-vectorización con target-cpu=x86-64-v3
    let ok = suffix.as_bytes().iter().all(|&b| {
        b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'/' || b == b'_' || b == b'-'
    });
    if !ok {
        return Err(ValidateError::Device(format!(
            "'{dev}' contiene caracteres no permitidos (solo [a-z0-9/_-])"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_username() {
        assert!(validate_username("euler").is_ok());
        assert!(validate_username("diez_123").is_ok());
    }

    #[test]
    fn invalid_username_upper() {
        assert!(validate_username("Euler").is_err());
    }

    #[test]
    fn invalid_username_digit_start() {
        assert!(validate_username("1euler").is_err());
    }

    #[test]
    fn invalid_username_dash_start() {
        assert!(validate_username("-euler").is_err());
    }

    #[test]
    fn invalid_username_upper2() {
        assert!(validate_username("Euler123").is_err());
    }

    #[test]
    fn invalid_username_reserved() {
        assert!(validate_username("root").is_err());
        assert!(validate_username("nobody").is_err());
        assert!(validate_username("daemon").is_err());
        assert!(validate_username("nologin").is_err());
        assert!(validate_username("www-data").is_err());
        assert!(validate_username("avahi").is_err());
        assert!(validate_username("avahi-autoipd").is_err());
    }

    #[test]
    fn invalid_username_too_long() {
        let long = "a".repeat(33);
        assert!(validate_username(&long).is_err());
    }

    #[test]
    fn valid_username_underscore_start() {
        assert!(validate_username("_euler").is_ok());
    }

    #[test]
    fn valid_username_max_len() {
        let max = "a".repeat(32);
        assert!(validate_username(&max).is_ok());
    }

    #[test]
    fn hostname_ok() {
        assert!(validate_hostname("euler-laptop").is_ok());
    }

    #[test]
    fn hostname_invalid_dash() {
        assert!(validate_hostname("-euler").is_err());
        assert!(validate_hostname("euler-").is_err());
    }

    #[test]
    fn hostname_invalid_numeric() {
        assert!(validate_hostname("12345").is_err());
        assert!(validate_hostname("007").is_err());
    }

    #[test]
    fn hostname_with_digits_ok() {
        assert!(validate_hostname("euler123").is_ok());
        assert!(validate_hostname("a1").is_ok());
    }

    #[test]
    fn hostname_too_long() {
        let long = "a".repeat(64);
        assert!(validate_hostname(&long).is_err());
    }

    #[test]
    fn password_too_short() {
        assert!(validate_password("abc").is_err());
        assert!(validate_password("abcd123").is_err());
        assert!(validate_password("abcd1234").is_ok());
        assert!(validate_password("12345678").is_ok());
    }

    #[test]
    fn password_whitespace_rejected() {
        assert!(validate_password("abcd 1234").is_err());
        assert!(validate_password("abcd\t1234").is_err());
        assert!(validate_password("abcd\n1234").is_err());
        assert!(validate_password(" abcd1234").is_err());
    }

    #[test]
    fn password_too_long() {
        let long = "a".repeat(129);
        assert!(validate_password(&long).is_err());
        let max = "a".repeat(128);
        assert!(validate_password(&max).is_ok());
    }

    #[test]
    fn device_ok() {
        assert!(validate_device("/dev/sda").is_ok());
        assert!(validate_device("/dev/sda1").is_ok());
        assert!(validate_device("/dev/nvme0n1").is_ok());
        assert!(validate_device("/dev/nvme0n1p1").is_ok());
        assert!(validate_device("/dev/vda").is_ok());
        assert!(validate_device("/dev/mmcblk0").is_ok());
        assert!(validate_device("/dev/mmcblk0p1").is_ok());
        assert!(validate_device("/dev/loop0").is_ok());
        assert!(is_valid_device_path("/dev/sda"));
        assert!(is_valid_device_path("/dev/nvme0n1p1"));
    }

    #[test]
    fn device_by_id_rejected() {
        assert!(validate_device("/dev/disk/by-id/test-disk").is_err());
        assert!(validate_device("/dev/disk/by-id/ata-Samsung_SSD").is_err());
        assert!(validate_device("/dev/disk/by-uuid/1234-ABCD").is_err());
        assert!(validate_device("/dev/disk/by-path/pci-0000:00").is_err());
        assert!(!is_valid_device_path("/dev/disk/by-id/test-disk"));
    }

    #[test]
    fn device_injection_rejected() {
        assert!(validate_device("/dev/sda; rm -rf").is_err());
        assert!(validate_device("/dev/sda; rm").is_err());
        assert!(validate_device("/dev/sda && reboot").is_err());
        assert!(validate_device("/dev/sda|cat /etc/passwd").is_err());
        assert!(validate_device("/dev/sda$(whoami)").is_err());
        assert!(validate_device("/dev/sda`whoami`").is_err());
        assert!(validate_device("/dev/sda> /tmp/foo").is_err());
        assert!(validate_device("/dev/sda*").is_err());
        assert!(validate_device("/dev/sda?").is_err());
        assert!(!is_valid_device_path("/dev/sda; rm"));
    }

    #[test]
    fn device_traversal_rejected() {
        assert!(validate_device("/dev/../etc/passwd").is_err());
        assert!(validate_device("/dev/sda/../../etc").is_err());
        assert!(validate_device("/dev//sda").is_err());
        assert!(validate_device("/dev/sda//p1").is_err());
    }

    #[test]
    fn device_must_be_under_dev() {
        assert!(validate_device("sda").is_err());
        assert!(validate_device("/tmp/sda").is_err());
        assert!(validate_device("").is_err());
        assert!(validate_device("/dev/").is_err());
        assert!(validate_device("/dev").is_err());
    }

    #[test]
    fn device_backslash_rejected() {
        assert!(validate_device("/dev/sda\\").is_err());
        assert!(validate_device("/dev/sda\"").is_err());
        assert!(validate_device("/dev/sda'").is_err());
    }

    #[test]
    fn password_forbidden_chars_rejected() {
        assert!(validate_password("abcd:1234").is_err());
        assert!(validate_password("abcd;1234").is_err());
        assert!(validate_password("abcd|1234").is_err());
        assert!(validate_password("abcd&1234").is_err());
        assert!(validate_password("abcd$1234").is_err());
        assert!(validate_password("abcd`1234").is_err());
        assert!(validate_password("abcd\"1234").is_err());
        assert!(validate_password("abcd'1234").is_err());
        assert!(validate_password("abcd\\1234").is_err());
        assert!(validate_password("abcd<1234").is_err());
        assert!(validate_password("abcd>1234").is_err());
        assert!(validate_password("abcd(1234").is_err());
        assert!(validate_password("abcd)1234").is_err());
        assert!(validate_password("abcd*1234").is_err());
        assert!(validate_password("abcd?1234").is_err());
        assert!(validate_password("abcd[1234").is_err());
        assert!(validate_password("abcd]1234").is_err());
        assert!(validate_password("abcd{1234").is_err());
        assert!(validate_password("abcd}1234").is_err());
    }

    #[test]
    fn password_control_char_rejected() {
        assert!(validate_password("abcd\x011234").is_err());
        assert!(validate_password("abcd\x7f1234").is_err());
        let err = validate_password("abcd\x011234").unwrap_err().to_string();
        assert!(err.contains("control") || err.contains("prohibido"));
    }

    #[test]
    fn password_complex_valid() {
        assert!(validate_password("S3cur3!Pass-2024").is_ok());
        assert!(validate_password("S3cur3!Pass_2024.").is_ok());
        assert!(validate_password("MyP@ss#2024!+-=").is_ok());
        assert!(validate_password("Abcdef!@#%^+=-_.,123").is_ok());
    }
}

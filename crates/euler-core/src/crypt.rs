//! LUKS2 configuración profesional Euler.
//! aes-xts-plain64 512b, argon2id, sector 4096.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptError {
    #[error("cryptsetup falló: {0}")]
    Failed(String),
    #[error("password vacío")]
    EmptyPassword,
    #[error("device inválido: {0}")]
    InvalidDevice(String),
}

#[derive(Debug, Clone)]
pub struct LuksConfig {
    pub device: String,
    pub mapper_name: String,
    pub cipher: String,
    pub key_size: u32,
    pub hash: String,
    pub pbkdf: String,
    pub sector_size: u32,
}

impl Default for LuksConfig {
    fn default() -> Self {
        Self {
            device: String::new(),
            mapper_name: "luks-euler".to_string(),
            cipher: "aes-xts-plain64".to_string(),
            key_size: 512,
            hash: "sha512".to_string(),
            pbkdf: "argon2id".to_string(),
            sector_size: 4096,
        }
    }
}

impl LuksConfig {
    pub fn new(device: &str) -> Result<Self, CryptError> {
        if device.is_empty() || !device.starts_with("/dev/") {
            return Err(CryptError::InvalidDevice(device.to_string()));
        }
        Ok(Self {
            device: device.to_string(),
            ..Default::default()
        })
    }

    /// Comando cryptsetup luksFormat.
    pub fn format_command(&self) -> Vec<String> {
        vec![
            "cryptsetup".to_string(),
            "luksFormat".to_string(),
            "--type".to_string(),
            "luks2".to_string(),
            "--cipher".to_string(),
            self.cipher.clone(),
            "--key-size".to_string(),
            self.key_size.to_string(),
            "--hash".to_string(),
            self.hash.clone(),
            "--pbkdf".to_string(),
            self.pbkdf.clone(),
            "--pbkdf-memory".to_string(),
            "1048576".to_string(),
            "--pbkdf-force-iterations".to_string(),
            "4".to_string(),
            "--sector-size".to_string(),
            self.sector_size.to_string(),
            "--label".to_string(),
            "EULER".to_string(),
            self.device.clone(),
        ]
    }

    /// Comando open con allow-discards.
    pub fn open_command(&self) -> Vec<String> {
        vec![
            "cryptsetup".to_string(),
            "open".to_string(),
            "--allow-discards".to_string(),
            self.device.clone(),
            self.mapper_name.clone(),
        ]
    }

    /// Path mapper.
    pub fn mapper_path(&self) -> String {
        format!("/dev/mapper/{}", self.mapper_name)
    }

    /// Entrada crypttab.
    pub fn crypttab_entry(&self, uuid: &str) -> String {
        format!(
            "{} UUID={} none luks,discard,no-read-workqueue,no-write-workqueue",
            self.mapper_name, uuid
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luks_format_contains_luks2() {
        let c = LuksConfig::new("/dev/nvme0n1p2").unwrap();
        let cmd = c.format_command();
        assert!(cmd.contains(&"luks2".to_string()));
        assert!(cmd.contains(&"argon2id".to_string()));
        assert!(cmd.contains(&"4096".to_string()));
    }

    #[test]
    fn luks_open_allow_discards() {
        let c = LuksConfig::new("/dev/sda2").unwrap();
        assert!(c.open_command().contains(&"--allow-discards".to_string()));
    }

    #[test]
    fn crypttab_entry_format() {
        let c = LuksConfig::new("/dev/sda2").unwrap();
        let e = c.crypttab_entry("1234-ABCD");
        assert!(e.contains("luks-euler"));
        assert!(e.contains("discard"));
        assert!(e.contains("no-read-workqueue"));
    }

    #[test]
    fn invalid_device() {
        assert!(LuksConfig::new("sda2").is_err());
    }

    #[test]
    fn mapper_path() {
        let c = LuksConfig::new("/dev/sda2").unwrap();
        assert_eq!(c.mapper_path(), "/dev/mapper/luks-euler");
    }
}

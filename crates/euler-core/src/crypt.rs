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
        crate::validate::validate_device(device)
            .map_err(|e| CryptError::InvalidDevice(e.to_string()))?;
        Ok(Self {
            device: device.to_string(),
            ..Default::default()
        })
    }

    /// Memoria PBKDF recomendada en MB según RAM disponible (MemAvailable).
    /// Escala argon2 para evitar OOM en live de 2G.
    pub fn recommended_pbkdf_memory_mb() -> u32 {
        use std::io::{BufRead, BufReader};
        let file = match std::fs::File::open("/proc/meminfo") {
            Ok(f) => f,
            Err(e) => {
                eprintln!("[warn] /proc/meminfo open failed: {e} -> default pbkdf 256MB");
                return 256;
            }
        };
        let mut reader = BufReader::new(file);
        let mut line = String::with_capacity(64);
        let mut avail_kb: Option<u64> = None;
        loop {
            line.clear();
            let bytes = match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) => {
                    eprintln!("[warn] /proc/meminfo read_line failed: {e} -> default pbkdf 256MB");
                    return 256;
                }
            };
            if bytes == 0 {
                break;
            }
            if let Some(rest) = line.strip_prefix("MemAvailable:") {
                // Formato: "MemAvailable:    123456 kB"
                let mut parts = rest.split_whitespace();
                if let Some(kb_str) = parts.next() {
                    if let Ok(kb) = kb_str.parse::<u64>() {
                        avail_kb = Some(kb);
                    }
                }
                break;
            }
        }
        let avail_kb = match avail_kb {
            Some(v) => v,
            None => {
                eprintln!("[warn] MemAvailable not found in /proc/meminfo -> default pbkdf 256MB");
                return 256;
            }
        };
        if avail_kb >= 8 * 1024 * 1024 {
            1024
        } else if avail_kb >= 4 * 1024 * 1024 {
            512
        } else {
            256
        }
    }

    /// Comando cryptsetup luksFormat.
    /// Nota: la passphrase se pasa via stdin `--key-file -` (el daemon hace piping).
    /// El password NO debe aparecer en argv ni en el plan JSON.
    pub fn format_command(&self) -> Vec<String> {
        let memory_kb = Self::recommended_pbkdf_memory_mb() * 1024;
        self.format_command_with_memory(memory_kb)
    }

    /// Variante testeable que permite inyectar memoria PBKDF en kB.
    pub fn format_command_with_memory(&self, memory_kb: u32) -> Vec<String> {
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
            memory_kb.to_string(),
            "--pbkdf-force-iterations".to_string(),
            "4".to_string(),
            "--sector-size".to_string(),
            self.sector_size.to_string(),
            "--label".to_string(),
            "EULER".to_string(),
            "--key-file".to_string(),
            "-".to_string(),
            "--batch-mode".to_string(),
            self.device.clone(),
        ]
    }

    /// Comando cryptsetup luksFormat con key-file externo (alternativa).
    pub fn format_command_with_keyfile(&self, keyfile: &str) -> Vec<String> {
        let mut cmd = self.format_command();
        // reemplaza "-" por keyfile
        if let Some(pos) = cmd.iter().position(|x| x == "-") {
            cmd[pos] = keyfile.to_string();
        }
        cmd
    }

    /// Comando open con allow-discards y key-file stdin (evita prompt TTY).
    pub fn open_command(&self) -> Vec<String> {
        vec![
            "cryptsetup".to_string(),
            "open".to_string(),
            "--allow-discards".to_string(),
            "--key-file".to_string(),
            "-".to_string(),
            self.device.clone(),
            self.mapper_name.clone(),
        ]
    }

    /// Valida formato UUID (36 hex con guiones) o placeholder permitido.
    #[inline]
    #[allow(clippy::needless_range_loop)]
    pub fn is_valid_uuid(uuid: &str) -> bool {
        if uuid == "UUID-REEMPLAZAR" {
            return true;
        }
        if uuid.len() != 36 {
            return false;
        }
        let b = uuid.as_bytes();
        // Evita `chars().enumerate().all` + `contains([8,13,18,23])` por iteración;
        // chequeo directo sobre bytes + `is_ascii_hexdigit` (u8).
        for (i, &ch) in b.iter().enumerate() {
            if i == 8 || i == 13 || i == 18 || i == 23 {
                if ch != b'-' {
                    return false;
                }
            } else if !ch.is_ascii_hexdigit() {
                return false;
            }
        }
        true
    }

    /// Path mapper.
    #[inline]
    pub fn mapper_path(&self) -> String {
        format!("/dev/mapper/{}", self.mapper_name)
    }

    /// Entrada crypttab. Valida UUID a menos que sea placeholder.
    pub fn crypttab_entry(&self, uuid: &str) -> String {
        // Permitir placeholder durante generación de plan; daemon lo reemplaza
        if uuid != "UUID-REEMPLAZAR" && !Self::is_valid_uuid(uuid) {
            // Fallback: no panic, pero log — caller debe validar antes
            eprintln!("[warn] UUID inválido para crypttab: {}", uuid);
        }
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

    #[test]
    fn recommended_pbkdf_returns_allowed_value() {
        let mb = LuksConfig::recommended_pbkdf_memory_mb();
        assert!(
            mb == 256 || mb == 512 || mb == 1024,
            "recommended_pbkdf_memory_mb() returned unexpected value {mb}"
        );
        let kb = mb * 1024;
        assert!(
            kb == 262144 || kb == 524288 || kb == 1048576,
            "kb value {kb} not in allowed set"
        );
    }

    #[test]
    fn luks_format_memory_is_allowed_set() {
        let c = LuksConfig::new("/dev/nvme0n1p2").unwrap();
        let cmd = c.format_command();
        assert!(cmd.contains(&"argon2id".to_string()));
        assert!(cmd.contains(&"4096".to_string()));
        // verifica que pbkdf-memory sea uno de los valores permitidos
        let pos = cmd.iter().position(|x| x == "--pbkdf-memory").unwrap();
        let val = &cmd[pos + 1];
        assert!(
            val == "262144" || val == "524288" || val == "1048576",
            "pbkdf-memory unexpected value {val}"
        );
        // mantiene iterations 4
        let pos2 = cmd
            .iter()
            .position(|x| x == "--pbkdf-force-iterations")
            .unwrap();
        assert_eq!(cmd[pos2 + 1], "4");
    }

    #[test]
    fn luks_format_with_memory_injection() {
        let c = LuksConfig::new("/dev/sda2").unwrap();
        for &mem in &[262144u32, 524288, 1048576] {
            let cmd = c.format_command_with_memory(mem);
            assert!(cmd.contains(&"argon2id".to_string()));
            assert!(cmd.contains(&"4096".to_string()));
            let pos = cmd.iter().position(|x| x == "--pbkdf-memory").unwrap();
            assert_eq!(cmd[pos + 1], mem.to_string());
            let pos2 = cmd
                .iter()
                .position(|x| x == "--pbkdf-force-iterations")
                .unwrap();
            assert_eq!(cmd[pos2 + 1], "4");
        }
        // valor arbitrario también debe inyectarse correctamente
        let cmd = c.format_command_with_memory(123456);
        let pos = cmd.iter().position(|x| x == "--pbkdf-memory").unwrap();
        assert_eq!(cmd[pos + 1], "123456");
    }
}

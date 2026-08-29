//! Validación de entrada para instalador Euler.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ValidateError {
    #[error("username inválido: {0}")]
    Username(String),
    #[error("hostname inválido: {0}")]
    Hostname(String),
    #[error("password vacío o muy corto")]
    Password,
    #[error("device inválido: {0}")]
    Device(String),
}

pub fn validate_username(name: &str) -> Result<(), ValidateError> {
    if name.is_empty() || name.len() > 32 {
        return Err(ValidateError::Username(name.to_string()));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err(ValidateError::Username(name.to_string()));
    }
    if name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return Err(ValidateError::Username(name.to_string()));
    }
    Ok(())
}

pub fn validate_hostname(name: &str) -> Result<(), ValidateError> {
    if name.is_empty() || name.len() > 63 {
        return Err(ValidateError::Hostname(name.to_string()));
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(ValidateError::Hostname(name.to_string()));
    }
    if name.starts_with('-') || name.ends_with('-') {
        return Err(ValidateError::Hostname(name.to_string()));
    }
    Ok(())
}

pub fn validate_password(pw: &str) -> Result<(), ValidateError> {
    if pw.len() < 4 {
        return Err(ValidateError::Password);
    }
    Ok(())
}

pub fn validate_device(dev: &str) -> Result<(), ValidateError> {
    if dev.is_empty() || !dev.starts_with("/dev/") {
        return Err(ValidateError::Device(dev.to_string()));
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
    fn hostname_ok() {
        assert!(validate_hostname("euler-laptop").is_ok());
    }

    #[test]
    fn hostname_invalid_dash() {
        assert!(validate_hostname("-euler").is_err());
    }

    #[test]
    fn password_too_short() {
        assert!(validate_password("abc").is_err());
        assert!(validate_password("abcd").is_ok());
    }
}

//! Progreso tipado para instalador.
//! Unifica con `InstallStatus::Running` — `Progress` es la representación local sin serializar enum.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Progress {
    pub current: usize,
    pub total: usize,
    pub message: String,
}

impl Progress {
    #[inline]
    pub fn new(current: usize, total: usize, message: impl Into<String>) -> Self {
        Self {
            current,
            total,
            message: message.into(),
        }
    }

    #[inline]
    #[allow(clippy::manual_checked_ops)]
    pub fn percent(&self) -> u8 {
        if self.total == 0 {
            0
        } else {
            ((self.current * 100 + self.total / 2) / self.total).min(100) as u8
        }
    }

    #[inline]
    pub fn is_done(&self) -> bool {
        self.current >= self.total && self.total != 0
    }

    #[inline]
    pub fn from_status(status: &crate::InstallStatus) -> Option<Self> {
        match status {
            crate::InstallStatus::Running {
                step,
                total,
                message,
            } => Some(Self {
                current: *step,
                total: *total,
                message: message.clone(),
            }),
            _ => None,
        }
    }

    #[inline]
    pub fn to_status(&self) -> crate::InstallStatus {
        crate::InstallStatus::Running {
            step: self.current,
            total: self.total,
            message: self.message.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_half() {
        let p = Progress::new(5, 10, "test");
        assert_eq!(p.percent(), 50);
    }

    #[test]
    fn percent_zero_total() {
        let p = Progress::new(0, 0, "x");
        assert_eq!(p.percent(), 0);
    }

    #[test]
    fn is_done_true() {
        let p = Progress::new(10, 10, "done");
        assert!(p.is_done());
    }
}

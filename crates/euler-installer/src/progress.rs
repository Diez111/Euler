//! Progreso tipado para instalador.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progress {
    pub current: usize,
    pub total: usize,
    pub message: String,
}

impl Progress {
    pub fn new(current: usize, total: usize, message: impl Into<String>) -> Self {
        Self {
            current,
            total,
            message: message.into(),
        }
    }

    pub fn percent(&self) -> u8 {
        if self.total == 0 {
            return 0;
        }
        ((self.current as f64 / self.total as f64) * 100.0).clamp(0.0, 100.0) as u8
    }

    pub fn is_done(&self) -> bool {
        self.current >= self.total
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

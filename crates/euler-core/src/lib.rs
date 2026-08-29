//! Euler core — lógica de particionado, LUKS2 y BTRFS.
//! Sin dependencias de UI; usado por daemon y tests.

pub mod btrfs;
pub mod codecs;
pub mod crypt;
pub mod disk;
pub mod hw;
pub mod install;
pub mod peripherals;
pub mod validate;

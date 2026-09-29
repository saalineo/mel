//! # MEL Loader
//!
//! Binary and executable loaders (flat binary, ELF64).

pub mod raw;

pub use mel_core::{Address, MelError, MelResult};
pub use raw::{load_raw_binary, load_raw_binary_from_file};

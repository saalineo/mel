//! # MEL Scenarios
//!
//! Challenge scenarios, objective parsing, validation, and auto-grading engine.

pub mod schema;

pub use mel_core::{Address, Architecture, MelError, MelResult};
pub use schema::{Constraint, Objective, Scenario};

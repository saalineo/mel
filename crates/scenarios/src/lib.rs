//! # MEL Scenarios
//!
//! Challenge scenarios, objective parsing, validation, and auto-grading engine.

pub mod compiler_challenge;
pub mod schema;
pub mod validator;

pub use compiler_challenge::{CompilerChallenge, Expr};
pub use mel_core::{Address, Architecture, MelError, MelResult};
pub use schema::{Constraint, Objective, Scenario};
pub use validator::{ObjectiveEvaluation, ObjectiveStatus, ScenarioReport, ScenarioValidator};

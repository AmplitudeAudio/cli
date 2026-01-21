//! Amplitude CLI library crate.
//!
//! This module exposes public APIs for testing and external use.

pub mod common;
pub mod database;
pub mod input;
pub mod presentation;

// Re-export sudo commands for testing (project commands depend on binary-only app module)
pub mod commands {
    pub mod sudo;
}

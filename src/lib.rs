//! Amplitude CLI library crate.
//!
//! This module exposes public APIs for testing and external use.

pub mod app;
pub mod common;
pub mod database;
pub mod input;
pub mod presentation;

// Re-export commands for testing
pub mod commands {
    pub mod project;
    pub mod sudo;
}

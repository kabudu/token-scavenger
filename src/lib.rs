//! TokenScavenger — Lightweight, self-hosted LLM proxy/router.
//!
//! This library crate exports the public API for testing and external use.
//! The main binary entry point is in `main.rs`.

pub mod api;
pub mod app;
pub mod cli;
pub mod config;
pub mod db;
pub mod discovery;
pub mod metrics;
pub mod providers;
pub mod resilience;
pub mod router;
pub mod ui;
pub mod usage;
pub mod util;

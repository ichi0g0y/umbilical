//! Umbilical core: finds target directories and keeps one
//! `claude remote-control` running in each of them.
//!
//! This crate has no GUI code, so a headless mode can use it later.

pub mod activity;
pub mod child;
pub mod config;
pub mod daemon;
pub mod detect;
pub mod discovery;
pub mod env;
pub mod supervisor;
pub mod trust;

pub use config::Config;
pub use supervisor::{Command, DirStatus, RunState, Snapshot, Supervisor};

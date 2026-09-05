//! arka-pulse — ArkaOS reliability engine (library).
//!
//! The binary is a thin driver; the engine lives here so it can be consumed
//! behind a service interface (see [`service::ReliabilityService`]) exactly as
//! the rest of ArkaOS is meant to talk to swappable Arka services rather than
//! concrete implementations. See `docs/RELIABILITY-ARKA-PULSE.md`.

pub mod detect;
pub mod explain;
pub mod model;
pub mod monitor;
pub mod predict;
pub mod recover;
pub mod service;
pub mod verify;

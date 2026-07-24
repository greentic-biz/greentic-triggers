#![forbid(unsafe_code)]

//! Time-based trigger schedules for the Greentic platform.
//!
//! Defines `TriggerSchedule` (a typed recurrence vocabulary with a raw-cron
//! escape hatch), compilation to cron expressions, a pure `next_fire`
//! computation, and `TriggerDef` (a schedule bound to a business-event it
//! emits). This crate is the contract + pure logic; the running scheduler
//! lives in operax (SP3).

pub mod schedule;
pub use schedule::{TimeOfDay, TriggerSchedule};

pub mod def;
pub use def::{TriggerDef, validate_trigger};

//! Shared fakes for the API integration test suites.
//!
//! Only genuinely identical fakes live here — small null-object repository
//! implementations that several suites wire in verbatim. Scenario-specific
//! doubles (the `Fixture*` repositories, which happen to share names across
//! suites but carry different seed data, method surfaces, and assertions)
//! stay local to their suite on purpose.
//!
//! Each suite that needs these declares `mod support;` and imports what it
//! uses. `dead_code` is allowed because any given test binary only exercises
//! a subset of the module.
#![allow(dead_code)]

pub mod inventory;
pub mod workspace;

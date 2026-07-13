//! Local Gantt schedule plans for the SIM office suite.
//!
//! The crate owns the local schedule model and SQLite backend used before any
//! vendor project placement is involved. Dependency analysis is routed through
//! `sim-lib-discrete-graph` so task links stay on the shared graph spine.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod critical;
pub mod local;
pub mod model;

pub use critical::{ScheduleError, critical_tasks};
pub use local::GanttStore;
pub use model::{GANTT_DOC_KIND, GanttPlan, LinkKind, Task, TaskLink};

/// Cookbook recipes for this lib, embedded at build time.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));

#[cfg(test)]
mod recipe_tests {
    use crate::RECIPES;

    #[test]
    fn recipes_export_local_gantt_plan() {
        let cards = sim_cookbook::recipes_from_embedded(RECIPES).unwrap();
        assert!(
            cards
                .iter()
                .any(|card| card.id.ends_with("local-gantt-plan"))
        );
    }
}

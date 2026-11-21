//! Deterministic, seeded discrete-event simulation engine.
//!
//! `run` turns a project into a [`SimReport`]. The same project and options
//! always produce the same report, on every platform.

mod engine;
pub mod live;
mod model;
mod report;
mod rng;
#[cfg(test)]
mod tests;

use crate::domain::project::Project;
use crate::domain::validation::ValidationError;

pub use report::{SimOptions, SimReport};

#[derive(Debug, thiserror::Error)]
pub enum SimError {
    #[error(transparent)]
    Invalid(#[from] ValidationError),
    /// The topology cannot be simulated as drawn.
    #[error("{0}")]
    Model(String),
    #[error("a live run is already in progress")]
    Busy,
    #[error("no live run is in progress")]
    NoActiveRun,
    #[error("the simulation exceeded {limit} events; shorten the run or lower the request rate")]
    EventLimit { limit: u64 },
}

/// Validates the inputs, compiles the model and runs it to the end of `options.duration_s`.
pub fn run(project: &Project, options: &SimOptions) -> Result<SimReport, SimError> {
    project.validate()?;
    options.validate()?;
    let model = model::Model::compile(project)?;
    engine::run(&model, options)
}

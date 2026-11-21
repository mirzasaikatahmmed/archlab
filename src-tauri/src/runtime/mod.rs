//! Things that touch the real machine: host detection, calibration benchmarks and real mode.

pub mod calibrate;
pub mod detect;
pub mod jobs;
pub mod real;

use crate::domain::validation::ValidationError;
use crate::sim::SimError;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Invalid(#[from] ValidationError),
    #[error(transparent)]
    Sim(#[from] SimError),
    #[error("{0} is already running")]
    Busy(&'static str),
    #[error("no {0} is running")]
    NothingToCancel(&'static str),
    #[error("{0}")]
    Failed(String),
}

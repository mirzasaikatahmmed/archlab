//! Pure domain types and validation: projects, server and traffic profiles,
//! config templates and topology. No I/O lives here.

pub mod app_info;
pub mod calibration;
pub mod chaos;
#[cfg(test)]
mod limits_sync;
pub mod project;
pub mod server_profile;
pub mod template;
pub mod topology;
pub mod traffic;
pub mod validation;
pub mod whatif;

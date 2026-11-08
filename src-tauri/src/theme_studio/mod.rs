//! Portable theme source and local catalog projects; no installation writes.
pub mod adapters;
pub mod capabilities;
#[cfg(test)]
mod capability_tests;
pub(crate) mod commands;
mod json;
pub mod model;
pub mod projects;
pub mod validation;

#[cfg(test)]
mod project_tests;
#[cfg(test)]
mod tests;

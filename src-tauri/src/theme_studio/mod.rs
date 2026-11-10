//! Portable theme source and local catalog projects; no installation writes.
pub mod adapters;
pub mod capabilities;
pub mod export;
#[cfg(test)]
mod capability_tests;
pub(crate) mod commands;
pub mod image_import;
pub mod image_store;
mod json;
pub mod model;
pub mod projects;
pub mod validation;
/// Your own video clips (ADR-117).
pub mod video_store;

#[cfg(test)]
mod project_tests;
#[cfg(test)]
mod tests;

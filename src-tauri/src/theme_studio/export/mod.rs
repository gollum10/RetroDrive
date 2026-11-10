//! Theme compilation and packaging. Writing happens only in `save` (Save for RetroBat, ADR-113),
//! and only into a new folder.
pub mod retrobat;
#[cfg(test)]
mod tests;
pub mod package;
/// Save for RetroBat to a new folder (ADR-113).
pub mod save;
#[cfg(test)]
mod package_tests;
#[cfg(test)]
mod native_fixture;

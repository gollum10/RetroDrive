//! Candidate compilation only: no filesystem, IPC or production adapter registration.
pub mod retrobat;
#[cfg(test)]
mod tests;
pub mod package;
#[cfg(test)]
mod package_tests;
#[cfg(test)]
mod native_fixture;

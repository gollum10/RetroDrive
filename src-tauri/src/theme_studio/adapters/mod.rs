//! Theme capability contracts are independent of the game-copy PlatformAdapter.
use super::capabilities::{Outcome, ProfileKey};

pub struct CapabilityRule {
    pub outcome: Outcome,
    pub reason: String,
    pub fallback: Option<String>,
    pub evidence: String,
}

/// Pure analysis only. Compiler/IR interfaces arrive with the first verified exporter.
/// An implementation cannot receive a writable installation/staging path here.
pub trait ThemeAdapter: Send + Sync {
    fn key(&self) -> &ProfileKey;
    fn capability(&self, feature: &str) -> Option<CapabilityRule>;
}

/// No real frontend/version has passed a native theme smoke test yet.
pub fn registry() -> Vec<Box<dyn ThemeAdapter>> {
    Vec::new()
}

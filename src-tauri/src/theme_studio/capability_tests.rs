use super::{
    adapters::{self, CapabilityRule, ThemeAdapter},
    capabilities::*,
    validation::parse_project,
};
use serde_json::Value;

fn profile(name: &str) -> TargetProfile {
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/theme_studio/target-profiles.json"
    ))
    .unwrap();
    serde_json::from_value(fixtures[name].clone()).unwrap()
}
struct Mock {
    key: ProfileKey,
    evidence: bool,
}
impl ThemeAdapter for Mock {
    fn key(&self) -> &ProfileKey {
        &self.key
    }
    fn capability(&self, feature: &str) -> Option<CapabilityRule> {
        let (outcome, fallback) = match feature {
            "node.text" => (Outcome::Native, None),
            "layout.position_size" => (Outcome::Convert, Some("Synthetic baked layout")),
            "binding.game.title" => (Outcome::Degraded, Some("Synthetic placeholder title")),
            "scene.startup" => (Outcome::Unsupported, None),
            _ => return None,
        };
        Some(CapabilityRule {
            outcome,
            fallback: fallback.map(str::to_string),
            reason: "Synthetic test rule only".into(),
            evidence: if self.evidence {
                "synthetic fixture; not a native frontend test".into()
            } else {
                String::new()
            },
        })
    }
}
fn mock() -> Mock {
    Mock {
        key: profile("exact").engine.unwrap(),
        evidence: true,
    }
}
fn project() -> super::model::ThemeProject {
    parse_project(include_str!(
        "../../tests/fixtures/theme_studio/starter.json"
    ))
    .unwrap()
}

#[test]
fn exact_adapter_reports_all_five_outcomes_and_preserves_source() {
    let project = project();
    let before = serde_json::to_string(&project).unwrap();
    let adapter = mock();
    let report = analyze(&project, profile("exact"), &[&adapter]);
    assert!(report.adapter_matched);
    assert!(
        !report.can_export && !report.can_deploy,
        "analysis is not an exporter"
    );
    for outcome in [
        Outcome::Native,
        Outcome::Convert,
        Outcome::Degraded,
        Outcome::Unsupported,
        Outcome::Unknown,
    ] {
        assert!(report.issues.iter().any(|i| i.outcome == outcome));
    }
    for issue in &report.issues {
        assert_eq!(
            issue.requires_consent,
            matches!(issue.outcome, Outcome::Convert | Outcome::Degraded)
        );
        if issue.requires_consent {
            assert!(issue.fallback.is_some());
        }
    }
    assert_eq!(serde_json::to_string(&project).unwrap(), before);
    let json = serde_json::to_value(report).unwrap();
    assert_eq!(json["canExport"], false);
    assert!(json["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["outcome"] == "native"));
}

#[test]
fn unknown_and_cross_frontend_profiles_never_use_a_near_match() {
    let adapter = mock();
    for name in ["unknown", "mismatch"] {
        let report = analyze(&project(), profile(name), &[&adapter]);
        assert!(!report.adapter_matched);
        assert!(report.issues.iter().all(|i| i.outcome == Outcome::Unknown));
    }
    for field in [
        "frontendVersion",
        "frontendFork",
        "os",
        "architecture",
        "themeFormatVersion",
    ] {
        let mut changed = serde_json::to_value(profile("exact")).unwrap();
        changed["engine"][field] = serde_json::json!("different");
        assert!(
            !analyze(
                &project(),
                serde_json::from_value(changed).unwrap(),
                &[&adapter]
            )
            .adapter_matched
        );
    }
    let mut unknown = profile("exact");
    unknown.engine.as_mut().unwrap().frontend_version = "*".into();
    let wildcard = Mock {
        key: unknown.engine.clone().unwrap(),
        evidence: true,
    };
    assert!(!analyze(&project(), unknown, &[&wildcard]).adapter_matched);
}

#[test]
fn ambiguous_or_unevidenced_rules_fail_closed() {
    let a = mock();
    let b = mock();
    assert!(!analyze(&project(), profile("exact"), &[&a, &b]).adapter_matched);
    let no_evidence = Mock {
        key: a.key,
        evidence: false,
    };
    assert!(analyze(&project(), profile("exact"), &[&no_evidence])
        .issues
        .iter()
        .all(|i| i.outcome == Outcome::Unknown));
    assert!(
        adapters::registry().is_empty(),
        "game copying is not proof of theme export support"
    );
}

#[test]
fn extensions_are_reported_without_modifying_or_omitting_editable_source() {
    let project = parse_project(include_str!(
        "../../tests/fixtures/theme_studio/future-extensions.json"
    ))
    .unwrap();
    let before = serde_json::to_value(&project).unwrap();
    let report = analyze(&project, intent(&project), &[]);
    for feature in [
        "extension.animations",
        "extension.audio",
        "extension.video",
        "extension.navigation",
        "node.future_hologram",
        "extension.futureSnap",
    ] {
        assert!(
            report.issues.iter().any(|i| i.feature == feature),
            "{feature}"
        );
    }
    assert_eq!(serde_json::to_value(&project).unwrap(), before);
    assert!(
        report.profile.frontend_family.is_none(),
        "universal editing requires no target"
    );
}

#[test]
fn discovery_reads_release_metadata_but_does_not_invent_an_engine_or_write() {
    use crate::{
        db, installations,
        volumes::{Presence, VolumeProbe},
    };
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("RetroBat");
    for path in ["emulationstation", "roms", "system"] {
        std::fs::create_dir_all(root.join(path)).unwrap();
    }
    std::fs::write(root.join("RetroBat.exe"), b"").unwrap();
    std::fs::write(root.join("retrobat.ini"), b"").unwrap();
    std::fs::write(root.join("system/version.info"), b"8.3.1.1-stable-win64").unwrap();
    let mut c = rusqlite::Connection::open_in_memory().unwrap();
    db::migrate(&mut c, None).unwrap();
    let id = installations::add(&c, &root, "retrobat", &|_| VolumeProbe::unknown()).unwrap();
    let before = c.total_changes();
    let found = discover(&c, &|_, _| Presence::Connected).unwrap();
    assert_eq!(found[0].installation_id.as_deref(), Some(id.as_str()));
    assert_eq!(
        found[0].application_version.as_deref(),
        Some("8.3.1.1-stable-win64")
    );
    assert!(found[0].engine.is_none());
    assert_eq!(found[0].observation, "detected");
    assert_eq!(c.total_changes(), before);
    assert_eq!(
        std::fs::read(root.join("system/version.info")).unwrap(),
        b"8.3.1.1-stable-win64"
    );
    assert!(!root.join(".retrodrive").exists());
    let unavailable = discover(&c, &|_, _| Presence::NotConnected).unwrap();
    assert_eq!(unavailable[0].observation, "unavailable");
    assert!(unavailable[0].application_version.is_none() && unavailable[0].engine.is_none());
}

#[test]
fn long_reports_are_bounded_and_never_enable_export() {
    let mut project = project();
    let node = project.scenes[3].nodes[0].clone();
    project.scenes[0].nodes = (0..600)
        .map(|n| {
            let mut node = node.clone();
            node.id = format!("layer-{n}");
            node
        })
        .collect();
    let report = analyze(&project, intent(&project), &[]);
    assert_eq!(report.issues.len(), 1000);
    assert!(report.omitted_issues > 0);
    assert!(!report.can_export && !report.can_deploy);
}

use super::{adapters::ThemeAdapter, model::*};
use crate::{adapters, installations, volumes::Presence};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileKey {
    pub frontend_family: String,
    pub frontend_fork: String,
    pub frontend_version: String,
    pub os: String,
    pub architecture: String,
    pub theme_format_version: String,
}

impl ProfileKey {
    fn complete(&self) -> bool {
        [
            &self.frontend_family,
            &self.frontend_fork,
            &self.frontend_version,
            &self.os,
            &self.architecture,
            &self.theme_format_version,
        ]
        .iter()
        .all(|s| !s.trim().is_empty() && !s.contains('*') && !s.eq_ignore_ascii_case("unknown"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetProfile {
    /// Local binding only. Never inserted into portable project JSON.
    pub installation_id: Option<String>,
    pub frontend_family: Option<String>,
    pub application_version: Option<String>,
    pub observation: String,
    /// Application release detection does not establish a verified theme engine.
    pub engine: Option<ProfileKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Native,
    Convert,
    Degraded,
    Unsupported,
    Unknown,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityIssue {
    pub id: String,
    pub scene_id: Option<String>,
    pub node_id: Option<String>,
    pub feature: String,
    pub target: Option<String>,
    pub outcome: Outcome,
    pub severity: String,
    pub reason: String,
    pub fallback: Option<String>,
    pub evidence: Option<String>,
    pub requires_consent: bool,
    pub user_action: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilityReport {
    pub profile: TargetProfile,
    pub adapter_matched: bool,
    pub can_export: bool,
    pub can_deploy: bool,
    pub issues: Vec<CapabilityIssue>,
    pub omitted_issues: usize,
}

pub fn intent(project: &ThemeProject) -> TargetProfile {
    TargetProfile {
        installation_id: None,
        frontend_family: project.targets.first().map(|t| t.frontend_family.clone()),
        application_version: None,
        observation: "project_intent".into(),
        engine: None,
    }
}

/// Reads existing saved locations and their detection metadata; never claims a share ID.
pub fn discover(
    connection: &Connection,
    presence: installations::PresenceFn,
) -> Result<Vec<TargetProfile>, String> {
    Ok(installations::located(connection, presence, None)?
        .into_iter()
        .filter(|i| ["retrobat", "batocera", "esde"].contains(&i.adapter_id.as_str()))
        .map(|i| {
            let live = if i.presence == Presence::Connected {
                adapters::find(&i.adapter_id).map(|a| a.detect(&i.root))
            } else {
                None
            };
            let matched = live
                .as_ref()
                .is_some_and(|d| d.confidence == adapters::Confidence::Detected);
            let observation = if i.presence != Presence::Connected {
                "unavailable"
            } else if matched {
                "detected"
            } else {
                "unrecognized"
            };
            TargetProfile {
                installation_id: Some(i.id),
                frontend_family: Some(i.adapter_id),
                application_version: live.filter(|_| matched).and_then(|d| d.version),
                observation: observation.into(),
                engine: None,
            }
        })
        .collect())
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Need {
    scene: Option<String>,
    node: Option<String>,
    feature: String,
}

fn needs(project: &ThemeProject) -> BTreeSet<Need> {
    let mut result = BTreeSet::new();
    let mut add = |scene: Option<&str>, node: Option<&str>, feature: String| {
        result.insert(Need {
            scene: scene.map(str::to_string),
            node: node.map(str::to_string),
            feature,
        });
    };
    for key in project.extra.keys().chain(project.metadata.extra.keys()) {
        add(None, None, format!("extension.{key}"));
    }
    if !project.tokens.is_empty() {
        add(None, None, "style.tokens".into());
    }
    if !project.variants.is_empty() {
        add(None, None, "scene.variants".into());
    }
    for target in &project.targets {
        for key in target.extra.keys() {
            add(None, None, format!("extension.target.{key}"));
        }
    }
    for asset in &project.assets {
        add(None, None, format!("media.{}", asset.media_type));
        for key in asset.extra.keys() {
            add(None, None, format!("extension.asset.{key}"));
        }
    }
    for scene in &project.scenes {
        add(Some(&scene.id), None, format!("scene.{}", scene.role));
        for key in scene.extra.keys().chain(scene.resolution.extra.keys()) {
            add(Some(&scene.id), None, format!("extension.{key}"));
        }
        for node in &scene.nodes {
            let s = Some(scene.id.as_str());
            let n = Some(node.id.as_str());
            add(s, n, format!("node.{}", node.node_type));
            add(s, n, "layout.position_size".into());
            if [
                &node.layout.x,
                &node.layout.y,
                &node.layout.width,
                &node.layout.height,
            ]
            .iter()
            .any(|d| d.unit == "%")
            {
                add(s, n, "layout.relative".into());
            }
            if node.layout.rotation != 0.0 {
                add(s, n, "layout.rotation".into());
            }
            for key in node.style.keys() {
                add(s, n, format!("style.{key}"));
            }
            for binding in node.bindings.values() {
                add(s, n, format!("binding.{}", binding.source));
                for key in binding.extra.keys() {
                    add(s, n, format!("extension.binding.{key}"));
                }
            }
            for key in node
                .extra
                .keys()
                .chain(node.layout.extra.keys())
                .chain(node.layout.x.extra.keys())
                .chain(node.layout.y.extra.keys())
                .chain(node.layout.width.extra.keys())
                .chain(node.layout.height.extra.keys())
            {
                add(s, n, format!("extension.{key}"));
            }
        }
    }
    result
}

/// Exact tuple matching; ambiguous or absent evidence fails closed. Source is borrowed.
pub fn analyze(
    project: &ThemeProject,
    profile: TargetProfile,
    registry: &[&dyn ThemeAdapter],
) -> CompatibilityReport {
    let matching: Vec<_> = registry
        .iter()
        .filter(|a| {
            profile.engine.as_ref().is_some_and(|key| {
                key.complete()
                    && key == a.key()
                    && Some(&key.frontend_family) == profile.frontend_family.as_ref()
            })
        })
        .collect();
    let adapter = if matching.len() == 1 {
        Some(*matching[0])
    } else {
        None
    };
    let required = needs(project);
    let omitted_issues = required.len().saturating_sub(1000);
    let issues = required.into_iter().take(1000).enumerate().map(|(index, need)| {
        let rule = adapter.and_then(|a|a.capability(&need.feature));
        // Unsupported evidence is never upgraded to native based solely on an app release.
        let rule = rule.filter(|r| !r.evidence.trim().is_empty() && (!matches!(r.outcome, Outcome::Convert|Outcome::Degraded) || r.fallback.as_ref().is_some_and(|f| !f.trim().is_empty())));
        let outcome = rule.as_ref().map(|r|r.outcome).unwrap_or(Outcome::Unknown);
        let severity = match outcome { Outcome::Native=>"info", Outcome::Convert|Outcome::Degraded=>"warning", _=>"error" };
        let user_action = match outcome { Outcome::Native=>"No fallback required.", Outcome::Convert|Outcome::Degraded=>"Review the fallback and explicitly accept its differences before a future build.", Outcome::Unsupported=>"Change the feature or target; source will be preserved.", Outcome::Unknown=>"Capture and verify the exact frontend engine/version/platform before native export." };
        CapabilityIssue { id:format!("capability-{index}"),scene_id:need.scene,node_id:need.node,feature:need.feature,target:profile.frontend_family.clone(),outcome,severity:severity.into(),reason:rule.as_ref().map(|r|r.reason.clone()).unwrap_or_else(||"No unique, evidenced theme adapter matches this exact target profile and feature.".into()),fallback:rule.as_ref().and_then(|r|r.fallback.clone()),evidence:rule.map(|r|r.evidence),requires_consent:matches!(outcome,Outcome::Convert|Outcome::Degraded),user_action:user_action.into() }
    }).collect();
    CompatibilityReport {
        profile,
        adapter_matched: adapter.is_some(),
        can_export: false,
        can_deploy: false,
        issues,
        omitted_issues,
    }
}

//! Structural checks only: no resource fetching, SVG rendering, asset decoding or writes.
use super::model::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_DOCUMENT_BYTES: usize = 2 * 1024 * 1024;
const MAX_NODES: usize = 10_000;
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub code: String,
    pub path: String,
    pub message: String,
}

fn issue(code: &str, path: &str, message: &str) -> Issue {
    Issue {
        code: code.into(),
        path: path.into(),
        message: message.into(),
    }
}

/// Parse bounded portable source. On error callers must retain the original input;
/// no recovery path is allowed to replace it with a default/empty document.
pub fn parse_project(json: &str) -> Result<ThemeProject, Vec<Issue>> {
    if json.len() > MAX_DOCUMENT_BYTES {
        return Err(vec![issue(
            "document_size",
            "$",
            "Project exceeds the 2 MiB source limit.",
        )]);
    }
    let super::json::UniqueJson(raw) =
        serde_json::from_str(json).map_err(|e| vec![issue("invalid_json", "$", &e.to_string())])?;
    check_version(&raw)?;
    check_portability(&raw)?;
    let project: ThemeProject = serde_json::from_str(json)
        .map_err(|e| vec![issue("invalid_shape", "$", &e.to_string())])?;
    validate_project(&project)?;
    Ok(project)
}

fn check_version(raw: &Value) -> Result<(), Vec<Issue>> {
    if raw.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err(vec![issue(
            "format",
            "$.format",
            "Not a RetroDrive theme source document.",
        )]);
    }
    // No migrations exist yet. Even a newer minor may change required semantics.
    if raw.get("schemaVersion").and_then(Value::as_str) != Some(SCHEMA_VERSION) {
        return Err(vec![issue(
            "schema_version",
            "$.schemaVersion",
            "Unsupported schema; keep the source unchanged and open with a compatible app.",
        )]);
    }
    Ok(())
}

fn absolute_location(text: &str) -> bool {
    let t = text.trim_start();
    let b = t.as_bytes();
    t.starts_with(['/', '\\'])
        || (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':')
        || t.get(..5).is_some_and(|s| s.eq_ignore_ascii_case("file:"))
}

fn check_portability(raw: &Value) -> Result<(), Vec<Issue>> {
    let mut pending = vec![(raw, 0)];
    while let Some((value, depth)) = pending.pop() {
        if depth > MAX_DEPTH {
            return Err(vec![issue(
                "document_depth",
                "$",
                "Project nesting exceeds 64 levels.",
            )]);
        }
        match value {
            Value::Number(n) if n.as_f64().is_none_or(|v| v.abs() > 9_007_199_254_740_991.0) => {
                return Err(vec![issue(
                    "number_range",
                    "$",
                    "Portable numbers must fit JavaScript's safe numeric range.",
                )]);
            }
            Value::String(s) if absolute_location(s) => {
                return Err(vec![issue("absolute_path", "$", "Portable source cannot contain absolute paths or file URLs, including extension fields.")]);
            }
            Value::Array(a) => pending.extend(a.iter().map(|v| (v, depth + 1))),
            Value::Object(o) => {
                if o.keys().any(|k| absolute_location(k)) {
                    return Err(vec![issue(
                        "absolute_path",
                        "$",
                        "Portable source keys cannot contain absolute paths.",
                    )]);
                }
                pending.extend(o.values().map(|v| (v, depth + 1)));
            }
            _ => {}
        }
    }
    Ok(())
}

fn id_valid(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}

fn text_valid(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max && !value.chars().any(char::is_control)
}

/// Validate constructed documents too; serialization must not turn NaN into null.
pub fn validate_project(project: &ThemeProject) -> Result<(), Vec<Issue>> {
    // Bound work even for callers constructing a document without parse_project.
    if project.scenes.len() > 128
        || project.assets.len() > 2000
        || project.targets.len() > 16
        || project.scenes.iter().map(|s| s.nodes.len()).sum::<usize>() > MAX_NODES
    {
        return Err(vec![issue(
            "document_count",
            "$",
            "Project exceeds scene, asset, target or node limits.",
        )]);
    }
    let mut errors = Vec::new();
    macro_rules! check {
        ($ok:expr, $code:expr, $path:expr, $message:expr) => {
            if !$ok {
                errors.push(issue($code, $path, $message));
            }
        };
    }
    check!(
        project.format == FORMAT,
        "format",
        "$.format",
        "Unexpected format."
    );
    check!(
        project.schema_version == SCHEMA_VERSION,
        "schema_version",
        "$.schemaVersion",
        "Unsupported schema."
    );
    check!(id_valid(&project.id), "id", "$.id", "Invalid project ID.");
    check!(
        text_valid(&project.metadata.name, 120),
        "name",
        "$.metadata.name",
        "Name must contain 1–120 readable characters."
    );
    check!(
        text_valid(&project.metadata.version, 64),
        "version",
        "$.metadata.version",
        "Version is required (up to 64 characters)."
    );
    check!(
        text_valid(&project.metadata.license, 256),
        "license",
        "$.metadata.license",
        "License must be explicit, including unknown or custom."
    );
    check!(
        project.scenes.len() <= 128 && !project.scenes.is_empty(),
        "scene_count",
        "$.scenes",
        "A project needs 1–128 scenes."
    );
    check!(
        project.assets.len() <= 2000,
        "asset_count",
        "$.assets",
        "At most 2,000 assets."
    );
    check!(
        project.targets.len() <= 16,
        "target_count",
        "$.targets",
        "At most 16 target intents."
    );
    for target in &project.targets {
        check!(
            id_valid(&target.frontend_family),
            "target",
            "$.targets",
            "Invalid frontend family identifier."
        );
        check!(
            target
                .frontend_version
                .as_ref()
                .is_none_or(|s| text_valid(s, 64)),
            "target",
            "$.targets",
            "A target version is null or readable version text."
        );
    }
    let mut asset_ids = BTreeSet::new();
    for asset in &project.assets {
        check!(
            id_valid(&asset.id) && asset_ids.insert(asset.id.as_str()),
            "asset_id",
            "$.assets",
            "Asset IDs must be valid and unique."
        );
        check!(
            asset.sha256.len() == 64
                && asset
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "asset_hash",
            "$.assets",
            "Expected lowercase SHA-256."
        );
        check!(
            asset.bytes <= 9_007_199_254_740_991,
            "asset_size",
            "$.assets",
            "Byte count exceeds JavaScript's safe integer range."
        );
        let ext = match asset.media_type.as_str() {
            "image/png" => Some(("images", vec!["png"])),
            "image/jpeg" => Some(("images", vec!["jpg", "jpeg"])),
            "image/webp" => Some(("images", vec!["webp"])),
            "image/svg+xml" => Some(("images", vec!["svg"])),
            "video/mp4" => Some(("video", vec!["mp4"])),
            "audio/ogg" => Some(("audio", vec!["ogg"])),
            "audio/mpeg" => Some(("audio", vec!["mp3"])),
            _ => None,
        };
        let safe = ext.is_some_and(|(folder, exts)| {
            exts.iter()
                .any(|e| asset.path == format!("assets/{folder}/{}.{e}", asset.sha256))
        });
        check!(
            safe,
            "asset_path",
            "$.assets",
            "Asset path must match its media type and content hash in the project assets folder."
        );
        check!(
            text_valid(&asset.license, 256),
            "license",
            "$.assets",
            "Record asset license, even if unknown."
        );
    }
    let mut scene_ids = BTreeSet::new();
    let mut node_ids = BTreeSet::new();
    let mut total_nodes = 0;
    for scene in &project.scenes {
        let path = format!("$.scenes[{}]", scene.id);
        check!(
            id_valid(&scene.id) && scene_ids.insert(scene.id.as_str()),
            "scene_id",
            &path,
            "Scene IDs must be valid and unique."
        );
        check!(
            id_valid(&scene.role),
            "scene_role",
            &path,
            "Invalid scene role identifier."
        );
        check!(
            (1..=16384).contains(&scene.resolution.width)
                && (1..=16384).contains(&scene.resolution.height),
            "resolution",
            &path,
            "Logical resolution must be 1–16,384 pixels per axis."
        );
        total_nodes += scene.nodes.len();
        let nodes: BTreeMap<&str, &Node> = scene.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        let mut parent_counts = BTreeMap::new();
        for node in &scene.nodes {
            let path = format!("{path}.nodes[{}]", node.id);
            check!(
                id_valid(&node.id) && node_ids.insert(node.id.as_str()),
                "node_id",
                &path,
                "Node IDs must be valid and project-wide unique."
            );
            check!(
                id_valid(&node.node_type),
                "node_type",
                &path,
                "Invalid node type identifier."
            );
            for (dimension, positive) in [
                (&node.layout.x, false),
                (&node.layout.y, false),
                (&node.layout.width, true),
                (&node.layout.height, true),
            ] {
                check!(
                    ["px", "%"].contains(&dimension.unit.as_str())
                        && dimension.value.is_finite()
                        && dimension.value.abs() <= 1_000_000.0
                        && (!positive || dimension.value > 0.0),
                    "dimension",
                    &path,
                    "Dimensions need px/% units, finite bounded values and positive size."
                );
            }
            check!(
                node.layout.rotation.is_finite() && node.layout.rotation.abs() <= 36000.0,
                "rotation",
                &path,
                "Rotation must be finite degrees within ±36,000."
            );
            for child in &node.children {
                check!(
                    nodes.contains_key(child.as_str()),
                    "child_reference",
                    &path,
                    "Child must exist in the same scene."
                );
                let count = parent_counts.entry(child.as_str()).or_insert(0);
                *count += 1;
                check!(
                    *count == 1,
                    "multiple_parents",
                    &path,
                    "A node can have only one parent and one position in its child list."
                );
            }
            for asset in &node.assets {
                check!(
                    asset_ids.contains(asset.as_str()),
                    "asset_reference",
                    &path,
                    "Node refers to a missing asset."
                );
            }
            for binding in node.bindings.values() {
                let kind = match binding.source.as_str() {
                    "game.title" | "game.description" | "system.name" => Some("text"),
                    "game.year" | "game.players" | "game.rating" => Some("number"),
                    "game.artwork" => Some("image"),
                    // The selected game's own video (ADR-111); played by the system, never stored in the theme.
                    "game.video" => Some("video"),
                    _ => None,
                };
                check!(
                    kind == Some(binding.kind.as_str()),
                    "binding",
                    &path,
                    "Binding must use a known typed data source; no expressions or paths."
                );
            }
        }
        // Kahn traversal detects cycles, including rootless components, without recursion.
        let mut ready: Vec<&str> = nodes
            .keys()
            .copied()
            .filter(|id| !parent_counts.contains_key(id))
            .collect();
        let mut visited = BTreeSet::new();
        while let Some(id) = ready.pop() {
            if visited.insert(id) {
                if let Some(node) = nodes.get(id) {
                    ready.extend(node.children.iter().map(String::as_str));
                }
            }
        }
        check!(
            visited.len() == nodes.len(),
            "scene_cycle",
            &path,
            "Scene contains a cycle or unreachable cyclic component."
        );
    }
    check!(
        total_nodes <= MAX_NODES,
        "node_count",
        "$.scenes",
        "At most 10,000 nodes per project."
    );
    if let Ok(text) = serde_json::to_string(project) {
        // Flattened extensions must not shadow reserved typed field names.
        if serde_json::from_str::<super::json::UniqueJson>(&text).is_err() {
            errors.push(issue(
                "field_collision",
                "$",
                "Extension fields shadow a reserved document field.",
            ));
        }
        let raw: Value = serde_json::from_str(&text).expect("serialized JSON is valid");
        if let Err(mut portable) = check_portability(&raw) {
            errors.append(&mut portable);
        }
        if serde_json::to_vec(&raw).is_ok_and(|bytes| bytes.len() > MAX_DOCUMENT_BYTES) {
            errors.push(issue(
                "document_size",
                "$",
                "Project exceeds the 2 MiB source limit.",
            ));
        }
    } else {
        errors.push(issue(
            "invalid_shape",
            "$",
            "Project cannot be represented as JSON.",
        ));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Unknown features are preserved, but never imply native/editor support.
pub fn unsupported_features(project: &ThemeProject) -> Vec<Issue> {
    let mut issues = Vec::new();
    if !project.extra.is_empty() || !project.variants.is_empty() {
        issues.push(issue("uninterpreted_data", "$", "Future extension/variant data is retained without support claims; a compiler must handle or explicitly report it."));
    }
    for scene in &project.scenes {
        if ![
            "startup",
            "main_menu",
            "system_select",
            "game_list",
            "game_detail",
            "launch",
            "idle",
            "exit",
        ]
        .contains(&scene.role.as_str())
        {
            issues.push(issue(
                "unknown_scene",
                &scene.id,
                "Scene role is preserved but unsupported.",
            ));
        }
        for node in &scene.nodes {
            if ![
                "group",
                "text",
                "image",
                "panel",
                "shape",
                "metadata",
                "game_art",
                "navigation",
                "video",
                "game_list",
            ]
            .contains(&node.node_type.as_str())
            {
                issues.push(issue(
                    "unknown_node",
                    &node.id,
                    "Node payload is preserved but unsupported; do not lower it silently.",
                ));
            }
        }
    }
    issues
}

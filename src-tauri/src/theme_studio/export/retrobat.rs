//! Experimental format-7 candidate, not verified native output or an installable package.
use crate::theme_studio::{model::*, validation::parse_project};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

const MAX_NODES: usize = 128;
const MAX_XML: usize = 1024 * 1024;

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct AssetRequirement {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Candidate {
    pub path: String,
    pub xml: String,
    /// Declarations only. Bytes/permissions/licenses still require managed-store verification.
    pub required_assets: Vec<AssetRequirement>,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Blocker(pub String);
fn fail(message: &str) -> Blocker {
    Blocker(message.into())
}
fn empty(extra: &Extensions) -> Result<(), Blocker> {
    if extra.is_empty() {
        Ok(())
    } else {
        Err(fail("Unknown or unsupported extension data."))
    }
}
fn escape(value: &str) -> Result<String, Blocker> {
    // XML 1.0 characters only; no evaluation of target variable syntax.
    if value.chars().any(|c| {
        !matches!(c, '\t' | '\n' | '\r') && (c < ' ' || c == '\u{fffe}' || c == '\u{ffff}')
    }) || value.contains("${")
    {
        return Err(fail(
            "Text contains invalid XML characters or target variable syntax.",
        ));
    }
    Ok(value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\"', "&quot;")
        .replace('\'', "&apos;"))
}
fn tag(xml: &mut String, name: &str, value: &str) -> Result<(), Blocker> {
    xml.push_str(&format!("      <{name}>{}</{name}>\n", escape(value)?));
    Ok(())
}
fn normalized(value: &Dimension, extent: u32) -> Result<String, Blocker> {
    empty(&value.extra)?;
    let n = if value.unit == "%" {
        value.value / 100.0
    } else {
        value.value / f64::from(extent)
    };
    // Six decimals must not erase a positive size or generate off-screen extreme output.
    if !(0.0..=1.0).contains(&n) {
        return Err(fail("Geometry outside the unit canvas is not supported."));
    }
    Ok(format!("{n:.6}"))
}
/// Untrusted JSON enters the existing bounded source parser first. Returns one deterministic
/// fail-closed blocker; no partial candidate is returned on any error.
pub fn compile_candidate(source: &str) -> Result<Candidate, Blocker> {
    let project = parse_project(source).map_err(|_| fail("Invalid portable source document."))?;
    empty(&project.extra)?;
    empty(&project.metadata.extra)?;
    empty(&project.tokens)?;
    empty(&project.variants)?;
    for target in &project.targets {
        empty(&target.extra)?;
    }
    if project.scenes.len() > 2
        || project.scenes.iter().map(|s| s.nodes.len()).sum::<usize>() > MAX_NODES
    {
        return Err(fail("Candidate limit is two scenes and 128 nodes."));
    }
    let mut roles = BTreeSet::new();
    let mut used = BTreeSet::new();
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<theme>\n  <formatVersion>7</formatVersion>\n",
    );
    for scene in &project.scenes {
        empty(&scene.extra)?;
        empty(&scene.resolution.extra)?;
        let view = match scene.role.as_str() {
            "system_select" => "system",
            "game_list" => "detailed",
            _ => {
                return Err(fail(
                    "Unsupported scene role; no scene is silently omitted.",
                ))
            }
        };
        if !roles.insert(view) {
            return Err(fail("Duplicate target view."));
        }
        let mut names = BTreeSet::new();
        xml.push_str(&format!("  <view name=\"{view}\">\n"));
        for (index, node) in scene.nodes.iter().enumerate() {
            // Editor flags are supported only when recognized; hide has no native semantics yet.
            for (key, value) in &node.extra {
                if key != "editor" {
                    return Err(fail("Unsupported node extension."));
                }
                let flags = value
                    .as_object()
                    .ok_or_else(|| fail("Unknown editor data."))?;
                for (flag, state) in flags {
                    if !["locked", "hidden"].contains(&flag.as_str())
                        || !state.is_boolean()
                        || (flag == "hidden" && state == &Value::Bool(true))
                    {
                        return Err(fail("Unsupported editor flag or hidden layer."));
                    }
                }
            }
            empty(&node.layout.extra)?;
            if !node.children.is_empty() || node.layout.rotation != 0.0 {
                return Err(fail("Nested or rotated layers require a verified mapping."));
            }
            let kind = match node.node_type.as_str() {
                "text" => "text",
                "image" => "image",
                _ => return Err(fail("Unsupported layer type.")),
            };
            let mut name = format!("rd_{:x}", Sha256::digest(node.id.as_bytes()));
            let mut extra = true;
            if !node.bindings.is_empty() {
                if kind != "text" || view != "detailed" || node.bindings.len() != 1 {
                    return Err(fail("Unsupported binding context."));
                }
                let binding = node
                    .bindings
                    .get("text")
                    .ok_or_else(|| fail("Unsupported binding slot."))?;
                empty(&binding.extra)?;
                if binding.kind != "text" {
                    return Err(fail("Unsupported binding kind."));
                }
                name = match binding.source.as_str() {
                    "game.title" => "md_name",
                    "game.description" => "md_description",
                    _ => return Err(fail("Unsupported metadata source.")),
                }
                .into();
                extra = false;
            }
            if !names.insert(name.clone()) {
                return Err(fail("Multiple layers map to the same native element."));
            }
            xml.push_str(&format!(
                "    <{kind} name=\"{name}\"{}>\n",
                if extra { " extra=\"true\"" } else { "" }
            ));
            let x = normalized(&node.layout.x, scene.resolution.width)?;
            let y = normalized(&node.layout.y, scene.resolution.height)?;
            let w = normalized(&node.layout.width, scene.resolution.width)?;
            let h = normalized(&node.layout.height, scene.resolution.height)?;
            if w == "0.000000" || h == "0.000000" {
                return Err(fail("Size is below candidate precision."));
            }
            tag(&mut xml, "pos", &format!("{x} {y}"))?;
            if kind == "text" {
                tag(&mut xml, "size", &format!("{w} {h}"))?;
            }
            tag(&mut xml, "origin", "0 0")?;
            tag(&mut xml, "zIndex", &index.to_string())?;
            let allowed = if kind == "text" {
                &["text", "color", "fontSize", "textAlign"][..]
            } else {
                &["imageFit"][..]
            };
            if node
                .style
                .keys()
                .any(|key| !allowed.contains(&key.as_str()))
            {
                return Err(fail("Unsupported style property."));
            }
            if kind == "text" {
                if !node.assets.is_empty() {
                    return Err(fail("Text assets are unsupported."));
                }
                if extra {
                    let text = match node.style.get("text") {
                        Some(Value::String(text)) => text.as_str(),
                        None => &node.name,
                        _ => return Err(fail("Unsupported text value.")),
                    };
                    if text.chars().count() > 500 || text.lines().count() > 20 {
                        return Err(fail("Text exceeds preview limits."));
                    }
                    tag(&mut xml, "text", text)?;
                } else if node.style.get("text").is_some_and(|v| !v.is_string()) {
                    return Err(fail("Unknown retained static text."));
                }
                let color = match node.style.get("color") {
                    Some(Value::String(v)) => v.as_str(),
                    None => "ffffff",
                    _ => return Err(fail("Unsupported color.")),
                };
                if ![6, 8].contains(&color.len()) || !color.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err(fail("Invalid color."));
                }
                tag(&mut xml, "color", color)?;
                let align = match node.style.get("textAlign") {
                    Some(Value::String(v)) => v.as_str(),
                    None => "left",
                    _ => return Err(fail("Unknown alignment.")),
                };
                if !["left", "center", "right"].contains(&align) {
                    return Err(fail("Unknown alignment."));
                }
                tag(&mut xml, "alignment", align)?;
                tag(&mut xml, "verticalAlignment", "top")?;
                tag(&mut xml, "lineSpacing", "1.2")?;
                let size = if let Some(value) = node.style.get("fontSize") {
                    let dimension: Dimension = serde_json::from_value(value.clone())
                        .map_err(|_| fail("Unsupported font size."))?;
                    empty(&dimension.extra)?;
                    if dimension.unit != "px"
                        || !dimension.value.is_finite()
                        || !(8.0..=256.0).contains(&dimension.value)
                    {
                        return Err(fail("Unsupported font size."));
                    }
                    dimension.value
                } else {
                    36.0
                };
                tag(
                    &mut xml,
                    "fontSize",
                    &format!(
                        "{:.6}",
                        size / f64::from(scene.resolution.width.min(scene.resolution.height))
                    ),
                )?;
            } else {
                if node.assets.len() != 1 {
                    return Err(fail("Image needs exactly one managed PNG."));
                }
                if node
                    .style
                    .get("imageFit")
                    .is_some_and(|v| v.as_str() != Some("contain"))
                {
                    return Err(fail("Only candidate Fit images are mapped."));
                }
                let asset = project
                    .assets
                    .iter()
                    .find(|a| a.id == node.assets[0])
                    .ok_or_else(|| fail("Missing managed asset."))?;
                if asset.media_type != "image/png" {
                    return Err(fail("Only PNG is mapped."));
                }
                used.insert(asset.id.clone());
                // maxSize overrides the generic rectangle for aspect-preserving fit.
                tag(&mut xml, "maxSize", &format!("{w} {h}"))?;
                tag(&mut xml, "path", &format!("./{}", asset.path))?;
            }
            xml.push_str(&format!("    </{kind}>\n"));
        }
        xml.push_str("  </view>\n");
    }
    xml.push_str("</theme>\n");
    if xml.len() > MAX_XML {
        return Err(fail("Candidate XML exceeds 1 MiB."));
    }
    // Asset extension data cannot be discarded, even on unused declarations.
    for asset in &project.assets {
        empty(&asset.extra)?;
    }
    let mut required_assets: Vec<_> = project
        .assets
        .iter()
        .filter(|a| used.contains(&a.id))
        .map(|a| AssetRequirement {
            path: a.path.clone(),
            sha256: a.sha256.clone(),
            bytes: a.bytes,
        })
        .collect();
    required_assets.sort_by(|a, b| a.path.cmp(&b.path));
    for pair in required_assets.windows(2) {
        if pair[0].path == pair[1].path && pair[0] != pair[1] {
            return Err(fail("Conflicting declarations for one asset path."));
        }
    }
    required_assets.dedup();
    Ok(Candidate {
        path: "theme.xml".into(),
        xml,
        required_assets,
    })
}

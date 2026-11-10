//! Experimental format-7 candidate, not verified native output or an installable package.
use crate::theme_studio::{model::*, validation::parse_project};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

const MAX_NODES: usize = 128;
const MAX_XML: usize = 1024 * 1024;
/// RetroBat's built-in game-list elements (THEMES.md at the pinned revision, "detailed" view).
/// Any the design doesn't place are hidden, so nothing appears where the design has art (ADR-112).
const HIDDEN_DEFAULTS: &[(&str, &str)] = &[
    ("text", "logoText"),
    ("image", "md_image"),
    ("video", "md_video"),
    ("image", "md_marquee"),
    ("text", "md_lbl_rating"),
    ("text", "md_lbl_releasedate"),
    ("text", "md_lbl_developer"),
    ("text", "md_lbl_publisher"),
    ("text", "md_lbl_genre"),
    ("text", "md_lbl_players"),
    ("text", "md_lbl_lastplayed"),
    ("text", "md_lbl_playcount"),
    ("text", "md_lbl_gametime"),
    ("rating", "md_rating"),
    ("datetime", "md_releasedate"),
    ("text", "md_developer"),
    ("text", "md_publisher"),
    ("text", "md_genre"),
    ("text", "md_players"),
    ("datetime", "md_lastplayed"),
    ("text", "md_playcount"),
    ("text", "md_gametime"),
    ("text", "md_description"),
];

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
        // The game list screen also styles RetroBat's "video" layout, which it uses when games
        // have videos (ADR-115; THEMES.md: a view name is a list).
        let view_names = if view == "detailed" { "detailed, video" } else { view };
        xml.push_str(&format!("  <view name=\"{view_names}\">\n"));
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
            // Native element kind and name (ADR-112). Text and pictures are extras; the game list,
            // the game's video and text linked to game data use RetroBat's own named elements.
            let mut name = format!("rd_{:x}", Sha256::digest(node.id.as_bytes()));
            let mut extra = true;
            let mut kind = match node.node_type.as_str() {
                "text" => "text",
                "image" => "image",
                "video" | "game_list" if view != "detailed" => {
                    return Err(fail("Game lists and game videos belong on the game list screen."))
                }
                "video" => {
                    let binding = node
                        .bindings
                        .get("video")
                        .filter(|_| node.bindings.len() == 1)
                        .ok_or_else(|| fail("A game video layer needs its game-video link."))?;
                    empty(&binding.extra)?;
                    if binding.source != "game.video" || binding.kind != "video" {
                        return Err(fail("Unsupported video source."));
                    }
                    name = "md_video".into();
                    extra = false;
                    "video"
                }
                "game_list" => {
                    if !node.bindings.is_empty() {
                        return Err(fail("A game list has no links."));
                    }
                    name = "gamelist".into();
                    extra = false;
                    "textlist"
                }
                _ => return Err(fail("Unsupported layer type.")),
            };
            if kind == "text" && !node.bindings.is_empty() {
                if node.bindings.len() != 1 {
                    return Err(fail("Unsupported binding context."));
                }
                let binding = node
                    .bindings
                    .get("text")
                    .ok_or_else(|| fail("Unsupported binding slot."))?;
                empty(&binding.extra)?;
                let (native_kind, native_name, data_kind) = match (view, binding.source.as_str()) {
                    ("detailed", "game.title") => ("text", "md_name", "text"),
                    ("detailed", "game.description") => ("text", "md_description", "text"),
                    ("detailed", "system.name") => ("text", "logoText", "text"),
                    ("detailed", "game.players") => ("text", "md_players", "number"),
                    // Shown as RetroBat formats the release date (no year-only option in format 7).
                    ("detailed", "game.year") => ("datetime", "md_releasedate", "number"),
                    ("detailed", _) => return Err(fail("Unsupported metadata source.")),
                    _ => return Err(fail("Unsupported binding context.")),
                };
                if binding.kind != data_kind {
                    return Err(fail("Unsupported binding kind."));
                }
                kind = native_kind;
                name = native_name.into();
                extra = false;
            } else if kind == "image" && !node.bindings.is_empty() {
                return Err(fail("Unsupported binding context."));
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
            if kind == "video" {
                // Centred in its box and as large as fits, keeping the video's own shape
                // (maxSize is RetroBat's recommendation for game videos of unknown shape).
                let centre = |a: &str, b: &str| -> Result<String, Blocker> {
                    let a: f64 = a.parse().map_err(|_| fail("Invalid position."))?;
                    let b: f64 = b.parse().map_err(|_| fail("Invalid size."))?;
                    Ok(format!("{:.6}", a + b / 2.0))
                };
                tag(&mut xml, "pos", &format!("{} {}", centre(&x, &w)?, centre(&y, &h)?))?;
                tag(&mut xml, "maxSize", &format!("{w} {h}"))?;
                tag(&mut xml, "origin", "0.5 0.5")?;
            } else {
                tag(&mut xml, "pos", &format!("{x} {y}"))?;
                if kind != "image" {
                    tag(&mut xml, "size", &format!("{w} {h}"))?;
                }
                tag(&mut xml, "origin", "0 0")?;
            }
            tag(&mut xml, "zIndex", &index.to_string())?;
            let allowed = match kind {
                "text" | "datetime" => &["text", "color", "fontSize", "textAlign"][..],
                "textlist" => &["color", "selectedColor", "selectorColor", "fontSize", "textAlign"][..],
                "video" => &[][..],
                _ => &["imageFit"][..],
            };
            if node
                .style
                .keys()
                .any(|key| !allowed.contains(&key.as_str()))
            {
                return Err(fail("Unsupported style property."));
            }
            let colour = |key: &str, default: &'static str| -> Result<String, Blocker> {
                let value = match node.style.get(key) {
                    Some(Value::String(v)) => v.clone(),
                    None => default.to_string(),
                    _ => return Err(fail("Unsupported color.")),
                };
                if ![6, 8].contains(&value.len()) || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err(fail("Invalid color."));
                }
                Ok(value)
            };
            let font_size = || -> Result<String, Blocker> {
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
                Ok(format!(
                    "{:.6}",
                    size / f64::from(scene.resolution.width.min(scene.resolution.height))
                ))
            };
            let alignment = || -> Result<&str, Blocker> {
                let align = match node.style.get("textAlign") {
                    Some(Value::String(v)) => v.as_str(),
                    None => "left",
                    _ => return Err(fail("Unknown alignment.")),
                };
                if !["left", "center", "right"].contains(&align) {
                    return Err(fail("Unknown alignment."));
                }
                Ok(align)
            };
            if kind == "text" || kind == "datetime" {
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
                tag(&mut xml, "color", &colour("color", "ffffff")?)?;
                tag(&mut xml, "alignment", alignment()?)?;
                if kind == "text" {
                    tag(&mut xml, "verticalAlignment", "top")?;
                    tag(&mut xml, "lineSpacing", "1.2")?;
                }
                tag(&mut xml, "fontSize", &font_size()?)?;
            } else if kind == "textlist" {
                if !node.assets.is_empty() {
                    return Err(fail("A game list has no pictures."));
                }
                let primary = colour("color", "c8d0e0")?;
                tag(&mut xml, "primaryColor", &primary)?;
                tag(&mut xml, "secondaryColor", &primary)?;
                tag(&mut xml, "selectedColor", &colour("selectedColor", "ffffff")?)?;
                tag(&mut xml, "selectorColor", &colour("selectorColor", "378cdd")?)?;
                tag(&mut xml, "fontSize", &font_size()?)?;
                tag(&mut xml, "alignment", alignment()?)?;
            } else if kind == "video" {
                if !node.assets.is_empty() {
                    return Err(fail("A game video has no pictures."));
                }
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
        if view == "detailed" {
            for (kind, name) in HIDDEN_DEFAULTS {
                if !names.contains(*name) {
                    xml.push_str(&format!(
                        "    <{kind} name=\"{name}\">\n      <visible>false</visible>\n    </{kind}>\n"
                    ));
                }
            }
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

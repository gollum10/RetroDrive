use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const FORMAT: &str = "retrodrive.theme";
pub const SCHEMA_VERSION: &str = "1.0.0";
pub type Extensions = BTreeMap<String, Value>;

/// Editable source, independent of installation paths and generated target files.
/// Unknown fields survive serialization at every extensible object boundary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ThemeProject {
    pub format: String,
    pub schema_version: String,
    pub id: String,
    pub metadata: Metadata,
    /// Empty means Universal / No Target Yet, not an inferred RetroBat target.
    pub targets: Vec<TargetIntent>,
    pub scenes: Vec<Scene>,
    pub assets: Vec<Asset>,
    pub tokens: Extensions,
    pub variants: Extensions,
    #[serde(flatten)]
    pub extra: Extensions,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub name: String,
    pub author: String,
    pub description: String,
    pub version: String,
    pub license: String,
    pub tags: Vec<String>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// Authoring intent only. It is not a detected or verified capability profile.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TargetIntent {
    pub frontend_family: String,
    pub frontend_version: Option<String>,
    #[serde(flatten)]
    pub extra: Extensions,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    pub id: String,
    /// Open vocabulary so future roles can be retained without inventing support.
    pub role: String,
    pub name: String,
    pub resolution: Resolution,
    pub nodes: Vec<Node>,
    #[serde(flatten)]
    pub extra: Extensions,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// Flat node arena per scene. Ordered child IDs define hierarchy/z-order.
/// Roots are nodes not referenced as children; their order is the arena order.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub name: String,
    pub layout: Layout,
    pub style: Extensions,
    pub children: Vec<String>,
    pub assets: Vec<String>,
    pub bindings: BTreeMap<String, Binding>,
    #[serde(flatten)]
    pub extra: Extensions,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub x: Dimension,
    pub y: Dimension,
    pub width: Dimension,
    pub height: Dimension,
    pub rotation: f64,
    #[serde(flatten)]
    pub extra: Extensions,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Dimension {
    pub value: f64,
    pub unit: String,
    #[serde(flatten)]
    pub extra: Extensions,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Binding {
    pub source: String,
    /// text, number or image; never a JavaScript/expression string.
    pub kind: String,
    #[serde(flatten)]
    pub extra: Extensions,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub id: String,
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    pub media_type: String,
    pub license: String,
    pub attribution: String,
    /// Portable provenance text, not a machine-local source path.
    pub source: String,
    #[serde(flatten)]
    pub extra: Extensions,
}

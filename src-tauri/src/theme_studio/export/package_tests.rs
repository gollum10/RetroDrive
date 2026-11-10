use super::package::*;
use crate::theme_studio::{
    image_store,
    projects::{self, WriteKind},
};
use base64::prelude::*;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
const SOURCE: &str = include_str!("../../../tests/fixtures/theme_studio/compiler-minimal.json");
fn setup() -> (Connection, projects::ProjectRecord) {
    let mut connection = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut connection, None).unwrap();
    let record = projects::create(&connection, SOURCE).unwrap();
    (connection, record)
}
fn imported(connection: &Connection, record: &projects::ProjectRecord) -> image_store::StoredImage {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[10, 20, 30, 255]).unwrap();
        writer.finish().unwrap();
    }
    image_store::import_png(connection, &record.id, &BASE64_STANDARD.encode(bytes)).unwrap()
}
fn with_image(
    connection: &Connection,
    record: &projects::ProjectRecord,
) -> (projects::ProjectRecord, image_store::StoredImage) {
    let image = imported(connection, record);
    let mut source: Value = serde_json::from_str(&record.saved_json).unwrap();
    source["assets"] = json!([{"id":"png","path":image.path,"sha256":image.sha256,"bytes":image.bytes,"mediaType":"image/png","license":"original","attribution":"Fixture creator","source":"User supplied"}]);
    let mut node = source["scenes"][0]["nodes"][0].clone();
    node["id"] = json!("image");
    node["type"] = json!("image");
    node["style"] = json!({"imageFit":"contain"});
    node["assets"] = json!(["png"]);
    source["scenes"][0]["nodes"]
        .as_array_mut()
        .unwrap()
        .push(node);
    let saved = projects::write(
        connection,
        &record.id,
        record.revision,
        &source.to_string(),
        WriteKind::Autosave,
    )
    .unwrap();
    (saved, image)
}
#[test]
fn deterministic_manifest_hashes_every_nonmanifest_file_without_writes() {
    let (connection, record) = setup();
    let (record, image) = with_image(&connection, &record);
    let changes = connection.total_changes();
    let first = assemble(&connection, &record.id, record.revision).unwrap();
    assert_eq!(
        first,
        assemble(&connection, &record.id, record.revision).unwrap()
    );
    let manifest: Manifest =
        serde_json::from_slice(&first.files["retrodrive-candidate.json"]).unwrap();
    assert_eq!(manifest.status, "experimental_unverified");
    assert_eq!(manifest.files.len(), 3);
    assert_eq!(first.files.len(), 4);
    for entry in &manifest.files {
        let bytes = &first.files[&entry.path];
        assert_eq!(entry.bytes, bytes.len());
        assert_eq!(entry.sha256, format!("{:x}", Sha256::digest(bytes)));
        assert!(!entry.path.contains(".."));
        assert!(!entry.path.contains(':'));
    }
    assert!(first.files.contains_key(&image.path));
    assert_eq!(connection.total_changes(), changes);
    let after = projects::open(&connection, &record.id).unwrap();
    assert_eq!(after.revision, record.revision);
    assert_eq!(after.saved_json, record.saved_json);
    assert_eq!(after.draft_json, record.draft_json);
}
#[test]
fn no_image_project_cannot_export_unreferenced_global_blobs() {
    let (connection, record) = setup();
    let image = imported(&connection, &record);
    let other = projects::create(&connection, SOURCE).unwrap();
    let package = assemble(&connection, &other.id, other.revision).unwrap();
    assert_eq!(package.files.len(), 3);
    assert!(!package.files.contains_key(&image.path));
}
#[test]
fn stale_revision_and_unsupported_draft_refuse_without_partial_files() {
    let (connection, record) = setup();
    let (record, _) = with_image(&connection, &record);
    assert_eq!(
        assemble(&connection, &record.id, record.revision - 1)
            .unwrap_err()
            .code,
        "conflict"
    );
    let mut source: Value = serde_json::from_str(record.draft_json.as_ref().unwrap()).unwrap();
    source["scenes"][0]["nodes"][0]["style"]["opacity"] = json!(0.5);
    let changed = projects::write(
        &connection,
        &record.id,
        record.revision,
        &source.to_string(),
        WriteKind::Autosave,
    )
    .unwrap();
    assert_eq!(
        assemble(&connection, &record.id, changed.revision)
            .unwrap_err()
            .code,
        "compile_blocked"
    );
}
#[test]
fn missing_corrupt_or_wrong_length_managed_bytes_are_never_packaged() {
    for mode in ["missing", "corrupt", "length"] {
        let (connection, record) = setup();
        let (record, image) = with_image(&connection, &record);
        match mode {
            "missing" => {
                connection
                    .execute(
                        "DELETE FROM theme_image_assets WHERE sha256=?1",
                        [&image.sha256],
                    )
                    .unwrap();
            }
            "corrupt" => {
                let mut bytes: Vec<u8> = connection
                    .query_row(
                        "SELECT png FROM theme_image_assets WHERE sha256=?1",
                        [&image.sha256],
                        |r| r.get(0),
                    )
                    .unwrap();
                bytes[20] ^= 1;
                connection
                    .execute(
                        "UPDATE theme_image_assets SET png=?1 WHERE sha256=?2",
                        params![bytes, image.sha256],
                    )
                    .unwrap();
            }
            _ => {
                let mut bytes: Vec<u8> = connection
                    .query_row(
                        "SELECT png FROM theme_image_assets WHERE sha256=?1",
                        [&image.sha256],
                        |r| r.get(0),
                    )
                    .unwrap();
                bytes.push(0);
                // Simulate damaged storage that bypassed the normal schema constraint.
                // This applies only to this disposable in-memory test catalog.
                connection
                    .pragma_update(None, "ignore_check_constraints", true)
                    .unwrap();
                connection
                    .execute(
                        "UPDATE theme_image_assets SET png=?1 WHERE sha256=?2",
                        params![bytes, image.sha256],
                    )
                    .unwrap();
            }
        }
        let changes = connection.total_changes();
        assert!(assemble(&connection, &record.id, record.revision).is_err());
        assert_eq!(connection.total_changes(), changes);
    }
}
#[test]
fn declared_byte_mismatch_and_size_budget_are_rejected() {
    let (connection, record) = setup();
    let (record, _) = with_image(&connection, &record);
    assert_eq!(
        with_limit(&connection, &record.id, record.revision, 1)
            .unwrap_err()
            .code,
        "package_limit"
    );
    let mut source: Value = serde_json::from_str(record.draft_json.as_ref().unwrap()).unwrap();
    source["assets"][0]["bytes"] = json!(1);
    let record = projects::write(
        &connection,
        &record.id,
        record.revision,
        &source.to_string(),
        WriteKind::Autosave,
    )
    .unwrap();
    assert!(assemble(&connection, &record.id, record.revision).is_err());
}
#[test]
fn credits_preserve_alias_attribution_and_license_warnings() {
    let (connection, record) = setup();
    let (record, image) = with_image(&connection, &record);
    let mut source: Value = serde_json::from_str(record.draft_json.as_ref().unwrap()).unwrap();
    let mut alias = source["assets"][0].clone();
    alias["id"] = json!("alias");
    alias["license"] = json!("unknown");
    alias["attribution"] = json!("Second creator");
    source["assets"].as_array_mut().unwrap().push(alias);
    let record = projects::write(
        &connection,
        &record.id,
        record.revision,
        &source.to_string(),
        WriteKind::Autosave,
    )
    .unwrap();
    let package = assemble(&connection, &record.id, record.revision).unwrap();
    assert!(package.files.contains_key(&image.path));
    assert_eq!(package.files.len(), 4);
    let credits: Value = serde_json::from_slice(&package.files["licenses.json"]).unwrap();
    assert_eq!(credits["assets"].as_array().unwrap().len(), 2);
    let manifest: Manifest =
        serde_json::from_slice(&package.files["retrodrive-candidate.json"]).unwrap();
    assert!(manifest
        .warnings
        .iter()
        .any(|w| w.contains("License review required")));
}
#[test]
fn checkpoint_has_same_files_as_acknowledged_draft() {
    let (connection, record) = setup();
    let (record, _) = with_image(&connection, &record);
    let before = assemble(&connection, &record.id, record.revision).unwrap();
    let record = projects::write(
        &connection,
        &record.id,
        record.revision,
        record.draft_json.as_ref().unwrap(),
        WriteKind::Checkpoint,
    )
    .unwrap();
    assert_eq!(
        assemble(&connection, &record.id, record.revision).unwrap(),
        before
    );
}

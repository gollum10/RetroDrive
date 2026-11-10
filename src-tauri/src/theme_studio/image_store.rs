//! Immutable, content-addressed catalog blobs. No caller-supplied filesystem paths.
use super::{
    image_import::{normalize_png, MAX_INPUT_BYTES, MAX_OUTPUT_BYTES},
    projects::{self, error_message, Result},
    validation::parse_project,
};
use base64::prelude::*;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const MAX_STORE_BYTES: i64 = 256 * 1024 * 1024;
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredImage {
    pub sha256: String,
    pub path: String,
    pub bytes: usize,
    pub width: u32,
    pub height: u32,
}

pub fn import_png(connection: &Connection, project_id: &str, encoded: &str) -> Result<StoredImage> {
    import_with_quota(connection, project_id, encoded, MAX_STORE_BYTES)
}
fn import_with_quota(
    connection: &Connection,
    project_id: &str,
    encoded: &str,
    quota: i64,
) -> Result<StoredImage> {
    projects::open(connection, project_id)?;
    if encoded.len() > MAX_INPUT_BYTES.div_ceil(3) * 4 {
        return Err(error_message("image_limit", "PNG input exceeds 16 MiB."));
    }
    let input = BASE64_STANDARD
        .decode(encoded)
        .map_err(|_| error_message("image_format", "Invalid PNG input encoding."))?;
    let image = normalize_png(&input).map_err(|error|error_message("image_format",&format!("PNG rejected: {error:?}. Limit: 16 MiB, 4096 per axis, 4 million pixels; static PNG only.")))?;
    let tx = connection.unchecked_transaction()?;
    let existing: Option<Vec<u8>> = tx
        .query_row(
            "SELECT png FROM theme_image_assets WHERE sha256=?1 AND length(png)<=?2",
            params![image.sha256, MAX_OUTPUT_BYTES as i64],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(existing) = existing {
        if existing != image.bytes {
            return Err(error_message(
                "image_integrity",
                "Existing managed image differs; it was not replaced.",
            ));
        }
    } else {
        let inserted = tx.execute("INSERT INTO theme_image_assets(sha256,png,width,height,byte_count) SELECT ?1,?2,?3,?4,?5 WHERE (SELECT COALESCE(SUM(byte_count),0) FROM theme_image_assets)+?5<=?6 AND (SELECT COUNT(*) FROM theme_image_assets)<2000",params![image.sha256,image.bytes,image.width,image.height,image.bytes.len() as i64,quota])?;
        if inserted != 1 {
            return Err(error_message(
                "image_quota",
                "Managed theme images have reached the 256 MiB or 2,000-image limit.",
            ));
        }
    }
    let stored: Vec<u8> = tx.query_row(
        "SELECT png FROM theme_image_assets WHERE sha256=?1",
        [&image.sha256],
        |row| row.get(0),
    )?;
    if format!("{:x}", Sha256::digest(&stored)) != image.sha256 || stored != image.bytes {
        return Err(error_message(
            "image_integrity",
            "Managed image verification failed.",
        ));
    }
    tx.commit()?;
    Ok(StoredImage {
        sha256: image.sha256,
        path: image.path,
        bytes: stored.len(),
        width: image.width,
        height: image.height,
    })
}

pub fn preview_png(connection: &Connection, project_id: &str, asset_id: &str) -> Result<String> {
    let record = projects::open(connection, project_id)?;
    let project = parse_project(record.draft_json.as_deref().unwrap_or(&record.saved_json))
        .map_err(|_| {
            error_message(
                "invalid_document",
                "Save a valid project before previewing its images.",
            )
        })?;
    let asset = project
        .assets
        .iter()
        .find(|asset| asset.id == asset_id && asset.media_type == "image/png")
        .ok_or_else(|| {
            error_message(
                "image_authorization",
                "Image is not declared by this saved project.",
            )
        })?;
    let bytes = verified_png(connection, asset)?;
    Ok(format!(
        "data:image/png;base64,{}",
        BASE64_STANDARD.encode(bytes)
    ))
}

/// Internal integrity reader. The caller must authorize the declaration against a stored
/// project snapshot; this function accepts no filesystem location and never writes.
pub(crate) fn verified_png(
    connection: &Connection,
    asset: &super::model::Asset,
) -> Result<Vec<u8>> {
    if asset.media_type != "image/png" || asset.bytes > MAX_OUTPUT_BYTES as u64 {
        return Err(error_message(
            "image_limit",
            "Image exceeds the managed PNG limit.",
        ));
    }
    let bytes: Vec<u8> = connection.query_row(
        "SELECT png FROM theme_image_assets WHERE sha256=?1 AND byte_count=?2 AND length(png)=?2 AND length(png)<=?3",
        params![asset.sha256,asset.bytes as i64,MAX_OUTPUT_BYTES as i64], |row|row.get(0)
    ).optional()?.ok_or_else(||error_message("image_missing","Managed image is unavailable or has an inconsistent length."))?;
    if format!("{:x}", Sha256::digest(&bytes)) != asset.sha256
        || !bytes.starts_with(b"\x89PNG\r\n\x1a\n")
    {
        return Err(error_message(
            "image_integrity",
            "Managed image is damaged; read refused.",
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    const BLANK: &str = include_str!("../../tests/fixtures/theme_studio/blank.json");
    fn setup() -> (Connection, projects::ProjectRecord) {
        let mut connection = Connection::open_in_memory().unwrap();
        crate::db::migrate(&mut connection, None).unwrap();
        let project = projects::create(&connection, BLANK).unwrap();
        (connection, project)
    }
    fn input() -> String {
        let mut png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png, 1, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[4, 8, 16, 255]).unwrap();
            writer.finish().unwrap();
        }
        BASE64_STANDARD.encode(png)
    }
    fn declared(project: &projects::ProjectRecord, image: &StoredImage) -> String {
        let mut document: serde_json::Value = serde_json::from_str(&project.saved_json).unwrap();
        document["assets"] = serde_json::json!([{"id":"picture","path":image.path,"sha256":image.sha256,"bytes":image.bytes,"mediaType":"image/png","license":"original","attribution":"Test creator","source":"User supplied"}]);
        document.to_string()
    }
    #[test]
    fn immutable_dedup_authorized_preview_and_copy() {
        let (connection, project) = setup();
        let image = import_png(&connection, &project.id, &input()).unwrap();
        let duplicate = import_with_quota(&connection, &project.id, &input(), 0).unwrap();
        assert_eq!(image.sha256, duplicate.sha256);
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM theme_image_assets", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            preview_png(&connection, &project.id, "picture")
                .unwrap_err()
                .code,
            "image_authorization"
        );
        let source = declared(&project, &image);
        let saved = projects::write(
            &connection,
            &project.id,
            project.revision,
            &source,
            projects::WriteKind::Autosave,
        )
        .unwrap();
        assert!(preview_png(&connection, &project.id, "picture")
            .unwrap()
            .starts_with("data:image/png;base64,iVBOR"));
        let checkpoint = projects::write(
            &connection,
            &project.id,
            saved.revision,
            &source,
            projects::WriteKind::Checkpoint,
        )
        .unwrap();
        let copy =
            projects::duplicate(&connection, &project.id, checkpoint.revision, "Copy", false)
                .unwrap();
        assert_eq!(
            preview_png(&connection, &copy.id, "picture").unwrap(),
            preview_png(&connection, &project.id, "picture").unwrap()
        );
        let unrelated = projects::create(&connection, BLANK).unwrap();
        assert_eq!(
            preview_png(&connection, &unrelated.id, "picture")
                .unwrap_err()
                .code,
            "image_authorization"
        );
        assert_eq!(
            preview_png(&connection, &project.id, "../../secret")
                .unwrap_err()
                .code,
            "image_authorization"
        );
    }
    #[test]
    fn corrupt_existing_bytes_are_never_overwritten_or_previewed() {
        let (connection, project) = setup();
        let image = import_png(&connection, &project.id, &input()).unwrap();
        projects::write(
            &connection,
            &project.id,
            project.revision,
            &declared(&project, &image),
            projects::WriteKind::Checkpoint,
        )
        .unwrap();
        let corrupt = vec![0; image.bytes];
        connection
            .execute(
                "UPDATE theme_image_assets SET png=?1 WHERE sha256=?2",
                params![corrupt, image.sha256],
            )
            .unwrap();
        assert_eq!(
            import_png(&connection, &project.id, &input())
                .unwrap_err()
                .code,
            "image_integrity"
        );
        assert_eq!(
            preview_png(&connection, &project.id, "picture")
                .unwrap_err()
                .code,
            "image_integrity"
        );
        assert_eq!(
            connection
                .query_row("SELECT png FROM theme_image_assets", [], |r| r
                    .get::<_, Vec<u8>>(0))
                .unwrap(),
            corrupt
        );
    }
    #[test]
    fn invalid_inputs_and_quota_leave_no_blob_or_project_edits() {
        let (connection, project) = setup();
        assert_eq!(
            import_png(&connection, "missing", &input())
                .unwrap_err()
                .code,
            "not_found"
        );
        assert_eq!(
            import_png(&connection, &project.id, "bad encoding!")
                .unwrap_err()
                .code,
            "image_format"
        );
        assert_eq!(
            import_png(&connection, &project.id, &BASE64_STANDARD.encode(b"<svg/>"))
                .unwrap_err()
                .code,
            "image_format"
        );
        assert_eq!(
            import_with_quota(&connection, &project.id, &input(), 1)
                .unwrap_err()
                .code,
            "image_quota"
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM theme_image_assets", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            projects::open(&connection, &project.id).unwrap().saved_json,
            project.saved_json
        );
    }
    #[test]
    fn encoded_size_count_limit_and_migration_conflict_fail_closed() {
        let (connection, project) = setup();
        assert_eq!(
            import_png(
                &connection,
                &project.id,
                &"A".repeat(MAX_INPUT_BYTES.div_ceil(3) * 4 + 1)
            )
            .unwrap_err()
            .code,
            "image_limit"
        );
        let tx = connection.unchecked_transaction().unwrap();
        for index in 0..2000 {
            tx.execute("INSERT INTO theme_image_assets(sha256,png,width,height,byte_count) VALUES(?1,?2,1,1,1)",params![format!("{index:064x}"),vec![0u8]]).unwrap();
        }
        tx.commit().unwrap();
        assert_eq!(
            import_png(&connection, &project.id, &input())
                .unwrap_err()
                .code,
            "image_quota"
        );
        connection.execute_batch("PRAGMA user_version=28;").unwrap();
        let mut connection = connection;
        assert!(crate::db::migrate(&mut connection, None).is_err());
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            28
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM theme_image_assets", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            2000
        );
        assert_eq!(
            projects::open(&connection, &project.id).unwrap().saved_json,
            project.saved_json
        );
    }
    #[test]
    fn readback_failure_rolls_back_the_insert() {
        let (connection, project) = setup();
        connection.execute_batch("CREATE TRIGGER corrupt_import AFTER INSERT ON theme_image_assets BEGIN UPDATE theme_image_assets SET png=zeroblob(NEW.byte_count) WHERE sha256=NEW.sha256; END;").unwrap();
        assert_eq!(
            import_png(&connection, &project.id, &input())
                .unwrap_err()
                .code,
            "image_integrity"
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM theme_image_assets", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn v28_upgrade_backs_up_and_preserves_projects() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("catalog.sqlite");
        let mut connection = crate::db::open(&path).unwrap();
        crate::db::migrate(&mut connection, Some(&path)).unwrap();
        let project = projects::create(&connection, BLANK).unwrap();
        connection
            .execute_batch("DROP TABLE theme_image_assets; PRAGMA user_version=28;")
            .unwrap();
        let report = crate::db::migrate(&mut connection, Some(&path)).unwrap();
        assert_eq!((report.from_version, report.to_version), (28, 29));
        let backup = crate::db::open(report.backup_path.as_ref().unwrap()).unwrap();
        assert_eq!(
            backup
                .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            28
        );
        assert_eq!(
            projects::open(&connection, &project.id).unwrap().saved_json,
            project.saved_json
        );
        assert_eq!(
            projects::open(&backup, &project.id).unwrap().saved_json,
            project.saved_json
        );
        assert!(import_png(&connection, &project.id, &input()).is_ok());
        assert!(crate::db::migrate(&mut connection, Some(&path))
            .unwrap()
            .backup_path
            .is_none());
    }
}

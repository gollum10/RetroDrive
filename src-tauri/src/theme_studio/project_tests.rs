use super::{projects::*, validation::parse_project};
use crate::{db, designs};
use rusqlite::{params, Connection};
use serde_json::{json, Value};

const BLANK: &str = include_str!("../../tests/fixtures/theme_studio/blank.json");
const FUTURE: &str = include_str!("../../tests/fixtures/theme_studio/future-extensions.json");

#[test]
fn simultaneous_writers_get_one_success_and_one_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.sqlite");
    let mut c = db::open(&path).unwrap();
    db::migrate(&mut c, Some(&path)).unwrap();
    let p = create(&c, BLANK).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = ["Writer A", "Writer B"]
        .into_iter()
        .map(|name| {
            let c = db::open(&path).unwrap();
            let id = p.id.clone();
            let document = edit(&p, name);
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                write(&c, &id, 0, &document, WriteKind::Checkpoint)
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter_map(|r| r.as_ref().err())
            .next()
            .unwrap()
            .code,
        "conflict"
    );
    assert_eq!(open(&c, &p.id).unwrap().revision, 1);
}

#[test]
fn failed_insert_is_rolled_back_and_sql_constraints_preserve_source_identity() {
    let c = catalog();
    let p = create(&c, BLANK).unwrap();
    c.execute_batch("CREATE TRIGGER fail_theme_insert AFTER INSERT ON theme_projects BEGIN SELECT RAISE(FAIL,'injected insert failure'); END;").unwrap();
    assert_eq!(create(&c, BLANK).unwrap_err().code, "storage");
    assert_eq!(
        duplicate(&c, &p.id, 0, "Copy", false).unwrap_err().code,
        "storage"
    );
    assert_eq!(
        list(&c, 50, 0).unwrap().len(),
        1,
        "even FAIL after insert must roll back the transaction"
    );
    for sql in [
        "UPDATE theme_projects SET saved_json='{'",
        "UPDATE theme_projects SET saved_json='{}'",
        "UPDATE theme_projects SET draft_json='{}'",
        "UPDATE theme_projects SET name='not the source name'",
        "UPDATE theme_projects SET revision=0.5",
    ] {
        assert!(c.execute_batch(sql).is_err(), "{sql}");
    }
    assert_eq!(open(&c, &p.id).unwrap().saved_json, p.saved_json);
    assert_eq!(open(&c, &p.id).unwrap().revision, 0);
}

fn catalog() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    db::configure(&c).unwrap();
    db::migrate(&mut c, None).unwrap();
    c
}

fn edit(record: &ProjectRecord, name: &str) -> String {
    let mut v: Value = serde_json::from_str(&record.saved_json).unwrap();
    v["metadata"]["name"] = json!(name);
    v.to_string()
}

#[test]
fn projects_create_list_open_and_checkpoint_without_a_target() {
    let c = catalog();
    let first = create(&c, BLANK).unwrap();
    let other = create(&c, BLANK).unwrap();
    assert_ne!(first.id, other.id, "same display names are allowed");
    assert_ne!(first.id, "universal-blank");
    assert_eq!(first.revision, 0);
    assert!(parse_project(&first.saved_json).unwrap().targets.is_empty());
    let saved = write(
        &c,
        &first.id,
        0,
        &edit(&first, "  Étoile   Studio  "),
        WriteKind::Checkpoint,
    )
    .unwrap();
    assert_eq!(saved.name, "Étoile Studio");
    assert_eq!(saved.revision, 1);
    assert!(saved.draft_json.is_none());
    assert_eq!(open(&c, &first.id).unwrap().saved_json, saved.saved_json);
    let rows = list(&c, 1, 0).unwrap();
    assert_eq!(rows.len(), 1);
    assert_ne!(rows[0].id, list(&c, 1, 1).unwrap()[0].id);
    assert!(list(&c, 100, 2).unwrap().is_empty());
    assert_eq!(list(&c, 101, 0).unwrap_err().code, "invalid_page");
}

#[test]
fn drafts_recover_without_promoting_and_discard_restores_checkpoint_name() {
    let c = catalog();
    let p = create(&c, BLANK).unwrap();
    let d = write(&c, &p.id, 0, &edit(&p, "Draft name"), WriteKind::Autosave).unwrap();
    assert_eq!(d.saved_json, p.saved_json);
    assert_eq!(d.saved_at, p.saved_at);
    assert!(list(&c, 50, 0).unwrap()[0].has_draft);
    let recovered = recover(&c, &p.id, 1).unwrap();
    assert_eq!(Some(recovered.document_json), d.draft_json);
    assert_eq!(
        open(&c, &p.id).unwrap().revision,
        1,
        "recovery is a read, not a checkpoint"
    );
    let discarded = discard_draft(&c, &p.id, 1).unwrap();
    assert_eq!(discarded.name, p.name);
    assert_eq!(discarded.revision, 2);
    assert!(discarded.draft_json.is_none());
    assert_eq!(recover(&c, &p.id, 2).unwrap_err().code, "no_draft");
}

#[test]
fn stale_and_reordered_saves_cannot_overwrite_newer_source() {
    let c = catalog();
    let p = create(&c, BLANK).unwrap();
    let d = write(&c, &p.id, 0, &edit(&p, "Draft"), WriteKind::Autosave).unwrap();
    assert_eq!(
        write(&c, &p.id, 0, &p.saved_json, WriteKind::Checkpoint)
            .unwrap_err()
            .code,
        "conflict"
    );
    let saved = write(
        &c,
        &p.id,
        1,
        d.draft_json.as_ref().unwrap(),
        WriteKind::Checkpoint,
    )
    .unwrap();
    assert_eq!(
        write(&c, &p.id, 1, &p.saved_json, WriteKind::Autosave)
            .unwrap_err()
            .code,
        "conflict"
    );
    for result in [
        rename(&c, &p.id, 1, "Old rename"),
        discard_draft(&c, &p.id, 1),
        duplicate(&c, &p.id, 1, "Copy", false),
    ] {
        assert_eq!(result.unwrap_err().code, "conflict");
    }
    assert_eq!(recover(&c, &p.id, 1).unwrap_err().code, "conflict");
    let current = open(&c, &p.id).unwrap();
    assert_eq!(current.saved_json, saved.saved_json);
    assert_eq!(current.revision, 2);
    assert!(current.draft_json.is_none());
}

#[test]
fn rename_and_save_as_preserve_unfinished_edits_and_future_payloads() {
    let c = catalog();
    let p = create(&c, FUTURE).unwrap();
    let mut draft: Value = serde_json::from_str(&p.saved_json).unwrap();
    draft["video"]["clips"] = json!([{"id":"future-clip","duration":3}]);
    let d = write(&c, &p.id, 0, &draft.to_string(), WriteKind::Autosave).unwrap();
    let renamed = rename(&c, &p.id, 1, " Renamed ").unwrap();
    assert_eq!(renamed.name, "Renamed");
    let checkpoint: Value = serde_json::from_str(&renamed.saved_json).unwrap();
    let draft: Value = serde_json::from_str(renamed.draft_json.as_ref().unwrap()).unwrap();
    assert_eq!(checkpoint["video"]["clips"], json!([]));
    assert_eq!(
        draft["video"]["clips"],
        json!([{"id":"future-clip","duration":3}])
    );
    let copy = duplicate(&c, &p.id, 2, "Saved copy", false).unwrap();
    let save_as = duplicate(&c, &p.id, 2, "Draft copy", true).unwrap();
    assert_ne!(copy.id, save_as.id);
    assert_eq!(copy.revision, 0);
    assert!(save_as.draft_json.is_none());
    let new: Value = serde_json::from_str(&save_as.saved_json).unwrap();
    assert_eq!(new["animations"], draft["animations"]);
    assert_eq!(
        new["scenes"], draft["scenes"],
        "internal node IDs/references are project-scoped"
    );
    assert_eq!(new["video"], draft["video"]);
    assert_eq!(open(&c, &p.id).unwrap().draft_json, renamed.draft_json);
    assert_ne!(d.name, renamed.name);
}

#[test]
fn failed_validation_and_storage_keep_checkpoint_draft_and_revision() {
    let c = catalog();
    let p = create(&c, BLANK).unwrap();
    let d = write(&c, &p.id, 0, &edit(&p, "Keep draft"), WriteKind::Autosave).unwrap();
    assert_eq!(
        write(&c, &p.id, 1, BLANK, WriteKind::Checkpoint)
            .unwrap_err()
            .code,
        "identity_mismatch"
    );
    assert_eq!(
        write(&c, &p.id, 1, "{", WriteKind::Checkpoint)
            .unwrap_err()
            .code,
        "invalid_document"
    );
    assert_eq!(
        rename(&c, &p.id, 1, "  ").unwrap_err().code,
        "invalid_document"
    );
    c.execute_batch("CREATE TRIGGER fail_theme_update AFTER UPDATE ON theme_projects BEGIN SELECT RAISE(ABORT,'injected storage failure'); END;").unwrap();
    assert_eq!(
        write(
            &c,
            &p.id,
            1,
            &edit(&p, "Should fail"),
            WriteKind::Checkpoint
        )
        .unwrap_err()
        .code,
        "storage"
    );
    let after = open(&c, &p.id).unwrap();
    assert_eq!(
        (after.saved_json, after.draft_json, after.revision),
        (p.saved_json, d.draft_json, 1)
    );
    assert_eq!(create(&c, "{}").unwrap_err().code, "invalid_document");
    assert_eq!(list(&c, 50, 0).unwrap().len(), 1);
}

#[test]
fn reopened_catalog_keeps_acknowledged_draft_and_rolls_back_uncommitted_work() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.sqlite");
    let mut c = db::open(&path).unwrap();
    db::migrate(&mut c, Some(&path)).unwrap();
    let p = create(&c, BLANK).unwrap();
    let d = write(&c, &p.id, 0, &edit(&p, "Recover me"), WriteKind::Autosave).unwrap();
    c.execute_batch("BEGIN IMMEDIATE; UPDATE theme_projects SET draft_json=NULL,name=json_extract(saved_json,'$.metadata.name'),revision=revision+1;").unwrap();
    drop(c); // A connection disappearing without commit must not acknowledge its writes.
    let c = db::open(&path).unwrap();
    let recovered = open(&c, &p.id).unwrap();
    assert_eq!(recovered.saved_json, p.saved_json);
    assert_eq!(recovered.draft_json, d.draft_json);
    assert_eq!(recovered.revision, 1);
}

#[test]
fn two_connections_cannot_save_the_same_revision() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.sqlite");
    let mut a = db::open(&path).unwrap();
    db::migrate(&mut a, Some(&path)).unwrap();
    let b = db::open(&path).unwrap();
    let p = create(&a, BLANK).unwrap();
    let old = open(&b, &p.id).unwrap();
    write(
        &a,
        &p.id,
        0,
        &edit(&p, "First writer"),
        WriteKind::Checkpoint,
    )
    .unwrap();
    assert_eq!(
        write(
            &b,
            &p.id,
            old.revision,
            &edit(&old, "Second writer"),
            WriteKind::Autosave
        )
        .unwrap_err()
        .code,
        "conflict"
    );
    assert_eq!(open(&b, &p.id).unwrap().name, "First writer");
}

#[test]
fn malformed_inputs_and_uneditable_stored_source_are_not_replaced_by_defaults() {
    let c = catalog();
    let p = create(&c, BLANK).unwrap();
    assert_eq!(open(&c, "../outside").unwrap_err().code, "invalid_id");
    assert_eq!(open(&c, "missing").unwrap_err().code, "not_found");
    assert_eq!(
        write(&c, &p.id, -1, &p.saved_json, WriteKind::Autosave)
            .unwrap_err()
            .code,
        "invalid_revision"
    );
    let mut future: Value = serde_json::from_str(&p.saved_json).unwrap();
    future["schemaVersion"] = json!("2.0.0");
    let raw = future.to_string();
    c.execute(
        "UPDATE theme_projects SET saved_json=?2,schema_version='2.0.0' WHERE id=?1",
        params![p.id, raw],
    )
    .unwrap();
    assert_eq!(
        open(&c, &p.id).unwrap().saved_json,
        raw,
        "reading preserves uneditable original source"
    );
    assert_eq!(
        rename(&c, &p.id, 0, "Don't replace").unwrap_err().code,
        "invalid_document"
    );
    assert_eq!(
        duplicate(&c, &p.id, 0, "Don't convert", false)
            .unwrap_err()
            .code,
        "invalid_document"
    );
    assert_eq!(
        discard_draft(&c, &p.id, 0).unwrap_err().code,
        "invalid_document"
    );
    assert!(write(&c, &p.id, 0, &p.saved_json, WriteKind::Checkpoint).is_err());
    assert!(write(&c, &p.id, 0, &p.saved_json, WriteKind::Autosave).is_err());
    assert_eq!(open(&c, &p.id).unwrap().saved_json, raw);
}

#[test]
fn project_workflows_leave_legacy_designs_and_synthetic_targets_untouched() {
    let c = catalog();
    let id = designs::create(&c, "Existing setup", designs::DesignSpec::default()).unwrap();
    designs::finish(&c, &id).unwrap();
    let before: String = c
        .query_row("SELECT spec_json FROM designs WHERE id=?1", [&id], |r| {
            r.get(0)
        })
        .unwrap();
    let target = tempfile::tempdir().unwrap();
    let rb = target.path().join("RetroBat.exe");
    let bat = target.path().join("batocera.conf");
    std::fs::write(&rb, b"existing RetroBat").unwrap();
    std::fs::write(&bat, b"existing Batocera").unwrap();
    let p = create(&c, BLANK).unwrap();
    let d = write(&c, &p.id, 0, &edit(&p, "Draft"), WriteKind::Autosave).unwrap();
    recover(&c, &p.id, d.revision).unwrap();
    discard_draft(&c, &p.id, d.revision).unwrap();
    rename(&c, &p.id, 2, "Renamed").unwrap();
    duplicate(&c, &p.id, 3, "Copy", false).unwrap();
    let after: String = c
        .query_row("SELECT spec_json FROM designs WHERE id=?1", [&id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(designs::list(&c).unwrap()[0].name, "Existing setup");
    assert_eq!(std::fs::read(&rb).unwrap(), b"existing RetroBat");
    assert_eq!(std::fs::read(&bat).unwrap(), b"existing Batocera");
    assert_eq!(std::fs::read_dir(target.path()).unwrap().count(), 2);
}

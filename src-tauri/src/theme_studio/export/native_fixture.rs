//! Original native comparison fixture, generated through the real compiler/package path.
//! The ignored preparation test writes only a fresh owned temporary directory; never launches ES.
use super::package::assemble;
use crate::theme_studio::{image_store, projects};
use base64::prelude::*;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn files() -> BTreeMap<String, Vec<u8>> {
    let mut connection = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut connection, None).unwrap();
    let input = include_str!("../../../tests/fixtures/theme_studio/compiler-minimal.json");
    let record = projects::create(&connection, input).unwrap();
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 64, 32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        let pixels: Vec<u8> = (0..64 * 32)
            .flat_map(|i| {
                if i % 64 < 32 {
                    [255, 40, 20, 255]
                } else {
                    [20, 180, 255, 255]
                }
            })
            .collect();
        writer.write_image_data(&pixels).unwrap();
        writer.finish().unwrap();
    }
    let image =
        image_store::import_png(&connection, &record.id, &BASE64_STANDARD.encode(bytes)).unwrap();
    let mut source: Value = serde_json::from_str(&record.saved_json).unwrap();
    let mut caption = source["scenes"][0]["nodes"][0].clone();
    caption["style"]["text"] = json!("RetroDrive native comparison");
    caption["layout"]["x"]["value"] = json!(10);
    caption["layout"]["x"]["unit"] = json!("%");
    caption["layout"]["y"]["value"] = json!(5);
    caption["layout"]["width"]["value"] = json!(80);
    let mut title = caption.clone();
    title["id"] = json!("title");
    title["bindings"] = json!({"text":{"source":"game.title","kind":"text"}});
    title["layout"]["y"]["value"] = json!(25);
    let mut description = caption.clone();
    description["id"] = json!("description");
    description["bindings"] = json!({"text":{"source":"game.description","kind":"text"}});
    description["layout"]["y"]["value"] = json!(40);
    description["layout"]["height"] = json!({"value":20,"unit":"%"});
    let mut picture = caption.clone();
    picture["id"] = json!("picture");
    picture["type"] = json!("image");
    picture["style"] = json!({"imageFit":"contain"});
    picture["assets"] = json!(["picture"]);
    picture["layout"]["y"]["value"] = json!(65);
    picture["layout"]["width"] = json!({"value":20,"unit":"%"});
    picture["layout"]["height"] = json!({"value":20,"unit":"%"});
    source["scenes"][0]["nodes"] = json!([caption.clone(), title, description, picture.clone()]);
    let mut systems = source["scenes"][0].clone();
    systems["id"] = json!("systems");
    systems["role"] = json!("system_select");
    caption["id"] = json!("systems-caption");
    picture["id"] = json!("systems-picture");
    systems["nodes"] = json!([caption, picture]);
    source["scenes"].as_array_mut().unwrap().insert(0, systems);
    source["assets"] = json!([{"id":"picture","path":image.path,"sha256":image.sha256,"bytes":image.bytes,"mediaType":"image/png","license":"original","attribution":"Generated RetroDrive native-test color blocks","source":"Original fixture"}]);
    let record = projects::write(
        &connection,
        &record.id,
        record.revision,
        &source.to_string(),
        projects::WriteKind::Checkpoint,
    )
    .unwrap();
    let candidate = assemble(&connection, &record.id, record.revision).unwrap();
    let mut files: BTreeMap<_, _> = candidate
        .files
        .into_iter()
        .map(|(path, bytes)| {
            (
                format!("home/.emulationstation/themes/retrodrive-native-check/{path}"),
                bytes,
            )
        })
        .collect();
    files.insert("source.json".into(), source.to_string().into_bytes());
    files.insert("home/.emulationstation/es_settings.cfg".into(),b"<config><string name=\"ThemeSet\" value=\"retrodrive-native-check\"/><string name=\"GamelistViewStyle\" value=\"detailed\"/><bool name=\"SaveGamelistsOnExit\" value=\"false\"/></config>".to_vec());
    // These are deliberately inert text markers, not game software or executable commands.
    files.insert("home/.emulationstation/es_systems.cfg".into(),b"<systemList><system><name>rdtest</name><fullname>RetroDrive Fixture</fullname><path>~/fixture-games</path><extension>.rdtest</extension><command>RETRODRIVE_NATIVE_TEST_LAUNCH_DISABLED</command><platform>ignore</platform><theme>rdtest</theme></system></systemList>".to_vec());
    files.insert(
        "home/fixture-games/first.rdtest".into(),
        b"Original inert fixture marker. No game data.".to_vec(),
    );
    files.insert(
        "home/fixture-games/second.rdtest".into(),
        b"Original inert fixture marker. No game data.".to_vec(),
    );
    files.insert("home/fixture-games/gamelist.xml".into(),b"<gameList><game><path>./first.rdtest</path><name>First Original Sample</name><desc>First original description.</desc></game><game><path>./second.rdtest</path><name>Second Original Sample</name><desc>Second description changes with selection.</desc></game></gameList>".to_vec());
    files
}
#[test]
fn original_fixture_has_safe_paths_and_uses_real_package_output() {
    let fixture = files();
    assert_eq!(fixture.len(), 10);
    for path in fixture.keys() {
        assert!(!path.contains(':'));
        assert!(!path.contains(".."));
        assert!(!path.starts_with('/'));
    }
    let theme = String::from_utf8(
        fixture["home/.emulationstation/themes/retrodrive-native-check/theme.xml"].clone(),
    )
    .unwrap();
    assert!(theme.contains("name=\"system\""));
    assert!(theme.contains("name=\"detailed, video\""));
    assert!(theme.contains("name=\"md_name\""));
    assert!(theme.contains("name=\"md_description\""));
    for (path, bytes) in &fixture {
        if path.ends_with(".xml") || path.ends_with(".cfg") {
            let text = std::str::from_utf8(bytes).unwrap();
            let mut reader = quick_xml::Reader::from_str(text);
            loop {
                if reader.read_event().unwrap() == quick_xml::events::Event::Eof {
                    break;
                }
            }
        }
    }
    let config =
        String::from_utf8(fixture["home/.emulationstation/es_systems.cfg"].clone()).unwrap();
    assert!(config.contains("RETRODRIVE_NATIVE_TEST_LAUNCH_DISABLED"));
}
#[test]
#[ignore = "Explicit opt-in: prepares original files in a fresh temporary directory; never launches a frontend"]
fn prepare_native_comparison_fixture() {
    use std::io::Write;
    let root = tempfile::Builder::new()
        .prefix("RetroDrive-native-fixture-")
        .tempdir()
        .unwrap();
    for (relative, bytes) in files() {
        assert!(std::path::Path::new(&relative)
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_))));
        let path = root.path().join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        file.write_all(&bytes).unwrap();
    }
    let root = root.keep();
    println!(
        "Prepared original fixture only (no process launched): {}",
        root.display()
    );
}

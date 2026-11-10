use super::retrobat::*;
use serde_json::{json, Value};
const SOURCE: &str = include_str!("../../../tests/fixtures/theme_studio/compiler-minimal.json");
const GOLDEN: &str = include_str!("../../../tests/fixtures/theme_studio/compiler-minimal.xml");
fn changed(edit: impl FnOnce(&mut Value)) -> String {
    let mut value: Value = serde_json::from_str(SOURCE).unwrap();
    edit(&mut value);
    value.to_string()
}
#[test]
fn original_golden_is_deterministic_parseable_and_preserves_source() {
    let original = SOURCE.to_string();
    let candidate = compile_candidate(&original).unwrap();
    assert_eq!(candidate.xml, GOLDEN.replace("\r\n", "\n"));
    assert!(candidate.xml.contains("\u{4e16}\u{754c}"));
    assert_eq!(candidate.path, "theme.xml");
    assert!(candidate.required_assets.is_empty());
    assert_eq!(original, SOURCE);
    assert_eq!(candidate, compile_candidate(SOURCE).unwrap());
    let mut reader = quick_xml::Reader::from_str(&candidate.xml);
    loop {
        if reader.read_event().unwrap() == quick_xml::events::Event::Eof {
            break;
        }
    }
}
#[test]
fn metadata_bindings_never_emit_static_or_sample_values() {
    for (source, name) in [
        ("game.title", "md_name"),
        ("game.description", "md_description"),
    ] {
        let input = changed(|v| {
            v["scenes"][0]["nodes"][0]["bindings"] = json!({"text":{"source":source,"kind":"text"}})
        });
        let c = compile_candidate(&input).unwrap();
        assert!(c.xml.contains(&format!("name=\"{name}\"")));
        assert!(!c.xml.contains("<text>"));
        assert!(!c.xml.contains("Original"));
    }
}
#[test]
fn unknown_data_and_unmapped_features_block_instead_of_disappearing() {
    for (key, value) in [
        ("opacity", json!(0.5)),
        ("imageCrop", json!({})),
        ("textAlign", json!("justify")),
        ("fontSize", json!({"value":10,"unit":"%"})),
        ("color", json!("red")),
        ("future", json!(true)),
    ] {
        assert!(compile_candidate(&changed(
            |v| v["scenes"][0]["nodes"][0]["style"][key] = value
        ))
        .is_err());
    }
    for input in [
        changed(|v| v["futureAnimation"] = json!({"tracks":[]})),
        changed(|v| v["scenes"][0]["role"] = json!("startup")),
        changed(|v| v["scenes"][0]["nodes"][0]["layout"]["rotation"] = json!(45)),
        changed(|v| v["tokens"] = json!({"color":"red"})),
        changed(|v| v["scenes"][0]["nodes"][0]["type"] = json!("panel")),
        changed(|v| v["scenes"][0]["nodes"][0]["editor"] = json!({"hidden":true})),
    ] {
        assert!(compile_candidate(&input).is_err());
    }
}
#[test]
fn bounds_and_malformed_input_fail_without_partial_output() {
    for text in ["${system.name}", "invalid\u{1}text", &"x".repeat(501)] {
        assert!(compile_candidate(&changed(
            |v| v["scenes"][0]["nodes"][0]["style"]["text"] = json!(text)
        ))
        .is_err());
    }
    for value in [json!(-1), json!(0.0000001), json!(1000000)] {
        assert!(
            compile_candidate(&changed(|v| v["scenes"][0]["nodes"][0]["layout"]
                ["width"] =
                json!({"value":value,"unit":"px"})))
            .is_err()
        );
    }
    assert!(compile_candidate("{}").is_err());
    assert!(compile_candidate(&" ".repeat(2 * 1024 * 1024 + 1)).is_err());
    assert!(compile_candidate(&SOURCE.replace("192,", "1e999,")).is_err());
}
#[test]
fn duplicate_views_and_builtin_bindings_are_blocked() {
    assert!(compile_candidate(&changed(|v| {
        let mut scene = v["scenes"][0].clone();
        scene["id"] = json!("other");
        scene["nodes"] = json!([]);
        v["scenes"].as_array_mut().unwrap().push(scene);
    }))
    .is_err());
    assert!(compile_candidate(&changed(|v| {
        let mut node = v["scenes"][0]["nodes"][0].clone();
        node["bindings"] = json!({"text":{"source":"game.title","kind":"text"}});
        v["scenes"][0]["nodes"][0] = node.clone();
        node["id"] = json!("other");
        v["scenes"][0]["nodes"].as_array_mut().unwrap().push(node);
    }))
    .is_err());
}
#[test]
fn png_requirements_are_declarations_not_copied_or_authorized_bytes() {
    let input = changed(|v| {
        v["assets"] = json!([{"id":"png","path":format!("assets/images/{}.png","a".repeat(64)),"sha256":"a".repeat(64),"bytes":100,"mediaType":"image/png","license":"original","attribution":"Original fixture","source":"local import"}]);
        let n = &mut v["scenes"][0]["nodes"][0];
        n["type"] = json!("image");
        n["style"] = json!({"imageFit":"contain"});
        n["assets"] = json!(["png"]);
    });
    let c = compile_candidate(&input).unwrap();
    assert_eq!(c.required_assets.len(), 1);
    assert_eq!(c.required_assets[0].bytes, 100);
    assert!(c.xml.contains("<maxSize>"));
    let mut missing: Value = serde_json::from_str(&input).unwrap();
    missing["assets"] = json!([]);
    assert!(compile_candidate(&missing.to_string()).is_err());
    let mut bad: Value = serde_json::from_str(&input).unwrap();
    bad["assets"][0]["path"] = json!("../escape.png");
    assert!(compile_candidate(&bad.to_string()).is_err());
}
#[test]
fn locks_are_editor_only_and_portrait_font_units_use_short_axis() {
    let input = changed(|v| {
        v["scenes"][0]["nodes"][0]["editor"] = json!({"locked":true,"hidden":false});
        v["scenes"][0]["resolution"] = json!({"width":1080,"height":1920});
    });
    assert!(compile_candidate(&input)
        .unwrap()
        .xml
        .contains("<fontSize>0.050000</fontSize>"));
    assert!(crate::theme_studio::adapters::registry().is_empty());
}

#[test]
fn candidate_node_budget_and_nested_layers_fail_closed() {
    let many = changed(|v| {
        let template = v["scenes"][0]["nodes"][0].clone();
        v["scenes"][0]["nodes"] = Value::Array(
            (0..128)
                .map(|i| {
                    let mut n = template.clone();
                    n["id"] = json!(format!("node-{i}"));
                    n
                })
                .collect(),
        );
    });
    assert!(compile_candidate(&many).is_ok());
    let mut too_many: Value = serde_json::from_str(&many).unwrap();
    let mut extra = too_many["scenes"][0]["nodes"][0].clone();
    extra["id"] = json!("extra");
    too_many["scenes"][0]["nodes"]
        .as_array_mut()
        .unwrap()
        .push(extra);
    assert_eq!(
        compile_candidate(&too_many.to_string()).unwrap_err().0,
        "Candidate limit is two scenes and 128 nodes."
    );
    let nested = changed(|v| {
        let mut child = v["scenes"][0]["nodes"][0].clone();
        child["id"] = json!("child");
        v["scenes"][0]["nodes"][0]["children"] = json!(["child"]);
        v["scenes"][0]["nodes"].as_array_mut().unwrap().push(child);
    });
    assert_eq!(
        compile_candidate(&nested).unwrap_err().0,
        "Nested or rotated layers require a verified mapping."
    );
}

/// ADR-112: the game list, the game's video and game data use RetroBat's own elements,
/// and built-in extras the design doesn't place are hidden.
#[test]
fn game_list_video_and_game_data_map_to_native_elements_and_unused_extras_are_hidden() {
    let input = changed(|v| {
        let base = v["scenes"][0]["nodes"][0].clone();
        let mut list = base.clone();
        list["id"] = json!("list-1");
        list["type"] = json!("game_list");
        list["style"] = json!({"color": "c8d0e0", "selectedColor": "ffffff", "selectorColor": "aa3300", "fontSize": {"value": 32, "unit": "px"}, "textAlign": "left"});
        let mut video = base.clone();
        video["id"] = json!("video-1");
        video["type"] = json!("video");
        video["style"] = json!({});
        video["layout"] = json!({"x": {"value": 50, "unit": "%"}, "y": {"value": 20, "unit": "%"}, "width": {"value": 40, "unit": "%"}, "height": {"value": 40, "unit": "%"}, "rotation": 0});
        video["bindings"] = json!({"video": {"source": "game.video", "kind": "video"}});
        let mut system = base.clone();
        system["id"] = json!("system-1");
        system["bindings"] = json!({"text": {"source": "system.name", "kind": "text"}});
        let mut year = base.clone();
        year["id"] = json!("year-1");
        year["style"] = json!({"color": "f4c96b"});
        year["bindings"] = json!({"text": {"source": "game.year", "kind": "number"}});
        v["scenes"][0]["nodes"] = json!([list, video, system, year]);
    });
    let xml = compile_candidate(&input).unwrap().xml;
    assert!(xml.contains("<textlist name=\"gamelist\">") && xml.contains("<selectorColor>aa3300</selectorColor>") && xml.contains("<primaryColor>c8d0e0</primaryColor>"), "{xml}");
    assert!(xml.contains("<video name=\"md_video\">\n      <pos>0.700000 0.400000</pos>\n      <maxSize>0.400000 0.400000</maxSize>\n      <origin>0.5 0.5</origin>"), "{xml}");
    assert!(xml.contains("<text name=\"logoText\">") && xml.contains("<datetime name=\"md_releasedate\">"), "{xml}");
    let date = &xml[xml.find("<datetime name=\"md_releasedate\">").unwrap()..];
    let date = &date[..date.find("</datetime>").unwrap()];
    assert!(!date.contains("<text>") && !date.contains("lineSpacing") && date.contains("<color>f4c96b</color>"), "{date}");
    // Placed elements are not hidden; unplaced ones are.
    assert!(!xml.contains("name=\"md_video\">\n      <visible>false</visible>") && !xml.contains("name=\"logoText\">\n      <visible>false</visible>"));
    assert!(xml.contains("<rating name=\"md_rating\">\n      <visible>false</visible>") && xml.contains("<image name=\"md_image\">\n      <visible>false</visible>"));
    assert!(!xml.contains("<datetime name=\"md_releasedate\">\n      <visible>false</visible>"));
    let mut reader = quick_xml::Reader::from_str(&xml);
    while reader.read_event().unwrap() != quick_xml::events::Event::Eof {}
    assert_eq!(xml.matches("<textlist name=\"gamelist\">").count(), 1, "a placed game list isn't duplicated");
    let plain = compile_candidate(SOURCE).unwrap().xml;
    assert!(plain.contains("<textlist name=\"gamelist\">
      <pos>0.040000 0.150000</pos>"), "a design without a game list still gets one (native check)");

    // Blocked instead of guessed.
    for bad in [
        changed(|v| { v["scenes"][0]["role"] = json!("system_select"); v["scenes"][0]["nodes"][0]["type"] = json!("game_list"); }),
        changed(|v| { v["scenes"][0]["nodes"][0]["type"] = json!("video"); v["scenes"][0]["nodes"][0]["style"] = json!({}); }),
        changed(|v| { v["scenes"][0]["nodes"][0]["type"] = json!("video"); v["scenes"][0]["nodes"][0]["style"] = json!({}); v["scenes"][0]["nodes"][0]["bindings"] = json!({"video": {"source": "game.title", "kind": "video"}}); }),
        changed(|v| { v["scenes"][0]["nodes"][0]["type"] = json!("game_list"); v["scenes"][0]["nodes"][0]["bindings"] = json!({"text": {"source": "game.title", "kind": "text"}}); }),
        changed(|v| v["scenes"][0]["nodes"][0]["bindings"] = json!({"text": {"source": "game.players", "kind": "text"}})),
        changed(|v| v["scenes"][0]["nodes"][0]["bindings"] = json!({"text": {"source": "game.rating", "kind": "number"}})),
    ] {
        assert!(compile_candidate(&bad).is_err(), "{bad}");
    }
}


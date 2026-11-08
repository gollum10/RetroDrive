use super::{model::*, validation::*};
use serde_json::{json, Value};

const BLANK: &str = include_str!("../../tests/fixtures/theme_studio/blank.json");
const STARTER: &str = include_str!("../../tests/fixtures/theme_studio/starter.json");
const FUTURE: &str = include_str!("../../tests/fixtures/theme_studio/future-extensions.json");

fn edited(edit: impl FnOnce(&mut Value)) -> Result<ThemeProject, Vec<Issue>> {
    let mut value: Value = serde_json::from_str(STARTER).unwrap();
    edit(&mut value);
    parse_project(&value.to_string())
}

fn rejects(code: &str, edit: impl FnOnce(&mut Value)) {
    let errors = edited(edit).unwrap_err();
    assert!(
        errors.iter().any(|e| e.code == code),
        "Expected {code}: {errors:?}"
    );
}

#[test]
fn blank_project_is_universal_without_a_target_or_installation() {
    let project = parse_project(BLANK).unwrap();
    assert!(project.targets.is_empty());
    assert_eq!(project.scenes[0].role, "system_select");
    assert_eq!(
        serde_json::to_value(project).unwrap(),
        serde_json::from_str::<Value>(BLANK).unwrap()
    );
}

#[test]
fn original_starter_has_all_design_roles_and_a_typed_binding() {
    let project = parse_project(STARTER).unwrap();
    assert_eq!(project.scenes.len(), 8);
    assert_eq!(project.scenes[3].nodes[0].bindings["text"].kind, "text");
    assert!(unsupported_features(&project).is_empty());
    let intended =
        edited(|v| v["targets"] = json!([{"frontendFamily":"retrobat","frontendVersion":null}]))
            .unwrap();
    assert_eq!(intended.targets[0].frontend_version, None);
}

#[test]
fn future_payloads_survive_exact_json_value_round_trip_without_claiming_support() {
    let project = parse_project(FUTURE).unwrap();
    let mut expected: Value = serde_json::from_str(FUTURE).unwrap();
    // Known continuous layout values normalize to f64; unknown payloads do not.
    for scene in expected["scenes"].as_array_mut().unwrap() {
        for node in scene["nodes"].as_array_mut().unwrap() {
            for property in ["x", "y", "width", "height"] {
                node["layout"][property]["value"] =
                    json!(node["layout"][property]["value"].as_f64().unwrap());
            }
            node["layout"]["rotation"] = json!(node["layout"]["rotation"].as_f64().unwrap());
        }
    }
    assert_eq!(serde_json::to_value(&project).unwrap(), expected);
    assert!(unsupported_features(&project)
        .iter()
        .any(|i| i.code == "unknown_node"));
    assert_eq!(
        parse_project(&serde_json::to_string(&project).unwrap()).unwrap(),
        project
    );
    let unknown_scene = edited(|v| v["scenes"][0]["role"] = json!("future_role")).unwrap();
    assert_eq!(
        unsupported_features(&unknown_scene)[0].code,
        "unknown_scene"
    );
}

#[test]
fn version_mismatch_and_corrupt_source_never_become_empty_projects() {
    for version in ["2.0.0", "1.1.0", "garbage", "0.1.0"] {
        rejects("schema_version", |v| v["schemaVersion"] = json!(version));
    }
    rejects("format", |v| v["format"] = json!("retrodrive-theme"));
    rejects("invalid_shape", |v| {
        v.as_object_mut().unwrap().remove("scenes");
    });
    assert_eq!(parse_project("{").unwrap_err()[0].code, "invalid_json");
    assert!(parse_project(&STARTER.replace(
        "\"id\": \"original-starter\"",
        "\"id\":\"one\",\"id\":\"two\""
    ))
    .is_err());
}

#[test]
fn identifiers_and_metadata_are_validated() {
    rejects("id", |v| v["id"] = json!("../escape"));
    rejects("name", |v| v["metadata"]["name"] = json!("   "));
    rejects("license", |v| v["metadata"]["license"] = json!(""));
    rejects("scene_id", |v| {
        v["scenes"][1]["id"] = v["scenes"][0]["id"].clone();
    });
    rejects("node_id", |v| {
        let node = v["scenes"][3]["nodes"][0].clone();
        v["scenes"][0]["nodes"] = json!([node]);
    });
}

#[test]
fn scene_graph_rejects_missing_children_multiple_parents_and_cycles() {
    assert!(parse_project(include_str!(
        "../../tests/fixtures/theme_studio/invalid-reference.json"
    ))
    .is_err());
    rejects("child_reference", |v| {
        v["scenes"][3]["nodes"][0]["children"] = json!(["missing"])
    });
    rejects("scene_cycle", |v| {
        v["scenes"][3]["nodes"][0]["children"] = json!(["title"])
    });
    rejects("scene_cycle", |v| {
        let mut node = v["scenes"][3]["nodes"][0].clone();
        node["id"] = json!("other");
        node["children"] = json!(["title"]);
        v["scenes"][3]["nodes"][0]["children"] = json!(["other"]);
        v["scenes"][3]["nodes"].as_array_mut().unwrap().push(node);
    });
    rejects("multiple_parents", |v| {
        let mut child = v["scenes"][3]["nodes"][0].clone();
        child["id"] = json!("child");
        v["scenes"][3]["nodes"][0]["children"] = json!(["child", "child"]);
        v["scenes"][3]["nodes"].as_array_mut().unwrap().push(child);
    });
    // A real hierarchy is allowed; arena order does not have to be topological.
    edited(|v| {
        let mut parent = v["scenes"][3]["nodes"][0].clone();
        parent["id"] = json!("parent");
        parent["type"] = json!("group");
        parent["children"] = json!(["title"]);
        v["scenes"][3]["nodes"].as_array_mut().unwrap().push(parent);
    })
    .unwrap();
}

#[test]
fn dimensions_resolutions_and_binding_types_are_bounded() {
    rejects("resolution", |v| {
        v["scenes"][0]["resolution"]["width"] = json!(0)
    });
    rejects("dimension", |v| {
        v["scenes"][3]["nodes"][0]["layout"]["x"]["unit"] = json!("em")
    });
    rejects("dimension", |v| {
        v["scenes"][3]["nodes"][0]["layout"]["width"]["value"] = json!(-1)
    });
    rejects("rotation", |v| {
        v["scenes"][3]["nodes"][0]["layout"]["rotation"] = json!(1e10)
    });
    rejects("binding", |v| {
        v["scenes"][3]["nodes"][0]["bindings"]["text"]["kind"] = json!("number")
    });
    rejects("binding", |v| {
        v["scenes"][3]["nodes"][0]["bindings"]["text"]["source"] = json!("eval(userInput)")
    });
    let mut project = parse_project(STARTER).unwrap();
    project.scenes[3].nodes[0].layout.x.value = f64::NAN;
    assert!(validate_project(&project)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "dimension"));
    project.scenes[3].nodes[0].layout.rotation = f64::INFINITY;
    assert!(validate_project(&project)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "rotation"));
}

#[test]
fn portable_source_rejects_machine_paths_even_inside_future_data() {
    for path in [
        "C:\\Users\\owner\\art.png",
        "D:relative.png",
        "\\\\nas\\share\\art.png",
        "/home/user/art.png",
        "FILE:///C:/art.png",
        "\\\\?\\C:\\art.png",
    ] {
        rejects("absolute_path", |v| v["futureMedia"] = json!({"path":path}));
    }
    rejects("absolute_path", |v| {
        v.as_object_mut()
            .unwrap()
            .insert("C:\\local".into(), json!(true));
    });
}

fn asset() -> Value {
    json!({"id":"background","path":format!("assets/images/{}.png", "a".repeat(64)),"sha256":"a".repeat(64),"bytes":4,"mediaType":"image/png","license":"original","attribution":"Creator","source":"user supplied"})
}

#[test]
fn assets_are_content_addressed_and_references_are_checked() {
    let project = edited(|v| {
        v["assets"] = json!([asset()]);
        v["scenes"][3]["nodes"][0]["assets"] = json!(["background"]);
    })
    .unwrap();
    assert_eq!(project.assets.len(), 1);
    rejects("asset_reference", |v| {
        v["scenes"][3]["nodes"][0]["assets"] = json!(["missing"])
    });
    rejects("asset_id", |v| v["assets"] = json!([asset(), asset()]));
    rejects("asset_hash", |v| {
        let mut a = asset();
        a["sha256"] = json!("A".repeat(64));
        v["assets"] = json!([a]);
    });
    rejects("number_range", |v| {
        let mut a = asset();
        a["bytes"] = json!(9_007_199_254_740_992_u64);
        v["assets"] = json!([a]);
    });
    for path in [
        "../secret.png",
        "assets/../secret.png",
        "assets\\images\\art.png",
        "https://host/art.png",
        "assets/images/con.png",
        "assets/images/script.exe",
        "assets/images/%2e%2e.png",
    ] {
        rejects("asset_path", |v| {
            let mut a = asset();
            a["path"] = json!(path);
            v["assets"] = json!([a]);
        });
    }
}

#[test]
fn source_size_depth_and_scene_count_have_explicit_limits() {
    assert_eq!(
        parse_project(&" ".repeat(MAX_DOCUMENT_BYTES + 1)).unwrap_err()[0].code,
        "document_size"
    );
    rejects("scene_count", |v| v["scenes"] = json!([]));
    rejects("document_depth", |v| {
        let mut nested = json!(null);
        for _ in 0..70 {
            nested = json!({"next":nested});
        }
        v["future"] = nested;
    });
}

#[test]
fn extensions_cannot_shadow_typed_fields_on_constructed_documents() {
    let mut project = parse_project(BLANK).unwrap();
    project
        .metadata
        .extra
        .insert("name".into(), json!("shadowed"));
    assert!(validate_project(&project)
        .unwrap_err()
        .iter()
        .any(|i| i.code == "field_collision"));
}

#[test]
fn duplicate_unknown_keys_are_rejected_instead_of_losing_data() {
    let json = BLANK.replace(
        "\"tokens\": {}",
        "\"tokens\": {\"future\": 1, \"future\": 2}",
    );
    assert_eq!(parse_project(&json).unwrap_err()[0].code, "invalid_json");
    let escaped = BLANK.replace("\"tokens\": {}", "\"tokens\": {\"a\": 1, \"\\u0061\": 2}");
    assert_eq!(parse_project(&escaped).unwrap_err()[0].code, "invalid_json");
}

#[test]
fn future_data_cannot_lose_large_integer_precision_in_the_typescript_editor() {
    rejects("number_range", |v| {
        v["futureCounter"] = json!(9_007_199_254_740_993_u64)
    });
}

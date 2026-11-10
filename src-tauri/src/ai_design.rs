//! Ask AI for design suggestions (ADR-120, docs/18 §71): step 2 of the AI plan (ADR-118).
//!
//! The editor sends one screen's layers as data (types, positions, sizes, colours, text, which
//! game detail a text shows) plus the user's request. Never sent: game names from the library,
//! file or folder paths, picture files, RetroDrive settings or other themes. The answer must
//! follow a fixed JSON schema (a short explanation plus a list of edits from a small fixed set);
//! the editor then checks every edit against the current theme before showing it.
//!
//! Model text is never run, never used as a path and never opens links.
use crate::ai::{self, KeyStore};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Requests per day (both services together); protects against surprise costs.
pub const DAILY_LIMIT: u32 = 50;
/// The longest request the user can type.
pub const MAX_REQUEST_CHARS: usize = 1000;
/// The largest screen description sent (one screen's layers as JSON).
pub const MAX_SCREEN_BYTES: usize = 100_000;
/// At most this many edits are taken from an answer.
pub const MAX_EDITS: usize = 40;

/// The kinds of edit the AI may suggest. Anything else is dropped by the editor.
pub const OPS: &[&str] = &[
    "move", "resize", "color", "fill", "selected_color", "highlight_color", "text", "font_size", "link", "align", "opacity",
    "order", "hide", "add_text", "add_panel",
];
const SOURCES: &[&str] = &["", "game.title", "game.description", "game.year", "game.players", "game.rating", "system.name", "system.games"];

fn nullable(kind: &str) -> Value {
    json!({ "anyOf": [{ "type": kind }, { "type": "null" }] })
}

fn nullable_enum(values: &[&str]) -> Value {
    json!({ "anyOf": [{ "type": "string", "enum": values }, { "type": "null" }] })
}

/// The answer's shape (structured output). Every field is present; unused ones are null.
pub fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["explanation", "edits"],
        "properties": {
            "explanation": { "type": "string", "description": "One or two short, friendly sentences saying what the changes do." },
            "edits": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["op", "layer", "x", "y", "width", "height", "color", "text", "fontSize", "source", "align", "opacity", "order"],
                    "properties": {
                        "op": { "type": "string", "enum": OPS },
                        "layer": { "type": "string", "description": "The id of an existing layer; empty for add_text and add_panel." },
                        "x": nullable("number"), "y": nullable("number"), "width": nullable("number"), "height": nullable("number"),
                        "color": nullable("string"),
                        "text": nullable("string"),
                        "fontSize": nullable("number"),
                        "source": nullable_enum(SOURCES),
                        "align": nullable_enum(&["left", "center", "right"]),
                        "opacity": nullable("number"),
                        "order": nullable_enum(&["front", "back", "forward", "backward"])
                    }
                }
            }
        }
    })
}

/// What the assistant is told. The user's request and the screen follow as the user message.
pub const SYSTEM_PROMPT: &str = "You help people design themes for retro game frontends (RetroBat / EmulationStation) in RetroDrive's Theme Studio. \
You get one screen as JSON: its size and its layers, back to front. Positions and sizes are percentages of the screen (x and y are the top-left corner). \
Layer types: text, panel (a coloured box), image (the user's picture; you cannot change the picture itself), video (the selected game's video or the user's clip), \
game_list (the list of games), system_carousel (the band of systems on System Select). Locked or hidden layers cannot be changed.\n\
Answer with a short explanation and a list of edits. Each edit has an op, the layer's id, and only the fields that op uses; set every other field to null.\n\
- move: x, y. resize: width, height (and optionally x, y).\n\
- color: text colour of a text, game_list or system_carousel. fill: background colour of a panel or system_carousel.\n\
- selected_color / highlight_color: the game list's selected-text colour and its highlight bar colour.\n\
- text: new wording for a text layer that is not linked to a game detail. font_size: size in pixels (text or game_list).\n\
- link: make a text show a game detail (source), or \"\" for typed text. align: left, center or right (text).\n\
- opacity: 0 to 1. order: front, back, forward or backward. hide: hide a layer.\n\
- add_text: x, y, width, height, text, color, fontSize (layer empty). add_panel: x, y, width, height, color as its fill (layer empty).\n\
Colours are hex without #: RRGGBB, or RRGGBBAA for see-through. Keep the game list, game video and carousel readable and on screen. \
Suggest at most 20 edits. Only use layer ids you were given. Text you write appears on the user's screen, so keep it short and suitable for all ages.";

/// One design request from the editor.
pub struct DesignAsk {
    pub provider: String,
    pub screen: Value,
    pub request: String,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DesignAnswer {
    pub explanation: String,
    pub edits: Vec<Value>,
    /// Which AI answered, for the result line.
    pub answered_by: String,
    /// Roughly what the request cost in US dollars (Claude reports usage); None for a private AI.
    pub cost_usd: Option<f64>,
    pub requests_left: u32,
}

/// The exact text sent besides the system prompt (shown by "Show exactly").
pub fn user_message(screen: &Value, request: &str) -> String {
    format!("Screen (JSON):\n{}\n\nRequest:\n{}", serde_json::to_string(screen).unwrap_or_default(), request.trim())
}

/// Check what the editor wants to send before anything leaves the PC.
pub fn check_ask(ask: &DesignAsk) -> Result<(), String> {
    let request = ask.request.trim();
    if request.is_empty() {
        return Err("Type what you'd like first.".into());
    }
    if request.chars().count() > MAX_REQUEST_CHARS {
        return Err(format!("Keep the request under {MAX_REQUEST_CHARS} characters."));
    }
    if !ask.screen.is_object() || serde_json::to_vec(&ask.screen).map(|b| b.len()).unwrap_or(usize::MAX) > MAX_SCREEN_BYTES {
        return Err("This screen is too big to send. Ask about a simpler screen.".into());
    }
    if ask.provider != "claude" && ask.provider != "private" {
        return Err("Choose Claude or your private AI.".into());
    }
    Ok(())
}

fn today() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() / 86_400).unwrap_or(0)
}

/// Count one request against today's limit (saved before sending). Returns how many are left.
pub fn take_request(catalog: &Path, day: u64) -> Result<u32, String> {
    let mut settings = ai::load_settings(catalog);
    let used = if settings.requests_day == Some(day) { settings.requests_today } else { 0 };
    if used >= DAILY_LIMIT {
        return Err(format!("You've asked {DAILY_LIMIT} times today, the daily limit that protects against surprise costs. Try again tomorrow."));
    }
    settings.requests_day = Some(day);
    settings.requests_today = used + 1;
    ai::save_settings(catalog, &settings)?;
    Ok(DAILY_LIMIT - used - 1)
}

/// US dollars per million input and output tokens for the Claude models offered.
fn prices(model: &str) -> Option<(f64, f64)> {
    match model {
        "claude-opus-5-5" => Some((4.0, 20.0)),
        "claude-sonnet-5-5" => Some((2.0, 10.0)),
        "claude-haiku-5-5" => Some((0.10, 0.50)),
        _ => None,
    }
}

/// The Messages API request for Claude.
pub fn claude_body(model: &str, screen: &Value, request: &str) -> Value {
    let mut output = json!({ "format": { "type": "json_schema", "schema": schema() } });
    if model != "claude-haiku-5-5" {
        output["effort"] = json!("medium");
    }
    json!({
        "model": model,
        "max_tokens": 16000,
        "system": SYSTEM_PROMPT,
        "messages": [{ "role": "user", "content": user_message(screen, request) }],
        "output_config": output
    })
}

/// The chat request for a private OpenAI-compatible server.
pub fn private_body(model: &str, screen: &Value, request: &str) -> Value {
    json!({
        "model": model,
        "stream": false,
        "temperature": 0.3,
        "messages": [
            { "role": "system", "content": SYSTEM_PROMPT },
            { "role": "user", "content": user_message(screen, request) }
        ],
        "response_format": { "type": "json_schema", "json_schema": { "name": "design_suggestions", "strict": true, "schema": schema() } }
    })
}

/// Check the answer's outline; the editor checks each edit against the theme.
pub fn check_answer(text: &str) -> Result<(String, Vec<Value>), String> {
    let unusable = || "That model's answer couldn't be used. Try again, or try a different model.".to_string();
    let trimmed = text.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
    let value: Value = serde_json::from_str(trimmed).map_err(|_| unusable())?;
    let explanation = value.get("explanation").and_then(Value::as_str).ok_or_else(unusable)?;
    let edits = value.get("edits").and_then(Value::as_array).ok_or_else(unusable)?;
    let explanation: String = explanation.chars().take(1000).collect();
    Ok((explanation, edits.iter().filter(|e| e.is_object()).take(MAX_EDITS).cloned().collect()))
}

/// Read Claude's reply: refusals and cut-off answers are explained; usage gives the cost.
pub fn read_claude_reply(model: &str, reply: &Value) -> Result<(String, Vec<Value>, Option<f64>), String> {
    match reply.get("stop_reason").and_then(Value::as_str) {
        Some("refusal") => return Err("Claude declined that request. Try asking another way.".into()),
        Some("max_tokens") => return Err("The answer was too long. Ask for fewer changes at once.".into()),
        _ => {}
    }
    let text = reply
        .get("content")
        .and_then(Value::as_array)
        .and_then(|blocks| blocks.iter().find(|b| b.get("type").and_then(Value::as_str) == Some("text")))
        .and_then(|b| b.get("text").and_then(Value::as_str))
        .ok_or("Claude's answer was empty. Try again.")?;
    let (explanation, edits) = check_answer(text)?;
    let usage = reply.get("usage");
    let tokens = |key: &str| usage.and_then(|u| u.get(key)).and_then(Value::as_f64).unwrap_or(0.0);
    let cost = prices(model).map(|(input, output)| (tokens("input_tokens") * input + tokens("output_tokens") * output) / 1_000_000.0);
    Ok((explanation, edits, cost))
}

fn claude_error(status: u16, body: Option<Value>) -> String {
    if status == 400 {
        let detail = body
            .as_ref()
            .and_then(|b| b.pointer("/error/message"))
            .and_then(Value::as_str)
            .map(|m| m.chars().take(200).collect::<String>())
            .unwrap_or_default();
        return format!("Claude couldn't use that request. {detail}").trim().to_string();
    }
    if status == 529 {
        return "Claude is very busy right now. Try again in a minute.".into();
    }
    ai::claude_check_message(status).err().unwrap_or_else(|| format!("Claude answered with an unexpected status ({status})."))
}

fn long_agent(seconds: u64, tls: bool) -> Result<ureq::Agent, String> {
    let mut builder = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(10)).timeout(Duration::from_secs(seconds)).redirects(0);
    if tls {
        let connector = native_tls::TlsConnector::new().map_err(|e| format!("Secure connections aren't available: {e}"))?;
        builder = builder.tls_connector(std::sync::Arc::new(connector));
    }
    Ok(builder.build())
}

/// Ask the chosen AI for suggestions. Counts against today's limit before sending.
pub fn ask(catalog: &Path, keys: &dyn KeyStore, ask: &DesignAsk) -> Result<DesignAnswer, String> {
    check_ask(ask)?;
    let settings = ai::load_settings(catalog);
    if ask.provider == "claude" {
        let key = keys.get()?.ok_or("Connect Claude in Settings → AI helpers first.")?;
        let status = ai::status(catalog, keys)?;
        let model = status.claude_model;
        let left = take_request(catalog, today())?;
        let response = long_agent(240, true)?
            .post(&format!("{}/v1/messages", ai::CLAUDE_API))
            .set("x-api-key", &key)
            .set("anthropic-version", ai::ANTHROPIC_VERSION)
            .set("content-type", "application/json")
            .send_json(claude_body(&model, &ask.screen, &ask.request));
        let reply: Value = match response {
            Ok(r) => r.into_json().map_err(|_| "Claude's answer couldn't be read. Try again.".to_string())?,
            Err(ureq::Error::Status(code, r)) => return Err(claude_error(code, r.into_json().ok())),
            Err(ureq::Error::Transport(t)) if t.kind() == ureq::ErrorKind::Io => {
                return Err("Claude took too long to answer. Try again, or ask for fewer changes.".into())
            }
            Err(_) => return Err("Couldn't reach Claude. Check the internet connection.".into()),
        };
        let (explanation, edits, cost_usd) = read_claude_reply(&model, &reply)?;
        let label = ai::CLAUDE_MODELS.iter().find(|(id, _)| *id == model).map(|(_, l)| l.split(" (").next().unwrap_or(l).to_string()).unwrap_or(model);
        return Ok(DesignAnswer { explanation, edits, answered_by: label, cost_usd, requests_left: left });
    }
    let (url, model) = match (settings.private_url, settings.private_model) {
        (Some(url), Some(model)) => (ai::private_url(&url)?, model),
        _ => return Err("Connect your private AI in Settings → AI helpers first.".into()),
    };
    let left = take_request(catalog, today())?;
    let reply: Value = long_agent(300, false)?
        .post(&format!("{url}/v1/chat/completions"))
        .send_json(private_body(&model, &ask.screen, &ask.request))
        .map_err(|e| match e {
            ureq::Error::Status(code, _) => format!("Your private AI answered with an error ({code}). Check the model is loaded."),
            _ => format!("Nothing answered at {url}, or it took too long. Is the AI program running?"),
        })?
        .into_json()
        .map_err(|_| "That model's answer couldn't be used. Try again, or try a different model.".to_string())?;
    let text = reply.pointer("/choices/0/message/content").and_then(Value::as_str).ok_or("That model's answer couldn't be used. Try again, or try a different model.")?;
    let (explanation, edits) = check_answer(text)?;
    Ok(DesignAnswer { explanation, edits, answered_by: model, cost_usd: None, requests_left: left })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen() -> Value {
        json!({ "screen": { "name": "Game list", "role": "game_list", "width": 1920, "height": 1080 }, "layers": [{ "id": "text-1", "type": "text", "x": 10, "y": 5, "width": 30, "height": 10, "text": "Hello" }] })
    }

    #[test]
    fn requests_are_checked_before_anything_is_sent() {
        let ok = DesignAsk { provider: "claude".into(), screen: screen(), request: "Make it purple".into() };
        assert!(check_ask(&ok).is_ok());
        assert!(check_ask(&DesignAsk { request: "   ".into(), ..ok_clone(&ok) }).is_err());
        assert!(check_ask(&DesignAsk { request: "x".repeat(MAX_REQUEST_CHARS + 1), ..ok_clone(&ok) }).is_err());
        assert!(check_ask(&DesignAsk { provider: "elsewhere".into(), ..ok_clone(&ok) }).is_err());
        assert!(check_ask(&DesignAsk { screen: json!("not a screen"), ..ok_clone(&ok) }).is_err());
        assert!(check_ask(&DesignAsk { screen: json!({ "big": "x".repeat(MAX_SCREEN_BYTES) }), ..ok_clone(&ok) }).is_err());
    }

    fn ok_clone(a: &DesignAsk) -> DesignAsk {
        DesignAsk { provider: a.provider.clone(), screen: a.screen.clone(), request: a.request.clone() }
    }

    #[test]
    fn the_request_holds_only_the_screen_and_the_words_typed() {
        let body = claude_body("claude-opus-5-5", &screen(), "  Make it purple ");
        assert_eq!(body["model"], "claude-opus-5-5");
        assert_eq!(body["output_config"]["format"]["type"], "json_schema");
        assert_eq!(body["output_config"]["effort"], "medium");
        assert!(body.get("tool_choice").is_none() && body.get("thinking").is_none());
        let text = body["messages"][0]["content"].as_str().unwrap();
        assert!(text.ends_with("Request:\nMake it purple") && text.contains("\"text-1\""));
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
        assert!(claude_body("claude-haiku-5-5", &screen(), "x")["output_config"].get("effort").is_none());
        let private = private_body("llama3", &screen(), "Make it purple");
        assert_eq!(private["response_format"]["json_schema"]["strict"], true);
        assert_eq!(private["messages"][1]["content"], body["messages"][0]["content"]);
    }

    #[test]
    fn every_object_in_the_schema_is_closed_and_fully_required() {
        fn walk(v: &Value) {
            if v.get("type") == Some(&json!("object")) {
                assert_eq!(v["additionalProperties"], false);
                let props: Vec<&String> = v["properties"].as_object().unwrap().keys().collect();
                let required: Vec<&str> = v["required"].as_array().unwrap().iter().map(|r| r.as_str().unwrap()).collect();
                assert!(props.iter().all(|p| required.contains(&p.as_str())), "{props:?} vs {required:?}");
            }
            match v {
                Value::Object(map) => map.values().for_each(walk),
                Value::Array(items) => items.iter().for_each(walk),
                _ => {}
            }
        }
        walk(&schema());
        assert!(!schema().to_string().contains("minimum") && !schema().to_string().contains("maxLength"));
    }

    #[test]
    fn answers_are_read_carefully_and_cost_is_worked_out() {
        let reply = json!({
            "stop_reason": "end_turn",
            "content": [{ "type": "thinking", "thinking": "" }, { "type": "text", "text": "{\"explanation\":\"Purple!\",\"edits\":[{\"op\":\"color\"},7]}" }],
            "usage": { "input_tokens": 2000, "output_tokens": 1000 }
        });
        let (explanation, edits, cost) = read_claude_reply("claude-opus-5-5", &reply).unwrap();
        assert_eq!(explanation, "Purple!");
        assert_eq!(edits.len(), 1, "non-objects are dropped");
        assert!((cost.unwrap() - 0.028).abs() < 1e-9);
        assert!(read_claude_reply("claude-opus-5-5", &json!({ "stop_reason": "refusal", "content": [] })).unwrap_err().contains("declined"));
        assert!(read_claude_reply("claude-opus-5-5", &json!({ "stop_reason": "max_tokens", "content": [] })).unwrap_err().contains("too long"));
        assert!(check_answer("not json").is_err());
        assert!(check_answer("{\"explanation\": 3, \"edits\": []}").is_err());
        assert!(check_answer("```json\n{\"explanation\":\"ok\",\"edits\":[]}\n```").is_ok());
        let many = format!("{{\"explanation\":\"ok\",\"edits\":[{}]}}", vec!["{}"; 100].join(","));
        assert_eq!(check_answer(&many).unwrap().1.len(), MAX_EDITS);
        assert!(claude_error(401, None).contains("didn't accept"));
        assert!(claude_error(400, Some(json!({ "error": { "message": "bad schema" } }))).contains("bad schema"));
    }

    #[test]
    fn the_daily_limit_counts_requests_and_resets_each_day() {
        let dir = std::env::temp_dir().join(format!("rd-ai-limit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let catalog = dir.join("catalog.sqlite");
        assert_eq!(take_request(&catalog, 100).unwrap(), DAILY_LIMIT - 1);
        for _ in 1..DAILY_LIMIT {
            take_request(&catalog, 100).unwrap();
        }
        assert!(take_request(&catalog, 100).unwrap_err().contains("daily limit"));
        assert_eq!(take_request(&catalog, 101).unwrap(), DAILY_LIMIT - 1, "a new day starts again");
        let saved = std::fs::read_to_string(ai::settings_path(&catalog)).unwrap();
        assert!(!saved.contains("sk-ant"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn nothing_is_sent_without_a_connection() {
        let dir = std::env::temp_dir().join(format!("rd-ai-none-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let catalog = dir.join("catalog.sqlite");
        struct Empty;
        impl KeyStore for Empty {
            fn get(&self) -> Result<Option<String>, String> { Ok(None) }
            fn set(&self, _: &str) -> Result<(), String> { Ok(()) }
            fn delete(&self) -> Result<(), String> { Ok(()) }
        }
        let claude = DesignAsk { provider: "claude".into(), screen: screen(), request: "Hi".into() };
        assert!(ask(&catalog, &Empty, &claude).unwrap_err().contains("Connect Claude"));
        let private = DesignAsk { provider: "private".into(), ..ok_clone(&claude) };
        assert!(ask(&catalog, &Empty, &private).unwrap_err().contains("Connect your private AI"));
        assert!(!ai::settings_path(&catalog).exists(), "no request was counted");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

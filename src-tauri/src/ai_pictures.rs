//! Make a picture with AI (ADR-122): step 3 of the AI plan (ADR-118).
//!
//! Two kinds of picture service, both the user's own:
//! - **OpenAI** image models, with the OpenAI key connected under Other AI services (ADR-121).
//! - **A picture program on this PC or network**: Automatic1111 (`/sdapi/v1/txt2img`) or
//!   ComfyUI (`/prompt`, a plain text-to-picture graph). Local or private-network addresses only.
//!
//! Only the description typed and the size are sent. The picture comes back as bytes that the
//! editor puts through the usual picture checks (converted to PNG, size limits, credit).
use crate::ai::{self, KeyStore};
use base64::prelude::*;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::Path;
use std::time::{Duration, Instant};

pub const MAX_PROMPT_CHARS: usize = 1000;
pub const MAX_PICTURE_BYTES: usize = 20 * 1024 * 1024;
pub const OPENAI_IMAGES: &str = "https://api.openai.com/v1/images/generations";
/// Usual addresses: Automatic1111 / Forge, then ComfyUI.
pub const USUAL_PROGRAMS: &[&str] = &["http://127.0.0.1:7860", "http://127.0.0.1:8188"];
const NEGATIVE: &str = "text, watermark, signature, blurry, low quality";

/// The OpenAI models that make pictures.
pub fn picture_models(all: &[String]) -> Vec<String> {
    all.iter().filter(|m| m.starts_with("gpt-image") || m.starts_with("dall-e")).cloned().collect()
}

/// Picture size for an OpenAI model and a shape (square, wide or tall).
pub fn openai_size(model: &str, shape: &str) -> &'static str {
    match (model.starts_with("dall-e-3"), model.starts_with("dall-e-2"), shape) {
        (_, true, _) => "1024x1024",
        (true, _, "wide") => "1792x1024",
        (true, _, "tall") => "1024x1792",
        (false, false, "wide") => "1536x1024",
        (false, false, "tall") => "1024x1536",
        _ => "1024x1024",
    }
}

/// Picture size for a program on this PC (sizes most models handle well).
pub fn local_size(shape: &str) -> (u32, u32) {
    match shape {
        "wide" => (1344, 768),
        "tall" => (768, 1344),
        _ => (1024, 1024),
    }
}

/// A picture the editor can read: PNG, JPEG or WebP, within the size limit.
pub fn check_picture(bytes: &[u8]) -> Result<(), String> {
    let png = bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
    let jpeg = bytes.starts_with(&[0xff, 0xd8, 0xff]);
    let webp = bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP";
    if bytes.is_empty() || bytes.len() > MAX_PICTURE_BYTES || !(png || jpeg || webp) {
        return Err("The picture that came back couldn't be used. Try again.".into());
    }
    Ok(())
}

fn decode(text: &str) -> Result<Vec<u8>, String> {
    let text = text.split_once("base64,").map(|(_, b)| b).unwrap_or(text);
    let bytes = BASE64_STANDARD.decode(text.trim()).map_err(|_| "The picture that came back couldn't be read.".to_string())?;
    check_picture(&bytes)?;
    Ok(bytes)
}

fn check_prompt(prompt: &str, shape: &str) -> Result<(), String> {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err("Describe the picture first.".into());
    }
    if prompt.chars().count() > MAX_PROMPT_CHARS {
        return Err(format!("Keep the description under {MAX_PROMPT_CHARS} characters."));
    }
    if !["square", "wide", "tall"].contains(&shape) {
        return Err("Choose square, wide or tall.".into());
    }
    Ok(())
}

/// The OpenAI request (only the description, the model and the size).
pub fn openai_body(model: &str, prompt: &str, shape: &str) -> Value {
    let mut body = json!({ "model": model, "prompt": prompt.trim(), "size": openai_size(model, shape), "n": 1 });
    if model.starts_with("dall-e") {
        body["response_format"] = json!("b64_json");
    }
    body
}

/// Automatic1111's text-to-picture request.
pub fn a1111_body(prompt: &str, shape: &str, model: Option<&str>) -> Value {
    let (width, height) = local_size(shape);
    let mut body = json!({ "prompt": prompt.trim(), "negative_prompt": NEGATIVE, "width": width, "height": height, "steps": 25, "cfg_scale": 7, "batch_size": 1, "n_iter": 1 });
    if let Some(model) = model {
        body["override_settings"] = json!({ "sd_model_checkpoint": model });
        body["override_settings_restore_afterwards"] = json!(true);
    }
    body
}

/// ComfyUI's plain text-to-picture graph. The result is a preview (ComfyUI's temporary folder),
/// so nothing is added to the user's ComfyUI output folder.
pub fn comfy_graph(prompt: &str, shape: &str, checkpoint: &str, seed: u64) -> Value {
    let (width, height) = local_size(shape);
    json!({
        "4": { "class_type": "CheckpointLoaderSimple", "inputs": { "ckpt_name": checkpoint } },
        "5": { "class_type": "EmptyLatentImage", "inputs": { "width": width, "height": height, "batch_size": 1 } },
        "6": { "class_type": "CLIPTextEncode", "inputs": { "text": prompt.trim(), "clip": ["4", 1] } },
        "7": { "class_type": "CLIPTextEncode", "inputs": { "text": NEGATIVE, "clip": ["4", 1] } },
        "3": { "class_type": "KSampler", "inputs": { "seed": seed, "steps": 25, "cfg": 7, "sampler_name": "euler", "scheduler": "normal", "denoise": 1, "model": ["4", 0], "positive": ["6", 0], "negative": ["7", 0], "latent_image": ["5", 0] } },
        "8": { "class_type": "VAEDecode", "inputs": { "samples": ["3", 0], "vae": ["4", 2] } },
        "9": { "class_type": "PreviewImage", "inputs": { "images": ["8", 0] } }
    })
}

fn local_agent(seconds: u64) -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(3)).timeout(Duration::from_secs(seconds)).redirects(0).build()
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProgramFound {
    pub url: String,
    /// "a1111" or "comfyui".
    pub kind: String,
    pub models: Vec<String>,
}

/// Which picture program answers at an address, and its models.
pub fn find_program(url: &str) -> Result<ProgramFound, String> {
    let url = ai::private_url(url)?;
    let agent = local_agent(10);
    if let Ok(r) = agent.get(&format!("{url}/sdapi/v1/sd-models")).call() {
        let list: Value = r.into_json().unwrap_or(Value::Null);
        let models: Vec<String> = list.as_array().map(|a| a.iter().filter_map(|m| m.get("title").and_then(Value::as_str)).map(String::from).take(500).collect()).unwrap_or_default();
        return Ok(ProgramFound { url, kind: "a1111".into(), models });
    }
    if let Ok(r) = agent.get(&format!("{url}/object_info/CheckpointLoaderSimple")).call() {
        let info: Value = r.into_json().unwrap_or(Value::Null);
        let models: Vec<String> = info
            .pointer("/CheckpointLoaderSimple/input/required/ckpt_name/0")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(String::from).take(500).collect())
            .unwrap_or_default();
        if models.is_empty() {
            return Err("ComfyUI answered, but has no models yet. Download a checkpoint into ComfyUI first.".into());
        }
        return Ok(ProgramFound { url, kind: "comfyui".into(), models });
    }
    Err(format!("No picture program answered at {url}. Start Automatic1111 (with --api) or ComfyUI first."))
}

fn openai_picture(key: &str, model: &str, prompt: &str, shape: &str) -> Result<Vec<u8>, String> {
    let tls = native_tls::TlsConnector::new().map_err(|e| format!("Secure connections aren't available: {e}"))?;
    let agent = ureq::AgentBuilder::new().tls_connector(std::sync::Arc::new(tls)).timeout_connect(Duration::from_secs(10)).timeout(Duration::from_secs(240)).redirects(0).build();
    let reply: Value = match agent.post(OPENAI_IMAGES).set("Authorization", &format!("Bearer {key}")).send_json(openai_body(model, prompt, shape)).map_err(Box::new).map_err(|e| *e) {
        Ok(r) => r.into_json().map_err(|_| "OpenAI's answer couldn't be read. Try again.".to_string())?,
        Err(ureq::Error::Status(400, r)) => {
            let detail = r.into_json::<Value>().ok().and_then(|b| b.pointer("/error/message").and_then(Value::as_str).map(|m| m.chars().take(200).collect::<String>())).unwrap_or_default();
            return Err(format!("OpenAI couldn't make that picture. {detail}").trim().to_string());
        }
        Err(ureq::Error::Status(code, _)) => return Err(ai::service_message(code, "OpenAI").err().unwrap_or_default()),
        Err(_) => return Err("Couldn't reach OpenAI, or it took too long. Try again.".into()),
    };
    decode(reply.pointer("/data/0/b64_json").and_then(Value::as_str).ok_or("OpenAI didn't send a picture back. Try again.")?)
}

fn a1111_picture(url: &str, prompt: &str, shape: &str, model: Option<&str>) -> Result<Vec<u8>, String> {
    let reply: Value = local_agent(600)
        .post(&format!("{url}/sdapi/v1/txt2img"))
        .send_json(a1111_body(prompt, shape, model))
        .map_err(|_| format!("The picture program at {url} didn't make the picture. Is it still running?"))?
        .into_json()
        .map_err(|_| "The picture program's answer couldn't be read.".to_string())?;
    decode(reply.pointer("/images/0").and_then(Value::as_str).ok_or("The picture program didn't send a picture back.")?)
}

fn comfy_picture(url: &str, prompt: &str, shape: &str, checkpoint: &str) -> Result<Vec<u8>, String> {
    let agent = local_agent(30);
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64 % 1_000_000_000_000).unwrap_or(1);
    let queued: Value = agent
        .post(&format!("{url}/prompt"))
        .send_json(json!({ "prompt": comfy_graph(prompt, shape, checkpoint, seed), "client_id": "retrodrive" }))
        .map_err(|_| format!("ComfyUI at {url} didn't accept the request. Is the model still there?"))?
        .into_json()
        .map_err(|_| "ComfyUI's answer couldn't be read.".to_string())?;
    let id = queued.get("prompt_id").and_then(Value::as_str).ok_or("ComfyUI didn't start the picture.")?.to_string();
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("ComfyUI's answer couldn't be read.".into());
    }
    let started = Instant::now();
    loop {
        if started.elapsed() > Duration::from_secs(600) {
            return Err("ComfyUI took more than 10 minutes. Try a smaller model.".into());
        }
        std::thread::sleep(Duration::from_millis(1000));
        let history: Value = match agent.get(&format!("{url}/history/{id}")).call() {
            Ok(r) => r.into_json().unwrap_or(Value::Null),
            Err(_) => continue,
        };
        let Some(entry) = history.get(&id) else { continue };
        if entry.pointer("/status/status_str").and_then(Value::as_str) == Some("error") {
            return Err("ComfyUI couldn't make the picture. Check its window for the reason.".into());
        }
        let Some(image) = entry.pointer("/outputs/9/images/0") else { continue };
        let field = |k: &str| image.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let mut bytes = Vec::new();
        agent
            .get(&format!("{url}/view"))
            .query("filename", &field("filename"))
            .query("subfolder", &field("subfolder"))
            .query("type", &field("type"))
            .call()
            .map_err(|_| "ComfyUI made the picture but didn't send it.".to_string())?
            .into_reader()
            .take(MAX_PICTURE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "ComfyUI made the picture but didn't send it.".to_string())?;
        check_picture(&bytes)?;
        return Ok(bytes);
    }
}

use std::io::Read;

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PictureStatus {
    pub openai_connected: bool,
    pub openai_model: Option<String>,
    pub program_url: Option<String>,
    pub program_kind: Option<String>,
    pub program_model: Option<String>,
}

pub fn status(catalog: &Path, openai_key: bool) -> PictureStatus {
    let s = ai::load_settings(catalog);
    PictureStatus {
        openai_connected: openai_key && s.service_models.contains_key("openai"),
        openai_model: s.picture_openai_model,
        program_url: s.picture_url,
        program_kind: s.picture_kind,
        program_model: s.picture_model,
    }
}

/// Make a picture: "openai" or "program". Counts against today's limit before sending.
pub fn make(catalog: &Path, openai_key: &dyn KeyStore, provider: &str, prompt: &str, shape: &str) -> Result<String, String> {
    check_prompt(prompt, shape)?;
    let settings = ai::load_settings(catalog);
    let bytes = match provider {
        "openai" => {
            let key = openai_key.get()?.filter(|_| settings.service_models.contains_key("openai")).ok_or("Connect OpenAI under Settings → AI helpers → Other AI services first.")?;
            let model = settings.picture_openai_model.clone().ok_or("Choose an OpenAI picture model in Settings → AI helpers first.")?;
            crate::ai_design::take_request(catalog, crate::ai_design::today())?;
            openai_picture(&key, &model, prompt, shape)?
        }
        "program" => {
            let url = ai::private_url(settings.picture_url.as_deref().ok_or("Connect your picture program in Settings → AI helpers first.")?)?;
            crate::ai_design::take_request(catalog, crate::ai_design::today())?;
            match settings.picture_kind.as_deref() {
                Some("comfyui") => comfy_picture(&url, prompt, shape, settings.picture_model.as_deref().ok_or("Choose a ComfyUI model in Settings → AI helpers first.")?)?,
                _ => a1111_picture(&url, prompt, shape, settings.picture_model.as_deref())?,
            }
        }
        _ => return Err("Choose where to make the picture.".into()),
    };
    Ok(BASE64_STANDARD.encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_picture_models_and_sizes_they_support() {
        let all: Vec<String> = ["gpt-4.1", "gpt-image-1", "dall-e-3", "whisper-1", "dall-e-2"].iter().map(|s| s.to_string()).collect();
        assert_eq!(picture_models(&all), ["gpt-image-1", "dall-e-3", "dall-e-2"]);
        assert_eq!(openai_size("gpt-image-1", "wide"), "1536x1024");
        assert_eq!(openai_size("gpt-image-1", "tall"), "1024x1536");
        assert_eq!(openai_size("dall-e-3", "wide"), "1792x1024");
        assert_eq!(openai_size("dall-e-2", "wide"), "1024x1024");
        assert_eq!(local_size("wide"), (1344, 768));
    }

    #[test]
    fn requests_hold_only_the_description_and_the_size() {
        let body = openai_body("gpt-image-1", "  a neon arcade at night ", "wide");
        assert_eq!(body, json!({ "model": "gpt-image-1", "prompt": "a neon arcade at night", "size": "1536x1024", "n": 1 }));
        assert_eq!(openai_body("dall-e-3", "x", "square")["response_format"], "b64_json");
        let a = a1111_body("stars", "tall", Some("sdxl.safetensors"));
        assert_eq!((a["width"].as_u64(), a["height"].as_u64()), (Some(768), Some(1344)));
        assert_eq!(a["override_settings"]["sd_model_checkpoint"], "sdxl.safetensors");
        assert!(a1111_body("stars", "tall", None).get("override_settings").is_none());
        let g = comfy_graph("stars", "square", "model.safetensors", 7);
        assert_eq!(g["9"]["class_type"], "PreviewImage", "nothing is saved in ComfyUI's output folder");
        assert_eq!(g["6"]["inputs"]["text"], "stars");
        assert_eq!(g["4"]["inputs"]["ckpt_name"], "model.safetensors");
        assert!(check_prompt("", "wide").is_err() && check_prompt(&"x".repeat(1001), "wide").is_err() && check_prompt("ok", "round").is_err() && check_prompt("ok", "tall").is_ok());
    }

    #[test]
    fn only_real_pictures_come_back() {
        let png = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0];
        assert!(check_picture(&png).is_ok());
        assert!(check_picture(&[0xff, 0xd8, 0xff, 0xe0]).is_ok());
        assert!(check_picture(b"RIFF\0\0\0\0WEBPVP8 ").is_ok());
        assert!(check_picture(b"<html>not a picture</html>").is_err());
        assert!(check_picture(&[]).is_err());
        assert_eq!(decode(&BASE64_STANDARD.encode(png)).unwrap(), png);
        assert_eq!(decode(&format!("data:image/png;base64,{}", BASE64_STANDARD.encode(png))).unwrap(), png);
        assert!(decode("%%%not base64").is_err());
        assert!(decode(&BASE64_STANDARD.encode(b"MZ executable")).is_err());
    }

    #[test]
    fn nothing_is_sent_without_a_connection() {
        let dir = std::env::temp_dir().join(format!("rd-ai-pictures-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let catalog = dir.join("catalog.sqlite");
        struct Empty;
        impl KeyStore for Empty {
            fn get(&self) -> Result<Option<String>, String> { Ok(None) }
            fn set(&self, _: &str) -> Result<(), String> { Ok(()) }
            fn delete(&self) -> Result<(), String> { Ok(()) }
        }
        assert!(make(&catalog, &Empty, "openai", "stars", "wide").unwrap_err().contains("Connect OpenAI"));
        assert!(make(&catalog, &Empty, "program", "stars", "wide").unwrap_err().contains("Connect your picture program"));
        assert!(make(&catalog, &Empty, "elsewhere", "stars", "wide").is_err());
        assert!(!ai::settings_path(&catalog).exists(), "no request was counted");
        assert!(!status(&catalog, false).openai_connected);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

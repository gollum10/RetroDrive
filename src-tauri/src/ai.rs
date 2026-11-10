//! Optional AI helpers for Theme Studio (ADR-118, docs/18 §70): connect and disconnect.
//!
//! - **Claude:** the user's own Claude Console API key, checked with one request that uses no
//!   tokens (`GET /v1/models`) and kept in Windows Credential Manager. Never written to a file,
//!   the catalog, a theme or a log.
//! - **Private AI:** an OpenAI-compatible server on this PC or the local network (Ollama,
//!   LM Studio, …), by address. Only local and private-network addresses are accepted.
//!
//! Nothing is sent anywhere until the user connects; these calls only check a connection.
//! Design requests come in a later step.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const CLAUDE_API: &str = "https://api.anthropic.com";
pub const ANTHROPIC_VERSION: &str = "2026-10-21";
/// Where the user creates a key (sign-in first if needed).
pub const CLAUDE_KEYS_PAGE: &str = "https://platform.claude.com/settings/keys";
pub const DEFAULT_CLAUDE_MODEL: &str = "claude-opus-5-5";
pub const CLAUDE_MODELS: &[(&str, &str)] = &[
    ("claude-opus-5-5", "Claude Opus 5.5 (best results)"),
    ("claude-sonnet-5-5", "Claude Sonnet 5.5 (faster, cheaper)"),
    ("claude-haiku-5-5", "Claude Haiku 5.5 (cheapest)"),
];
const CREDENTIAL_TARGET: &str = "RetroDrive/ThemeStudio/Claude";

// ---------------------------------------------------------------- key store

/// Where the Claude key is kept. Windows Credential Manager in the app; memory in tests.
pub trait KeyStore {
    fn get(&self) -> Result<Option<String>, String>;
    fn set(&self, key: &str) -> Result<(), String>;
    fn delete(&self) -> Result<(), String>;
}

/// Windows Credential Manager (a generic credential for this user, kept on this PC).
pub struct CredentialManager;

#[cfg(windows)]
mod credman {
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_NOT_FOUND, FILETIME};
    use windows_sys::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn read(target: &str) -> Result<Option<String>, String> {
        let name = wide(target);
        let mut found: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: valid NUL-terminated name and out pointer; the result is freed with CredFree.
        let ok = unsafe { CredReadW(name.as_ptr(), CRED_TYPE_GENERIC, 0, &mut found) };
        if ok == 0 {
            // SAFETY: reads this thread's last error.
            let error = unsafe { GetLastError() };
            return if error == ERROR_NOT_FOUND { Ok(None) } else { Err(format!("Windows Credential Manager couldn't be read (error {error}).")) };
        }
        // SAFETY: CredReadW succeeded, so `found` points to a credential with a blob of the stated size.
        let secret = unsafe {
            let credential = &*found;
            let bytes = std::slice::from_raw_parts(credential.CredentialBlob, credential.CredentialBlobSize as usize).to_vec();
            CredFree(found as *const _);
            bytes
        };
        String::from_utf8(secret).map(Some).map_err(|_| "The stored key isn't readable; connect again.".to_string())
    }

    pub fn write(target: &str, secret: &str) -> Result<(), String> {
        let mut name = wide(target);
        let mut user = wide("RetroDrive");
        let mut blob = secret.as_bytes().to_vec();
        let credential = CREDENTIALW {
            Flags: 0,
            Type: CRED_TYPE_GENERIC,
            TargetName: name.as_mut_ptr(),
            Comment: std::ptr::null_mut(),
            LastWritten: FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 },
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            AttributeCount: 0,
            Attributes: std::ptr::null_mut(),
            TargetAlias: std::ptr::null_mut(),
            UserName: user.as_mut_ptr(),
        };
        // SAFETY: every pointer refers to a live buffer for the duration of the call.
        let ok = unsafe { CredWriteW(&credential, 0) };
        blob.iter_mut().for_each(|b| *b = 0);
        if ok == 0 {
            // SAFETY: reads this thread's last error.
            return Err(format!("Windows Credential Manager couldn't save the key (error {}).", unsafe { GetLastError() }));
        }
        Ok(())
    }

    pub fn delete(target: &str) -> Result<(), String> {
        let name = wide(target);
        // SAFETY: valid NUL-terminated name.
        let ok = unsafe { CredDeleteW(name.as_ptr(), CRED_TYPE_GENERIC, 0) };
        // SAFETY: reads this thread's last error.
        if ok == 0 && unsafe { GetLastError() } != ERROR_NOT_FOUND {
            return Err("Windows Credential Manager couldn't remove the key.".into());
        }
        Ok(())
    }
}

impl KeyStore for CredentialManager {
    #[cfg(windows)]
    fn get(&self) -> Result<Option<String>, String> { credman::read(CREDENTIAL_TARGET) }
    #[cfg(windows)]
    fn set(&self, key: &str) -> Result<(), String> { credman::write(CREDENTIAL_TARGET, key) }
    #[cfg(windows)]
    fn delete(&self) -> Result<(), String> { credman::delete(CREDENTIAL_TARGET) }
    #[cfg(not(windows))]
    fn get(&self) -> Result<Option<String>, String> { Ok(None) }
    #[cfg(not(windows))]
    fn set(&self, _: &str) -> Result<(), String> { Err("Keys can only be stored on Windows.".into()) }
    #[cfg(not(windows))]
    fn delete(&self) -> Result<(), String> { Ok(()) }
}

// ---------------------------------------------------------------- other AI services (ADR-121)

/// A key for another AI service, kept in Credential Manager under its own name.
pub struct ServiceKey(pub String);

impl ServiceKey {
    fn target(&self) -> String {
        format!("RetroDrive/ThemeStudio/Service/{}", self.0)
    }
}

impl KeyStore for ServiceKey {
    #[cfg(windows)]
    fn get(&self) -> Result<Option<String>, String> { credman::read(&self.target()) }
    #[cfg(windows)]
    fn set(&self, key: &str) -> Result<(), String> { credman::write(&self.target(), key) }
    #[cfg(windows)]
    fn delete(&self) -> Result<(), String> { credman::delete(&self.target()) }
    #[cfg(not(windows))]
    fn get(&self) -> Result<Option<String>, String> { Ok(None) }
    #[cfg(not(windows))]
    fn set(&self, _: &str) -> Result<(), String> { Err("Keys can only be stored on Windows.".into()) }
    #[cfg(not(windows))]
    fn delete(&self) -> Result<(), String> { Ok(()) }
}

/// An online AI service that speaks the OpenAI-style chat format. Addresses are fixed here,
/// so a key only ever goes to the service it belongs to.
pub struct Service {
    pub id: &'static str,
    pub name: &'static str,
    /// The API address; `/models` and `/chat/completions` follow it.
    pub base: &'static str,
    /// Where the user signs in and creates a key.
    pub keys_page: &'static str,
    /// A request that needs a valid key (most services: the model list).
    pub check: &'static str,
}

pub const SERVICES: &[Service] = &[
    Service { id: "openai", name: "OpenAI (ChatGPT)", base: "https://api.openai.com/v1", keys_page: "https://platform.openai.com/api-keys", check: "/models" },
    Service { id: "gemini", name: "Google Gemini", base: "https://generativelanguage.googleapis.com/v1beta/openai", keys_page: "https://aistudio.google.com/apikey", check: "/models" },
    Service { id: "openrouter", name: "OpenRouter (many models)", base: "https://openrouter.ai/api/v1", keys_page: "https://openrouter.ai/settings/keys", check: "/key" },
    Service { id: "mistral", name: "Mistral", base: "https://api.mistral.ai/v1", keys_page: "https://console.mistral.ai/api-keys", check: "/models" },
    Service { id: "groq", name: "Groq", base: "https://api.groq.com/openai/v1", keys_page: "https://console.groq.com/keys", check: "/models" },
    Service { id: "deepseek", name: "DeepSeek", base: "https://api.deepseek.com", keys_page: "https://platform.deepseek.com/api_keys", check: "/models" },
    Service { id: "xai", name: "xAI (Grok)", base: "https://api.x.ai/v1", keys_page: "https://console.x.ai/", check: "/models" },
    Service { id: "together", name: "Together AI", base: "https://api.together.xyz/v1", keys_page: "https://api.together.ai/settings/api-keys", check: "/models" },
    Service { id: "fireworks", name: "Fireworks AI", base: "https://api.fireworks.ai/inference/v1", keys_page: "https://app.fireworks.ai/settings/users/api-keys", check: "/models" },
];
/// A service at an address the user types (Pro only in the app).
pub const CUSTOM: &str = "custom";

pub fn service(id: &str) -> Option<&'static Service> {
    SERVICES.iter().find(|s| s.id == id)
}

/// A custom online service address: https only, a real internet host (local ones belong under
/// Private AI), no user name, query or fragment. Returns it without a trailing slash.
pub fn custom_url(text: &str) -> Result<String, String> {
    let text = text.trim().trim_end_matches('/');
    let rest = text.strip_prefix("https://").ok_or("Use an address that starts with https://.")?;
    if text.len() > 200 || rest.contains('@') || rest.contains('?') || rest.contains('#') || rest.contains('\\') || rest.contains("..") {
        return Err("That address isn't allowed.".into());
    }
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    if !path.chars().all(|c| c.is_ascii_alphanumeric() || "/._-".contains(c)) {
        return Err("That address isn't allowed.".into());
    }
    let host = authority.rsplit_once(':').map(|(h, port)| if port.chars().all(|c| c.is_ascii_digit()) { h } else { authority }).unwrap_or(authority).to_ascii_lowercase();
    if host.is_empty() || !host.contains('.') || !host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
        return Err("Use the service's internet address, for example https://api.example.com/v1.".into());
    }
    if local_host(&host) {
        return Err("That's on your own PC or network: connect it under Private AI instead.".into());
    }
    Ok(text.to_string())
}

/// The address and name for a service, known or custom.
pub fn service_address(id: &str, settings: &AiSettings) -> Result<(String, String, &'static str), String> {
    if id == CUSTOM {
        let url = settings.custom_url.as_deref().ok_or("Add the custom address first.")?;
        return Ok((custom_url(url)?, "Your custom service".into(), "/models"));
    }
    let s = service(id).ok_or("Unknown AI service.")?;
    Ok((s.base.to_string(), s.name.to_string(), s.check))
}

/// What a service's HTTP status means, in plain words. Some services answer a bad key with 400.
pub fn service_message(status: u16, name: &str) -> Result<(), String> {
    match status {
        200..=299 => Ok(()),
        400 | 401 | 403 => Err(format!("{name} didn't accept that key. Copy it again from {name}.")),
        402 => Err(format!("{name} says the account needs credit or a payment method.")),
        404 => Err(format!("{name} couldn't find that. Check the address.")),
        429 => Err(format!("{name} is busy or the account has reached a limit. Try again in a minute.")),
        500..=599 => Err(format!("{name} isn't responding right now. Try again later.")),
        other => Err(format!("{name} answered with an unexpected status ({other}).")),
    }
}

/// A plausible key: printable, no spaces, sensible length (formats differ by service).
pub fn service_key_looks_valid(key: &str) -> bool {
    (16..=400).contains(&key.len()) && key.bytes().all(|b| b.is_ascii_graphic())
}

/// Read model ids from an OpenAI-style `/models` answer.
pub fn model_ids(answer: &serde_json::Value) -> Vec<String> {
    let mut ids: Vec<String> = answer
        .get("data")
        .and_then(|d| d.as_array())
        .map(|items| items.iter().filter_map(|m| m.get("id").and_then(|i| i.as_str())).filter(|id| !id.is_empty() && id.len() <= 200).map(String::from).collect())
        .unwrap_or_default();
    ids.sort();
    ids.dedup();
    ids.truncate(1000);
    ids
}

/// Check a key with the service and list the models it can use.
pub fn service_models(base: &str, check: &str, name: &str, key: &str) -> Result<Vec<String>, String> {
    if !service_key_looks_valid(key) {
        return Err("That doesn't look like a key. Copy it again, without spaces.".into());
    }
    let agent = agent()?;
    let call = |path: &str| agent.get(&format!("{base}{path}")).set("Authorization", &format!("Bearer {key}")).call().map_err(Box::new);
    let unreachable = || format!("Couldn't reach {name}. Check the internet connection.");
    if check != "/models" {
        match call(check).map_err(|e| *e) {
            Ok(_) => {}
            Err(ureq::Error::Status(code, _)) => service_message(code, name)?,
            Err(_) => return Err(unreachable()),
        }
    }
    let answer: serde_json::Value = match call("/models").map_err(|e| *e) {
        Ok(r) => r.into_json().map_err(|_| format!("{name} answered, but not like an AI service."))?,
        Err(ureq::Error::Status(code, _)) => return Err(service_message(code, name).err().unwrap_or_default()),
        Err(_) => return Err(unreachable()),
    };
    let ids = model_ids(&answer);
    if ids.is_empty() {
        return Err(format!("{name} has no models for that key."));
    }
    Ok(ids)
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceStatus {
    pub id: String,
    pub name: String,
    pub keys_page: Option<String>,
    pub connected: bool,
    pub model: Option<String>,
    pub url: Option<String>,
}

/// Every offered service and whether it's connected (a stored key and a chosen model).
pub fn services_status(settings: &AiSettings, key_for: &dyn Fn(&str) -> bool) -> Vec<ServiceStatus> {
    let entry = |id: &str, name: &str, keys_page: Option<&str>, url: Option<String>| {
        let model = settings.service_models.get(id).cloned();
        ServiceStatus { id: id.into(), name: name.into(), keys_page: keys_page.map(String::from), connected: model.is_some() && key_for(id), model, url }
    };
    let mut list: Vec<ServiceStatus> = SERVICES.iter().map(|s| entry(s.id, s.name, Some(s.keys_page), None)).collect();
    list.push(entry(CUSTOM, "Custom address", None, settings.custom_url.clone()));
    list
}

// ---------------------------------------------------------------- settings file (no secrets)

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct AiSettings {
    pub claude_model: Option<String>,
    pub private_url: Option<String>,
    pub private_model: Option<String>,
    /// Design requests today (ADR-120): the day (days since 1970, UTC) and how many so far.
    pub requests_day: Option<u64>,
    pub requests_today: u32,
    /// Other AI services (ADR-121): the chosen model for each connected service, and a custom address.
    pub service_models: std::collections::BTreeMap<String, String>,
    pub custom_url: Option<String>,
}

pub fn settings_path(catalog: &Path) -> PathBuf {
    catalog.parent().unwrap_or_else(|| Path::new(".")).join("ai-settings.json")
}

pub fn load_settings(catalog: &Path) -> AiSettings {
    std::fs::read(settings_path(catalog)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// Save RetroDrive's own settings file (written whole, then renamed into place).
pub fn save_settings(catalog: &Path, settings: &AiSettings) -> Result<(), String> {
    let path = settings_path(catalog);
    let temp = path.with_extension("json.part");
    std::fs::write(&temp, serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?).map_err(|e| format!("Couldn't save the AI settings: {e}"))?;
    std::fs::rename(&temp, &path).map_err(|e| format!("Couldn't save the AI settings: {e}"))
}

// ---------------------------------------------------------------- checks

/// A Claude Console API key looks like `sk-ant-…` (no spaces, sensible length).
pub fn key_looks_valid(key: &str) -> bool {
    key.starts_with("sk-ant-") && (20..=300).contains(&key.len()) && key.bytes().all(|b| b.is_ascii_graphic())
}

/// A private AI address: http(s) to this PC or the local network only, no user info, no path tricks.
/// Returns the address without a trailing slash.
pub fn private_url(text: &str) -> Result<String, String> {
    let text = text.trim().trim_end_matches('/');
    let rest = text
        .strip_prefix("http://")
        .or_else(|| text.strip_prefix("https://"))
        .ok_or("Use an address that starts with http:// (for example http://localhost:11434).")?;
    if rest.contains('@') || rest.contains('?') || rest.contains('#') || rest.contains('\\') {
        return Err("That address isn't allowed.".into());
    }
    let authority = rest.split('/').next().unwrap_or("");
    if rest.len() > authority.len() && rest[authority.len()..] != *"/v1" {
        return Err("Use just the server's address, for example http://localhost:11434.".into());
    }
    let host = if let Some(inner) = authority.strip_prefix('[') {
        inner.split(']').next().unwrap_or("").to_string()
    } else {
        authority.rsplit_once(':').map(|(h, port)| if port.chars().all(|c| c.is_ascii_digit()) { h } else { authority }).unwrap_or(authority).to_ascii_lowercase()
    };
    if host.is_empty() || !local_host(&host) {
        return Err("A private AI must be on this PC or your local network (for example localhost, 192.168.x.x or a computer name).".into());
    }
    Ok(text.trim_end_matches("/v1").to_string())
}

fn local_host(host: &str) -> bool {
    if host == "localhost" || host == "::1" || host.ends_with(".local") || host.ends_with(".lan") || host.ends_with(".home.arpa") {
        return true;
    }
    if let Ok(ip) = host.parse::<std::net::Ipv4Addr>() {
        return ip.is_loopback() || ip.is_private() || ip.is_link_local();
    }
    if let Ok(ip) = host.parse::<std::net::Ipv6Addr>() {
        return ip.is_loopback() || (ip.segments()[0] & 0xfe00) == 0xfc00;
    }
    // A plain computer name on the local network (no dots).
    !host.contains('.') && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// What a Claude key check's HTTP status means, in plain words.
pub fn claude_check_message(status: u16) -> Result<(), String> {
    match status {
        200..=299 => Ok(()),
        401 => Err("Claude didn't accept that key. Copy it again from the Claude Console.".into()),
        403 => Err("That key isn't allowed to use Claude. Check the key's workspace in the Claude Console.".into()),
        429 => Err("Claude is busy or the account has reached a limit. Try again in a minute.".into()),
        500..=599 => Err("Claude isn't responding right now. Try again later.".into()),
        other => Err(format!("Claude answered with an unexpected status ({other}).")),
    }
}

fn agent() -> Result<ureq::Agent, String> {
    let tls = native_tls::TlsConnector::new().map_err(|e| format!("Secure connections aren't available: {e}"))?;
    Ok(ureq::AgentBuilder::new()
        .tls_connector(std::sync::Arc::new(tls))
        .timeout_connect(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .redirects(0)
        .build())
}

/// Check a Claude key with one request that uses no tokens.
pub fn check_claude_key(key: &str) -> Result<(), String> {
    if !key_looks_valid(key) {
        return Err("That doesn't look like a Claude key. It starts with sk-ant-.".into());
    }
    let response = agent()?
        .get(&format!("{CLAUDE_API}/v1/models?limit=1"))
        .set("x-api-key", key)
        .set("anthropic-version", ANTHROPIC_VERSION)
        .call();
    match response {
        Ok(r) => claude_check_message(r.status()),
        Err(ureq::Error::Status(code, _)) => claude_check_message(code),
        Err(_) => Err("Couldn't reach Claude. Check the internet connection.".into()),
    }
}

/// The models a private OpenAI-compatible server offers (`GET <url>/v1/models`).
pub fn private_models(url: &str) -> Result<Vec<String>, String> {
    let url = private_url(url)?;
    #[derive(Deserialize)]
    struct Model { id: String }
    #[derive(Deserialize)]
    struct Models { data: Vec<Model> }
    let response = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(3)).timeout(Duration::from_secs(10)).redirects(0).build()
        .get(&format!("{url}/v1/models"))
        .call()
        .map_err(|_| format!("Nothing answered at {url}. Is the AI program running?"))?;
    let models: Models = response.into_json().map_err(|_| format!("{url} answered, but not like an AI server."))?;
    let mut ids: Vec<String> = models.data.into_iter().map(|m| m.id).filter(|id| !id.is_empty() && id.len() <= 200).take(200).collect();
    ids.sort();
    if ids.is_empty() {
        return Err(format!("{url} has no models yet. Download one in the AI program first."));
    }
    Ok(ids)
}

/// Usual addresses of private AI programs on this PC (Ollama, LM Studio).
pub const USUAL_PRIVATE: &[&str] = &["http://localhost:11434", "http://localhost:1234"];

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AiStatus {
    pub claude_connected: bool,
    pub claude_model: String,
    pub claude_models: Vec<(String, String)>,
    pub private_url: Option<String>,
    pub private_model: Option<String>,
    pub services: Vec<ServiceStatus>,
}

pub fn status(catalog: &Path, keys: &dyn KeyStore) -> Result<AiStatus, String> {
    let settings = load_settings(catalog);
    let services = services_status(&settings, &|id| ServiceKey(id.to_string()).get().ok().flatten().is_some());
    Ok(AiStatus {
        claude_connected: keys.get()?.is_some(),
        claude_model: settings.claude_model.filter(|m| CLAUDE_MODELS.iter().any(|(id, _)| id == m)).unwrap_or_else(|| DEFAULT_CLAUDE_MODEL.into()),
        claude_models: CLAUDE_MODELS.iter().map(|(id, label)| (id.to_string(), label.to_string())).collect(),
        private_url: settings.private_url,
        services,
        private_model: settings.private_model,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct Memory(RefCell<Option<String>>);
    impl KeyStore for Memory {
        fn get(&self) -> Result<Option<String>, String> { Ok(self.0.borrow().clone()) }
        fn set(&self, key: &str) -> Result<(), String> { *self.0.borrow_mut() = Some(key.into()); Ok(()) }
        fn delete(&self) -> Result<(), String> { *self.0.borrow_mut() = None; Ok(()) }
    }

    #[test]
    fn only_local_and_private_network_addresses_are_accepted_for_private_ai() {
        for good in ["http://localhost:11434", "http://127.0.0.1:1234/", "http://192.168.1.20:11434", "http://10.0.0.5:8080/v1", "http://my-pc:11434", "http://gamebox.local:1234", "https://[::1]:8443", "http://172.20.1.1"] {
            assert!(private_url(good).is_ok(), "{good}");
        }
        assert_eq!(private_url("http://localhost:11434/v1/").unwrap(), "http://localhost:11434");
        for bad in ["https://api.openai.com", "http://8.8.8.8:11434", "ftp://localhost", "localhost:11434", "http://user@localhost:11434", "http://localhost:11434/api/../x", "http://example.com", "http://172.32.0.1", "http://localhost:11434?x=1", ""] {
            assert!(private_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn keys_are_checked_for_shape_and_status_codes_say_what_happened() {
        assert!(key_looks_valid("sk-ant-api03-abcdefghijklmnopqrstuvwxyz"));
        for bad in ["", "sk-abc", "sk-ant-short", "sk-ant-api03-has space inside", "not-a-key-at-all-but-long-enough"] {
            assert!(!key_looks_valid(bad), "{bad}");
        }
        assert!(claude_check_message(200).is_ok());
        assert!(claude_check_message(401).unwrap_err().contains("didn't accept"));
        assert!(claude_check_message(429).unwrap_err().contains("limit"));
        assert!(claude_check_message(503).unwrap_err().contains("isn't responding"));
        assert!(check_claude_key("not a key").unwrap_err().contains("sk-ant-"), "a malformed key is refused before any request");
    }

    #[test]
    fn settings_hold_no_secrets_and_status_reflects_the_key_store() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = dir.path().join("retrodrive.sqlite");
        let keys = Memory::default();
        let fresh = status(&catalog, &keys).unwrap();
        assert!(!fresh.claude_connected && fresh.claude_model == DEFAULT_CLAUDE_MODEL && fresh.private_url.is_none());
        keys.set("sk-ant-api03-abcdefghijklmnopqrstuvwxyz").unwrap();
        save_settings(&catalog, &AiSettings { claude_model: Some("claude-haiku-5-5".into()), private_url: Some("http://localhost:11434".into()), private_model: Some("llama3".into()), ..Default::default() }).unwrap();
        let now = status(&catalog, &keys).unwrap();
        assert!(now.claude_connected && now.claude_model == "claude-haiku-5-5" && now.private_model.as_deref() == Some("llama3"));
        let text = std::fs::read_to_string(settings_path(&catalog)).unwrap();
        assert!(!text.contains("sk-ant"), "the key never goes in the settings file");
        // An unknown model in the file falls back to the default.
        save_settings(&catalog, &AiSettings { claude_model: Some("gpt-whatever".into()), ..Default::default() }).unwrap();
        assert_eq!(status(&catalog, &keys).unwrap().claude_model, DEFAULT_CLAUDE_MODEL);
        keys.delete().unwrap();
        assert!(!status(&catalog, &keys).unwrap().claude_connected);
    }

    /// Writes to and removes from the real Windows Credential Manager under a test name.
    /// `cargo test real_credential_manager -- --ignored`
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn real_credential_manager_round_trip() {
        let target = "RetroDrive/ThemeStudio/Test";
        credman::write(target, "sk-ant-test-value-1234567890").unwrap();
        assert_eq!(credman::read(target).unwrap().as_deref(), Some("sk-ant-test-value-1234567890"));
        credman::delete(target).unwrap();
        assert_eq!(credman::read(target).unwrap(), None);
    }

    #[test]
    fn other_services_use_fixed_addresses_and_custom_ones_must_be_internet_https() {
        assert!(SERVICES.iter().all(|s| s.base.starts_with("https://") && s.keys_page.starts_with("https://") && !s.base.ends_with('/')));
        let ids: std::collections::BTreeSet<_> = SERVICES.iter().map(|s| s.id).collect();
        assert_eq!(ids.len(), SERVICES.len());
        assert!(!ids.contains(CUSTOM));
        assert_eq!(custom_url(" https://api.example.com/v1/ ").unwrap(), "https://api.example.com/v1");
        for bad in ["http://api.example.com/v1", "https://localhost/v1", "https://192.168.1.5/v1", "https://box.local/v1", "https://user@api.example.com", "https://api.example.com/v1?x=1", "https://api.example.com/../x", "https://nodots/v1", "https://api.example.com/v 1"] {
            assert!(custom_url(bad).is_err(), "{bad}");
        }
        let settings = AiSettings { custom_url: Some("https://api.example.com/v1".into()), ..Default::default() };
        assert_eq!(service_address("custom", &settings).unwrap().0, "https://api.example.com/v1");
        assert_eq!(service_address("openai", &settings).unwrap().0, "https://api.openai.com/v1");
        assert!(service_address("evil", &settings).is_err());
        assert!(service_address("custom", &AiSettings::default()).is_err());
    }

    #[test]
    fn service_answers_are_explained_and_models_listed() {
        assert!(service_message(200, "X").is_ok());
        for bad_key in [400, 401, 403] {
            assert!(service_message(bad_key, "Google Gemini").unwrap_err().contains("didn't accept that key"));
        }
        assert!(service_message(402, "X").unwrap_err().contains("credit"));
        assert!(!service_key_looks_valid("short") && !service_key_looks_valid("has space in the key 123") && service_key_looks_valid("sk-proj-abcdefghijklmnop"));
        let ids = model_ids(&serde_json::json!({ "data": [{ "id": "b" }, { "id": "a" }, { "id": "a" }, { "id": "" }, { "nope": 1 }] }));
        assert_eq!(ids, ["a", "b"]);
        assert!(model_ids(&serde_json::json!({ "error": "x" })).is_empty());
        let settings = AiSettings { service_models: [("groq".to_string(), "llama".to_string()), ("openai".to_string(), "gpt".to_string())].into_iter().collect(), ..Default::default() };
        let list = services_status(&settings, &|id| id == "groq");
        assert!(list.iter().find(|s| s.id == "groq").unwrap().connected);
        assert!(!list.iter().find(|s| s.id == "openai").unwrap().connected, "a model without a stored key isn't connected");
        assert_eq!(list.last().unwrap().id, CUSTOM);
    }
}


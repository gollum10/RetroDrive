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
}

pub fn status(catalog: &Path, keys: &dyn KeyStore) -> Result<AiStatus, String> {
    let settings = load_settings(catalog);
    Ok(AiStatus {
        claude_connected: keys.get()?.is_some(),
        claude_model: settings.claude_model.filter(|m| CLAUDE_MODELS.iter().any(|(id, _)| id == m)).unwrap_or_else(|| DEFAULT_CLAUDE_MODEL.into()),
        claude_models: CLAUDE_MODELS.iter().map(|(id, label)| (id.to_string(), label.to_string())).collect(),
        private_url: settings.private_url,
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
}

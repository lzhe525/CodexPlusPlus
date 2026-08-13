use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const API_PREFIX: &str = "launcher/v1";
const DEFAULT_LAUNCHER_URL: &str = "https://api.ai.rydf-design.com";
const PROVIDER_ID: &str = "company-ai";
const BEGIN_MARKER: &str = "# >>> codex-plus-plus enterprise managed block >>>";
const END_MARKER: &str = "# <<< codex-plus-plus enterprise managed block <<<";
const PRESERVED_PREFIX: &str = "# codex-plus-plus enterprise preserved-root: ";
const SESSION_TARGET: &str = "CodexPlusPlus/enterprise/session";
const REFRESH_TARGET: &str = "CodexPlusPlus/enterprise/refresh";
const CREDENTIAL_TARGET: &str = "CodexPlusPlus/enterprise/inference";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EnterpriseUser {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub balance_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EnterpriseKeyInfo {
    pub alias: String,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub rpm: u64,
    #[serde(default)]
    pub tpm: u64,
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub models: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EnterpriseUsage {
    #[serde(default)]
    pub today_usd: f64,
    #[serde(default)]
    pub month_usd: f64,
    #[serde(default)]
    pub budget_usd: Option<f64>,
    #[serde(default)]
    pub remaining_usd: Option<f64>,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EnterpriseStatus {
    pub user: EnterpriseUser,
    pub usage: EnterpriseUsage,
    pub key: EnterpriseKeyInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EnterpriseProfile {
    pub gateway_url: String,
    pub provider_id: String,
    pub default_model: String,
    pub allowed_models: Vec<String>,
    pub wire_api: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionResponse {
    #[serde(alias = "access_token")]
    access_token: String,
    #[serde(alias = "refresh_token")]
    refresh_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EnsureKeyResponse {
    virtual_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnterpriseSnapshot {
    pub enabled: bool,
    pub state: String,
    pub user: Option<EnterpriseUser>,
    pub status: Option<EnterpriseStatus>,
    pub profile: Option<EnterpriseProfile>,
    pub credential_available: bool,
    pub config_managed: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnterpriseDiagnostics {
    pub server: bool,
    pub login: bool,
    pub credential: bool,
    pub model: bool,
    pub responses_api: bool,
    pub codex_configuration: bool,
    pub message: String,
}

pub fn enterprise_mode_enabled() -> bool {
    !matches!(
        std::env::var("CODEX_PLUS_ENTERPRISE_MODE")
            .unwrap_or_else(|_| "true".to_string())
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "0" | "false" | "no" | "off"
    )
}

pub fn enterprise_credential() -> anyhow::Result<Option<String>> {
    if !enterprise_mode_enabled() {
        return Ok(None);
    }
    secure_store::read(CREDENTIAL_TARGET)
}

pub async fn restore() -> anyhow::Result<EnterpriseSnapshot> {
    if !enterprise_mode_enabled() {
        return Ok(snapshot("disabled", "Community Mode is active."));
    }
    let Some(access_token) = secure_store::read(SESSION_TARGET)? else {
        return Ok(snapshot(
            "unauthenticated",
            "Sign in with your company account.",
        ));
    };
    match load_snapshot(&access_token).await {
        Ok(snapshot) => Ok(snapshot),
        Err(error) if is_unauthorized(&error) => {
            let Some(refresh_token) = secure_store::read(REFRESH_TARGET)? else {
                clear_credentials()?;
                return Ok(snapshot(
                    "expired",
                    "Your session expired. Please sign in again.",
                ));
            };
            let session: SessionResponse = request_json(
                reqwest::Method::POST,
                "refresh",
                None,
                Some(json!({ "refreshToken": refresh_token })),
            )
            .await?;
            save_session(&session)?;
            load_snapshot(&session.access_token).await
        }
        Err(error) => Err(error),
    }
}

pub async fn login(
    email: &str,
    password: &str,
    executable: &Path,
) -> anyhow::Result<EnterpriseSnapshot> {
    if !enterprise_mode_enabled() {
        anyhow::bail!("Enterprise Mode is not enabled");
    }
    if email.trim().is_empty() || password.is_empty() {
        anyhow::bail!("Email and password are required");
    }
    let session: SessionResponse = request_json(
        reqwest::Method::POST,
        "login",
        None,
        Some(json!({
            "username": email.trim(),
            "password": password,
            "clientVersion": env!("CARGO_PKG_VERSION")
        })),
    )
    .await?;
    save_session(&session)?;
    if let Err(error) = provision(&session.access_token, executable).await {
        clear_credentials()?;
        return Err(error);
    }
    load_snapshot(&session.access_token).await
}

pub async fn refresh(executable: &Path) -> anyhow::Result<EnterpriseSnapshot> {
    let access_token = secure_store::read(SESSION_TARGET)?
        .ok_or_else(|| anyhow::anyhow!("Authentication required"))?;
    provision(&access_token, executable).await?;
    load_snapshot(&access_token).await
}

pub async fn logout() -> anyhow::Result<EnterpriseSnapshot> {
    if let Some(access_token) = secure_store::read(SESSION_TARGET)? {
        let _ = request_value(reqwest::Method::POST, "logout", Some(&access_token), None).await;
    }
    clear_credentials()?;
    remove_managed_config(&crate::codex_home::default_codex_home_dir().join("config.toml"))?;
    Ok(snapshot(
        "unauthenticated",
        "Signed out and removed this device credential.",
    ))
}

pub async fn diagnostics() -> EnterpriseDiagnostics {
    let server = request_value(reqwest::Method::GET, "health", None, None)
        .await
        .is_ok();
    let token = secure_store::read(SESSION_TARGET).ok().flatten();
    let login = token.is_some();
    let status = match token.as_deref() {
        Some(token) => load_status(token).await.ok(),
        None => None,
    };
    let credential = secure_store::read(CREDENTIAL_TARGET)
        .ok()
        .flatten()
        .is_some();
    let profile = match token.as_deref() {
        Some(token) => request_json::<EnterpriseProfile>(
            reqwest::Method::GET,
            "codex-profile",
            Some(token),
            None,
        )
        .await
        .ok(),
        None => None,
    };
    let model = profile.as_ref().is_some_and(|profile| {
        status
            .as_ref()
            .is_some_and(|status| status.key.models.contains(&profile.default_model))
    });
    let codex_configuration =
        config_is_managed(&crate::codex_home::default_codex_home_dir().join("config.toml"));
    EnterpriseDiagnostics {
        server,
        login,
        credential,
        model,
        responses_api: server && credential && model,
        codex_configuration,
        message: if server && login && credential && model && codex_configuration {
            "Enterprise connection is ready.".to_string()
        } else {
            "One or more enterprise checks require attention.".to_string()
        },
    }
}

async fn provision(access_token: &str, executable: &Path) -> anyhow::Result<()> {
    let profile: EnterpriseProfile = request_json(
        reqwest::Method::GET,
        "codex-profile",
        Some(access_token),
        None,
    )
    .await?;
    validate_profile(&profile)?;
    let rotate = secure_store::read(CREDENTIAL_TARGET)?.is_none();
    let ensured: EnsureKeyResponse = request_json(
        reqwest::Method::POST,
        "codex-key/ensure",
        Some(access_token),
        Some(json!({ "rotate": rotate })),
    )
    .await?;
    if let Some(secret) = ensured.virtual_key.filter(|value| !value.trim().is_empty()) {
        secure_store::write(CREDENTIAL_TARGET, &secret)?;
    }
    if secure_store::read(CREDENTIAL_TARGET)?.is_none() {
        anyhow::bail!("Enterprise credential is unavailable; sign in again to recover it");
    }
    write_managed_config(
        &crate::codex_home::default_codex_home_dir().join("config.toml"),
        executable,
        &profile,
    )
}

async fn load_snapshot(access_token: &str) -> anyhow::Result<EnterpriseSnapshot> {
    let status = load_status(access_token).await?;
    let profile: EnterpriseProfile = request_json(
        reqwest::Method::GET,
        "codex-profile",
        Some(access_token),
        None,
    )
    .await?;
    Ok(EnterpriseSnapshot {
        enabled: true,
        state: "authenticated".to_string(),
        user: Some(status.user.clone()),
        status: Some(status),
        profile: Some(profile),
        credential_available: secure_store::read(CREDENTIAL_TARGET)?.is_some(),
        config_managed: config_is_managed(
            &crate::codex_home::default_codex_home_dir().join("config.toml"),
        ),
        message: "Connected to Company AI.".to_string(),
    })
}

#[derive(Debug, Deserialize)]
struct MeResponse {
    user: EnterpriseUser,
}

async fn load_status(access_token: &str) -> anyhow::Result<EnterpriseStatus> {
    let me: MeResponse = request_json(reqwest::Method::GET, "me", Some(access_token), None).await?;
    let mut status: EnterpriseStatus =
        request_json(reqwest::Method::GET, "usage", Some(access_token), None).await?;
    status.user = me.user;
    Ok(status)
}

fn save_session(session: &SessionResponse) -> anyhow::Result<()> {
    secure_store::write(SESSION_TARGET, &session.access_token)?;
    secure_store::write(REFRESH_TARGET, &session.refresh_token)
}

fn clear_credentials() -> anyhow::Result<()> {
    secure_store::delete(SESSION_TARGET)?;
    secure_store::delete(REFRESH_TARGET)?;
    secure_store::delete(CREDENTIAL_TARGET)
}

fn snapshot(state: &str, message: &str) -> EnterpriseSnapshot {
    EnterpriseSnapshot {
        enabled: enterprise_mode_enabled(),
        state: state.to_string(),
        user: None,
        status: None,
        profile: None,
        credential_available: false,
        config_managed: config_is_managed(
            &crate::codex_home::default_codex_home_dir().join("config.toml"),
        ),
        message: message.to_string(),
    }
}

async fn request_json<T: for<'de> Deserialize<'de>>(
    method: reqwest::Method,
    endpoint: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> anyhow::Result<T> {
    let value = request_value(method, endpoint, token, body).await?;
    let value = value.get("data").cloned().unwrap_or(value);
    serde_json::from_value(value).map_err(|error| {
        anyhow::anyhow!("Company AI returned an invalid response for {endpoint}: {error}")
    })
}

fn deserialize_null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Option::<T>::deserialize(deserializer).map(Option::unwrap_or_default)
}

async fn request_value(
    method: reqwest::Method,
    endpoint: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> anyhow::Result<Value> {
    let base = std::env::var("CODEX_PLUS_ENTERPRISE_URL")
        .unwrap_or_else(|_| DEFAULT_LAUNCHER_URL.to_string());
    let base = base.trim_end_matches('/');
    let url = format!("{base}/{API_PREFIX}/{endpoint}");
    let client = crate::http_client::proxied_client("CodexPlusPlus-Enterprise")?;
    let mut request = client.request(method, &url);
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request
        .send()
        .await
        .context("Unable to connect to company AI service")?;
    let status = response.status();
    let bytes = response.bytes().await.unwrap_or_default();
    if !status.is_success() {
        let code = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .and_then(|value| {
                value
                    .pointer("/error/code")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            });
        anyhow::bail!(
            "Company AI request failed (HTTP {}, code {})",
            status.as_u16(),
            code.unwrap_or_else(|| "unknown".to_string())
        );
    }
    if bytes.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(&bytes).context("Company AI returned invalid JSON")
}

fn is_unauthorized(error: &anyhow::Error) -> bool {
    error.to_string().contains("HTTP 401") || error.to_string().contains("HTTP 403")
}

fn validate_profile(profile: &EnterpriseProfile) -> anyhow::Result<()> {
    if profile.provider_id != PROVIDER_ID || profile.wire_api != "responses" {
        anyhow::bail!("Company AI returned an unsupported provider policy");
    }
    if profile.default_model.trim().is_empty()
        || !profile.allowed_models.contains(&profile.default_model)
    {
        anyhow::bail!("Company AI returned an invalid model policy");
    }
    let gateway =
        reqwest::Url::parse(&profile.gateway_url).context("Company AI gateway URL is invalid")?;
    let configured = std::env::var("CODEX_PLUS_ENTERPRISE_URL").ok();
    let allow_development = configured.is_some();
    if gateway.scheme() != "https" && !allow_development {
        anyhow::bail!("Company AI gateway must use HTTPS");
    }
    if gateway.username() != ""
        || gateway.password().is_some()
        || gateway.query().is_some()
        || gateway.fragment().is_some()
    {
        anyhow::bail!("Company AI gateway URL contains unsupported components");
    }
    Ok(())
}

pub fn write_managed_config(
    path: &Path,
    executable: &Path,
    profile: &EnterpriseProfile,
) -> anyhow::Result<()> {
    validate_profile(profile)?;
    let original = std::fs::read_to_string(path).unwrap_or_default();
    let clean = remove_managed_block_text(&original)?;
    if clean
        .lines()
        .any(|line| line.trim_start().starts_with("model_providers.company-ai."))
    {
        anyhow::bail!("An unmanaged company-ai provider already exists");
    }
    let preserved = preserve_root_selections(clean.trim_end());
    let block = build_managed_block(executable, profile);
    let updated = if preserved.trim().is_empty() {
        format!("{block}\n")
    } else {
        format!("{block}\n\n{preserved}\n")
    };
    write_atomic(path, updated.as_bytes())
}

pub fn remove_managed_config(path: &Path) -> anyhow::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let original = std::fs::read_to_string(path)?;
    let restored = restore_root_selections(&remove_managed_block_text(&original)?)?;
    write_atomic(path, restored.trim().as_bytes())
}

fn build_managed_block(executable: &Path, profile: &EnterpriseProfile) -> String {
    let quote = |value: &str| toml::Value::String(value.to_string()).to_string();
    format!(
        "{BEGIN_MARKER}\nmodel = {}\nmodel_provider = {}\n\nmodel_providers.{}.name = {}\nmodel_providers.{}.base_url = {}\nmodel_providers.{}.wire_api = {}\nmodel_providers.{}.auth.command = {}\nmodel_providers.{}.auth.args = [\"--enterprise-credential\", \"get\"]\n{END_MARKER}",
        quote(&profile.default_model),
        quote(PROVIDER_ID),
        PROVIDER_ID,
        quote(&profile.display_name),
        PROVIDER_ID,
        quote(profile.gateway_url.trim_end_matches('/')),
        PROVIDER_ID,
        quote(&profile.wire_api),
        PROVIDER_ID,
        quote(&executable.to_string_lossy()),
        PROVIDER_ID,
    )
}

fn remove_managed_block_text(text: &str) -> anyhow::Result<String> {
    let Some(begin) = text.find(BEGIN_MARKER) else {
        return Ok(text.to_string());
    };
    let end = text[begin..]
        .find(END_MARKER)
        .map(|offset| begin + offset + END_MARKER.len())
        .ok_or_else(|| anyhow::anyhow!("Enterprise managed block is incomplete"))?;
    Ok(format!("{}{}", &text[..begin], &text[end..])
        .trim()
        .to_string())
}

fn preserve_root_selections(text: &str) -> String {
    let mut root = true;
    text.lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if trimmed.starts_with('[') {
                root = false;
            }
            if root && (trimmed.starts_with("model =") || trimmed.starts_with("model_provider =")) {
                use base64::Engine;
                return format!(
                    "{PRESERVED_PREFIX}{}",
                    base64::engine::general_purpose::STANDARD.encode(line)
                );
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn restore_root_selections(text: &str) -> anyhow::Result<String> {
    use base64::Engine;
    text.lines()
        .map(|line| {
            let Some(encoded) = line.strip_prefix(PRESERVED_PREFIX) else {
                return Ok(line.to_string());
            };
            let decoded = base64::engine::general_purpose::STANDARD.decode(encoded)?;
            String::from_utf8(decoded).context("Preserved Codex root setting is invalid")
        })
        .collect::<anyhow::Result<Vec<_>>>()
        .map(|lines| lines.join("\n"))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid Codex config path"))?;
    std::fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".{}.enterprise.tmp",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));
    std::fs::write(&temporary, bytes)?;
    let document = std::fs::read_to_string(&temporary)?;
    document
        .parse::<toml::Value>()
        .context("Generated enterprise Codex configuration is invalid TOML")?;
    std::fs::rename(&temporary, path).or_else(|_| {
        std::fs::copy(&temporary, path)?;
        std::fs::remove_file(&temporary)
    })?;
    Ok(())
}

fn config_is_managed(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .is_ok_and(|text| text.contains(BEGIN_MARKER) && text.contains(END_MARKER))
}

#[cfg(windows)]
mod secure_store {
    use std::ffi::c_void;
    use std::ptr;

    use anyhow::Context;
    use windows::Win32::Foundation::ERROR_NOT_FOUND;
    use windows::Win32::Security::Credentials::{
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree,
        CredReadW, CredWriteW,
    };
    use windows::core::{PCWSTR, PWSTR};

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }

    pub fn write(target: &str, secret: &str) -> anyhow::Result<()> {
        let mut target = wide(target);
        let mut username = wide("CodexPlusPlus");
        let mut bytes = secret
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: PWSTR(target.as_mut_ptr()),
            CredentialBlobSize: u32::try_from(bytes.len())?,
            CredentialBlob: bytes.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            UserName: PWSTR(username.as_mut_ptr()),
            ..Default::default()
        };
        let result = unsafe { CredWriteW(&credential, 0) };
        bytes.fill(0);
        result.context("Unable to save enterprise credential")?;
        Ok(())
    }

    pub fn read(target: &str) -> anyhow::Result<Option<String>> {
        let target = wide(target);
        let mut credential = ptr::null_mut();
        let result = unsafe {
            CredReadW(
                PCWSTR(target.as_ptr()),
                CRED_TYPE_GENERIC,
                0,
                &mut credential,
            )
        };
        if let Err(error) = result {
            if error.code().0 as u32 == ERROR_NOT_FOUND.to_hresult().0 as u32 {
                return Ok(None);
            }
            return Err(error).context("Unable to read enterprise credential");
        }
        if credential.is_null() {
            return Ok(None);
        }
        let value = unsafe {
            let entry = &*credential;
            let bytes =
                std::slice::from_raw_parts(entry.CredentialBlob, entry.CredentialBlobSize as usize);
            let units = bytes
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect::<Vec<_>>();
            String::from_utf16(&units).context("Enterprise credential is invalid")
        };
        unsafe { CredFree(credential.cast::<c_void>()) };
        value.map(Some)
    }

    pub fn delete(target: &str) -> anyhow::Result<()> {
        let target = wide(target);
        let result = unsafe { CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, 0) };
        if let Err(error) = result {
            if error.code().0 as u32 != ERROR_NOT_FOUND.to_hresult().0 as u32 {
                return Err(error).context("Unable to delete enterprise credential");
            }
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod secure_store {
    #[cfg(target_os = "linux")]
    use std::io::Write;
    use std::process::{Command, Stdio};

    pub fn write(target: &str, secret: &str) -> anyhow::Result<()> {
        #[cfg(target_os = "macos")]
        {
            let status = Command::new("security")
                .args([
                    "add-generic-password",
                    "-U",
                    "-a",
                    "CodexPlusPlus",
                    "-s",
                    target,
                    "-w",
                    secret,
                ])
                .status()?;
            if !status.success() {
                anyhow::bail!("Unable to save enterprise credential in Keychain");
            }
            return Ok(());
        }
        #[cfg(target_os = "linux")]
        {
            let mut child = Command::new("secret-tool")
                .args([
                    "store",
                    "--label=CodexPlusPlus Enterprise",
                    "service",
                    "CodexPlusPlus",
                    "target",
                    target,
                ])
                .stdin(Stdio::piped())
                .spawn()?;
            child
                .stdin
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("Unable to open Secret Service input"))?
                .write_all(secret.as_bytes())?;
            if !child.wait()?.success() {
                anyhow::bail!("Unable to save enterprise credential in Secret Service");
            }
            return Ok(());
        }
        #[allow(unreachable_code)]
        anyhow::bail!("Enterprise secure storage is not implemented on this platform")
    }
    pub fn read(target: &str) -> anyhow::Result<Option<String>> {
        #[cfg(target_os = "macos")]
        let output = Command::new("security")
            .args([
                "find-generic-password",
                "-a",
                "CodexPlusPlus",
                "-s",
                target,
                "-w",
            ])
            .output()?;
        #[cfg(target_os = "linux")]
        let output = Command::new("secret-tool")
            .args(["lookup", "service", "CodexPlusPlus", "target", target])
            .output()?;
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        return Ok(None);
        if !output.status.success() {
            return Ok(None);
        }
        Ok(Some(
            String::from_utf8(output.stdout)?.trim_end().to_string(),
        ))
    }
    pub fn delete(target: &str) -> anyhow::Result<()> {
        #[cfg(target_os = "macos")]
        let _ = Command::new("security")
            .args([
                "delete-generic-password",
                "-a",
                "CodexPlusPlus",
                "-s",
                target,
            ])
            .status();
        #[cfg(target_os = "linux")]
        let _ = Command::new("secret-tool")
            .args(["clear", "service", "CodexPlusPlus", "target", target])
            .status();
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        let _ = target;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> EnterpriseProfile {
        EnterpriseProfile {
            gateway_url: "https://api.ai.rydf-design.com/v1".to_string(),
            provider_id: PROVIDER_ID.to_string(),
            default_model: "gpt-5.4-mini".to_string(),
            allowed_models: vec!["gpt-5.4-mini".to_string()],
            wire_api: "responses".to_string(),
            display_name: "Company AI".to_string(),
        }
    }

    #[test]
    fn managed_config_uses_command_auth_without_secret() {
        let block = build_managed_block(
            Path::new("C:/Program Files/Codex++/manager.exe"),
            &profile(),
        );
        assert!(block.contains("auth.command"));
        assert!(block.contains("--enterprise-credential"));
        assert!(!block.contains("OPENAI_API_KEY"));
        assert!(!block.contains("sk-"));
    }

    #[test]
    fn managed_config_preserves_and_restores_community_selection() {
        let original = "model = \"community-model\"\nmodel_provider = \"community\"\n[model_providers.community]\nbase_url = \"https://example.test/v1\"\n";
        let clean = preserve_root_selections(original);
        let restored = restore_root_selections(&clean).unwrap();
        assert_eq!(restored, original.trim_end());
    }

    #[test]
    fn profile_rejects_non_responses_protocol() {
        let mut invalid = profile();
        invalid.wire_api = "chat_completions".to_string();
        assert!(validate_profile(&invalid).is_err());
    }

    #[test]
    fn session_response_accepts_launcher_camel_case_without_user() {
        let session: SessionResponse = serde_json::from_value(json!({
            "accessToken": "access",
            "refreshToken": "refresh",
            "expiresAt": "2026-08-13T12:00:00Z",
            "tokenType": "Bearer"
        }))
        .unwrap();
        assert_eq!(session.access_token, "access");
        assert_eq!(session.refresh_token, "refresh");
    }

    #[test]
    fn session_response_accepts_sub2api_snake_case() {
        let session: SessionResponse = serde_json::from_value(json!({
            "access_token": "access",
            "refresh_token": "refresh"
        }))
        .unwrap();
        assert_eq!(session.access_token, "access");
        assert_eq!(session.refresh_token, "refresh");
    }

    #[test]
    fn usage_key_accepts_null_models_from_existing_upstream_key() {
        let key: EnterpriseKeyInfo = serde_json::from_value(json!({
            "alias": "codex-launcher",
            "expiresAt": null,
            "rpm": 0,
            "tpm": 0,
            "models": null
        }))
        .unwrap();
        assert!(key.models.is_empty());
    }

    #[test]
    fn ensure_key_ignores_nullable_status_metadata() {
        let key: EnsureKeyResponse = serde_json::from_value(json!({
            "alias": "codex-launcher",
            "expiresAt": null,
            "rpm": 0,
            "tpm": 0,
            "models": null,
            "virtualKey": "secret"
        }))
        .unwrap();
        assert_eq!(key.virtual_key.as_deref(), Some("secret"));
    }
}

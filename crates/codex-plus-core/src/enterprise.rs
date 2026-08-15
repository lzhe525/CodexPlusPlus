use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::settings::LaunchMode;

const API_PREFIX: &str = "launcher/v1";
const DEFAULT_LAUNCHER_URL: &str = "https://api.ai.rydf-design.com";
const PROVIDER_ID: &str = "company-ai";
const BEGIN_MARKER: &str = "# >>> codex-plus-plus enterprise managed block >>>";
const END_MARKER: &str = "# <<< codex-plus-plus enterprise managed block <<<";
const PRESERVED_PREFIX: &str = "# codex-plus-plus enterprise preserved-root: ";
const SESSION_TARGET: &str = "CodexPlusPlus/enterprise/session";
const REFRESH_TARGET: &str = "CodexPlusPlus/enterprise/refresh";
const CREDENTIAL_TARGET: &str = "CodexPlusPlus/enterprise/inference";
const LOGIN_METHOD_FILE: &str = "enterprise-login-method.json";
const PREFERENCES_FILE: &str = "enterprise-preferences.json";
const BRAND_NAME: &str = "Azalea Plugin for Codex";
pub const LOGIN_METHOD_COMPANY: &str = "company";
pub const LOGIN_METHOD_OFFICIAL: &str = "official";

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
pub struct EnterpriseSubscriptionQuotaWindow {
    pub period: String,
    pub used_usd: f64,
    pub limit_usd: f64,
    pub remaining_usd: f64,
    pub percentage: f64,
    pub resets_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EnterpriseSubscriptionProgress {
    pub id: i64,
    pub group_id: i64,
    pub group_name: String,
    pub expires_at: String,
    #[serde(default)]
    pub windows: Vec<EnterpriseSubscriptionQuotaWindow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EnterpriseStatus {
    pub user: EnterpriseUser,
    pub usage: EnterpriseUsage,
    pub subscription: EnterpriseSubscriptionProgress,
    pub key: EnterpriseKeyInfo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnterpriseFailure {
    pub code: String,
    pub retryable: bool,
}

#[derive(Debug)]
struct EnterpriseRequestError {
    status: u16,
    code: String,
    retryable: bool,
    session_auth: bool,
    message: String,
}

impl std::fmt::Display for EnterpriseRequestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{BRAND_NAME} request failed (HTTP {}, code {}): {}",
            self.status, self.code, self.message
        )
    }
}

impl std::error::Error for EnterpriseRequestError {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EnterpriseProfile {
    pub gateway_url: String,
    pub provider_id: String,
    #[serde(default)]
    pub default_model: String,
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub allowed_models: Vec<String>,
    pub wire_api: String,
    pub display_name: String,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub group_id: Option<i64>,
    #[serde(default)]
    pub group_name: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct EnterprisePreferences {
    #[serde(default = "preference_schema_version")]
    schema_version: u32,
    #[serde(default)]
    default_model_by_user_id: std::collections::BTreeMap<String, String>,
}

fn preference_schema_version() -> u32 {
    1
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
    #[serde(default)]
    pub login_method: String,
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

pub async fn restore(executable: &Path) -> anyhow::Result<EnterpriseSnapshot> {
    if !enterprise_mode_enabled() {
        return Ok(snapshot("disabled", "Community Mode is active."));
    }
    if current_login_method() == LOGIN_METHOD_OFFICIAL {
        return Ok(snapshot(
            "official",
            "已切换到原账号登录；可使用官方 ChatGPT 账号。",
        ));
    }
    let Some(access_token) = secure_store::read(SESSION_TARGET)? else {
        return Ok(snapshot(
            "unauthenticated",
            "Sign in with your company account.",
        ));
    };
    match provision_and_load(&access_token, executable).await {
        Ok(snapshot) => Ok(snapshot),
        Err(error) if is_unauthorized(&error) => {
            retry_after_renew(|access_token| async move {
                provision_and_load(&access_token, executable).await
            })
            .await
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
    write_login_method(LOGIN_METHOD_COMPANY)?;
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
        clear_enterprise_state()?;
        return Err(error);
    }
    load_snapshot(&session.access_token).await
}

pub async fn refresh(executable: &Path) -> anyhow::Result<EnterpriseSnapshot> {
    let access_token = secure_store::read(SESSION_TARGET)?
        .ok_or_else(|| anyhow::anyhow!("Authentication required"))?;
    match provision_and_load(&access_token, executable).await {
        Ok(snapshot) => Ok(snapshot),
        Err(error) if is_unauthorized(&error) => {
            retry_after_renew(|access_token| async move {
                provision_and_load(&access_token, executable).await
            })
            .await
        }
        Err(error) => Err(error),
    }
}

pub async fn reload() -> anyhow::Result<EnterpriseSnapshot> {
    if !enterprise_mode_enabled() {
        return Ok(snapshot("disabled", "Community Mode is active."));
    }
    if current_login_method() == LOGIN_METHOD_OFFICIAL {
        return Ok(snapshot(
            "official",
            "Official ChatGPT account login is active.",
        ));
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
            retry_after_renew(|access_token| async move { load_snapshot(&access_token).await })
                .await
        }
        Err(error) => Err(error),
    }
}

pub async fn set_default_model(
    model: &str,
    executable: &Path,
) -> anyhow::Result<EnterpriseSnapshot> {
    let requested = model.trim();
    if requested.is_empty() {
        anyhow::bail!("Default model is required");
    }
    let access_token = secure_store::read(SESSION_TARGET)?
        .ok_or_else(|| anyhow::anyhow!("Authentication required"))?;
    match set_default_model_with_token(model, executable, &access_token).await {
        Ok(snapshot) => Ok(snapshot),
        Err(error) if is_unauthorized(&error) => {
            retry_after_renew(|access_token| async move {
                set_default_model_with_token(model, executable, &access_token).await
            })
            .await
        }
        Err(error) => Err(error),
    }
}

async fn set_default_model_with_token(
    model: &str,
    executable: &Path,
    access_token: &str,
) -> anyhow::Result<EnterpriseSnapshot> {
    let requested = model.trim();
    let credential = secure_store::read(CREDENTIAL_TARGET)?.ok_or_else(|| {
        anyhow::anyhow!("Enterprise credential is unavailable; refresh the account first")
    })?;
    let user = load_user(access_token).await?;
    let mut profile: EnterpriseProfile = request_json(
        reqwest::Method::GET,
        "codex-profile",
        Some(access_token),
        None,
    )
    .await?;
    validate_profile(&profile)?;
    let models = discover_user_models(&profile, &credential).await?;
    if !models.iter().any(|available| available == requested) {
        anyhow::bail!("The selected model is not available to this Sub2API account");
    }
    profile.allowed_models = models;
    profile.default_model = requested.to_string();
    apply_user_identity(&mut profile, &user);

    let mut preferences = load_preferences()?;
    let previous = preferences
        .default_model_by_user_id
        .insert(user.id.clone(), requested.to_string());
    save_preferences(&preferences)?;
    if let Err(error) = write_managed_config(
        &crate::codex_home::default_codex_home_dir().join("config.toml"),
        executable,
        &profile,
        &credential,
    ) {
        match previous {
            Some(value) => {
                preferences.default_model_by_user_id.insert(user.id, value);
            }
            None => {
                preferences.default_model_by_user_id.remove(&user.id);
            }
        }
        let _ = save_preferences(&preferences);
        return Err(error);
    }
    load_snapshot(access_token).await
}

pub async fn logout() -> anyhow::Result<EnterpriseSnapshot> {
    if let Some(access_token) = secure_store::read(SESSION_TARGET)? {
        let _ = request_value(reqwest::Method::POST, "logout", Some(&access_token), None).await;
    }
    clear_enterprise_state()?;
    write_login_method(LOGIN_METHOD_COMPANY)?;
    Ok(snapshot(
        "unauthenticated",
        "Signed out and removed this device credential.",
    ))
}

pub async fn use_official_login() -> anyhow::Result<EnterpriseSnapshot> {
    write_login_method(LOGIN_METHOD_OFFICIAL)?;
    if let Some(access_token) = secure_store::read(SESSION_TARGET).ok().flatten() {
        let _ = request_value(reqwest::Method::POST, "logout", Some(&access_token), None).await;
    }
    let _ = clear_enterprise_state();
    apply_official_login_config(&crate::codex_home::default_codex_home_dir())?;
    if let Ok(mut settings) = crate::settings::SettingsStore::default().load() {
        settings.launch_mode = LaunchMode::Relay;
        let _ = crate::settings::SettingsStore::default().save(&settings);
    }
    Ok(snapshot(
        "official",
        "已切换到原账号登录，并恢复官方 ChatGPT 登录途径。",
    ))
}

pub async fn use_company_login() -> anyhow::Result<EnterpriseSnapshot> {
    if !enterprise_mode_enabled() {
        anyhow::bail!("Enterprise Mode is not enabled");
    }
    write_login_method(LOGIN_METHOD_COMPANY)?;
    let executable = std::env::current_exe()?;
    restore(&executable).await
}

pub fn apply_official_login_config(home: &Path) -> anyhow::Result<()> {
    remove_managed_config(&home.join("config.toml"))?;
    crate::relay_config::clear_relay_config_to_home(home)?;
    Ok(())
}

pub fn current_login_method() -> String {
    read_login_method(&login_method_path())
}

pub async fn diagnostics() -> EnterpriseDiagnostics {
    let server = request_value(reqwest::Method::GET, "health", None, None)
        .await
        .is_ok();
    let token = secure_store::read(SESSION_TARGET).ok().flatten();
    let login = token.is_some();
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
    let model = match (
        profile.as_ref(),
        secure_store::read(CREDENTIAL_TARGET).ok().flatten(),
    ) {
        (Some(profile), Some(credential)) => {
            discover_user_models(profile, &credential).await.is_ok()
        }
        _ => false,
    };
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

async fn renew_session() -> anyhow::Result<String> {
    let Some(refresh_token) = secure_store::read(REFRESH_TARGET)? else {
        clear_enterprise_state()?;
        anyhow::bail!("Enterprise session expired (HTTP 401); sign in again");
    };
    let session = request_json::<SessionResponse>(
        reqwest::Method::POST,
        "refresh",
        None,
        Some(json!({ "refreshToken": refresh_token })),
    )
    .await;
    match session {
        Ok(session) => {
            save_session(&session)?;
            Ok(session.access_token)
        }
        Err(error) => {
            if is_unauthorized(&error) {
                clear_enterprise_state()?;
            }
            Err(error)
        }
    }
}

async fn retry_after_renew<F, Fut>(operation: F) -> anyhow::Result<EnterpriseSnapshot>
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = anyhow::Result<EnterpriseSnapshot>>,
{
    let access_token = match renew_session().await {
        Ok(access_token) => access_token,
        Err(error) if is_unauthorized(&error) => return Ok(expired_snapshot()),
        Err(error) => return Err(error),
    };
    match operation(access_token).await {
        Err(error) if is_unauthorized(&error) => {
            clear_enterprise_state()?;
            Ok(expired_snapshot())
        }
        result => result,
    }
}

async fn provision_and_load(
    access_token: &str,
    executable: &Path,
) -> anyhow::Result<EnterpriseSnapshot> {
    provision(access_token, executable).await?;
    load_snapshot(access_token).await
}

async fn provision(access_token: &str, executable: &Path) -> anyhow::Result<()> {
    let mut profile: EnterpriseProfile = request_json(
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
    let credential = secure_store::read(CREDENTIAL_TARGET)?.ok_or_else(|| {
        anyhow::anyhow!("Enterprise credential is unavailable; sign in again to recover it")
    })?;
    let user = load_user(access_token).await?;
    let models = discover_user_models(&profile, &credential).await?;
    apply_user_models(&mut profile, models, &user)?;
    write_managed_config(
        &crate::codex_home::default_codex_home_dir().join("config.toml"),
        executable,
        &profile,
        &credential,
    )
}

async fn load_snapshot(access_token: &str) -> anyhow::Result<EnterpriseSnapshot> {
    let status = load_status(access_token).await?;
    let mut profile: EnterpriseProfile = request_json(
        reqwest::Method::GET,
        "codex-profile",
        Some(access_token),
        None,
    )
    .await?;
    if let Some(credential) = secure_store::read(CREDENTIAL_TARGET)? {
        let models = discover_user_models(&profile, &credential).await?;
        apply_user_models(&mut profile, models, &status.user)?;
    }
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
        login_method: current_login_method(),
        message: format!("Connected to {BRAND_NAME}."),
    })
}

#[derive(Debug, Deserialize)]
struct MeResponse {
    user: EnterpriseUser,
}

async fn load_user(access_token: &str) -> anyhow::Result<EnterpriseUser> {
    let me: MeResponse = request_json(reqwest::Method::GET, "me", Some(access_token), None).await?;
    Ok(me.user)
}

async fn load_status(access_token: &str) -> anyhow::Result<EnterpriseStatus> {
    let user = load_user(access_token).await?;
    let mut status: EnterpriseStatus =
        request_json(reqwest::Method::GET, "usage", Some(access_token), None).await?;
    status.user = user;
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

fn clear_enterprise_state() -> anyhow::Result<()> {
    let home = crate::codex_home::default_codex_home_dir();
    let mut failures = Vec::new();
    let credential = match secure_store::read(CREDENTIAL_TARGET) {
        Ok(credential) => credential,
        Err(error) => {
            failures.push(error.to_string());
            None
        }
    };
    if let Err(error) = clear_enterprise_configuration(&home, credential.as_deref()) {
        failures.push(error.to_string());
    }
    if let Err(error) = clear_credentials() {
        failures.push(error.to_string());
    }
    if failures.is_empty() {
        Ok(())
    } else {
        anyhow::bail!(
            "Unable to fully clear enterprise credentials: {}",
            failures.join("; ")
        )
    }
}

fn clear_enterprise_configuration(home: &Path, credential: Option<&str>) -> anyhow::Result<()> {
    let mut failures = Vec::new();
    let config_path = home.join("config.toml");
    let legacy_credential = std::fs::read_to_string(&config_path)
        .ok()
        .and_then(|config| managed_legacy_bearer_token(&config));
    if let Err(error) = remove_managed_config(&config_path) {
        failures.push(error.to_string());
    }
    let credentials = [credential, legacy_credential.as_deref()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    if let Err(error) = remove_matching_enterprise_auth_keys(home, &credentials) {
        failures.push(error.to_string());
    }
    if failures.is_empty() {
        Ok(())
    } else {
        anyhow::bail!(
            "Unable to clear enterprise configuration: {}",
            failures.join("; ")
        )
    }
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
        login_method: current_login_method(),
        message: message.to_string(),
    }
}

fn expired_snapshot() -> EnterpriseSnapshot {
    snapshot("expired", "Your session expired. Please sign in again.")
}

fn login_method_path() -> PathBuf {
    LOGIN_METHOD_PATH_FOR_TESTS
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|path| path.clone())
        .unwrap_or_else(|| crate::paths::default_app_state_dir().join(LOGIN_METHOD_FILE))
}

fn preferences_path() -> PathBuf {
    PREFERENCES_PATH_FOR_TESTS
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|path| path.clone())
        .unwrap_or_else(|| crate::paths::default_app_state_dir().join(PREFERENCES_FILE))
}

fn load_preferences() -> anyhow::Result<EnterprisePreferences> {
    let path = preferences_path();
    if !path.exists() {
        return Ok(EnterprisePreferences {
            schema_version: preference_schema_version(),
            ..EnterprisePreferences::default()
        });
    }
    let preferences: EnterprisePreferences = serde_json::from_slice(&std::fs::read(&path)?)
        .with_context(|| format!("{} is not valid enterprise preference JSON", path.display()))?;
    if preferences.schema_version != preference_schema_version() {
        anyhow::bail!("Unsupported enterprise preference schema version");
    }
    Ok(preferences)
}

fn save_preferences(preferences: &EnterprisePreferences) -> anyhow::Result<()> {
    let path = preferences_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::settings::atomic_write(&path, &serde_json::to_vec_pretty(preferences)?)
}

fn read_login_method(path: &Path) -> String {
    let Ok(text) = std::fs::read_to_string(path) else {
        return LOGIN_METHOD_COMPANY.to_string();
    };
    serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|value| {
            value
                .get("method")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .map(|method| {
            if method == LOGIN_METHOD_OFFICIAL {
                LOGIN_METHOD_OFFICIAL.to_string()
            } else {
                LOGIN_METHOD_COMPANY.to_string()
            }
        })
        .unwrap_or_else(|| LOGIN_METHOD_COMPANY.to_string())
}

fn write_login_method(method: &str) -> anyhow::Result<()> {
    let path = login_method_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&json!({ "method": method }))?,
    )?;
    Ok(())
}

static LOGIN_METHOD_PATH_FOR_TESTS: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
static PREFERENCES_PATH_FOR_TESTS: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();

#[cfg(test)]
pub fn set_login_method_path_for_tests(path: Option<PathBuf>) -> Option<PathBuf> {
    LOGIN_METHOD_PATH_FOR_TESTS
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|mut current| std::mem::replace(&mut *current, path))
}

#[cfg(test)]
fn set_preferences_path_for_tests(path: Option<PathBuf>) -> Option<PathBuf> {
    PREFERENCES_PATH_FOR_TESTS
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|mut current| std::mem::replace(&mut *current, path))
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
        anyhow::anyhow!("{BRAND_NAME} returned an invalid response for {endpoint}: {error}")
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
        .with_context(|| format!("Unable to connect to {BRAND_NAME}"))?;
    let status = response.status();
    let bytes = response.bytes().await.unwrap_or_default();
    if !status.is_success() {
        let error_value = serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null);
        let code = error_value
            .pointer("/error/code")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let retryable = error_value
            .pointer("/error/retryable")
            .and_then(Value::as_bool)
            .unwrap_or(status.is_server_error());
        let message = error_value
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("request failed")
            .to_string();
        return Err(anyhow::Error::new(EnterpriseRequestError {
            status: status.as_u16(),
            code,
            retryable,
            session_auth: true,
            message,
        }));
    }
    if bytes.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(&bytes).with_context(|| format!("{BRAND_NAME} returned invalid JSON"))
}

fn is_unauthorized(error: &anyhow::Error) -> bool {
    if let Some(error) = error.downcast_ref::<EnterpriseRequestError>() {
        return error.session_auth
            && (error.status == 401
                || error.code == "unauthorized"
                || error.code == "reauthentication_required");
    }
    let message = error.to_string();
    message.contains("HTTP 401")
        || message.contains("code unauthorized")
        || message.contains("code reauthentication_required")
}

pub fn enterprise_failure(error: &anyhow::Error) -> EnterpriseFailure {
    if let Some(error) = error.downcast_ref::<EnterpriseRequestError>() {
        return EnterpriseFailure {
            code: error.code.clone(),
            retryable: error.retryable,
        };
    }
    if error
        .chain()
        .any(|cause| cause.downcast_ref::<reqwest::Error>().is_some())
    {
        return EnterpriseFailure {
            code: "network_unavailable".to_string(),
            retryable: true,
        };
    }
    if error.chain().any(|cause| {
        let message = cause.to_string();
        message.contains("Enterprise configuration")
            || message.contains("enterprise Codex configuration")
            || message.contains("config.toml")
    }) {
        return EnterpriseFailure {
            code: "enterprise_configuration_repair_failed".to_string(),
            retryable: true,
        };
    }
    EnterpriseFailure {
        code: "client_error".to_string(),
        retryable: false,
    }
}

fn validate_profile(profile: &EnterpriseProfile) -> anyhow::Result<()> {
    if profile.provider_id != PROVIDER_ID || profile.wire_api != "responses" {
        anyhow::bail!("{BRAND_NAME} returned an unsupported provider policy");
    }
    let gateway = reqwest::Url::parse(&profile.gateway_url)
        .with_context(|| format!("{BRAND_NAME} gateway URL is invalid"))?;
    let configured = std::env::var("CODEX_PLUS_ENTERPRISE_URL").ok();
    let allow_development = configured.is_some();
    if gateway.scheme() != "https" && !allow_development {
        anyhow::bail!("{BRAND_NAME} gateway must use HTTPS");
    }
    if gateway.username() != ""
        || gateway.password().is_some()
        || gateway.query().is_some()
        || gateway.fragment().is_some()
    {
        anyhow::bail!("{BRAND_NAME} gateway URL contains unsupported components");
    }
    Ok(())
}

fn models_url(gateway_url: &str) -> String {
    let base = gateway_url.trim_end_matches('/');
    if base.to_ascii_lowercase().ends_with("/models") {
        base.to_string()
    } else if base
        .rsplit('/')
        .next()
        .is_some_and(|segment| segment.eq_ignore_ascii_case("v1"))
    {
        format!("{base}/models")
    } else {
        format!("{base}/v1/models")
    }
}

async fn discover_user_models(
    profile: &EnterpriseProfile,
    credential: &str,
) -> anyhow::Result<Vec<String>> {
    validate_profile(profile)?;
    let response = crate::http_client::proxied_client("CodexPlusPlus-Enterprise")?
        .get(models_url(&profile.gateway_url))
        .bearer_auth(credential)
        .send()
        .await
        .context("Unable to load models available to this Sub2API account")?;
    let status = response.status();
    let bytes = response.bytes().await.unwrap_or_default();
    if !status.is_success() {
        return Err(anyhow::Error::new(EnterpriseRequestError {
            status: status.as_u16(),
            code: "model_discovery_failed".to_string(),
            retryable: status.is_server_error() || status.as_u16() == 429,
            session_auth: false,
            message: "unable to load models available to this account".to_string(),
        }));
    }
    let value: Value =
        serde_json::from_slice(&bytes).context("Sub2API returned an invalid model list")?;
    let items = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("Sub2API returned an invalid model list"))?;
    let mut models = Vec::new();
    for item in items {
        let Some(model) = item
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|model| !model.is_empty())
        else {
            continue;
        };
        if !models.iter().any(|existing| existing == model) {
            models.push(model.to_string());
        }
    }
    if models.is_empty() {
        anyhow::bail!("This Sub2API account currently has no available models");
    }
    Ok(models)
}

fn apply_user_models(
    profile: &mut EnterpriseProfile,
    models: Vec<String>,
    user: &EnterpriseUser,
) -> anyhow::Result<()> {
    let preferences = load_preferences()?;
    let preferred = preferences
        .default_model_by_user_id
        .get(user.id.trim())
        .filter(|model| models.contains(model))
        .cloned();
    let default_model = if let Some(preferred) = preferred {
        preferred
    } else if models.contains(&profile.default_model) {
        profile.default_model.clone()
    } else {
        models.first().cloned().ok_or_else(|| {
            anyhow::anyhow!("This Sub2API account currently has no available models")
        })?
    };
    profile.default_model = default_model;
    profile.allowed_models = models;
    apply_user_identity(profile, user);
    Ok(())
}

fn apply_user_identity(profile: &mut EnterpriseProfile, user: &EnterpriseUser) {
    let user_id = user.id.trim();
    if !user_id.is_empty() {
        profile.user_id = Some(user_id.to_string());
    }

    let display_name = user.display_name.trim();
    if !display_name.is_empty() {
        profile.display_name = display_name.to_string();
    } else if !user_id.is_empty() {
        profile.display_name = user_id.to_string();
    }
}

pub fn write_managed_config(
    path: &Path,
    executable: &Path,
    profile: &EnterpriseProfile,
    credential: &str,
) -> anyhow::Result<()> {
    validate_profile(profile)?;
    if credential.trim().is_empty() {
        anyhow::bail!("Enterprise API key is empty");
    }
    let original = std::fs::read_to_string(path).unwrap_or_default();
    let legacy_credential = managed_legacy_bearer_token(&original);
    let clean = remove_managed_block_text(&original)?;
    if clean
        .lines()
        .any(|line| line.trim_start().starts_with("model_providers.company-ai."))
    {
        anyhow::bail!("An unmanaged company-ai provider already exists");
    }
    let preserved = preserve_root_selections(clean.trim_end());
    let (preserved_root, preserved_tables) = split_root_preamble(&preserved);
    let block = build_managed_block(executable, profile);
    let mut sections = Vec::new();
    if !preserved_root.trim().is_empty() {
        sections.push(preserved_root.trim().to_string());
    }
    sections.push(block);
    if !preserved_tables.trim().is_empty() {
        sections.push(preserved_tables.trim().to_string());
    }
    let updated = format!("{}\n", sections.join("\n\n"));
    let home = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Codex config path has no parent directory"))?;
    let credentials = [Some(credential), legacy_credential.as_deref()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let auth_bytes = auth_json_without_matching_api_keys(home, &credentials)?;
    crate::relay_config::write_codex_live_atomic(
        home,
        Some(&updated),
        auth_bytes.as_deref(),
        false,
    )?;
    Ok(())
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
    let image_mcp_executable = executable.with_file_name(if cfg!(windows) {
        "codex-plus-image-mcp.exe"
    } else {
        "codex-plus-image-mcp"
    });
    format!(
        "{BEGIN_MARKER}\nmodel = {}\nmodel_provider = {}\n\nmodel_providers.{}.name = {}\nmodel_providers.{}.base_url = {}\nmodel_providers.{}.wire_api = {}\n\n[model_providers.{}.auth]\ncommand = {}\nargs = [\"--enterprise-credential\", \"get\"]\ntimeout_ms = 5000\nrefresh_interval_ms = 300000\n\n[mcp_servers.company_imagegen]\ncommand = {}\nstartup_timeout_sec = 30\ntool_timeout_sec = 300\n\n[mcp_servers.company_imagegen.env]\nCODEX_PLUS_IMAGE_BASE_URL = {}\nCODEX_PLUS_RESPONSES_MODEL = {}\nCODEX_PLUS_IMAGE_MODEL = \"gpt-image-2\"\n{END_MARKER}",
        quote(&profile.default_model),
        quote(PROVIDER_ID),
        PROVIDER_ID,
        quote(&profile.display_name),
        PROVIDER_ID,
        quote(profile.gateway_url.trim_end_matches('/')),
        PROVIDER_ID,
        quote(&profile.wire_api),
        PROVIDER_ID,
        quote(&image_mcp_executable.to_string_lossy()),
        quote(&image_mcp_executable.to_string_lossy()),
        quote(profile.gateway_url.trim_end_matches('/')),
        quote("gpt-5.6-sol"),
    )
}

fn auth_json_without_matching_api_keys(
    home: &Path,
    credentials: &[&str],
) -> anyhow::Result<Option<Vec<u8>>> {
    let path = home.join("auth.json");
    if !path.exists() {
        return Ok(None);
    }
    let mut auth = serde_json::from_slice::<Value>(&std::fs::read(&path)?)
        .with_context(|| format!("{} is not valid JSON", path.display()))?;
    let object = auth
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("{} must contain a JSON object", path.display()))?;
    let matches_enterprise_credential = object
        .get("OPENAI_API_KEY")
        .and_then(Value::as_str)
        .is_some_and(|value| {
            credentials
                .iter()
                .map(|credential| credential.trim())
                .any(|credential| !credential.is_empty() && value == credential)
        });
    if !matches_enterprise_credential {
        return Ok(None);
    }
    object.remove("OPENAI_API_KEY");
    Ok(Some(serde_json::to_vec_pretty(&auth)?))
}

fn remove_matching_enterprise_auth_keys(home: &Path, credentials: &[&str]) -> anyhow::Result<()> {
    if let Some(auth) = auth_json_without_matching_api_keys(home, credentials)? {
        crate::settings::atomic_write(&home.join("auth.json"), &auth)?;
    }
    Ok(())
}

fn managed_legacy_bearer_token(config: &str) -> Option<String> {
    let begin = config.find(BEGIN_MARKER)?;
    let relative_end = config[begin..].find(END_MARKER)?;
    let managed = &config[begin..begin + relative_end + END_MARKER.len()];
    managed
        .parse::<toml::Value>()
        .ok()?
        .get("model_providers")?
        .get(PROVIDER_ID)?
        .get("experimental_bearer_token")?
        .as_str()
        .map(str::trim)
        .filter(|credential| !credential.is_empty())
        .map(str::to_string)
}

fn remove_managed_block_text(text: &str) -> anyhow::Result<String> {
    let begin = text.find(BEGIN_MARKER);
    let end = text.find(END_MARKER);
    if let (Some(begin), Some(end)) = (begin, end) {
        if end >= begin {
            return Ok(
                format!("{}{}", &text[..begin], &text[end + END_MARKER.len()..])
                    .trim()
                    .to_string(),
            );
        }
    }

    let document = text
        .parse::<toml_edit::DocumentMut>()
        .context("Enterprise configuration cannot be recovered because config.toml is invalid")?;
    let marker_damaged = begin.is_some() || end.is_some();
    if !marker_damaged && !has_managed_enterprise_shape(&document) {
        return Ok(text.to_string());
    }
    remove_managed_config_structurally(document, marker_damaged)
}

fn has_managed_enterprise_shape(document: &toml_edit::DocumentMut) -> bool {
    let company_provider = document
        .get("model_providers")
        .and_then(toml_edit::Item::as_table)
        .is_some_and(|providers| providers.contains_key(PROVIDER_ID));
    let image_helper = document
        .get("mcp_servers")
        .and_then(toml_edit::Item::as_table)
        .is_some_and(|servers| servers.contains_key("company_imagegen"));
    document
        .get("model_provider")
        .and_then(toml_edit::Item::as_str)
        == Some(PROVIDER_ID)
        && company_provider
        && image_helper
}

fn remove_managed_config_structurally(
    mut document: toml_edit::DocumentMut,
    marker_damaged: bool,
) -> anyhow::Result<String> {
    if !marker_damaged && !has_managed_enterprise_shape(&document) {
        anyhow::bail!("Enterprise managed configuration cannot be identified safely");
    }

    if document
        .get("model_provider")
        .and_then(toml_edit::Item::as_str)
        == Some(PROVIDER_ID)
    {
        document.as_table_mut().remove("model");
        document.as_table_mut().remove("model_provider");
    }
    remove_child_table(&mut document, "model_providers", PROVIDER_ID);
    remove_child_table(&mut document, "mcp_servers", "company_imagegen");

    let cleaned = document
        .to_string()
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed != BEGIN_MARKER && trimmed != END_MARKER
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(cleaned.trim().to_string())
}

fn remove_child_table(document: &mut toml_edit::DocumentMut, parent: &str, child: &str) {
    let empty = document
        .get_mut(parent)
        .and_then(toml_edit::Item::as_table_mut)
        .is_some_and(|table| {
            table.remove(child);
            table.is_empty()
        });
    if empty {
        document.as_table_mut().remove(parent);
    }
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

/// Splits a Codex configuration before its first table header.
///
/// TOML has no syntax for returning to the document root after entering a table.
/// The enterprise managed block contains provider and MCP tables, so existing
/// root settings must be emitted before that block. Otherwise settings such as
/// `notify = [...]` become values in `mcp_servers.company_imagegen.env`.
fn split_root_preamble(text: &str) -> (&str, &str) {
    let table_offset = text
        .lines()
        .scan(0usize, |offset, line| {
            let start = *offset;
            *offset += line.len();
            if text.as_bytes().get(*offset) == Some(&b'\r') {
                *offset += 1;
            }
            if text.as_bytes().get(*offset) == Some(&b'\n') {
                *offset += 1;
            }
            Some((start, line))
        })
        .find_map(|(offset, line)| line.trim_start().starts_with('[').then_some(offset))
        .unwrap_or(text.len());
    text.split_at(table_offset)
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
            display_name: BRAND_NAME.to_string(),
            user_id: None,
            group_id: None,
            group_name: None,
        }
    }

    fn user(id: &str, display_name: &str) -> EnterpriseUser {
        EnterpriseUser {
            id: id.to_string(),
            display_name: display_name.to_string(),
            balance_usd: None,
        }
    }

    #[test]
    fn managed_config_uses_direct_gateway_with_codex_auth() {
        let block = build_managed_block(
            Path::new("C:/Program Files/Codex++/manager.exe"),
            &profile(),
        );
        let document = block.parse::<toml::Value>().unwrap();
        let provider = &document["model_providers"][PROVIDER_ID];
        let auth = &provider["auth"];
        let command = auth["command"].as_str().unwrap();
        assert!(block.contains("base_url = \"https://api.ai.rydf-design.com/v1\""));
        assert!(block.contains("[model_providers.company-ai.auth]"));
        assert!(command.ends_with("codex-plus-image-mcp.exe"));
        assert_eq!(
            auth["args"].as_array().unwrap(),
            &[
                toml::Value::String("--enterprise-credential".to_string()),
                toml::Value::String("get".to_string()),
            ]
        );
        assert!(!block.contains("127.0.0.1"));
        assert!(!block.contains("requires_openai_auth"));
        assert!(!block.contains("experimental_bearer_token"));
        assert!(!block.contains("sk-enterprise-test"));
        assert!(!block.contains("OPENAI_API_KEY"));
        assert!(block.contains("[mcp_servers.company_imagegen]"));
        assert!(block.contains("codex-plus-image-mcp.exe"));
        assert!(
            block.contains("CODEX_PLUS_IMAGE_BASE_URL = \"https://api.ai.rydf-design.com/v1\"")
        );
        assert!(block.contains("CODEX_PLUS_RESPONSES_MODEL = \"gpt-5.6-sol\""));
        assert!(block.contains("CODEX_PLUS_IMAGE_MODEL = \"gpt-image-2\""));
    }

    #[test]
    fn managed_provider_name_uses_authenticated_sub2api_username() {
        let mut profile = profile();
        apply_user_models(
            &mut profile,
            vec!["gpt-5.4-mini".to_string()],
            &user("sub2-user-42", "alice"),
        )
        .unwrap();
        let block =
            build_managed_block(Path::new("C:/Program Files/Codex++/manager.exe"), &profile);
        assert!(block.contains("model_providers.company-ai.name = \"alice\""));
        assert!(block.contains("model_provider = \"company-ai\""));
        assert_eq!(profile.user_id.as_deref(), Some("sub2-user-42"));
    }

    #[test]
    fn managed_config_preserves_and_restores_community_selection() {
        let original = "model = \"community-model\"\nmodel_provider = \"community\"\n[model_providers.community]\nbase_url = \"https://example.test/v1\"\n";
        let clean = preserve_root_selections(original);
        let restored = restore_root_selections(&clean).unwrap();
        assert_eq!(restored, original.trim_end());
    }

    #[test]
    fn managed_config_keeps_existing_root_settings_out_of_image_mcp_env() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let original = r#"model = "official-model"
model_provider = "openai"
model_reasoning_effort = "medium"
approval_policy = "never"
sandbox_mode = "danger-full-access"
service_tier = "priority"
notify = ["powershell", "-File", "notify.ps1"]

[windows]
sandbox = "elevated"

[features]
apps = true
"#;
        std::fs::write(home.join("config.toml"), original).unwrap();

        write_managed_config(
            &home.join("config.toml"),
            Path::new("C:/Program Files/Codex++/codex-plus-plus.exe"),
            &profile(),
            "sk-enterprise-test",
        )
        .unwrap();

        let config = std::fs::read_to_string(home.join("config.toml")).unwrap();
        let document = config.parse::<toml::Value>().unwrap();
        assert_eq!(document["model_provider"].as_str(), Some(PROVIDER_ID));
        assert_eq!(document["model_reasoning_effort"].as_str(), Some("medium"));
        assert_eq!(document["service_tier"].as_str(), Some("priority"));
        assert!(document["notify"].is_array());
        assert_eq!(document["windows"]["sandbox"].as_str(), Some("elevated"));
        assert_eq!(document["features"]["apps"].as_bool(), Some(true));
        let image_env = document["mcp_servers"]["company_imagegen"]["env"]
            .as_table()
            .unwrap();
        assert!(image_env.values().all(toml::Value::is_str));
        assert!(!image_env.contains_key("notify"));
        assert!(!image_env.contains_key("model_reasoning_effort"));

        remove_managed_config(&home.join("config.toml")).unwrap();
        let restored = std::fs::read_to_string(home.join("config.toml")).unwrap();
        let restored_document = restored.parse::<toml::Value>().unwrap();
        assert_eq!(restored_document["model"].as_str(), Some("official-model"));
        assert_eq!(restored_document["model_provider"].as_str(), Some("openai"));
        assert!(restored_document["notify"].is_array());
        assert_eq!(
            restored_document["windows"]["sandbox"].as_str(),
            Some("elevated")
        );
    }

    #[test]
    fn managed_config_recovers_when_codex_drops_the_end_marker() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let original = r#"model = "official-model"
model_provider = "openai"
notify = ["powershell", "-File", "notify.ps1"]

[windows]
sandbox = "elevated"
"#;
        std::fs::write(home.join("config.toml"), original).unwrap();
        write_managed_config(
            &home.join("config.toml"),
            Path::new("C:/Program Files/Codex++/codex-plus-plus.exe"),
            &profile(),
            "sk-enterprise-test",
        )
        .unwrap();

        let damaged = std::fs::read_to_string(home.join("config.toml"))
            .unwrap()
            .replace(END_MARKER, "");
        std::fs::write(home.join("config.toml"), damaged).unwrap();

        let mut updated_profile = profile();
        updated_profile.default_model = "gpt-5.6-sol".to_string();
        write_managed_config(
            &home.join("config.toml"),
            Path::new("C:/Program Files/Codex++/codex-plus-plus.exe"),
            &updated_profile,
            "sk-enterprise-test",
        )
        .unwrap();

        let repaired = std::fs::read_to_string(home.join("config.toml")).unwrap();
        assert_eq!(repaired.matches(BEGIN_MARKER).count(), 1);
        assert_eq!(repaired.matches(END_MARKER).count(), 1);
        let document = repaired.parse::<toml::Value>().unwrap();
        assert_eq!(document["model"].as_str(), Some("gpt-5.6-sol"));
        assert!(document["notify"].is_array());
        assert_eq!(document["windows"]["sandbox"].as_str(), Some("elevated"));

        remove_managed_config(&home.join("config.toml")).unwrap();
        let restored = std::fs::read_to_string(home.join("config.toml")).unwrap();
        let document = restored.parse::<toml::Value>().unwrap();
        assert_eq!(document["model"].as_str(), Some("official-model"));
        assert_eq!(document["model_provider"].as_str(), Some("openai"));
        assert!(document["notify"].is_array());
    }

    #[test]
    fn managed_config_recovers_when_codex_drops_both_markers() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        write_managed_config(
            &home.join("config.toml"),
            Path::new("C:/Program Files/Codex++/codex-plus-plus.exe"),
            &profile(),
            "sk-enterprise-test",
        )
        .unwrap();
        let damaged = std::fs::read_to_string(home.join("config.toml"))
            .unwrap()
            .replace(BEGIN_MARKER, "")
            .replace(END_MARKER, "");
        std::fs::write(home.join("config.toml"), damaged).unwrap();

        write_managed_config(
            &home.join("config.toml"),
            Path::new("C:/Program Files/Codex++/codex-plus-plus.exe"),
            &profile(),
            "sk-enterprise-test",
        )
        .unwrap();

        let repaired = std::fs::read_to_string(home.join("config.toml")).unwrap();
        assert_eq!(repaired.matches(BEGIN_MARKER).count(), 1);
        assert_eq!(repaired.matches(END_MARKER).count(), 1);
        repaired.parse::<toml::Value>().unwrap();
    }

    #[test]
    fn enterprise_configuration_errors_preserve_authenticated_ui_state() {
        let failure = enterprise_failure(&anyhow::anyhow!(
            "Enterprise configuration cannot be recovered because config.toml is invalid"
        ));
        assert_eq!(failure.code, "enterprise_configuration_repair_failed");
        assert!(failure.retryable);
    }

    #[test]
    fn profile_rejects_non_responses_protocol() {
        let mut invalid = profile();
        invalid.wire_api = "chat_completions".to_string();
        assert!(validate_profile(&invalid).is_err());
    }

    #[test]
    fn user_models_replace_launcher_policy_and_select_available_default() {
        let mut profile = profile();
        apply_user_models(
            &mut profile,
            vec!["gpt-5.5".to_string(), "gpt-5.6-sol".to_string()],
            &user("user-1", "Alice"),
        )
        .unwrap();
        assert_eq!(profile.default_model, "gpt-5.5");
        assert_eq!(profile.allowed_models, ["gpt-5.5", "gpt-5.6-sol"]);
    }

    #[test]
    fn user_models_keep_launcher_default_when_sub2_allows_it() {
        let mut profile = profile();
        apply_user_models(
            &mut profile,
            vec!["gpt-5.5".to_string(), "gpt-5.4-mini".to_string()],
            &user("user-1", "Alice"),
        )
        .unwrap();
        assert_eq!(profile.default_model, "gpt-5.4-mini");
        assert_eq!(profile.allowed_models, ["gpt-5.5", "gpt-5.4-mini"]);
    }

    #[test]
    fn user_model_preferences_are_isolated_and_override_launcher_default() {
        let temp = tempfile::tempdir().unwrap();
        let previous = set_preferences_path_for_tests(Some(temp.path().join("preferences.json")));
        let preferences = EnterprisePreferences {
            schema_version: preference_schema_version(),
            default_model_by_user_id: std::collections::BTreeMap::from([
                ("user-a".to_string(), "gpt-5.6-sol".to_string()),
                ("user-b".to_string(), "gpt-5.5".to_string()),
            ]),
        };
        save_preferences(&preferences).unwrap();

        let models = vec!["gpt-5.4-mini".to_string(), "gpt-5.6-sol".to_string()];
        let mut profile_a = profile();
        apply_user_models(&mut profile_a, models.clone(), &user("user-a", "Alice")).unwrap();
        assert_eq!(profile_a.default_model, "gpt-5.6-sol");
        assert_eq!(profile_a.display_name, "Alice");
        assert_eq!(profile_a.user_id.as_deref(), Some("user-a"));

        let mut profile_b = profile();
        apply_user_models(&mut profile_b, models, &user("user-b", "Bob")).unwrap();
        assert_eq!(profile_b.default_model, "gpt-5.4-mini");
        assert_eq!(profile_b.display_name, "Bob");
        assert_eq!(profile_b.user_id.as_deref(), Some("user-b"));
        set_preferences_path_for_tests(previous);
    }

    #[test]
    fn empty_username_falls_back_to_authenticated_user_id() {
        let mut profile = profile();
        apply_user_identity(&mut profile, &user(" user-42 ", "   "));
        assert_eq!(profile.display_name, "user-42");
        assert_eq!(profile.user_id.as_deref(), Some("user-42"));
    }

    #[test]
    fn models_endpoint_uses_gateway_version_prefix_once() {
        assert_eq!(
            models_url("https://api.ai.rydf-design.com/v1"),
            "https://api.ai.rydf-design.com/v1/models"
        );
        assert_eq!(
            models_url("https://api.example.test"),
            "https://api.example.test/v1/models"
        );
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
    fn group_policy_forbidden_is_not_treated_as_session_expiry() {
        assert!(!is_unauthorized(&anyhow::anyhow!(
            "request failed (HTTP 403, code group_access_denied)"
        )));
        assert!(is_unauthorized(&anyhow::anyhow!(
            "request failed (HTTP 401, code unauthorized)"
        )));
        assert!(is_unauthorized(&anyhow::anyhow!(
            "request failed (HTTP 403, code reauthentication_required)"
        )));
    }

    #[test]
    fn launcher_error_metadata_preserves_retryability() {
        let retryable = anyhow::Error::new(EnterpriseRequestError {
            status: 503,
            code: "subscription_progress_unavailable".to_string(),
            retryable: true,
            session_auth: true,
            message: "temporary".to_string(),
        });
        assert_eq!(
            enterprise_failure(&retryable),
            EnterpriseFailure {
                code: "subscription_progress_unavailable".to_string(),
                retryable: true,
            }
        );

        let forbidden = anyhow::Error::new(EnterpriseRequestError {
            status: 403,
            code: "subscription_required".to_string(),
            retryable: false,
            session_auth: true,
            message: "assign a subscription".to_string(),
        });
        assert!(!is_unauthorized(&forbidden));
        assert!(!enterprise_failure(&forbidden).retryable);

        let rejected_inference_key = anyhow::Error::new(EnterpriseRequestError {
            status: 401,
            code: "model_discovery_failed".to_string(),
            retryable: false,
            session_auth: false,
            message: "invalid key".to_string(),
        });
        assert!(!is_unauthorized(&rejected_inference_key));
    }

    #[test]
    fn enterprise_status_accepts_subscription_quota_windows() {
        let status: EnterpriseStatus = serde_json::from_value(json!({
            "user": {"id":"42", "displayName":"Alice", "balanceUsd":0},
            "usage": {"todayUsd":2, "monthUsd":12, "updatedAt":"2026-08-14T00:00:00Z"},
            "subscription": {
                "id":88,
                "groupId":10,
                "groupName":"Codex Pro",
                "expiresAt":"2026-09-14T00:00:00Z",
                "windows":[{"period":"monthly", "usedUsd":12, "limitUsd":50, "remainingUsd":38, "percentage":24, "resetsAt":"2026-09-01T00:00:00Z"}]
            },
            "key": {"alias":"codex-launcher", "rpm":0, "tpm":0, "models":[]}
        }))
        .unwrap();
        assert_eq!(status.subscription.group_id, 10);
        assert_eq!(status.subscription.windows[0].remaining_usd, 38.0);
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

    #[test]
    fn login_method_file_defaults_to_company_and_reads_official() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("enterprise-login-method.json");
        assert_eq!(read_login_method(&path), LOGIN_METHOD_COMPANY);
        std::fs::write(&path, r#"{"method":"official"}"#).unwrap();
        assert_eq!(read_login_method(&path), LOGIN_METHOD_OFFICIAL);
        std::fs::write(&path, r#"{"method":"company"}"#).unwrap();
        assert_eq!(read_login_method(&path), LOGIN_METHOD_COMPANY);
    }

    #[test]
    fn official_login_config_removes_enterprise_provider_and_restores_chatgpt_path() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        std::fs::write(
            home.join("auth.json"),
            br#"{"OPENAI_API_KEY":"sk-enterprise-test","auth_mode":"chatgpt","tokens":{"access_token":"keep"}}"#,
        )
        .unwrap();
        write_managed_config(
            &home.join("config.toml"),
            Path::new("C:/Program Files/Codex++/codex-plus-plus.exe"),
            &profile(),
            "sk-enterprise-test",
        )
        .unwrap();

        apply_official_login_config(home).unwrap();

        let config = std::fs::read_to_string(home.join("config.toml")).unwrap_or_default();
        assert!(!config.contains("company-ai"));
        assert!(!config.contains("company_imagegen"));
        assert!(!config.contains("CODEX_PLUS_IMAGE_BASE_URL"));
        assert!(!config.contains("model_provider"));
        assert!(!config.contains(BEGIN_MARKER));
        let auth: Value =
            serde_json::from_str(&std::fs::read_to_string(home.join("auth.json")).unwrap())
                .unwrap();
        assert!(auth.get("OPENAI_API_KEY").is_none());
        assert_eq!(auth["auth_mode"], "chatgpt");
        assert_eq!(auth["tokens"]["access_token"], "keep");
    }

    #[test]
    fn terminal_session_cleanup_removes_managed_provider_and_inference_key() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        std::fs::write(
            home.join("auth.json"),
            br#"{"OPENAI_API_KEY":"sk-enterprise-test","auth_mode":"chatgpt"}"#,
        )
        .unwrap();
        write_managed_config(
            &home.join("config.toml"),
            Path::new("C:/Program Files/Codex++/codex-plus-plus.exe"),
            &profile(),
            "sk-enterprise-test",
        )
        .unwrap();

        clear_enterprise_configuration(home, Some("sk-enterprise-test")).unwrap();

        let config = std::fs::read_to_string(home.join("config.toml")).unwrap_or_default();
        assert!(!config.contains(BEGIN_MARKER));
        assert!(!config.contains("experimental_bearer_token"));
        assert!(!config.contains("company_imagegen"));
        let auth: Value =
            serde_json::from_str(&std::fs::read_to_string(home.join("auth.json")).unwrap())
                .unwrap();
        assert!(auth.get("OPENAI_API_KEY").is_none());
        assert_eq!(auth["auth_mode"], "chatgpt");
    }

    #[test]
    fn enterprise_provisioning_preserves_a_different_official_api_key() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        std::fs::write(
            home.join("auth.json"),
            br#"{"OPENAI_API_KEY":"sk-official-user-key","auth_mode":"apikey"}"#,
        )
        .unwrap();

        write_managed_config(
            &home.join("config.toml"),
            Path::new("C:/Program Files/Codex++/codex-plus-plus.exe"),
            &profile(),
            "sk-enterprise-test",
        )
        .unwrap();
        clear_enterprise_configuration(home, Some("sk-enterprise-test")).unwrap();

        let auth: Value =
            serde_json::from_str(&std::fs::read_to_string(home.join("auth.json")).unwrap())
                .unwrap();
        assert_eq!(auth["OPENAI_API_KEY"], "sk-official-user-key");
        assert_eq!(auth["auth_mode"], "apikey");
    }

    #[test]
    fn enterprise_migration_removes_rotated_legacy_key_from_managed_block_only() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        std::fs::write(
            home.join("config.toml"),
            format!(
                "{BEGIN_MARKER}\nmodel_provider = \"company-ai\"\nmodel_providers.company-ai.base_url = \"https://example.com/v1\"\nmodel_providers.company-ai.experimental_bearer_token = \"sk-enterprise-old\"\n{END_MARKER}\n"
            ),
        )
        .unwrap();
        std::fs::write(
            home.join("auth.json"),
            br#"{"OPENAI_API_KEY":"sk-enterprise-old","auth_mode":"chatgpt","tokens":{"access_token":"keep"}}"#,
        )
        .unwrap();

        clear_enterprise_configuration(home, Some("sk-enterprise-new")).unwrap();

        let auth: Value =
            serde_json::from_str(&std::fs::read_to_string(home.join("auth.json")).unwrap())
                .unwrap();
        assert!(auth.get("OPENAI_API_KEY").is_none());
        assert_eq!(auth["tokens"]["access_token"], "keep");
        assert!(
            !std::fs::read_to_string(home.join("config.toml"))
                .unwrap()
                .contains(BEGIN_MARKER)
        );
    }

    #[test]
    fn enterprise_provisioning_does_not_create_auth_json() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        write_managed_config(
            &home.join("config.toml"),
            Path::new("C:/Program Files/Codex++/codex-plus-plus.exe"),
            &profile(),
            "sk-enterprise-test",
        )
        .unwrap();
        assert!(!home.join("auth.json").exists());
    }

    #[test]
    fn current_login_method_reads_overridden_path() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("enterprise-login-method.json");
        let previous = set_login_method_path_for_tests(Some(path));
        write_login_method(LOGIN_METHOD_OFFICIAL).unwrap();
        assert_eq!(current_login_method(), LOGIN_METHOD_OFFICIAL);
        write_login_method(LOGIN_METHOD_COMPANY).unwrap();
        assert_eq!(current_login_method(), LOGIN_METHOD_COMPANY);
        set_login_method_path_for_tests(previous);
    }
}

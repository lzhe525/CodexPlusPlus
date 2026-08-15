# Enterprise Sub2API Integration

## Current Architecture

Codex++ is a Rust workspace with a React/TypeScript Tauri manager. Provider profiles are represented by `RelayProfile` in `crates/codex-plus-core/src/settings.rs`, edited in `apps/codex-plus-manager/src/App.tsx`, and persisted by `SettingsStore` in the application settings JSON. A pure API profile currently carries `base_url`, `api_key`, `config_contents`, and `auth_contents`. Provider switching in `apps/codex-plus-manager/src-tauri/src/commands.rs` delegates to `crates/codex-plus-core/src/relay_config.rs`, which atomically writes `~/.codex/config.toml` and `~/.codex/auth.json`.

That path is retained for Community Mode compatibility, but it is not suitable for the enterprise credential because the API key is ordinary provider state and is materialized in `auth.json`.

The repository also contains Sub2API billing support, provider diagnostics, model discovery, protocol conversion, MCP/Skill/Plugin context handling, sessions, worktrees, and launcher enhancements. These unrelated capabilities remain unchanged.

## Target Architecture

```text
Sub2API user/auth and policy
        |
        | existing Launcher API contract
        v
Enterprise Codex++ (Tauri backend)
  - EnterpriseAuthSession
  - EnterpriseCredentialStore
  - EnterpriseProviderProvisioner
  - EnterpriseDiagnostics
        |
        | managed config.toml block + command-backed OS credential lookup
        v
Codex Desktop ---- Responses API ----> Sub2API gateway
```

The existing company Launcher API is the client-facing provisioning boundary. It authenticates against the real Sub2API endpoints (`/api/v1/auth/login`, `/auth/me`, `/keys`, `/usage/*`) and does not return the upstream member JWT to the desktop client. The desktop receives opaque launcher access/refresh tokens and a restricted inference credential.

This fork ships in Enterprise Mode by default. `CODEX_PLUS_ENTERPRISE_MODE=0` restores the upstream-compatible Community Mode for development or upstream synchronization. In Enterprise Mode, the login screen offers company account or original official ChatGPT login. Choosing original account login removes the enterprise-managed config, restores the default official ChatGPT login path, and shows the community Provider page. After a company login, the Provider page becomes Azalea Plugin for Codex, with a switch back to original account login.

Production uses the single configured Launcher API origin `https://api.ai.rydf-design.com`. Development may override it with `CODEX_PLUS_ENTERPRISE_URL`; provisioning validates that the returned gateway is an absolute URL without embedded credentials, query, or fragment, and production requires HTTPS.

## Module Mapping

| Existing module | Decision | Enterprise responsibility |
| --- | --- | --- |
| `settings.rs` / `RelayProfile` | Preserve | Community providers remain intact; enterprise secrets are never inserted into profiles. |
| `relay_config.rs` | Preserve | Community switching remains intact. Enterprise provisioning uses a separately marked managed TOML block. |
| Tauri `commands.rs` | Extend | Expose login, restore, refresh, diagnostics, logout, and provisioning commands. |
| React `App.tsx` | Extend | Render a company-login/account entry without exposing Base URL, JWT, or API-key input fields. |
| `sub2api.rs` | Preserve | Existing billing helper remains for Community providers. |
| MCP/Skills/Plugins | Preserve | No schema or behavior change. |
| Session/Worktree/Context/Goals | Preserve | No schema or behavior change. |
| New `enterprise.rs` | Add | Launcher client, auth state, OS credential storage, managed provider configuration, and diagnostics. |

## Subscription Group Policy

Administrators create subscription-type OpenAI groups, configure their available models and daily, weekly, or monthly quota windows, and directly assign subscriptions to users in Sub2API.

Launcher never trusts a group ID supplied by the desktop client. For every authenticated profile or key-provisioning request it reads `/api/v1/groups/available`, keeps active OpenAI groups whose `subscription_type` is `subscription`, and requires exactly one result. Sub2API already limits that endpoint to subscription groups for which the user has an active subscription. `department`, `allowed_groups`, group-name matching, and `CODEX_GROUP_ID` do not participate in authorization. Missing and ambiguous assignments fail closed with an explicit error.

The resolved group ID is applied to the user's `codex-launcher` key. If an administrator changes the assigned subscription or deletes and recreates a group, Launcher rotates a stale key whose group ID no longer matches. Sub2API checks the active subscription and quota windows again on every inference request, so the managed key does not need to expire with the subscription.

## Security Model

- Passwords exist only in the login request and React component state; they are cleared after each attempt and are never persisted.
- The upstream Sub2API JWT remains inside the Launcher API service. The client stores only opaque Launcher access/refresh tokens.
- On Windows, session, refresh, and inference credentials are stored as Generic Credentials in Windows Credential Manager; macOS uses Keychain and Linux uses Secret Service (`secret-tool`). They are not stored in `settings.json`, provider JSON, LocalStorage, or logs.
- Enterprise login points `config.toml` directly to the Sub2AI inference gateway. Provider authentication invokes the adjacent `codex-plus-image-mcp` helper, which reads the inference credential from the OS credential manager only while company login is active. The credential is never written to `config.toml` or `auth.json`.
- Provisioning removes a legacy `OPENAI_API_KEY` only when its value exactly matches the current enterprise credential. Existing official ChatGPT tokens and unrelated user API keys are preserved.
- Logout clears the opaque session, refresh token, inference credential, user cache, and the managed config block. Community profiles are preserved.
- Diagnostics return booleans and sanitized messages only. Passwords, authorization headers, JWTs, and full keys are excluded.
- The client is a UX and local-secret boundary. Model permission, quota, concurrency, rate limits, account routing, and credential validity remain server-side Sub2API/Launcher policy.
- A local administrator or process running as the user can still inspect process memory or invoke the credential helper. This is an unavoidable desktop-client boundary; server-side revocation remains authoritative.

## Provisioning and Migration

1. The company-login entry attempts session restore from OS credentials; Codex startup itself is not gated by enterprise authentication.
2. Login calls the existing Launcher API, stores opaque tokens, resolves the user's unique active OpenAI subscription group, obtains the server-controlled Codex profile, and calls `codex-key/ensure`.
3. The restricted credential is stored in the OS credential manager.
4. A marked `company-ai` block is atomically merged into `~/.codex/config.toml`; existing configuration is preserved and root model selections are restorable.
5. Codex invokes `codex-plus-image-mcp --enterprise-credential get` through provider `auth.command`, then sends Responses requests directly to the Sub2AI gateway. The helper refuses credential output while official login mode is active.
6. Logout removes only the managed block and enterprise credentials. Existing community providers are not deleted or uploaded.
7. Community Mode continues using the original Provider manager, allowing future upstream merges with a contained enterprise module.

The current implementation uses one managed `codex-launcher` key per user and automatically rotates that key when the resolved subscription group changes. The Manager displays the current Sub2API user ID, subscription expiration, and each configured quota window's used, total, remaining, reset time, and percentage. Account balance, RPM/TPM, and key internals are hidden from ordinary users. A user's default model is stored locally by Sub2API user ID and is accepted only while it remains in the live `/v1/models` result.

Per-device Sub2API keys, native Device Tokens, cross-device model-preference sync, multiple simultaneous Codex subscriptions, admin device management, and MCP/Skill policy distribution remain future work.

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
        | managed config.toml block + command-backed auth
        v
Codex Desktop ---- Responses API ----> Sub2API gateway
```

The existing company Launcher API is the client-facing provisioning boundary. It authenticates against the real Sub2API endpoints (`/api/v1/auth/login`, `/auth/me`, `/keys`, `/usage/*`) and does not return the upstream member JWT to the desktop client. The desktop receives opaque launcher access/refresh tokens and a restricted inference credential.

This fork ships in Enterprise Mode by default. `CODEX_PLUS_ENTERPRISE_MODE=0` restores the upstream-compatible Community Mode for development or upstream synchronization. In Enterprise Mode, the login screen offers company account or original official ChatGPT login. Choosing original account login removes the enterprise-managed config, restores the default official ChatGPT login path, and shows the community Provider page. After a company login, the Provider page becomes Company Account, with a switch back to original account login.

Production uses the single configured Launcher API origin `https://api.ai.rydf-design.com`. Development may override it with `CODEX_PLUS_ENTERPRISE_URL`; provisioning validates that the returned gateway is an absolute URL without embedded credentials, query, or fragment, and production requires HTTPS.

## Module Mapping

| Existing module | Decision | Enterprise responsibility |
| --- | --- | --- |
| `settings.rs` / `RelayProfile` | Preserve | Community providers remain intact; enterprise secrets are never inserted into profiles. |
| `relay_config.rs` | Preserve | Community switching remains intact. Enterprise provisioning uses a separately marked managed TOML block. |
| Tauri `commands.rs` | Extend | Expose login, restore, refresh, diagnostics, logout, and provisioning commands. |
| React `App.tsx` | Extend | Gate startup in Enterprise Mode and render login/account UI without Base URL, JWT, or API key. |
| `sub2api.rs` | Preserve | Existing billing helper remains for Community providers. |
| MCP/Skills/Plugins | Preserve | No schema or behavior change. |
| Session/Worktree/Context/Goals | Preserve | No schema or behavior change. |
| New `enterprise.rs` | Add | Launcher client, auth state, OS credential storage, managed provider configuration, and diagnostics. |

## Security Model

- Passwords exist only in the login request and React component state; they are cleared after each attempt and are never persisted.
- The upstream Sub2API JWT remains inside the Launcher API service. The client stores only opaque Launcher access/refresh tokens.
- On Windows, session, refresh, and inference credentials are stored as Generic Credentials in Windows Credential Manager; macOS uses Keychain and Linux uses Secret Service (`secret-tool`). They are not stored in `settings.json`, provider JSON, LocalStorage, logs, or `auth.json`.
- `config.toml` contains a managed `company-ai` provider that points to the Codex++ loopback proxy. The proxy reads the inference credential from the OS credential store and adds the upstream Bearer header; the credential is never written to `config.toml`, `auth.json`, or application settings.
- Logout clears the opaque session, refresh token, inference credential, user cache, and the managed config block. Community profiles are preserved.
- Diagnostics return booleans and sanitized messages only. Passwords, authorization headers, JWTs, and full keys are excluded.
- The client is a UX and local-secret boundary. Model permission, quota, concurrency, rate limits, account routing, and credential validity remain server-side Sub2API/Launcher policy.
- A local administrator or process running as the user can still inspect process memory or invoke the credential helper. This is an unavoidable desktop-client boundary; server-side revocation remains authoritative.

## Provisioning and Migration

1. Enterprise Mode starts in `unauthenticated` and attempts session restore from OS credentials.
2. Login calls the existing Launcher API, stores opaque tokens, obtains the server-controlled Codex profile, and calls `codex-key/ensure`.
3. The restricted credential is stored in the OS credential manager.
4. A marked `company-ai` block is atomically merged into `~/.codex/config.toml`; existing configuration is preserved and root model selections are restorable.
5. Codex reads the key with the command-backed credential provider and sends Responses requests to the approved gateway.
6. Logout removes only the managed block and enterprise credentials. Existing community providers are not deleted or uploaded.
7. Community Mode continues using the original Provider manager, allowing future upstream merges with a contained enterprise module.

The current MVP uses the existing Launcher API's one-key-per-user policy. Per-device Sub2API keys, native Device Tokens, server-downloaded Client Policy, credential rotation, admin device management, and MCP/Skill policy distribution remain future work.

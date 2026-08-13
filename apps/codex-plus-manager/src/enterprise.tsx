import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState, type FormEvent } from "react";
import { CheckCircle2, CircleX, KeyRound, LogOut, RefreshCw, ShieldCheck } from "lucide-react";
import { t } from "@/i18n";

export type EnterpriseSnapshot = {
  enabled: boolean;
  state: "disabled" | "unauthenticated" | "authenticating" | "authenticated" | "expired" | "error" | "official" | string;
  user: { id: string; displayName: string; balanceUsd?: number | null } | null;
  status: {
    usage: { todayUsd: number; monthUsd: number; budgetUsd?: number | null; remainingUsd?: number | null };
    key: { alias: string; expiresAt?: string | null; rpm: number; tpm: number; models: string[] };
  } | null;
  profile: { gatewayUrl: string; providerId: string; defaultModel: string; allowedModels: string[]; wireApi: string; displayName: string } | null;
  credentialAvailable: boolean;
  configManaged: boolean;
  loginMethod?: "company" | "official" | string;
  message: string;
};

type EnterpriseDiagnostics = {
  server: boolean;
  login: boolean;
  credential: boolean;
  model: boolean;
  responsesApi: boolean;
  codexConfiguration: boolean;
  message: string;
};

type CommandResult<T> = T & { status: string; message: string };

export function isEnterpriseOfficialLogin(snapshot: EnterpriseSnapshot | null | undefined): boolean {
  if (!snapshot?.enabled || snapshot.state === "authenticated") return false;
  return snapshot.state === "official" || snapshot.loginMethod === "official";
}

export function isEnterpriseCompanyAuthenticated(snapshot: EnterpriseSnapshot | null | undefined): boolean {
  return !!snapshot?.enabled && snapshot.state === "authenticated";
}

export function useEnterpriseAuth() {
  const [snapshot, setSnapshot] = useState<EnterpriseSnapshot | null>(null);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState("");

  const run = async (command: string, args?: Record<string, unknown>) => {
    setBusy(true);
    setError("");
    try {
      const result = await invoke<CommandResult<EnterpriseSnapshot>>(command, args);
      setSnapshot(result);
      if (result.status !== "ok") setError(result.message);
      return result;
    } catch (reason) {
      setError(String(reason));
      return null;
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    void run("enterprise_restore");
  }, []);

  return {
    snapshot,
    busy,
    error,
    login: (email: string, password: string) => run("enterprise_login", { request: { email, password } }),
    refresh: () => run("enterprise_refresh"),
    logout: () => run("enterprise_logout"),
    useOfficialLogin: () => run("enterprise_use_official_login"),
    useCompanyLogin: () => run("enterprise_use_company_login"),
  };
}

export function EnterpriseLoginMethodPicker({
  auth,
  active,
  compact = false,
}: {
  auth: ReturnType<typeof useEnterpriseAuth>;
  active: "company" | "official";
  compact?: boolean;
}) {
  return (
    <section className={`enterprise-method-picker ${compact ? "compact" : ""}`}>
      <div className="enterprise-method-picker-head">
        <strong>{t("选择登录方式")}</strong>
        {compact ? null : (
          <span>{t("选择登录方式后，可使用公司账号或恢复原来的官方 ChatGPT 账号登录。")}</span>
        )}
      </div>
      <div className="enterprise-login-methods" role="tablist" aria-label={t("选择登录方式")}>
        <button
          className={`enterprise-login-method ${active === "company" ? "active" : ""}`}
          disabled={auth.busy}
          onClick={() => {
            if (active !== "company") void auth.useCompanyLogin();
          }}
          type="button"
        >
          <ShieldCheck />
          <strong>{t("公司账号登录")}</strong>
          <span>{t("Sub2API 企业账号")}</span>
        </button>
        <button
          className={`enterprise-login-method ${active === "official" ? "active" : ""}`}
          disabled={auth.busy}
          onClick={() => {
            if (active !== "official") void auth.useOfficialLogin();
          }}
          type="button"
        >
          <KeyRound />
          <strong>{t("原账号登录")}</strong>
          <span>{t("官方 ChatGPT 账号")}</span>
        </button>
      </div>
    </section>
  );
}

export function EnterpriseLogin({ auth }: { auth: ReturnType<typeof useEnterpriseAuth> }) {
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [method, setMethod] = useState<"company" | "official">("company");

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    const attempt = password;
    setPassword("");
    await auth.login(email, attempt);
  };

  return (
    <div className="enterprise-gate">
      <form className="enterprise-login-card" onSubmit={submit}>
        <div className="enterprise-mark"><ShieldCheck /></div>
        <h1>{t("选择登录方式")}</h1>
        <p>{t("选择登录方式后，可使用公司账号或恢复原来的官方 ChatGPT 账号登录。")}</p>
        <div className="enterprise-login-methods" role="tablist" aria-label={t("选择登录方式")}>
          <button
            className={`enterprise-login-method ${method === "company" ? "active" : ""}`}
            onClick={() => setMethod("company")}
            type="button"
          >
            <ShieldCheck />
            <strong>{t("公司账号登录")}</strong>
            <span>{t("Sub2API 企业账号")}</span>
          </button>
          <button
            className={`enterprise-login-method ${method === "official" ? "active" : ""}`}
            onClick={() => setMethod("official")}
            type="button"
          >
            <KeyRound />
            <strong>{t("原账号登录")}</strong>
            <span>{t("官方 ChatGPT 账号")}</span>
          </button>
        </div>
        {method === "company" ? (
          <>
            <p>使用公司分配的 Sub2API 账号登录。客户端会自动配置 Company AI，不需要 API Key。</p>
            <label>
              <span>公司账号</span>
              <input autoComplete="username" disabled={auth.busy} onChange={(event) => setEmail(event.currentTarget.value)} required type="email" value={email} />
            </label>
            <label>
              <span>密码</span>
              <input autoComplete="current-password" disabled={auth.busy} onChange={(event) => setPassword(event.currentTarget.value)} required type="password" value={password} />
            </label>
            {auth.error ? <div className="enterprise-error">{auth.error}</div> : null}
            <button className="primary" disabled={auth.busy} type="submit">{auth.busy ? "正在连接…" : "登录"}</button>
            <small>密码不会保存在本机；会话和推理凭据由 Windows Credential Manager 保护。</small>
          </>
        ) : (
          <>
            <p>{t("使用 ChatGPT / Codex 官方账号。将移除企业托管配置，并恢复默认官方登录途径。")}</p>
            {auth.error ? <div className="enterprise-error">{auth.error}</div> : null}
            <button
              className="primary"
              disabled={auth.busy}
              onClick={() => void auth.useOfficialLogin()}
              type="button"
            >
              {auth.busy ? t("正在恢复…") : t("进入并恢复官方登录")}
            </button>
            <small>{t("之后可在供应商配置中继续使用官方登录，或随时切回公司账号。")}</small>
          </>
        )}
      </form>
    </div>
  );
}

export function EnterpriseAccount({ auth }: { auth: ReturnType<typeof useEnterpriseAuth> }) {
  const [diagnostics, setDiagnostics] = useState<EnterpriseDiagnostics | null>(null);
  const [checking, setChecking] = useState(false);
  const snapshot = auth.snapshot!;
  const checks = diagnostics ? [
    ["Server", diagnostics.server], ["Login", diagnostics.login], ["Credential", diagnostics.credential],
    ["Model", diagnostics.model], ["Responses API", diagnostics.responsesApi], ["Codex Configuration", diagnostics.codexConfiguration],
  ] as const : [];
  const diagnose = async () => {
    setChecking(true);
    try { setDiagnostics(await invoke<CommandResult<EnterpriseDiagnostics>>("enterprise_diagnostics")); }
    finally { setChecking(false); }
  };

  return (
    <div className="enterprise-account">
      <EnterpriseLoginMethodPicker auth={auth} active="company" />
      <div className="enterprise-account-head">
        <div><p className="eyebrow">COMPANY ACCOUNT</p><h2>{snapshot.user?.displayName || "Company user"}</h2><p>{snapshot.message}</p></div>
        <span className="enterprise-connected"><CheckCircle2 /> 已登录</span>
      </div>
      <div className="enterprise-metrics">
        <article><span>服务</span><strong>Connected</strong><small>{snapshot.profile?.displayName || "Company AI"}</small></article>
        <article><span>默认模型</span><strong>{snapshot.profile?.defaultModel || "—"}</strong><small>{snapshot.profile?.allowedModels.length || 0} 个授权模型</small></article>
        <article><span>本月用量</span><strong>${(snapshot.status?.usage.monthUsd || 0).toFixed(2)}</strong><small>今日 ${(snapshot.status?.usage.todayUsd || 0).toFixed(2)}</small></article>
        <article><span>凭据</span><strong>{snapshot.credentialAvailable ? "Protected" : "Missing"}</strong><small>{snapshot.configManaged ? "Codex 已配置" : "配置待修复"}</small></article>
      </div>
      <section className="enterprise-models"><h3>授权模型</h3><div>{snapshot.profile?.allowedModels.map((model) => <span key={model}>{model}</span>)}</div></section>
      <div className="enterprise-actions">
        <button onClick={() => void auth.refresh()} disabled={auth.busy}><RefreshCw /> 刷新并修复</button>
        <button onClick={() => void diagnose()} disabled={checking}><ShieldCheck /> {checking ? "诊断中…" : "连接诊断"}</button>
        <button onClick={() => void auth.useOfficialLogin()} disabled={auth.busy}><KeyRound /> {t("切换到原账号登录")}</button>
        <button className="danger" onClick={() => void auth.logout()} disabled={auth.busy}><LogOut /> 退出并移除此设备</button>
      </div>
      {checks.length ? <section className="enterprise-diagnostics"><h3>Enterprise Diagnostics</h3>{checks.map(([label, ok]) => <div key={label}>{ok ? <CheckCircle2 /> : <CircleX />}<span>{label}</span></div>)}</section> : null}
      {auth.error ? <div className="enterprise-error">{auth.error}</div> : null}
    </div>
  );
}

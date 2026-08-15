import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { CheckCircle2, CircleX, KeyRound, LogOut, RefreshCw, Save, ShieldCheck } from "lucide-react";
import { shouldPreserveAuthenticatedSnapshot } from "@/enterprise-state";
import { t } from "@/i18n";

export const ENTERPRISE_REFRESH_MAX_AGE_MS = 60_000;

export type EnterpriseSnapshot = {
  enabled: boolean;
  state: "disabled" | "unauthenticated" | "authenticating" | "authenticated" | "expired" | "error" | "official" | string;
  user: { id: string; displayName: string; balanceUsd?: number | null } | null;
  status: {
    usage: { todayUsd: number; monthUsd: number; budgetUsd?: number | null; remainingUsd?: number | null };
    subscription: {
      id: number;
      groupId: number;
      groupName: string;
      expiresAt: string;
      windows: Array<{
        period: "daily" | "weekly" | "monthly" | string;
        usedUsd: number;
        limitUsd: number;
        remainingUsd: number;
        percentage: number;
        resetsAt: string;
      }>;
    };
    key: { alias: string; expiresAt?: string | null; rpm: number; tpm: number; models: string[] };
  } | null;
  profile: {
    gatewayUrl: string;
    providerId: string;
    defaultModel: string;
    allowedModels: string[];
    wireApi: string;
    displayName: string;
    userId?: string | null;
    groupId?: number | null;
    groupName?: string | null;
  } | null;
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
type EnterpriseCommandResult<T> = T & { outcome: string; message: string; code?: string | null; retryable?: boolean };

export function formatEnterpriseUsd(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) return "--";
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }).format(value);
}

export function formatEnterpriseDate(value: string | null | undefined): string {
  if (!value) return "--";
  const date = new Date(value);
  if (!Number.isFinite(date.getTime())) return "--";
  return new Intl.DateTimeFormat("zh-CN", { dateStyle: "medium", timeStyle: "short" }).format(date);
}

export function enterpriseQuotaPeriodLabel(period: string): string {
  return ({ daily: "每日", weekly: "每周", monthly: "每月" } as Record<string, string>)[period] ?? "当前";
}

export function isEnterpriseOfficialLogin(snapshot: EnterpriseSnapshot | null | undefined): boolean {
  if (!snapshot?.enabled || snapshot.state === "authenticated") return false;
  return snapshot.state === "official" || snapshot.loginMethod === "official";
}

export function isEnterpriseCompanyAuthenticated(snapshot: EnterpriseSnapshot | null | undefined): boolean {
  return !!snapshot?.enabled && snapshot.state === "authenticated";
}

export function useEnterpriseAuth() {
  const [snapshot, setSnapshotState] = useState<EnterpriseSnapshot | null>(null);
  const [busy, setBusyState] = useState(true);
  const [error, setError] = useState("");
  const [enterpriseEnabled, setEnterpriseEnabled] = useState(false);
  const snapshotRef = useRef<EnterpriseSnapshot | null>(null);
  const busyRef = useRef(true);
  const lastUpdatedAtRef = useRef(0);

  const setSnapshot = useCallback((next: EnterpriseSnapshot | null) => {
    snapshotRef.current = next;
    setSnapshotState(next);
  }, []);

  const setBusy = useCallback((next: boolean) => {
    busyRef.current = next;
    setBusyState(next);
  }, []);

  const run = useCallback(async (
    command: string,
    args?: Record<string, unknown>,
    options?: { clearSnapshot?: boolean; preserveAuthenticatedSnapshotOnError?: boolean },
  ) => {
    setBusy(true);
    setError("");
    if (options?.clearSnapshot) setSnapshot(null);
    try {
      const result = await invoke<EnterpriseCommandResult<EnterpriseSnapshot>>(command, args);
      setEnterpriseEnabled(result.enabled);
      if (result.outcome === "ok") {
        setSnapshot(result);
        lastUpdatedAtRef.current = Date.now();
      } else {
        setError(result.message);
        if (!shouldPreserveAuthenticatedSnapshot(
          result,
          snapshotRef.current?.state,
          options?.preserveAuthenticatedSnapshotOnError === true,
        )) {
          setSnapshot(result);
        }
      }
      return result;
    } catch (reason) {
      setError(String(reason));
      setSnapshot(null);
      return null;
    } finally {
      setBusy(false);
    }
  }, [setBusy, setSnapshot]);

  const preserveAccount = { preserveAuthenticatedSnapshotOnError: true } as const;
  const refreshData = useCallback(() => run("enterprise_reload", undefined, preserveAccount), [run]);
  const refreshIfStale = useCallback(() => {
    if (
      snapshotRef.current?.state !== "authenticated"
      || busyRef.current
      || Date.now() - lastUpdatedAtRef.current < ENTERPRISE_REFRESH_MAX_AGE_MS
    ) return Promise.resolve(null);
    return refreshData();
  }, [refreshData]);

  useEffect(() => {
    void run("enterprise_restore");
  }, [run]);

  useEffect(() => {
    const refreshOnFocus = () => void refreshIfStale();
    window.addEventListener("focus", refreshOnFocus);
    return () => window.removeEventListener("focus", refreshOnFocus);
  }, [refreshIfStale]);

  return {
    snapshot,
    enterpriseEnabled,
    busy,
    error,
    login: (email: string, password: string) => run("enterprise_login", { request: { email, password } }, { clearSnapshot: true }),
    refreshData,
    refreshIfStale,
    refreshAndRepair: () => run("enterprise_refresh", undefined, preserveAccount),
    setDefaultModel: (model: string) => run("enterprise_set_default_model", { model }, preserveAccount),
    logout: () => run("enterprise_logout", undefined, { clearSnapshot: true }),
    useOfficialLogin: () => run("enterprise_use_official_login", undefined, { clearSnapshot: true }),
    useCompanyLogin: () => run("enterprise_use_company_login", undefined, { clearSnapshot: true }),
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
        <h1>Azalea Plugin for Codex</h1>
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
            <p>使用公司分配的 Sub2API 账号登录。客户端会自动配置 Azalea Plugin for Codex，不需要 API Key。</p>
            <label>
              <span>公司账号</span>
              <input autoComplete="username" disabled={auth.busy} onChange={(event) => setEmail(event.currentTarget.value)} required type="email" value={email} />
            </label>
            <label>
              <span>密码</span>
              <input autoComplete="current-password" disabled={auth.busy} onChange={(event) => setPassword(event.currentTarget.value)} required type="password" value={password} />
            </label>
            {auth.error ? <div className="enterprise-error" role="alert">{auth.error}</div> : null}
            <button className="primary" disabled={auth.busy} type="submit">{auth.busy ? "正在连接…" : "登录"}</button>
            <small>密码不会保存在本机；会话和推理凭据由 Windows Credential Manager 保护。</small>
          </>
        ) : (
          <>
            <p>{t("使用 ChatGPT / Codex 官方账号。将移除企业托管配置，并恢复默认官方登录途径。")}</p>
            {auth.error ? <div className="enterprise-error" role="alert">{auth.error}</div> : null}
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
  const [selectedModel, setSelectedModel] = useState(auth.snapshot?.profile?.defaultModel ?? "");
  const [saveMessage, setSaveMessage] = useState("");
  const snapshot = auth.snapshot!;
  const allowedModels = snapshot.profile?.allowedModels ?? [];
  const userId = snapshot.profile?.userId || snapshot.user?.id || "--";
  const subscription = snapshot.status?.subscription;
  const quotaWindows = subscription?.windows ?? [];
  const checks = diagnostics ? [
    ["Server", diagnostics.server], ["Login", diagnostics.login], ["Credential", diagnostics.credential],
    ["Model", diagnostics.model], ["Responses API", diagnostics.responsesApi], ["Codex Configuration", diagnostics.codexConfiguration],
  ] as const : [];

  useEffect(() => {
    setSelectedModel(snapshot.profile?.defaultModel ?? "");
    setSaveMessage("");
  }, [snapshot.profile?.defaultModel, snapshot.profile?.userId, snapshot.user?.id]);

  const diagnose = async () => {
    setChecking(true);
    try { setDiagnostics(await invoke<CommandResult<EnterpriseDiagnostics>>("enterprise_diagnostics")); }
    finally { setChecking(false); }
  };

  const saveDefaultModel = async () => {
    setSaveMessage("");
    const result = await auth.setDefaultModel(selectedModel);
    if (result?.outcome === "ok") setSaveMessage("默认模型已保存");
  };

  return (
    <div className="enterprise-account">
      <EnterpriseLoginMethodPicker auth={auth} active="company" />
      <div className="enterprise-account-head">
        <div>
          <p className="eyebrow">AZALEA PLUGIN FOR CODEX</p>
          <h2>{snapshot.user?.displayName || "Azalea 用户"}</h2>
          <p className="enterprise-user-id">Sub2AI 用户 ID <code>{userId}</code></p>
          {snapshot.profile?.groupName ? <p className="enterprise-group">{snapshot.profile.groupName}</p> : null}
          {subscription?.expiresAt ? <p className="enterprise-subscription-expiry">订阅有效期至 {formatEnterpriseDate(subscription.expiresAt)}</p> : null}
        </div>
        <span className="enterprise-connected"><CheckCircle2 /> 已登录</span>
      </div>
      <div className="enterprise-metrics">
        {quotaWindows.map((window) => (
          <article className="enterprise-quota-metric" key={window.period}>
            <span>{enterpriseQuotaPeriodLabel(window.period)}剩余额度</span>
            <strong>{formatEnterpriseUsd(window.remainingUsd)}</strong>
            <small>已用 {formatEnterpriseUsd(window.usedUsd)} / 总额 {formatEnterpriseUsd(window.limitUsd)}</small>
            <div
              aria-label={`${enterpriseQuotaPeriodLabel(window.period)}额度使用进度`}
              aria-valuemax={100}
              aria-valuemin={0}
              aria-valuenow={Math.round(window.percentage)}
              className="enterprise-quota-progress"
              role="progressbar"
            >
              <i style={{ width: `${Math.min(100, Math.max(0, window.percentage))}%` }} />
            </div>
            <small>{formatEnterpriseDate(window.resetsAt)} 重置</small>
          </article>
        ))}
        {quotaWindows.length === 0 ? (
          <article>
            <span>订阅状态</span>
            <strong>有效</strong>
            <small>当前套餐未设置金额额度窗口</small>
          </article>
        ) : null}
        <article className="enterprise-model-metric">
          <span>默认模型</span>
          <div className="enterprise-model-control">
            <select
              aria-label="默认模型"
              disabled={auth.busy || allowedModels.length === 0}
              onChange={(event) => {
                setSelectedModel(event.currentTarget.value);
                setSaveMessage("");
              }}
              value={selectedModel}
            >
              {allowedModels.map((model) => <option key={model} value={model}>{model}</option>)}
            </select>
            <button
              disabled={auth.busy || !selectedModel || selectedModel === snapshot.profile?.defaultModel}
              onClick={() => void saveDefaultModel()}
              type="button"
            >
              <Save /> {auth.busy ? "保存中…" : "保存"}
            </button>
          </div>
          <small>{saveMessage || `${allowedModels.length} 个可用模型`}</small>
        </article>
      </div>
      <div className="enterprise-actions">
        <button onClick={() => void auth.refreshData()} disabled={auth.busy}><RefreshCw /> {auth.busy ? "刷新中…" : "刷新数据"}</button>
        <button onClick={() => void auth.refreshAndRepair()} disabled={auth.busy}><ShieldCheck /> 刷新并修复</button>
        <button onClick={() => void diagnose()} disabled={checking}><ShieldCheck /> {checking ? "诊断中…" : "连接诊断"}</button>
        <button onClick={() => void auth.useOfficialLogin()} disabled={auth.busy}><KeyRound /> {t("切换到原账号登录")}</button>
        <button className="danger" onClick={() => void auth.logout()} disabled={auth.busy}><LogOut /> 退出并移除此设备</button>
      </div>
      {checks.length ? <section className="enterprise-diagnostics"><h3>连接诊断</h3>{checks.map(([label, ok]) => <div key={label}>{ok ? <CheckCircle2 /> : <CircleX />}<span>{label}</span></div>)}</section> : null}
      {auth.error ? <div className="enterprise-error" role="alert">{auth.error}</div> : null}
    </div>
  );
}

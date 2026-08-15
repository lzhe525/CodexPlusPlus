import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { shouldPreserveAuthenticatedSnapshot } from "./enterprise-state.ts";

const enterprise = readFileSync(new URL("./enterprise.tsx", import.meta.url), "utf8");
const app = readFileSync(new URL("./App.tsx", import.meta.url), "utf8");
const chrome = readFileSync(new URL("./components/WindowChrome.tsx", import.meta.url), "utf8");
const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

test("enterprise branding and account metrics match the Azalea contract", () => {
  assert.doesNotMatch(`${enterprise}\n${app}`, /Company (?:AI|Account|Codex)/);
  assert.match(enterprise, /Azalea Plugin for Codex/);
  assert.match(enterprise, /Sub2AI 用户 ID/);
  for (const label of ["剩余额度", "已用", "总额", "默认模型", "订阅有效期至"]) {
    assert.match(enterprise, new RegExp(label));
  }
  assert.doesNotMatch(enterprise, /账户余额|本自然月累计|今日累计/);
  assert.doesNotMatch(enterprise, />RPM<|>TPM</);
});

test("enterprise refresh and model commands remain distinct", () => {
  assert.match(enterprise, /refreshData:?[\s\S]*enterprise_reload/);
  assert.match(enterprise, /refreshAndRepair:[\s\S]*enterprise_refresh/);
  assert.match(enterprise, /enterprise_set_default_model/);
  assert.match(enterprise, /ENTERPRISE_REFRESH_MAX_AGE_MS = 60_000/);
  assert.match(enterprise, /clearSnapshot: true/);
  assert.match(enterprise, /shouldPreserveAuthenticatedSnapshot/);
});

test("authenticated snapshot is preserved only for explicitly retryable failures", () => {
  assert.equal(shouldPreserveAuthenticatedSnapshot({ outcome: "failed", retryable: true }, "authenticated", true), true);
  assert.equal(shouldPreserveAuthenticatedSnapshot({ outcome: "failed", retryable: false }, "authenticated", true), false);
  assert.equal(shouldPreserveAuthenticatedSnapshot({ outcome: "failed", retryable: true }, "expired", true), false);
  assert.equal(shouldPreserveAuthenticatedSnapshot({ outcome: "failed", retryable: true }, "authenticated", false), false);
  assert.equal(shouldPreserveAuthenticatedSnapshot({ outcome: "ok", retryable: true }, "authenticated", true), false);
});

test("window chrome has Windows Tauri controls and a native-chrome fallback", () => {
  for (const method of ["minimize", "toggleMaximize", "close"]) {
    assert.match(chrome, new RegExp(`\\.${method}\\(`));
  }
  assert.match(chrome, /__TAURI_INTERNALS__/);
  assert.match(chrome, /Windows/);
  assert.match(chrome, /if \(!tauri\) return null/);
  assert.match(chrome, /custom-window-chrome/);
  assert.match(chrome, /data-tauri-drag-region/);
  assert.match(styles, /\.window-chrome-controls \.window-chrome-close:hover/);
  assert.match(styles, /\.window-maximized \.shell/);
  assert.match(styles, /\.shell:has\(> \.window-chrome\)/);
});

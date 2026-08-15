import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Square, X } from "lucide-react";
import { useCallback, useEffect, useState, type MouseEvent } from "react";

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function isWindowsRuntime(): boolean {
  return isTauriRuntime() && /Windows/i.test(window.navigator.userAgent);
}

export function WindowChrome({ title = "Codex++ Manager" }: { title?: string }) {
  const [maximized, setMaximized] = useState(false);
  const tauri = isWindowsRuntime();

  const syncMaximized = useCallback(async () => {
    if (!tauri) return;
    try {
      setMaximized(await getCurrentWindow().isMaximized());
    } catch {
      setMaximized(false);
    }
  }, [tauri]);

  useEffect(() => {
    document.documentElement.classList.toggle("window-maximized", maximized);
    return () => document.documentElement.classList.remove("window-maximized");
  }, [maximized]);

  useEffect(() => {
    document.documentElement.classList.toggle("custom-window-chrome", tauri);
    return () => document.documentElement.classList.remove("custom-window-chrome");
  }, [tauri]);

  useEffect(() => {
    if (!tauri) return;
    const appWindow = getCurrentWindow();
    void syncMaximized();
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void appWindow.onResized(() => void syncMaximized()).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [syncMaximized, tauri]);

  const toggleMaximize = async () => {
    if (!tauri) return;
    try {
      await getCurrentWindow().toggleMaximize();
      await syncMaximized();
    } catch {
      // Browser previews and partially initialized Tauri windows keep native chrome.
    }
  };

  const handleDoubleClick = (event: MouseEvent<HTMLElement>) => {
    if ((event.target as HTMLElement).closest("button")) return;
    void toggleMaximize();
  };

  const runWindowAction = async (action: "minimize" | "close") => {
    if (!tauri) return;
    try {
      const appWindow = getCurrentWindow();
      await (action === "minimize" ? appWindow.minimize() : appWindow.close());
    } catch {
      // No-op outside a fully initialized Tauri runtime.
    }
  };

  if (!tauri) return null;

  return (
    <header
      className={`window-chrome ${maximized ? "is-maximized" : ""}`}
      data-tauri-drag-region
      onDoubleClick={handleDoubleClick}
    >
      <div className="window-chrome-title" data-tauri-drag-region>
        <span className="window-chrome-mark" aria-hidden="true">A</span>
        <span data-tauri-drag-region>{title}</span>
      </div>
      <div className="window-chrome-controls" aria-label="窗口控制">
        <button aria-label="最小化" onClick={() => void runWindowAction("minimize")} type="button">
          <Minus aria-hidden="true" />
        </button>
        <button aria-label={maximized ? "还原" : "最大化"} onClick={() => void toggleMaximize()} type="button">
          <Square aria-hidden="true" />
        </button>
        <button className="window-chrome-close" aria-label="关闭" onClick={() => void runWindowAction("close")} type="button">
          <X aria-hidden="true" />
        </button>
      </div>
    </header>
  );
}

import { useCallback, useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast, Toaster } from "sonner";

import { AppShell, type PageId } from "./components/AppShell";
import { CaptchaDialog } from "./components/CaptchaDialog";
import { SettingsDialog } from "./components/SettingsDialog";
import { SmsDialog } from "./components/SmsDialog";
import { useConnect, useDisconnect, submitCaptcha, submitSms } from "./hooks/useConnection";
import { useEngineEvents } from "./hooks/useEngineEvents";
import { api } from "./lib/api";
import { applyTheme } from "./lib/theme";
import { useI18n } from "./lib/i18n";
import { queryKeys } from "./lib/query";
import { ConnectPage } from "./pages/ConnectPage";
import { DiagnosticsPage } from "./pages/DiagnosticsPage";
import type {
  ConnectOptions,
  LoginForm,
  SessionSummary,
  ThemeMode,
} from "./types/engine";

export default function App() {
  const { t } = useI18n();
  const engine = useEngineEvents();
  const [page, setPage] = useState<PageId>("connect");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [summary, setSummary] = useState<SessionSummary | null>(null);
  const [resolvedTheme, setResolvedTheme] = useState<"dark" | "light">("dark");
  const [collapsed, setCollapsed] = useState(
    () => localStorage.getItem("topsap.sidebar") === "1",
  );
  const queryClient = useQueryClient();

  const connect = useConnect();
  const disconnect = useDisconnect();

  const infoQuery = useQuery({ queryKey: queryKeys.appInfo, queryFn: api.appInfo });
  const settingsQuery = useQuery({
    queryKey: queryKeys.settings,
    queryFn: api.loadSettings,
  });

  const themeMode = settingsQuery.data?.theme ?? "system";

  const toggleCollapsed = useCallback(() => {
    setCollapsed((prev) => {
      const next = !prev;
      localStorage.setItem("topsap.sidebar", next ? "1" : "0");
      return next;
    });
  }, []);

  const toggleTheme = useCallback(() => {
    const current = settingsQuery.data;
    if (!current) return;
    const next: ThemeMode = resolvedTheme === "dark" ? "light" : "dark";
    const updated = { ...current, theme: next };

    const apply = () => {
      applyTheme(next);
      queryClient.setQueryData(queryKeys.settings, updated);
      void api.saveSettings(updated).catch(() => toast.error(t("保存主题偏好失败")));
    };

    const doc = document as Document & {
      startViewTransition?: (callback: () => void) => { finished: Promise<void> };
    };
    if (!doc.startViewTransition) {
      apply();
      return;
    }

    // 浅→深从右向左展开，深→浅从左向右展开
    document.documentElement.dataset.themeDir =
      next === "dark" ? "to-dark" : "to-light";
    const transition = doc.startViewTransition(apply);
    void transition.finished.finally(() => {
      delete document.documentElement.dataset.themeDir;
    });
  }, [resolvedTheme, settingsQuery.data, queryClient, t]);

  // 主题解析交给 App 统一处理，让 Toaster 等外部组件也能拿到实际明暗
  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: light)");

    const apply = () => {
      const resolved: "dark" | "light" =
        themeMode === "system" ? (media.matches ? "light" : "dark") : themeMode;
      const root = document.documentElement;
      root.classList.toggle("light", resolved === "light");
      root.classList.toggle("dark", resolved === "dark");
      root.style.colorScheme = resolved;
      setResolvedTheme(resolved);
    };

    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [themeMode]);

  // 连接成功后自动回到连接页查看状态
  const handleConnect = (form: LoginForm, options?: ConnectOptions) => {
    setPage("connect");
    connect.mutate(
      { form, ...options },
      {
        onSuccess: (summary) => {
          if (summary.routes > 0) setSummary(summary);
        },
      },
    );
  };

  useEffect(() => {
    if (engine.phase === "idle" || engine.phase === "failed") setSummary(null);
  }, [engine.phase]);

  const handleNavigate = (next: PageId) => {
    if (next === "settings") {
      setSettingsOpen(true);
      return;
    }
    setPage(next);
  };

  return (
    <>
      <AppShell
        page={page}
        onNavigate={handleNavigate}
        phase={engine.phase}
        label={engine.label}
        vip={engine.stats.vip}
        version={infoQuery.data ? `v${infoQuery.data.version}` : ""}
        collapsed={collapsed}
        onToggleCollapsed={toggleCollapsed}
        theme={resolvedTheme}
        onToggleTheme={toggleTheme}
        settingsOpen={settingsOpen}
      >
        {page === "connect" ? (
          <ConnectPage
            engine={engine}
            session={summary}
            connecting={connect.isPending}
            onConnect={handleConnect}
            onDisconnect={() =>
              disconnect.mutate(undefined, { onSuccess: () => setSummary(null) })
            }
            onClearLogs={engine.clearLogs}
          />
        ) : null}
        <div className={page === "diagnostics" ? "" : "hidden"}>
          <DiagnosticsPage />
        </div>
      </AppShell>

      <SettingsDialog open={settingsOpen} onClose={() => setSettingsOpen(false)} />

      <CaptchaDialog
        png={engine.captcha}
        onSubmit={async (code) => {
          const accepted = await submitCaptcha(code);
          if (accepted) engine.clearCaptcha();
        }}
        onCancel={() => {
          engine.clearCaptcha();
          // 取消验证码等同于取消本次登录，触发后端清理
          disconnect.mutate();
        }}
      />

      <SmsDialog
        open={Boolean(engine.smsChallenge)}
        challenge={engine.smsChallenge?.challenge ?? false}
        message={engine.smsChallenge?.message}
        onSubmit={async (code) => {
          const accepted = await submitSms(code);
          if (accepted) engine.clearSms();
          return accepted;
        }}
        onResend={() => api.resendSms()}
        onCancel={() => {
          engine.clearSms();
          disconnect.mutate();
        }}
      />

      <Toaster
        theme={resolvedTheme}
        position="top-center"
        richColors
        closeButton
        toastOptions={{ duration: 5000 }}
      />
    </>
  );
}

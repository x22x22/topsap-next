import { useCallback, useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast, Toaster } from "sonner";

import { AppShell, type PageId } from "./components/AppShell";
import { Button, Modal } from "./components/ui";
import { CaptchaDialog } from "./components/CaptchaDialog";
import { RouteConflictDialog } from "./components/RouteConflictDialog";
import { SettingsDialog } from "./components/SettingsDialog";
import { SmsDialog } from "./components/SmsDialog";
import { useConnect, useDisconnect, submitCaptcha, submitSms } from "./hooks/useConnection";
import { useEngineEvents } from "./hooks/useEngineEvents";
import { api } from "./lib/api";
import { localizeError } from "./lib/errorText";
import { firstLine } from "./lib/format";
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
  UpdateProgress,
} from "./types/engine";

export default function App() {
  const { t } = useI18n();
  const engine = useEngineEvents();
  const [page, setPage] = useState<PageId>("connect");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [elevationNoticeOpen, setElevationNoticeOpen] = useState(true);
  const [importNoticeOpen, setImportNoticeOpen] = useState(true);
  const [updateOpen, setUpdateOpen] = useState(false);
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [updating, setUpdating] = useState(false);
  const [updateProgress, setUpdateProgress] = useState<UpdateProgress | null>(null);
  const [updateError, setUpdateError] = useState<string | null>(null);
  const [routeDeciding, setRouteDeciding] = useState(false);
  const [summary, setSummary] = useState<SessionSummary | null>(null);
  const [resolvedTheme, setResolvedTheme] = useState<"dark" | "light">("dark");
  const [collapsed, setCollapsed] = useState(
    () => localStorage.getItem("topsap.sidebar") === "1",
  );
  const queryClient = useQueryClient();

  const connect = useConnect();
  const disconnect = useDisconnect();

  const infoQuery = useQuery({ queryKey: queryKeys.appInfo, queryFn: api.appInfo });
  const updateQuery = useQuery({
    queryKey: ["update"],
    queryFn: api.checkUpdate,
    staleTime: Infinity,
    retry: false,
    refetchOnWindowFocus: false,
  });
  const settingsQuery = useQuery({
    queryKey: queryKeys.settings,
    queryFn: api.loadSettings,
  });
  const connectionsQuery = useQuery({
    queryKey: ["connections"],
    queryFn: api.listConnections,
  });

  const themeMode = settingsQuery.data?.theme ?? "system";
  const appInfo = infoQuery.data;
  const updateInfo = updateQuery.data?.update ?? null;
  const needsRootNotice =
    appInfo?.platform === "linux" && !appInfo.elevated && elevationNoticeOpen;
  const noConnectionConfig =
    connectionsQuery.data !== undefined &&
    settingsQuery.data !== undefined &&
    connectionsQuery.data.connections.length === 0 &&
    !settingsQuery.data.gateway.trim();
  const needsImportNotice = noConnectionConfig && importNoticeOpen && !needsRootNotice;
  const canAutoUpdate =
    appInfo?.platform === "windows" || appInfo?.platform === "macos";

  const handleAutoUpdate = async () => {
    if (!updateInfo || updating) return;
    setUpdating(true);
    setUpdateError(null);
    setUpdateProgress({ stage: "downloading", downloaded: 0, total: 0 });
    try {
      await api.startUpdate((progress) => setUpdateProgress(progress));
    } catch (error) {
      setUpdating(false);
      setUpdateError(localizeError(firstLine(String(error))));
    }
  };

  const updateStageText = (() => {
    if (!updateProgress) return "";
    switch (updateProgress.stage) {
      case "downloading": {
        if (updateProgress.total > 0) {
          const percent = Math.min(
            100,
            Math.round((updateProgress.downloaded / updateProgress.total) * 100),
          );
          return t("正在下载更新包 {percent}%", { percent });
        }
        const size = (updateProgress.downloaded / 1024 / 1024).toFixed(1);
        return t("正在下载更新包（已下载 {size} MB）", { size });
      }
      case "verifying":
        return t("正在校验更新包");
      case "disconnecting":
        return t("正在断开连接");
      case "installing":
        return t("正在安装并重启");
      case "restarting":
        return t("即将重启客户端");
      default:
        return t("正在更新");
    }
  })();

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

  const decideRouteConflict = async (accept: boolean) => {
    if (routeDeciding) return;
    setRouteDeciding(true);
    try {
      const accepted = await api.submitRouteConflict(accept);
      if (!accepted) {
        toast.error(t("连接流程已结束，无需再确认"));
      }
      engine.clearRouteConflict?.();
    } catch (error) {
      toast.error(`${t("提交失败")}：${firstLine(String(error))}`);
    } finally {
      setRouteDeciding(false);
    }
  };

  const handleNavigate = (next: PageId) => {
    if (next === "settings") {
      setSettingsOpen(true);
      return;
    }
    setPage(next);
  };

  const handleCheckUpdate = async () => {
    if (checkingUpdate) return;
    setCheckingUpdate(true);
    try {
      const result = await updateQuery.refetch();
      if (!result.data?.checked) {
        toast.error(t("检查更新失败"));
        return;
      }
      if (result.data.update) {
        setUpdateOpen(true);
      } else {
        toast.success(t("已是最新版本"));
      }
    } catch {
      toast.error(t("检查更新失败"));
    } finally {
      setCheckingUpdate(false);
    }
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
        updateAvailable={Boolean(updateInfo)}
        onUpdateClick={() => setUpdateOpen(true)}
        checkingUpdate={checkingUpdate}
        onCheckUpdate={handleCheckUpdate}
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

      <RouteConflictDialog
        report={engine.routeConflict}
        busy={routeDeciding}
        onAccept={() => void decideRouteConflict(true)}
        onCancel={() => void decideRouteConflict(false)}
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

      <Modal
        open={needsRootNotice}
        title={t("需要 root 权限")}
        description={t("Linux 版需要 root 权限才能创建 TUN 接口与修改路由")}
        onClose={() => setElevationNoticeOpen(false)}
        footer={
          <Button variant="primary" onClick={() => setElevationNoticeOpen(false)}>
            {t("我知道了")}
          </Button>
        }
      >
        <div className="space-y-3 text-xs leading-relaxed text-muted">
          <p>
            {t(
              "当前以普通用户运行，创建 TUN 接口与修改路由会失败。请关闭窗口，在终端执行以下命令重新启动：",
            )}
          </p>
          <pre className="overflow-x-auto rounded-xl border border-border bg-surface-2 px-3 py-2 text-[11px] text-text">
            sudo /usr/bin/topsap-client
          </pre>
          <p className="text-[11px] text-faint">
            {t("便携包请把路径替换为实际的 topsap-client 路径。")}
          </p>
        </div>
      </Modal>

      <Modal
        open={needsImportNotice}
        title={t("尚未导入连接配置")}
        description={t("请先前往「设置」-「连接管理」中导入配置")}
        onClose={() => setImportNoticeOpen(false)}
        footer={
          <>
            <Button variant="ghost" onClick={() => setImportNoticeOpen(false)}>
              {t("稍后")}
            </Button>
            <Button
              variant="primary"
              onClick={() => {
                setImportNoticeOpen(false);
                setSettingsOpen(true);
              }}
            >
              {t("前往设置")}
            </Button>
          </>
        }
      >
        <p className="text-xs leading-relaxed text-muted">
          {t("导入后可直接选择网关与线路，无需每次手动填写。")}
        </p>
      </Modal>

      <Modal
        open={updateOpen && Boolean(updateInfo)}
        title={t("发现新版本")}
        description={
          updateInfo
            ? t("当前版本 v{current}，最新版本 v{latest}", {
                current: updateInfo.current,
                latest: updateInfo.latest,
              })
            : undefined
        }
        onClose={() => {
          if (!updating) setUpdateOpen(false);
        }}
        footer={
          <>
            <Button
              variant="ghost"
              onClick={() => setUpdateOpen(false)}
              disabled={updating}
            >
              {t("稍后")}
            </Button>
            {canAutoUpdate ? (
              <Button
                variant="primary"
                onClick={() => void handleAutoUpdate()}
                disabled={updating}
              >
                {updating ? t("正在更新") : t("自动更新")}
              </Button>
            ) : null}
            <Button
              variant={canAutoUpdate ? "outline" : "primary"}
              disabled={updating}
              onClick={() => {
                if (!updateInfo) return;
                void api.openUrl(updateInfo.url).then(
                  () => setUpdateOpen(false),
                  () => toast.error(t("无法打开下载页面")),
                );
              }}
            >
              {t("前往")}
            </Button>
          </>
        }
      >
        <p className="text-xs leading-relaxed text-muted">
          {t(
            canAutoUpdate
              ? "建议更新到最新版本以获取修复与改进。自动更新会先断开当前 VPN 连接，然后下载、安装并重启客户端；若手动下载，请先点击「前往」打开下载页面，再完整退出当前程序后安装。"
              : "建议更新到最新版本以获取修复与改进。请先点击「前往」打开下载页面，再完整退出当前程序后安装更新。",
          )}
        </p>
        {updating && updateProgress ? (
          <p className="mt-3 text-xs font-medium text-accent">{updateStageText}</p>
        ) : null}
        {updateError ? (
          <p className="mt-3 text-xs leading-relaxed text-danger">{updateError}</p>
        ) : null}
      </Modal>

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

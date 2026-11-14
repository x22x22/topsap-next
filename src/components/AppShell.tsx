import type { ReactNode } from "react";
import {
  Activity,
  Languages,
  Loader2,
  Moon,
  PanelLeftClose,
  PanelLeftOpen,
  PlugZap,
  Settings2,
  Sun,
} from "lucide-react";

import { useI18n } from "../lib/i18n";
import type { EnginePhase } from "../types/engine";
import { cn, IconButton, Logo, StatusDot } from "./ui";

export type PageId = "connect" | "diagnostics" | "settings";

const NAV: Array<{
  id: PageId;
  label: string;
  hint: string;
  icon: typeof PlugZap;
}> = [
  { id: "connect", label: "连接", hint: "网关登录与隧道", icon: PlugZap },
  { id: "diagnostics", label: "诊断", hint: "网卡自检与残留清理", icon: Activity },
  { id: "settings", label: "设置", hint: "连接管理与偏好", icon: Settings2 },
];

export function AppShell({
  page,
  onNavigate,
  phase,
  everConnected = false,
  label,
  vip,
  version,
  collapsed,
  onToggleCollapsed,
  theme,
  onToggleTheme,
  settingsOpen = false,
  updateAvailable = false,
  onUpdateClick,
  checkingUpdate = false,
  onCheckUpdate,
  children,
}: {
  page: PageId;
  everConnected?: boolean;
  onNavigate: (page: PageId) => void;
  phase: EnginePhase;
  label: string;
  vip: string;
  version: string;
  collapsed: boolean;
  onToggleCollapsed: () => void;
  theme: "dark" | "light";
  onToggleTheme: () => void;
  settingsOpen?: boolean;
  updateAvailable?: boolean;
  onUpdateClick?: () => void;
  checkingUpdate?: boolean;
  onCheckUpdate?: () => void;
  children: ReactNode;
}) {
  const { t, lang, toggleLang } = useI18n();
  const active =
    (settingsOpen ? NAV[2] : NAV.find((item) => item.id === page)) ?? NAV[0];
  const pageActive = (id: PageId) =>
    id === "settings" ? settingsOpen : !settingsOpen && id === page;

  return (
    <div className="flex h-full">
      <aside
        className={cn(
          "flex shrink-0 flex-col border-r border-border bg-bg-elevated/60 py-5 transition-[width] duration-200",
          collapsed ? "w-[76px] px-2" : "w-60 px-3",
        )}
      >
        <div className={cn("flex items-center", collapsed ? "flex-col gap-2" : "gap-2 px-1")}>
          <Logo className={collapsed ? "h-8 w-8" : "h-9 w-9 shrink-0"} />
          {!collapsed ? (
            <div className="min-w-0 flex-1">
              <div className="truncate text-sm font-semibold tracking-wide">TopSAP Client</div>
              <div className="text-[11px] text-faint">{t("Sunline VPN 客户端")}</div>
            </div>
          ) : null}
          <IconButton
            label={t(collapsed ? "展开侧栏" : "收起侧栏")}
            onClick={onToggleCollapsed}
            className="shrink-0"
          >
            {collapsed ? (
              <PanelLeftOpen className="h-4 w-4" />
            ) : (
              <PanelLeftClose className="h-4 w-4" />
            )}
          </IconButton>
        </div>

        <nav className={cn("mt-8 flex flex-col gap-1", collapsed && "items-center")} aria-label={t("主导航")}>
          {NAV.map((item) => {
            const Icon = item.icon;
            const isActive = pageActive(item.id);
            return (
              <button
                key={item.id}
                type="button"
                onClick={() => onNavigate(item.id)}
                aria-current={isActive ? "page" : undefined}
                title={collapsed ? `${t(item.label)} · ${t(item.hint)}` : undefined}
                className={cn(
                  "group flex rounded-xl text-left transition",
                  collapsed
                    ? "h-11 w-11 items-center justify-center"
                    : "items-center gap-3 px-3 py-2.5",
                  isActive
                    ? "bg-surface-2 text-text shadow-sm ring-1 ring-border-strong"
                    : "text-muted hover:bg-hover hover:text-text hover:ring-1 hover:ring-border-strong",
                )}
              >
                <span
                  className={cn(
                    "flex items-center justify-center rounded-lg border transition",
                    collapsed ? "h-9 w-9" : "h-8 w-8",
                    isActive
                      ? "border-accent/30 bg-accent-soft text-accent"
                      : "border-transparent bg-surface-3/60 text-faint group-hover:border-accent/25 group-hover:bg-accent-soft group-hover:text-accent",
                  )}
                >
                  <Icon className="h-4 w-4" />
                </span>
                {!collapsed ? (
                  <span className="min-w-0">
                    <span className="block text-sm font-medium">{t(item.label)}</span>
                    <span className="block truncate text-[11px] text-faint">{t(item.hint)}</span>
                  </span>
                ) : null}
              </button>
            );
          })}
        </nav>

        <div className={cn("mt-auto", collapsed ? "flex flex-col items-center gap-2" : "space-y-3")}>
          {collapsed ? (
            <div className="flex h-11 w-11 items-center justify-center rounded-xl border border-border bg-surface/70" title={t(label)}>
              <StatusDot phase={phase} everConnected={everConnected} />
            </div>
          ) : (
            <div className="rounded-xl border border-border bg-surface/70 p-3">
              <div className="flex items-center gap-2">
                <StatusDot phase={phase} everConnected={everConnected} />
                <span className="text-xs font-medium text-text">{t(label)}</span>
              </div>
              <dl className="mt-2 space-y-1 text-[11px]">
                <div className="flex items-center justify-between gap-2">
                  <dt className="text-faint">{t("虚拟 IP")}</dt>
                  <dd className="font-mono text-muted">{vip || "—"}</dd>
                </div>
                <div className="flex items-center justify-between gap-2">
                  <dt className="text-faint">{t("版本")}</dt>
                  <dd className="flex min-w-0 items-center justify-end gap-1.5 font-mono text-muted">
                    <button
                      type="button"
                      onClick={onCheckUpdate}
                      disabled={checkingUpdate}
                      title={t("检查更新")}
                      className="inline-flex min-w-0 items-center gap-1 rounded text-left transition hover:text-text disabled:opacity-60"
                    >
                      <span className="truncate">{version || "—"}</span>
                      {checkingUpdate ? (
                        <Loader2 className="h-3 w-3 shrink-0 animate-spin" />
                      ) : null}
                    </button>
                    {updateAvailable ? (
                      <button
                        type="button"
                        onClick={onUpdateClick}
                        title={t("发现新版本")}
                        className="inline-flex shrink-0 items-center gap-1 rounded-md border border-accent/30 bg-accent-soft px-1.5 py-0.5 font-sans text-[10px] font-medium text-accent transition hover:border-accent/60"
                      >
                        <span className="h-1.5 w-1.5 rounded-full bg-accent" />
                        {t("可更新")}
                      </button>
                    ) : null}
                  </dd>
                </div>
              </dl>
            </div>
          )}
        </div>
      </aside>

      <main className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-14 shrink-0 items-center justify-between border-b border-border px-6">
          <div className="min-w-0">
            <h1 className="text-sm font-semibold text-text">{t(active.label)}</h1>
            <p className="truncate text-[11px] text-faint">{t(active.hint)}</p>
          </div>
          <div className="flex items-center gap-1.5">
            <button
              type="button"
              onClick={toggleLang}
              title={t(lang === "zh" ? "切换到英文" : "切换到中文")}
              className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border bg-surface/70 px-2.5 text-[11px] font-medium text-muted transition hover:bg-hover hover:text-text"
            >
              <Languages className="h-3.5 w-3.5" />
              {lang === "zh" ? "EN" : "中"}
            </button>
            <IconButton
              label={t(theme === "dark" ? "切换到浅色主题" : "切换到深色主题")}
              onClick={onToggleTheme}
            >
              {theme === "dark" ? <Sun className="h-4 w-4" /> : <Moon className="h-4 w-4" />}
            </IconButton>
          </div>
        </header>

        <div className="flex-1 overflow-y-auto px-6 py-6">{children}</div>
      </main>
    </div>
  );
}

import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open as openFile, save as saveFile } from "@tauri-apps/plugin-dialog";
import {
  Download,
  Info,
  Palette,
  Pencil,
  Plus,
  RotateCcw,
  Server,
  Settings2,
  ShieldAlert,
  ShieldOff,
  Star,
  Trash2,
  Upload,
  X,
} from "lucide-react";
import { toast } from "sonner";

import { ConfirmDialog } from "./ConfirmDialog";
import { ProfileDialog } from "./ProfileDialog";
import {
  Badge,
  Button,
  Card,
  CardHeader,
  EmptyState,
  IconButton,
  Input,
  Select,
  Spinner,
  Switch,
} from "./ui";
import { api } from "../lib/api";
import { firstLine } from "../lib/format";
import { useI18n, type LanguageMode } from "../lib/i18n";
import {
  DEFAULT_PROXY_BYPASS,
  addProxyEntry,
  mergeDefaultProxyBypass,
} from "../lib/proxyBypass";
import { isTauriRuntime } from "../lib/platform";
import { queryKeys } from "../lib/query";
import { applyTheme } from "../lib/theme";
import type { AppSettings, ConnectionProfile, ThemeMode } from "../types/engine";

type SectionId = "general" | "connections" | "proxy" | "appearance" | "about";

const MODE_TONE = ["neutral", "accent", "warn"] as const;

export function SettingsDialog({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  const { t, languageMode, setLanguageMode } = useI18n();
  const queryClient = useQueryClient();
  const [section, setSection] = useState<SectionId>("general");

  const settingsQuery = useQuery({
    queryKey: queryKeys.settings,
    queryFn: api.loadSettings,
  });
  const infoQuery = useQuery({ queryKey: queryKeys.appInfo, queryFn: api.appInfo });
  const connectionsQuery = useQuery({
    queryKey: ["connections"],
    queryFn: api.listConnections,
  });
  const bypassStatusQuery = useQuery({
    queryKey: queryKeys.proxyBypass,
    queryFn: api.proxyBypassStatus,
  });

  const [draft, setDraft] = useState<AppSettings | null>(null);
  const [bypassInput, setBypassInput] = useState("");
  const [profileOpen, setProfileOpen] = useState(false);
  const [editingProfile, setEditingProfile] = useState<ConnectionProfile | null>(null);
  const [confirmState, setConfirmState] = useState<
    | { kind: "delete"; name: string }
    | { kind: "clear" }
    | null
  >(null);

  useEffect(() => {
    if (settingsQuery.data && !draft) setDraft(settingsQuery.data);
  }, [settingsQuery.data, draft]);

  useEffect(() => {
    if (!open) return;
    setSection("general");
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  const save = useMutation({
    mutationFn: (value: AppSettings) => api.saveSettings(value),
    onSuccess: (_result, value) => {
      // 直接写回缓存，避免重新拉取旧数据导致开关或主题回跳
      queryClient.setQueryData(queryKeys.settings, value);
      void queryClient.invalidateQueries({ queryKey: queryKeys.proxyBypass });
    },
    onError: (error) => {
      toast.error(`${t("保存失败")}：${firstLine(String(error))}`, { duration: 5000 });
    },
  });

  const saveConnection = useMutation({
    mutationFn: (profile: ConnectionProfile) => api.saveConnection(profile),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["connections"] });
      setProfileOpen(false);
      toast.success(t("连接配置已保存"));
    },
    onError: (error) => toast.error(firstLine(String(error))),
  });

  const deleteConnection = useMutation({
    mutationFn: (name: string) => api.deleteConnection(name),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["connections"] });
      toast.success(t("连接配置已删除"));
    },
    onError: (error) => toast.error(firstLine(String(error))),
  });

  const setActiveConnection = useMutation({
    mutationFn: (name: string) => api.setActiveConnection(name),
    onSuccess: (_data, name) => {
      void queryClient.invalidateQueries({ queryKey: ["connections"] });
      toast.success(t("已将「{name}」设为默认连接", { name }));
    },
    onError: (error) => toast.error(firstLine(String(error))),
  });

  const importConnections = useMutation({
    mutationFn: async () => {
      if (!isTauriRuntime()) return api.importConnections("demo.json");
      const path = await openFile({
        multiple: false,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (typeof path !== "string") return 0;
      return api.importConnections(path);
    },
    onSuccess: (count) => {
      void queryClient.invalidateQueries({ queryKey: ["connections"] });
      if (count > 0) toast.success(t("已导入 {count} 条连接配置", { count }));
    },
    onError: (error) =>
      toast.error(`${t("导入失败")}：${firstLine(String(error))}`, { duration: 5000 }),
  });

  const exportConnections = useMutation({
    mutationFn: async () => {
      if (!isTauriRuntime()) return api.exportConnections("demo.json");
      const path = await saveFile({
        defaultPath: "topsap-connections.json",
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!path) return 0;
      return api.exportConnections(path);
    },
    onSuccess: (count) => {
      if (count > 0) toast.success(t("已导出 {count} 条连接配置", { count }));
    },
    onError: (error) =>
      toast.error(`${t("导出失败")}：${firstLine(String(error))}`, { duration: 5000 }),
  });

  const clearConnections = useMutation({
    mutationFn: () => api.clearConnections(),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["connections"] });
      toast.success(t("连接配置已清空"));
    },
    onError: (error) => toast.error(firstLine(String(error))),
  });

  if (!open) return null;

  const sections: Array<{ id: SectionId; label: string; hint: string; icon: typeof Settings2 }> = [
    { id: "general", label: t("通用"), hint: t("托盘与自动重连"), icon: Settings2 },
    { id: "connections", label: t("连接管理"), hint: t("网关配置与线路"), icon: Server },
    { id: "proxy", label: t("代理绕过"), hint: t("指定域名/IP 不走系统代理"), icon: ShieldOff },
    { id: "appearance", label: t("外观"), hint: t("深浅色主题与界面语言"), icon: Palette },
    { id: "about", label: t("关于"), hint: t("版本与安全说明"), icon: Info },
  ];
  const activeSection = sections.find((item) => item.id === section) ?? sections[0];

  const update = (patch: Partial<AppSettings>, onSaved?: () => void) => {
    if (!draft) return;
    const next = { ...draft, ...patch };
    setDraft(next);
    save.mutate(next, onSaved ? { onSuccess: onSaved } : undefined);
  };

  const proxyEntries = draft?.proxy_bypass ?? [];
  const bypassStatus = bypassStatusQuery.data;
  const bypassSupported = bypassStatus?.supported ?? true;
  const bypassStatusText = !bypassStatus
    ? ""
    : !bypassSupported
      ? t("当前系统不支持或未检测到可用的系统代理设置")
      : !draft?.proxy_bypass_enabled
        ? t("代理绕过已关闭")
        : proxyEntries.length === 0
          ? t("未设置代理绕过条目")
          : bypassStatus.applied
            ? t("已写入系统代理绕过列表（{count} 条）", { count: proxyEntries.length })
            : t("尚未写入系统代理绕过列表，保存设置后重试");

  const addBypassEntry = () => {
    if (!draft) return;
    const result = addProxyEntry(proxyEntries, bypassInput);
    if (result.error) {
      toast.error(t(result.error));
      return;
    }
    setBypassInput("");
    update({ proxy_bypass: result.entries });
  };

  const removeBypassEntry = (entry: string) => {
    if (!draft) return;
    update({ proxy_bypass: proxyEntries.filter((item) => item !== entry) });
  };

  const restoreBypassDefaults = () => {
    if (!draft) return;
    const merged = mergeDefaultProxyBypass(proxyEntries);
    if (merged.length === proxyEntries.length) {
      toast.info(t("代理绕过已包含默认条目"));
      return;
    }
    update({ proxy_bypass: merged }, () =>
      toast.success(t("已恢复默认代理绕过条目")),
    );
  };

  const changeTheme = (theme: ThemeMode) => {
    update({ theme });
    applyTheme(theme);
  };

  const changeLanguage = (next: LanguageMode) => {
    setLanguageMode(next);
  };

  const info = infoQuery.data;
  const connections = connectionsQuery.data?.connections ?? [];
  const modeLabel = (type: number) =>
    t(["单线路", "多线路优选", "多线路备份"][type] ?? "未知模式");

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 sm:p-6">
      <div
        className="absolute inset-0 bg-black/55 backdrop-blur-sm"
        onClick={onClose}
        aria-hidden="true"
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-label={t("设置")}
        className="fade-up relative flex h-[min(720px,88vh)] w-full max-w-4xl overflow-hidden rounded-2xl border border-border bg-surface shadow-2xl"
      >
        <aside className="flex w-56 shrink-0 flex-col border-r border-border bg-bg-elevated/60 p-3">
          <div className="px-2 py-2">
            <div className="text-sm font-semibold text-text">{t("设置")}</div>
            <div className="mt-0.5 text-[11px] text-faint">{t("连接管理与偏好")}</div>
          </div>
          <nav className="mt-2 space-y-1">
            {sections.map((item) => {
              const Icon = item.icon;
              const isActive = item.id === section;
              return (
                <button
                  key={item.id}
                  type="button"
                  onClick={() => setSection(item.id)}
                  aria-current={isActive ? "page" : undefined}
                  className={`flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-left transition ${
                    isActive
                      ? "bg-accent-soft text-accent ring-1 ring-accent/20"
                      : "text-muted hover:bg-hover hover:text-text"
                  }`}
                >
                  <Icon className="h-4 w-4 shrink-0" />
                  <span className="min-w-0">
                    <span className="block text-sm font-medium">{item.label}</span>
                    <span className="block truncate text-[11px] text-faint">{item.hint}</span>
                  </span>
                </button>
              );
            })}
          </nav>
        </aside>

        <div className="flex min-w-0 flex-1 flex-col">
          <header className="flex items-start justify-between gap-4 border-b border-border px-6 py-4">
            <div>
              <h2 className="text-base font-semibold text-text">{activeSection.label}</h2>
              <p className="mt-0.5 text-xs text-muted">
                {section === "general"
                  ? t("应用行为与连接偏好")
                  : activeSection.hint}
              </p>
            </div>
            <IconButton label={t("关闭")} onClick={onClose}>
              <X className="h-4 w-4" />
            </IconButton>
          </header>

          <div className="flex-1 overflow-y-auto px-6 py-5">
            {!draft ? (
              <div className="flex h-40 items-center justify-center text-muted">
                <Spinner className="h-5 w-5" />
              </div>
            ) : (
              <div className="space-y-4">
                {section === "general" ? (
                  <>
                    <Card>
                      <CardHeader title={t("通用")} />
                      <div className="divide-y divide-border">
                        <SettingRow
                          title={t("关闭窗口时最小化到托盘")}
                          description={t("关闭按钮不退出程序，连接保持；从托盘菜单可彻底退出")}
                          control={
                            <Switch
                              checked={draft.close_to_tray}
                              onChange={(next) => update({ close_to_tray: next })}
                              label={t("关闭窗口时最小化到托盘")}
                            />
                          }
                        />
                        <SettingRow
                          title={t("断线自动重连")}
                          description={t("隧道异常断开后按指数退避重试，最多 5 次；主动断开不触发")}
                          control={
                            <Switch
                              checked={draft.auto_reconnect}
                              onChange={(next) => update({ auto_reconnect: next })}
                              label={t("断线自动重连")}
                            />
                          }
                        />
                      </div>
                    </Card>

                  </>
                ) : null}

                {section === "connections" ? (
                  <Card>
                    <CardHeader title={t("连接管理")} />
                    <div className="space-y-2 p-5">
                      {connections.length === 0 ? (
                        <EmptyState
                          icon={<Server className="h-6 w-6" />}
                          title={t("暂无连接配置")}
                          description={t("点击「新增」保存第一个网关配置")}
                        />
                      ) : (
                        connections.map((profile) => (
                          <div
                            key={profile.name}
                            className="flex items-center gap-3 rounded-xl border border-border bg-surface-2/40 px-3 py-2.5"
                          >
                            <div className="min-w-0 flex-1">
                              <div className="flex flex-wrap items-center gap-2">
                                <span className="text-sm font-medium text-text">
                                  {profile.name}
                                </span>
                                <Badge tone={MODE_TONE[profile.connect_type] ?? "neutral"}>
                                  {modeLabel(profile.connect_type)}
                                </Badge>
                                {connectionsQuery.data?.active === profile.name ? (
                                  <Badge tone="ok">{t("默认")}</Badge>
                                ) : null}
                              </div>
                              <div className="mt-0.5 truncate font-mono text-[11px] text-faint">
                                {profile.lines.join(" · ")}
                              </div>
                            </div>
                            {connectionsQuery.data?.active === profile.name ? (
                              <span
                                title={t("默认连接")}
                                className="flex h-8 w-8 items-center justify-center rounded-lg text-warn"
                              >
                                <Star className="h-4 w-4 fill-current" />
                              </span>
                            ) : (
                              <IconButton
                                label={t("设为默认")}
                                onClick={() => setActiveConnection.mutate(profile.name)}
                              >
                                <Star className="h-4 w-4" />
                              </IconButton>
                            )}
                            <IconButton
                              label={t("编辑")}
                              onClick={() => {
                                setEditingProfile(profile);
                                setProfileOpen(true);
                              }}
                            >
                              <Pencil className="h-4 w-4" />
                            </IconButton>
                            <IconButton
                              label={t("删除")}
                              onClick={() =>
                                setConfirmState({ kind: "delete", name: profile.name })
                              }
                            >
                              <Trash2 className="h-4 w-4" />
                            </IconButton>
                          </div>
                        ))
                      )}

                      <div className="flex flex-wrap gap-2 pt-1">
                        <Button
                          size="sm"
                          onClick={() => importConnections.mutate()}
                          disabled={importConnections.isPending}
                        >
                          <Upload className="h-3.5 w-3.5" />
                          {t("导入配置")}
                        </Button>
                        <Button
                          size="sm"
                          onClick={() => exportConnections.mutate()}
                          disabled={exportConnections.isPending}
                        >
                          <Download className="h-3.5 w-3.5" />
                          {t("导出配置")}
                        </Button>
                        <Button
                          size="sm"
                          variant="danger"
                          onClick={() => setConfirmState({ kind: "clear" })}
                          disabled={clearConnections.isPending}
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                          {t("清空配置")}
                        </Button>
                        <Button
                          size="sm"
                          variant="primary"
                          className="ml-auto"
                          onClick={() => {
                            setEditingProfile(null);
                            setProfileOpen(true);
                          }}
                        >
                          <Plus className="h-3.5 w-3.5" />
                          {t("新增")}
                        </Button>
                      </div>
                    </div>
                  </Card>
                ) : null}

                {section === "proxy" ? (
                  <Card>
                    <CardHeader
                      title={t("代理绕过")}
                      description={t(
                        "写入系统代理绕过列表，让指定域名/IP 不经过本地代理；仅对使用系统代理的程序生效，TUN/透明代理无法覆盖。",
                      )}
                    />
                    <div className="divide-y divide-border">
                      <SettingRow
                        title={t("启用代理绕过")}
                        description={t("关闭后只移除本应用写入的条目，不修改其他代理设置")}
                        control={
                          <Switch
                            checked={draft.proxy_bypass_enabled}
                            onChange={(next) => update({ proxy_bypass_enabled: next })}
                            disabled={!bypassSupported}
                            label={t("启用代理绕过")}
                          />
                        }
                      />
                    </div>
                    <div className="space-y-3 p-5">
                      <div className="flex flex-wrap items-center gap-2">
                        <Input
                          value={bypassInput}
                          onChange={(event) => setBypassInput(event.target.value)}
                          onKeyDown={(event) => {
                            if (event.key === "Enter") addBypassEntry();
                          }}
                          placeholder={t("*.example.com 或 192.0.2.10")}
                          disabled={!draft.proxy_bypass_enabled || !bypassSupported}
                          className="min-w-56 flex-1"
                        />
                        <Button
                          size="sm"
                          onClick={addBypassEntry}
                          disabled={!draft.proxy_bypass_enabled || !bypassSupported}
                        >
                          <Plus className="h-3.5 w-3.5" />
                          {t("添加")}
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={restoreBypassDefaults}
                          disabled={!draft.proxy_bypass_enabled || !bypassSupported}
                        >
                          <RotateCcw className="h-3.5 w-3.5" />
                          {t("恢复默认")}
                        </Button>
                      </div>

                      {proxyEntries.length === 0 ? (
                        <EmptyState
                          icon={<ShieldOff className="h-6 w-6" />}
                          title={t("暂无代理绕过条目")}
                          description={t("默认会加入 *.sunline.cn 与 *.sunline-i.com")}
                        />
                      ) : (
                        <div className="space-y-1.5">
                          {proxyEntries.map((entry) => (
                            <div
                              key={entry}
                              className="flex items-center gap-2 rounded-xl border border-border bg-surface-2/40 px-3 py-2"
                            >
                              <span className="min-w-0 flex-1 truncate font-mono text-xs text-text">
                                {entry}
                              </span>
                              {DEFAULT_PROXY_BYPASS.includes(entry) ? (
                                <Badge tone="accent">{t("默认")}</Badge>
                              ) : null}
                              <IconButton
                                label={t("删除")}
                                onClick={() => removeBypassEntry(entry)}
                                disabled={!draft.proxy_bypass_enabled || !bypassSupported}
                              >
                                <Trash2 className="h-4 w-4" />
                              </IconButton>
                            </div>
                          ))}
                        </div>
                      )}

                      {!bypassSupported ? (
                        <div className="flex items-start gap-2 rounded-xl border border-warn/25 bg-warn-soft px-3 py-2.5">
                          <ShieldAlert className="mt-0.5 h-4 w-4 shrink-0 text-warn" />
                          <p className="text-[11px] leading-relaxed text-warn">
                            {bypassStatusText}
                          </p>
                        </div>
                      ) : (
                        <p className="text-[11px] text-faint">{bypassStatusText}</p>
                      )}
                    </div>
                  </Card>
                ) : null}

                {section === "appearance" ? (
                  <Card>
                    <CardHeader title={t("外观")} />
                    <div className="divide-y divide-border">
                      <SettingRow
                        title={t("主题")}
                        description={t("深浅色跟随或固定")}
                        control={
                          <Select<ThemeMode>
                            value={draft.theme}
                            onChange={changeTheme}
                            ariaLabel={t("主题")}
                            options={[
                              { value: "system", label: t("跟随系统") },
                              { value: "dark", label: t("深色") },
                              { value: "light", label: t("浅色") },
                            ]}
                            className="w-40"
                          />
                        }
                      />
                      <SettingRow
                        title={t("语言")}
                        description={t("界面显示语言；跟随系统时按系统语言解析")}
                        control={
                          <Select<LanguageMode>
                            value={languageMode}
                            onChange={changeLanguage}
                            ariaLabel={t("语言")}
                            options={[
                              { value: "system", label: t("跟随系统") },
                              { value: "zh", label: "中文" },
                              { value: "en", label: "English" },
                            ]}
                            className="w-40"
                          />
                        }
                      />
                    </div>
                  </Card>
                ) : null}

                {section === "about" ? (
                  <Card>
                    <CardHeader title={t("关于")} />
                    <div className="space-y-4 p-5">
                      <dl className="grid gap-3 sm:grid-cols-3">
                        <InfoItem label={t("版本")} value={info ? `v${info.version}` : "—"} />
                        <InfoItem
                          label={t("平台")}
                          value={info ? `${info.platform}/${info.arch}` : "—"}
                        />
                        <InfoItem
                          label={t("管理员权限")}
                          value={info?.elevated ? t("已提权") : t("未提权")}
                        />
                      </dl>

                      {info && !info.elevated ? (
                        <div className="flex items-start gap-2 rounded-xl border border-warn/25 bg-warn-soft px-3 py-2.5">
                          <ShieldAlert className="mt-0.5 h-4 w-4 shrink-0 text-warn" />
                          <p className="text-[11px] leading-relaxed text-warn">
                            {elevationHint(info.platform, t)}
                          </p>
                        </div>
                      ) : null}

                    </div>
                  </Card>
                ) : null}
              </div>
            )}
          </div>

        </div>
      </div>

      <ConfirmDialog
        open={confirmState !== null}
        title={
          confirmState?.kind === "clear"
            ? t("清空连接配置")
            : t("删除连接配置")
        }
        description={
          confirmState?.kind === "clear"
            ? t("确定清空全部连接配置？此操作不可撤销。")
            : confirmState?.kind === "delete"
              ? t("确定删除连接「{name}」？", { name: confirmState.name })
              : ""
        }
        confirmLabel={
          confirmState?.kind === "clear" ? t("清空配置") : t("删除")
        }
        busy={
          confirmState?.kind === "clear"
            ? clearConnections.isPending
            : deleteConnection.isPending
        }
        onClose={() => setConfirmState(null)}
        onConfirm={() => {
          if (confirmState?.kind === "clear") {
            clearConnections.mutate();
          } else if (confirmState?.kind === "delete") {
            deleteConnection.mutate(confirmState.name);
          }
          setConfirmState(null);
        }}
      />

      <ProfileDialog
        open={profileOpen}
        initial={editingProfile}
        onClose={() => setProfileOpen(false)}
        onSubmit={(profile) => saveConnection.mutate(profile)}
      />
    </div>
  );
}

function SettingRow({
  title,
  description,
  control,
}: {
  title: string;
  description: string;
  control: React.ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-4 px-5 py-4">
      <div className="min-w-0">
        <div className="text-sm font-medium text-text">{title}</div>
        <div className="mt-0.5 text-[11px] leading-relaxed text-faint">{description}</div>
      </div>
      <div className="shrink-0">{control}</div>
    </div>
  );
}

function InfoItem({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt className="text-[11px] text-faint">{label}</dt>
      <dd className="mt-0.5 break-words font-mono text-xs text-muted">{value}</dd>
    </div>
  );
}

function elevationHint(platform: string, t: (zh: string) => string): string {
  if (platform === "macos") {
    return t(
      "当前未以 root 运行，无法创建 TUN 接口与注入路由。请在终端执行 sudo open \"/Applications/TopSAP Client.app\"，或直接使用 sudo 启动应用。",
    );
  }
  if (platform === "linux") {
    return t("当前未以 root 运行，无法创建 TUN 接口与注入路由。请使用 sudo 启动应用。");
  }
  return t(
    "当前未以管理员身份运行，创建虚拟网卡与注入路由会失败。请右键「以管理员身份运行」，或使用安装包的提权启动方式。",
  );
}

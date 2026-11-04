import { useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { KeyRound, Loader2, Server, UserRound } from "lucide-react";
import { toast } from "sonner";

import { BasicInfoCard } from "../components/BasicInfoCard";
import { ChangePasswordDialog } from "../components/ChangePasswordDialog";
import { ExpiredPasswordDialog } from "../components/ExpiredPasswordDialog";
import { ForgotPasswordDialog } from "../components/ForgotPasswordDialog";
import { LinePickerDialog } from "../components/LinePickerDialog";
import { LoginInfoCard } from "../components/LoginInfoCard";
import { LogPanel } from "../components/LogPanel";
import { ResourceListCard } from "../components/ResourceListCard";

import { StatusHero } from "../components/StatusHero";
import { TrafficChartCard } from "../components/TrafficChartCard";
import {
  Button,
  Card,
  CardHeader,
  Field,
  Input,
  PasswordInput,
  Select,
  Switch,
} from "../components/ui";
import { isBusyPhase, type EngineView } from "../hooks/useEngineEvents";
import { api } from "../lib/api";
import { firstLine, isValidGateway, isValidPort } from "../lib/format";
import { useI18n } from "../lib/i18n";
import { queryKeys } from "../lib/query";
import type {
  ConnectOptions,
  ConnectionProfile,
  LineProbe,
  LoginForm,
  SessionSummary,
} from "../types/engine";

const EMPTY_FORM: LoginForm = {
  gateway: "",
  port: 443,
  user: "",
  password: "",
  remember: false,
};

function splitLine(line: string): { host: string; port: number } {
  const index = line.lastIndexOf(":");
  if (index <= 0) return { host: line, port: 443 };
  const host = line.slice(0, index).replace(/^\[|\]$/g, "");
  const port = Number(line.slice(index + 1));
  return { host, port: Number.isInteger(port) && port > 0 ? port : 443 };
}

function formFromProfile(profile: ConnectionProfile, base: LoginForm): LoginForm {
  const first = splitLine(profile.lines[0] ?? "");
  return {
    ...base,
    gateway: first.host,
    port: first.port,
    lines: profile.lines,
    connect_type: profile.connect_type,
    connect_timeout: profile.connect_timeout,
  };
}

export function ConnectPage({
  engine,
  session,
  connecting,
  onConnect,
  onDisconnect,
  onClearLogs,
}: {
  engine: EngineView;
  session: SessionSummary | null;
  connecting: boolean;
  onConnect: (form: LoginForm, options?: ConnectOptions) => void;
  onDisconnect: () => void;
  onClearLogs: () => void;
}) {
  const { t } = useI18n();
  const settingsQuery = useQuery({
    queryKey: queryKeys.settings,
    queryFn: api.loadSettings,
  });
  const connectionsQuery = useQuery({
    queryKey: ["connections"],
    queryFn: api.listConnections,
  });

  const [form, setForm] = useState<LoginForm>(EMPTY_FORM);
  const [selectedProfile, setSelectedProfile] = useState("manual");
  const [forgotOpen, setForgotOpen] = useState(false);
  const [changePasswordOpen, setChangePasswordOpen] = useState(false);
  const [passwordAutoFilled, setPasswordAutoFilled] = useState(false);
  const [passwordAccount, setPasswordAccount] = useState<string | null>(null);
  const [expiredOpen, setExpiredOpen] = useState(false);
  const [probingLines, setProbingLines] = useState(false);
  const [lineProbes, setLineProbes] = useState<LineProbe[]>([]);
  const [linePickerOpen, setLinePickerOpen] = useState(false);
  const initialized = useRef(false);

  // 网关要求先改密时自动弹窗；关闭后清空事件，避免下次挂载重复弹出
  useEffect(() => {
    if (engine.passwordChange) setExpiredOpen(true);
  }, [engine.passwordChange]);

  // 登录成功后的账号/密码到期提醒，对应原客户端门户页的 expire 提示
  useEffect(() => {
    const reminder = engine.expiryReminder;
    if (!reminder) return;
    if (reminder.accountEnd > 0) {
      toast.warning(
        t("您的账号将于 {days} 天后到期，请联系管理员", { days: reminder.accountEnd }),
        { duration: 10000 },
      );
    }
    if (reminder.passwordRemind > 0) {
      toast.warning(
        t("您的密码将在 {days} 天后过期，请及时修改！", {
          days: reminder.passwordRemind,
        }),
        { duration: 10000 },
      );
    }
    engine.clearExpiryReminder?.();
  }, [engine.expiryReminder, engine.clearExpiryReminder, t]);

  // 只在首次拿到设置时填充，避免覆盖用户正在输入的内容
  useEffect(() => {
    const settings = settingsQuery.data;
    if (!settings || initialized.current) return;
    // 等待连接配置加载完成，否则会先按手动输入初始化并错过默认配置
    if (connectionsQuery.isLoading) return;
    initialized.current = true;

    const store = connectionsQuery.data;
    const active = store?.connections.find((item) => item.name === store.active);

    if (active) {
      setSelectedProfile(active.name);
      setForm(
        formFromProfile(active, {
          gateway: active.lines[0] ? splitLine(active.lines[0]).host : settings.gateway,
          port: active.lines[0] ? splitLine(active.lines[0]).port : settings.port || 443,
          user: settings.user,
          password: "",
          remember: settings.remember,
        }),
      );
    } else {
      setForm({
        gateway: settings.gateway,
        port: settings.port || 443,
        user: settings.user,
        password: "",
        remember: settings.remember,
      });
    }

    if (settings.remember && settings.user) {
      api
        .loadPassword(settings.user)
        .then((password) => {
          if (password) {
            setForm((prev) => ({ ...prev, password }));
            setPasswordAccount(settings.user);
          }
        })
        .catch(() => {
          // 读不到已保存密码时保持空值，由用户重新输入
        });
    }
  }, [settingsQuery.data, connectionsQuery.data, connectionsQuery.isLoading]);

  const profiles = connectionsQuery.data?.connections ?? [];
  const activeProfile = profiles.find((item) => item.name === selectedProfile);
  const isProfileMode = selectedProfile !== "manual" && Boolean(activeProfile);
  const modeLabels = [t("单线路"), t("多线路优选"), t("多线路备份")];

  const applyProfile = (name: string) => {
    setSelectedProfile(name);
    if (name === "manual") {
      setForm((prev) => ({ ...prev, lines: undefined, connect_type: 0 }));
      return;
    }
    const profile = profiles.find((item) => item.name === name);
    if (!profile) return;
    setForm((prev) => formFromProfile(profile, prev));
  };

  const update = (patch: Partial<LoginForm>) =>
    setForm((prev) => ({ ...prev, ...patch }));

  const updateUser = (user: string) => {
    const switched =
      passwordAutoFilled && passwordAccount !== null && user.trim() !== passwordAccount;
    if (switched) {
      setPasswordAutoFilled(false);
      setPasswordAccount(null);
      setForm((prev) => ({ ...prev, user, password: "" }));
      return;
    }
    setForm((prev) => ({ ...prev, user }));
  };


  const connected = engine.phase === "connected";
  const busy = isBusyPhase(engine.phase) || connecting || probingLines;
  const connectDisabled = busy || connected;
  const disconnectDisabled = !connected && !isBusyPhase(engine.phase);

  const submit = async () => {
    if (!form.gateway.trim()) {
      toast.error(t("请填写网关地址"));
      return;
    }
    if (!isValidGateway(form.gateway)) {
      toast.error(t("网关地址格式不正确"), {
        description: t("只填写域名或 IPv4，例如 vpn.example.com"),
      });
      return;
    }
    if (!isValidPort(form.port ?? 0)) {
      toast.error(t("端口必须在 1-65535 之间"));
      return;
    }
    if (!form.user.trim()) {
      toast.error(t("请填写账号"));
      return;
    }
    if (!form.password) {
      toast.error(t("请填写密码"));
      return;
    }

    // 多线路优选且未勾选自动选择时，先探测并按延迟让用户选择线路
    if (
      activeProfile &&
      activeProfile.connect_type === 1 &&
      !activeProfile.connect_select
    ) {
      setLinePickerOpen(true);
      setProbingLines(true);
      setLineProbes([]);
      try {
        const probes = await api.probeLines(
          activeProfile.lines,
          activeProfile.connect_timeout,
        );
        setLineProbes(probes);
      } catch (error) {
        toast.error(firstLine(String(error)), { duration: 5000 });
        setLinePickerOpen(false);
      } finally {
        setProbingLines(false);
      }
      return;
    }

    onConnect(
      { ...form, gateway: form.gateway.trim(), user: form.user.trim() },
      { passwordAutoFilled },
    );
  };

  return (
    <div className="mx-auto flex w-full max-w-4xl flex-col gap-4">
      <StatusHero
        phase={engine.phase}
        label={engine.label}
        detail={engine.detail}
        vip={engine.stats.vip}
        masked=""
      />

      {connected ? (
        <>
          <Card className="flex flex-wrap items-center justify-between gap-3 px-4 py-3">
            <div className="text-xs text-muted">
              <span className="text-faint">{t("当前账号")}</span>{" "}
              <span className="font-medium text-text">{form.user || "—"}</span>
            </div>
            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={() => setChangePasswordOpen(true)}
                className="text-[11px] font-medium text-accent hover:underline"
              >
                {t("修改密码")}
              </button>
              <Button variant="ghost" size="sm" onClick={onDisconnect}>
                {t("断开")}
              </Button>
            </div>
          </Card>
          <ResourceListCard resources={session?.resources ?? []} />
          <BasicInfoCard session={session} stats={engine.stats} />
          <TrafficChartCard stats={engine.stats} />
          <LoginInfoCard session={session} />
        </>
      ) : (
        <Card>
          <CardHeader
            title={t("网关登录")}
            description={t("账号密码通过 TLS 传输；验证码按网关要求弹出")}
          />

          <div className="space-y-3 p-5">
            <Field label={t("连接配置")} hint={t("选择已保存的配置可自动填充网关与线路")}>
              <Select
                value={selectedProfile}
                onChange={applyProfile}
                disabled={busy}
                ariaLabel={t("连接配置")}
                className="w-full"
                options={[
                  {
                    value: "manual",
                    label: t("手动输入"),
                    hint: t("自行填写网关地址与端口"),
                  },
                  ...profiles.map((profile) => ({
                    value: profile.name,
                    label: profile.name,
                    hint: `${profile.lines.length} ${t("条线路")} · ${
                      modeLabels[profile.connect_type] ?? t("未知模式")
                    }`,
                  })),
                ]}
              />
            </Field>

            {isProfileMode && activeProfile ? (
              <div className="grid gap-3 sm:grid-cols-2">
                <Field label={t("线路")}>
                  <div className="flex h-10 items-center rounded-xl border border-border bg-surface-2/40 px-3 text-sm text-text">
                    {activeProfile.lines.length} {t("条线路")}
                  </div>
                </Field>
                <Field label={t("连接模式")}>
                  <div className="flex h-10 items-center gap-1.5 rounded-xl border border-border bg-surface-2/40 px-3 text-sm text-text">
                    <span>{modeLabels[activeProfile.connect_type] ?? t("未知模式")}</span>
                    {activeProfile.connect_type === 1 ? (
                      <span className="text-[11px] text-faint">
                        · {activeProfile.connect_select ? t("自动选路") : t("手动选路")}
                      </span>
                    ) : null}
                  </div>
                </Field>
                <div className="sm:col-span-2 rounded-xl border border-border bg-surface-2/30 px-3 py-2 font-mono text-[11px] leading-relaxed text-faint">
                  {activeProfile.lines.join(" · ")}
                </div>
              </div>
            ) : (
              <div className="grid gap-3 sm:grid-cols-[1fr_120px]">
                <Field label={t("网关地址")} hint={t("域名或 IPv4，例如 vpn.example.com")}>
                  <div className="relative">
                    <Server className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-faint" />
                    <Input
                      value={form.gateway}
                      onChange={(event) => update({ gateway: event.target.value })}
                      placeholder="vpn.example.com"
                      autoComplete="off"
                      spellCheck={false}
                      className="pl-9"
                      disabled={busy}
                    />
                  </div>
                </Field>

                <Field label={t("端口")}>
                  <Input
                    type="number"
                    min={1}
                    max={65535}
                    value={form.port ?? 443}
                    onChange={(event) => update({ port: Number(event.target.value) })}
                    disabled={busy}
                  />
                </Field>
              </div>
            )}

            <div className="grid gap-3 sm:grid-cols-2">
              <Field label={t("账号")}>
                <div className="relative">
                  <UserRound className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-faint" />
                  <Input
                    value={form.user}
                    onChange={(event) => updateUser(event.target.value)}
                    placeholder={t("登录账号")}
                    autoComplete="username"
                    spellCheck={false}
                    className="pl-9"
                    disabled={busy}
                  />
                </div>
              </Field>

              <Field label={t("密码")}>
                <div className="relative">
                  <KeyRound className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-faint" />
                  <PasswordInput
                    value={form.password}
                    onChange={(event) => {
                      setPasswordAutoFilled(false);
                      update({ password: event.target.value });
                    }}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") submit();
                    }}
                    placeholder={t("登录密码")}
                    autoComplete="current-password"
                    className="pl-9"
                    disabled={busy}
                  />
                </div>
              </Field>
            </div>

            <div
              role="button"
              tabIndex={busy ? -1 : 0}
              aria-pressed={form.remember}
              onClick={() => {
                if (!busy) update({ remember: !form.remember });
              }}
              onKeyDown={(event) => {
                if (busy) return;
                if (event.key === "Enter" || event.key === " ") {
                  event.preventDefault();
                  update({ remember: !form.remember });
                }
              }}
              className="flex cursor-pointer items-center justify-between rounded-xl border border-border bg-surface-2/40 px-3 py-2.5 transition hover:bg-hover"
            >
              <div>
                <div className="text-xs font-medium text-text">{t("记住密码")}</div>
                <div className="text-[11px] text-faint">
                  {t("保存到系统凭据管理器，不可用时回退为机器绑定加密文件")}
                </div>
              </div>
              <span onClick={(event) => event.stopPropagation()}>
                <Switch
                  checked={form.remember}
                  onChange={(next) => update({ remember: next })}
                  disabled={busy}
                  label={t("记住密码")}
                />
              </span>
            </div>

            <div className="flex items-center justify-between gap-3 pt-1">
              <div className="space-y-1">
                <button
                  type="button"
                  onClick={() => setForgotOpen(true)}
                  disabled={busy}
                  className="text-[11px] font-medium text-accent hover:underline disabled:opacity-50"
                >
                  {t("忘记密码？")}
                </button>
              </div>
              <div className="flex shrink-0 items-center gap-2">
                <Button
                  variant="primary"
                  onClick={submit}
                  disabled={connectDisabled}
                  className="w-32"
                >
                  {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : null}
                  {busy ? t("处理中…") : t("连接")}
                </Button>
                <Button
                  variant="ghost"
                  onClick={onDisconnect}
                  disabled={disconnectDisabled}
                  className="w-32"
                >
                  {t("断开")}
                </Button>
              </div>
            </div>
          </div>
        </Card>
      )}

      <LogPanel logs={engine.logs} onClear={onClearLogs} />

      <LinePickerDialog
        open={linePickerOpen}
        probes={lineProbes}
        loading={probingLines}
        onClose={() => setLinePickerOpen(false)}
        onPick={(probe) => {
          setLinePickerOpen(false);
          onConnect(
            {
              ...form,
              gateway: probe.host,
              port: probe.port,
              lines: [probe.line],
              connect_type: 0,
              user: form.user.trim(),
            },
            { passwordAutoFilled },
          );
        }}
      />

      <ExpiredPasswordDialog
        open={expiredOpen && engine.passwordChange !== null}
        host={engine.passwordChange?.host || form.gateway}
        port={engine.passwordChange?.port || form.port || 443}
        user={engine.passwordChange?.user || form.user}
        message={t(engine.passwordChange?.message || "需要修改密码后才能登录")}
        onClose={() => {
          setExpiredOpen(false);
          engine.clearPasswordChange?.();
        }}
        onSaved={(newPassword) => {
          const next = { ...form, password: newPassword };
          setForm(next);
          setPasswordAutoFilled(true);
          setPasswordAccount(next.user.trim());
          setExpiredOpen(false);
          engine.clearPasswordChange?.();
          onConnect(next, { passwordAutoFilled: true });
        }}
      />

      <ChangePasswordDialog
        open={changePasswordOpen}
        onClose={() => setChangePasswordOpen(false)}
        onSaved={(newPassword) => {
          setForm((prev) => ({ ...prev, password: newPassword }));
          setPasswordAutoFilled(true);
          setPasswordAccount(form.user.trim());
        }}
      />

      <ForgotPasswordDialog
        open={forgotOpen}
        gateway={form.gateway}
        port={form.port ?? 443}
        defaultUser={form.user}
        onClose={() => setForgotOpen(false)}
        onReset={(user, newPassword) => {
          setForm((prev) => ({ ...prev, user, password: newPassword }));
          setPasswordAutoFilled(true);
          setPasswordAccount(user);
        }}
      />
    </div>
  );
}

import { Loader2, ShieldAlert, ShieldCheck, ShieldOff } from "lucide-react";

import { useI18n } from "../lib/i18n";
import type { EnginePhase } from "../types/engine";
import { Card, StatusDot, cn } from "./ui";

const BUSY: EnginePhase[] = [
  "probing",
  "captcha_pending",
  "authenticating",
  "creating_tun",
  "handshaking",
  "configuring",
  "reconnecting",
  "disconnecting",
];

export function StatusHero({
  phase,
  label,
  detail,
  vip,
  masked,
}: {
  phase: EnginePhase;
  label: string;
  detail: string;
  vip: string;
  masked: string;
}) {
  const { t } = useI18n();
  const connected = phase === "connected";
  const busy = BUSY.includes(phase);
  const failed = phase === "failed";

  return (
    <Card className="relative overflow-hidden">
      <div className="grain pointer-events-none absolute inset-0 opacity-[0.14]" />
      <div
        className={cn(
          "pointer-events-none absolute -right-24 -top-24 h-64 w-64 rounded-full blur-3xl",
          connected ? "bg-ok/15" : failed ? "bg-danger/15" : "bg-accent/10",
        )}
      />

      <div className="relative flex flex-col gap-4 p-5 lg:flex-row lg:items-center lg:justify-between">
        <div className="flex items-center gap-4">
          <div
            className={cn(
              "flex h-16 w-16 shrink-0 items-center justify-center rounded-2xl border",
              connected
                ? "border-ok/30 bg-ok-soft text-ok"
                : failed
                  ? "border-danger/30 bg-danger-soft text-danger"
                  : "border-accent/25 bg-accent-soft text-accent",
            )}
          >
            {busy && phase !== "reconnecting" ? (
              <Loader2 className="h-7 w-7 animate-spin" />
            ) : connected ? (
              <ShieldCheck className="h-7 w-7" />
            ) : failed ? (
              <ShieldAlert className="h-7 w-7" />
            ) : (
              <ShieldOff className="h-7 w-7" />
            )}
          </div>

          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <StatusDot phase={phase} />
              <span className="text-[11px] uppercase tracking-[0.18em] text-faint">
                {t("连接状态")}
              </span>
            </div>
            <h2 className="mt-1 text-xl font-semibold tracking-tight">{t(label)}</h2>
            {failed ? null : (
              <p className="mt-1 min-h-4 text-xs text-muted">
                {detail ? t(detail) : t(connected ? "内网流量已进入隧道" : "填写网关信息后开始连接")}
              </p>
            )}
          </div>
        </div>

        <div className="text-right text-[11px] text-faint">
          {t("虚拟 IP：")} <span className="font-mono text-muted">{vip || "—"}</span>
          {masked ? <span className="ml-1 font-mono">{masked}</span> : null}
        </div>
      </div>
    </Card>
  );
}

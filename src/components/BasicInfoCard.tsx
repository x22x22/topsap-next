import { Card, CardHeader } from "./ui";
import { formatBytes, formatDuration } from "../lib/format";
import { useI18n } from "../lib/i18n";
import type { SessionSummary, Stats } from "../types/engine";

export function BasicInfoCard({
  session,
  stats,
}: {
  session: SessionSummary | null;
  stats: Stats;
}) {
  const { t } = useI18n();
  const tunnelLabel: Record<number, string> = {
    0: t("透明访问"),
    1: t("透明访问"),
    2: t("网络隔离"),
    3: t("网络完全代理"),
  };
  const tunnelType = session ? (tunnelLabel[session.tunnel_type] ?? "—") : "—";

  return (
    <Card>
      <CardHeader title={t("基本信息")} />
      <dl className="grid gap-y-3 p-5 sm:grid-cols-2 sm:gap-x-16">
        <InfoRow
          label={t("网关地址")}
          value={
            session ? `${session.gateway}:${session.port}` : "—"
          }
        />
        <InfoRow label={t("当前用户")} value={session?.user || "—"} />
        <InfoRow
          label={t("网关版本")}
          value={session?.gateway_version || "—"}
        />
        <InfoRow label={t("工作状态")} value={t("隧道建立成功")} />
        <InfoRow label={t("隧道类型")} value={tunnelType} />
        <InfoRow
          label={t("连接时间")}
          value={formatDuration(stats.duration_secs)}
        />
        <InfoRow
          label={t("收发数据")}
          value={`${formatBytes(stats.bytes_up)} / ${formatBytes(stats.bytes_down)}`}
        />
      </dl>
    </Card>
  );
}

function InfoRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline justify-between gap-4 border-b border-border/60 pb-2 last:border-0 last:pb-0">
      <dt className="shrink-0 text-[11px] text-faint">{label}</dt>
      <dd className="min-w-0 break-words text-right font-mono text-xs text-text">
        {value}
      </dd>
    </div>
  );
}

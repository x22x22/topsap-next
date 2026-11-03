import { Card, CardHeader } from "./ui";
import { formatUnixTime } from "../lib/format";
import { useI18n } from "../lib/i18n";
import type { SessionSummary } from "../types/engine";

export function LoginInfoCard({ session }: { session: SessionSummary | null }) {
  const { t } = useI18n();
  const info = session?.login_info;
  const address = session?.vip || "";

  return (
    <Card>
      <CardHeader title={t("登录信息")} />
      <dl className="grid gap-y-3 p-5 sm:grid-cols-2 sm:gap-x-16">
        <InfoRow label={t("地址")} value={address || "—"} />
        <InfoRow
          label={t("本次登录成功IP")}
          value={info?.current_ip || "—"}
        />
        <InfoRow
          label={t("本次登录成功时间")}
          value={info?.current_time ? formatUnixTime(info.current_time) : "—"}
        />
        <InfoRow
          label={t("上次登录成功IP")}
          value={info?.previous_ip || "—"}
        />
        <InfoRow
          label={t("上次登录成功时间")}
          value={info?.previous_time ? formatUnixTime(info.previous_time) : "—"}
        />
        <InfoRow
          label={t("上次登录失败IP")}
          value={info?.failed_ip || "—"}
        />
        <InfoRow
          label={t("上次登录失败时间")}
          value={info?.failed_time ? formatUnixTime(info.failed_time) : "—"}
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

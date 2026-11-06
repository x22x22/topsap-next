import { AlertTriangle, Loader2 } from "lucide-react";

import { useI18n } from "../lib/i18n";
import type { RouteConflictReport } from "../types/engine";
import { Badge, Button, Modal } from "./ui";

/** 连接前检测到同前缀路由时，由用户确认是否接管 */
export function RouteConflictDialog({
  report,
  busy,
  onAccept,
  onCancel,
}: {
  report: RouteConflictReport | null;
  busy: boolean;
  onAccept: () => void;
  onCancel: () => void;
}) {
  const { t } = useI18n();
  if (!report) return null;

  const interfaces = Array.from(new Set(report.conflicts.map((item) => item.ifname)));
  const preview = report.conflicts.slice(0, 8);

  return (
    <Modal
      open
      title={t("检测到路由冲突")}
      description={t(
        "以下网段已被其他接口占用（可能是其他 VPN、旧会话残留或物理网络同网段），继续连接需要先删除这些路由",
      )}
      onClose={onCancel}
      footer={
        <>
          <Button variant="ghost" onClick={onCancel} disabled={busy}>
            {t("取消连接")}
          </Button>
          <Button variant="danger" onClick={onAccept} disabled={busy}>
            {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : null}
            {t("接管并连接")}
          </Button>
        </>
      }
    >
      <div className="space-y-3 text-xs leading-relaxed text-muted">
        <div className="flex items-start gap-2 rounded-xl border border-warn/25 bg-warn-soft px-3 py-2.5">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-warn" />
          <p className="text-[11px] leading-relaxed text-warn">
            {t("接管会删除这些冲突路由，可能中断官方 TopSAP 或其他 VPN；断开后不会自动恢复。")}
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <Badge tone="warn">{t("共 {total} 条", { total: report.total })}</Badge>
          <span className="text-[11px] text-faint">
            {t("涉及接口：{interfaces}", { interfaces: interfaces.join(", ") })}
          </span>
        </div>

        <ul className="max-h-52 space-y-1.5 overflow-y-auto rounded-xl border border-border bg-surface-2/40 p-3 font-mono text-[11px]">
          {preview.map((item) => (
            <li
              key={`${item.dest}/${item.prefix}-${item.ifname}`}
              className="flex items-center justify-between gap-3"
            >
              <span className="text-text">
                {item.dest}/{item.prefix}
              </span>
              <span className="truncate text-faint">
                {item.ifname} · {item.gateway}
              </span>
            </li>
          ))}
        </ul>

        {report.total > preview.length ? (
          <p className="text-[11px] text-faint">
            {t("仅显示前 {n} 条，完整清单已写入运行日志。", { n: preview.length })}
          </p>
        ) : null}
      </div>
    </Modal>
  );
}

import { Loader2 } from "lucide-react";

import { useI18n } from "../lib/i18n";
import type { LineProbe } from "../types/engine";
import { Badge, Button, Modal, Spinner } from "./ui";

export function LinePickerDialog({
  open,
  probes,
  loading,
  onPick,
  onClose,
}: {
  open: boolean;
  probes: LineProbe[];
  loading: boolean;
  onPick: (probe: LineProbe) => void;
  onClose: () => void;
}) {
  const { t } = useI18n();

  return (
    <Modal
      open={open}
      title={t("选择线路")}
      description={t("已按延迟从低到高排序，点击线路开始连接")}
      onClose={onClose}
      footer={
        <Button variant="ghost" onClick={onClose}>
          {t("取消")}
        </Button>
      }
    >
      {loading ? (
        <div className="flex h-32 flex-col items-center justify-center gap-2 text-muted">
          <Spinner className="h-5 w-5" />
          <span className="text-xs">{t("正在探测线路…")}</span>
        </div>
      ) : (
        <div className="space-y-1.5">
          {probes.map((probe) => (
            <button
              key={probe.line}
              type="button"
              disabled={!probe.ok}
              onClick={() => onPick(probe)}
              className="flex w-full items-center justify-between gap-3 rounded-xl border border-border bg-surface-2/40 px-3 py-2.5 text-left transition hover:border-accent/40 hover:bg-hover disabled:cursor-not-allowed disabled:opacity-50"
            >
              <span className="min-w-0">
                <span className="block truncate font-mono text-xs text-text">{probe.line}</span>
                <span className="mt-0.5 block truncate text-[11px] text-faint">
                  {probe.ok
                    ? t("延迟 {ms} ms", { ms: probe.latency_ms })
                    : probe.error || t("不可用")}
                </span>
              </span>
              {probe.ok ? (
                <Badge tone="ok">{t("可用")}</Badge>
              ) : (
                <Badge tone="danger">{t("不可用")}</Badge>
              )}
            </button>
          ))}
          {probes.length === 0 && !loading ? (
            <div className="flex h-24 items-center justify-center text-xs text-muted">
              <Loader2 className="mr-2 h-4 w-4" />
              {t("没有可用的线路")}
            </div>
          ) : null}
        </div>
      )}
    </Modal>
  );
}

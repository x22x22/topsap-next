import { useState } from "react";
import { AlertCircle, CheckCircle2, Info, Play, ShieldCheck } from "lucide-react";
import { toast } from "sonner";

import { api } from "../lib/api";
import { firstLine } from "../lib/format";
import { useI18n } from "../lib/i18n";
import { localizeSelfTestDetail } from "../lib/selfTestText";
import type { SelfTestReport, SelfTestStatus } from "../types/engine";
import { Badge, Button, Card, CardHeader, EmptyState, Spinner, cn } from "./ui";

export function SelfTestPanel() {
  const { t } = useI18n();
  const [running, setRunning] = useState(false);
  const [report, setReport] = useState<SelfTestReport | null>(null);

  const STATUS_META: Record<
    SelfTestStatus,
    { icon: typeof CheckCircle2; tone: string; label: string }
  > = {
    ok: { icon: CheckCircle2, tone: "text-ok", label: t("通过") },
    fail: { icon: AlertCircle, tone: "text-danger", label: t("失败") },
    info: { icon: Info, tone: "text-warn", label: t("提示") },
  };

  const run = async () => {
    setRunning(true);
    setReport(null);
    try {
      const result = await api.selfTest();
      setReport(result);
      if (result.ok) {
        toast.success(t("网卡自检通过（{passed}/{total}）", { passed: result.passed, total: result.total }), {
          description: t("数据面读写正常，可以尝试连接网关"),
        });
      } else {
        toast.warning(t("网卡自检未全部通过（{passed}/{total}）", { passed: result.passed, total: result.total }), {
          description: t("请查看失败项的具体原因"),
          duration: 5000,
        });
      }
    } catch (error) {
      toast.error(`${t("自检执行失败")}：${firstLine(String(error))}`, { duration: 5000 });
    } finally {
      setRunning(false);
    }
  };

  return (
    <Card>
      <CardHeader
        title={t("网卡自检")}
        description={t("临时创建虚拟网卡并验证收发，不接触网关")}
        actions={
          <Button variant="primary" size="sm" onClick={run} disabled={running}>
            {running ? <Spinner /> : <Play className="h-3.5 w-3.5" />}
            {running ? t("自检中…") : t("开始自检")}
          </Button>
        }
      />

      <div className="space-y-4 p-5">
        <p className="text-xs leading-relaxed text-muted">
          {t("自检会依次验证：管理员权限、wintun.dll、创建网卡、配置地址、回读校验、测试路由、写入路径与读取路径。它能把「网卡层故障」和「协议层故障」区分开，排障时先跑一次。")}
        </p>

        {report ? (
          <div className="space-y-3">
            <div className="flex items-center gap-2">
              <Badge tone={report.ok ? "ok" : "danger"}>
                {report.ok ? t("自检通过") : t("存在失败项")}
              </Badge>
              <span className="text-xs text-faint">
                {t("{passed}/{total} 项", { passed: report.passed, total: report.total })}
              </span>
            </div>

            <ul className="space-y-1.5">
              {report.steps.map((step) => {
                const meta = STATUS_META[step.status];
                const Icon = meta.icon;
                return (
                  <li
                    key={step.name}
                    className="flex items-start gap-2.5 rounded-xl border border-border bg-surface-2/40 px-3 py-2"
                  >
                    <Icon className={cn("mt-0.5 h-4 w-4 shrink-0", meta.tone)} />
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="text-xs font-medium text-text">{t(step.name)}</span>
                        <span className={cn("text-[10px]", meta.tone)}>{meta.label}</span>
                      </div>
                      <p className="mt-0.5 break-words text-[11px] leading-relaxed text-muted">
                        {localizeSelfTestDetail(step.detail)}
                      </p>
                    </div>
                  </li>
                );
              })}
            </ul>
          </div>
        ) : (
          <EmptyState
            icon={<ShieldCheck className="h-6 w-6" />}
            title={t("尚未运行自检")}
            description={t("建议在首次连接前或连接失败时执行一次")}
          />
        )}
      </div>
    </Card>
  );
}

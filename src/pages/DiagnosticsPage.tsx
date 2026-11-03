import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Eraser, FileWarning, RefreshCw } from "lucide-react";
import { toast } from "sonner";

import { SelfTestPanel } from "../components/SelfTestPanel";
import { Badge, Button, Card, CardHeader, Spinner, cn } from "../components/ui";
import { api } from "../lib/api";
import { firstLine } from "../lib/format";
import { useI18n } from "../lib/i18n";
import { queryKeys } from "../lib/query";

export function DiagnosticsPage() {
  const { t } = useI18n();
  const queryClient = useQueryClient();

  const residueQuery = useQuery({
    queryKey: queryKeys.residue,
    queryFn: api.checkResidue,
  });

  const purge = useMutation({
    mutationFn: api.purgeResidue,
    onSuccess: (changed) => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.residue });
      toast.success(t(changed ? "已清理 hosts 残留" : "没有需要清理的残留"));
    },
    onError: (error) => {
      toast.error(`${t("清理失败")}：${firstLine(String(error))}`, { duration: 5000 });
    },
  });

  const hasResidue = residueQuery.data?.hosts ?? false;

  const recheckResidue = async () => {
    const result = await residueQuery.refetch();
    if (result.error) {
      toast.error(`${t("检查失败")}：${firstLine(String(result.error))}`, {
        duration: 5000,
      });
      return;
    }
    if (result.data?.hosts) {
      toast.warning(t("检测到 hosts 残留，可点击清理"));
    } else {
      toast.success(t("检查完成，未发现 hosts 残留"));
    }
  };

  return (
    <div className="mx-auto flex w-full max-w-4xl flex-col gap-4">
      <SelfTestPanel />

      <Card>
        <CardHeader
          title={t("残留检查")}
          description={t("旧版本异常退出可能留下 hosts 标记段，检测到后可一键清理")}
          actions={
            <Button
              variant="ghost"
              size="sm"
              onClick={() => void recheckResidue()}
              disabled={residueQuery.isFetching}
            >
              {residueQuery.isFetching ? (
                <Spinner />
              ) : (
                <RefreshCw className="h-3.5 w-3.5" />
              )}
              {t("重新检查")}
            </Button>
          }
        />

        <div className="space-y-4 p-5">
          <div className="flex items-start gap-3 rounded-xl border border-border bg-surface-2/40 px-3 py-3">
            <FileWarning
              className={cn(
                "mt-0.5 h-4 w-4 shrink-0",
                hasResidue ? "text-warn" : "text-ok",
              )}
            />
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <span className="text-xs font-medium text-text">{t("hosts 标记段")}</span>
                <Badge tone={hasResidue ? "warn" : "ok"}>
                  {residueQuery.isLoading
                    ? t("检查中")
                    : hasResidue
                      ? t("存在残留")
                      : t("无残留")}
                </Badge>
              </div>
              <p className="mt-1 text-[11px] leading-relaxed text-muted">
                {hasResidue
                  ? t("检测到旧版本写入的内网主机名映射，清理后会从 hosts 中移除该标记段，不影响其他条目。")
                  : t("hosts 中未发现旧版本遗留的标记段。")}
              </p>
            </div>
            {hasResidue ? (
              <Button
                variant="danger"
                size="sm"
                onClick={() => purge.mutate()}
                disabled={purge.isPending}
              >
                {purge.isPending ? <Spinner /> : <Eraser className="h-3.5 w-3.5" />}
                {t("清理")}
              </Button>
            ) : null}
          </div>

          <div className="rounded-xl border border-warn/25 bg-warn-soft px-3 py-2.5">
            <p className="text-[11px] leading-relaxed text-warn">
              {t("强杀进程不会触发清理流程，可能残留路由；旧版本遗留的 hosts 标记段也可在此清理。")}
            </p>
          </div>
        </div>
      </Card>
    </div>
  );
}

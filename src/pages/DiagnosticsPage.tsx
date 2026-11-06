import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Eraser, FileWarning, Network, RefreshCw } from "lucide-react";
import { toast } from "sonner";

import { ConfirmDialog } from "../components/ConfirmDialog";
import { SelfTestPanel } from "../components/SelfTestPanel";
import { Badge, Button, Card, CardHeader, Spinner, cn } from "../components/ui";
import { api } from "../lib/api";
import { firstLine } from "../lib/format";
import { useI18n } from "../lib/i18n";
import { queryKeys } from "../lib/query";

export function DiagnosticsPage() {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const [pendingUtun, setPendingUtun] = useState<string | null>(null);

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

  const cleanupUtun = useMutation({
    mutationFn: (name: string) => api.cleanupUtunResidue(name),
    onSuccess: (result) => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.residue });
      setPendingUtun(null);
      const notify = result.still_present ? toast.warning : toast.success;
      notify(result.message || t("已清理 utun 残留"), {
        description: result.still_present
          ? t("接口仍存在，可能仍有进程持有该隧道")
          : undefined,
      });
    },
    onError: (error) => {
      toast.error(`${t("清理失败")}：${firstLine(String(error))}`, { duration: 5000 });
    },
  });

  const hasResidue = residueQuery.data?.hosts ?? false;
  const utuns = residueQuery.data?.utuns ?? [];

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
    } else if ((result.data?.utuns.length ?? 0) > 0) {
      toast.warning(t("检测到 utun 残留，可点击清理"));
    } else {
      toast.success(t("检查完成，未发现残留"));
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

          {residueQuery.data?.utun_supported ? (
            <div className="rounded-xl border border-border bg-surface-2/40 px-3 py-3">
              <div className="flex flex-wrap items-center gap-2">
                <Network
                  className={cn(
                    "h-4 w-4",
                    utuns.length ? "text-warn" : "text-ok",
                  )}
                />
                <span className="text-xs font-medium text-text">
                  {t("utun 虚拟网卡残留")}
                </span>
                <Badge tone={utuns.length ? "warn" : "ok"}>
                  {residueQuery.isLoading
                    ? t("检查中")
                    : utuns.length
                      ? t("存在 {n} 个", { n: utuns.length })
                      : t("无残留")}
                </Badge>
              </div>
              <p className="mt-1 text-[11px] leading-relaxed text-muted">
                {utuns.length
                  ? t("系统里仍有带 IPv4 地址的 utun 虚拟网卡；确认不再使用后可关闭并删除其关联路由。")
                  : t("未发现带 IPv4 地址的 utun 虚拟网卡。")}
              </p>

              {utuns.length ? (
                <ul className="mt-3 space-y-2">
                  {utuns.map((item) => (
                    <li
                      key={item.name}
                      className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-border bg-surface px-3 py-2"
                    >
                      <div className="min-w-0">
                        <div className="flex flex-wrap items-center gap-2 text-[11px]">
                          <span className="font-mono font-medium text-text">
                            {item.name}
                          </span>
                          <span className="font-mono text-muted">
                            {item.address}
                            {item.peer ? ` → ${item.peer}` : ""}
                          </span>
                          <span className="text-faint">
                            MTU {item.mtu} · {t("路由 {n} 条", { n: item.route_count })}
                          </span>
                        </div>
                        {item.active ? (
                          <p className="mt-0.5 text-[10px] text-faint">
                            {t("当前连接正在使用，断开后可清理")}
                          </p>
                        ) : null}
                      </div>
                      <Button
                        variant={item.active ? "ghost" : "danger"}
                        size="sm"
                        disabled={item.active || cleanupUtun.isPending}
                        onClick={() => setPendingUtun(item.name)}
                      >
                        {cleanupUtun.isPending && pendingUtun === item.name ? (
                          <Spinner />
                        ) : (
                          <Eraser className="h-3.5 w-3.5" />
                        )}
                        {item.active ? t("使用中") : t("清理")}
                      </Button>
                    </li>
                  ))}
                </ul>
              ) : null}
            </div>
          ) : null}

          <div className="rounded-xl border border-warn/25 bg-warn-soft px-3 py-2.5">
            <p className="text-[11px] leading-relaxed text-warn">
              {t("强杀进程不会触发清理流程，可能残留路由与 utun 网卡；旧版本遗留的 hosts 标记段也可在此清理。")}
            </p>
          </div>
        </div>
      </Card>

      <ConfirmDialog
        open={Boolean(pendingUtun)}
        title={t("清理 utun 残留")}
        description={t(
          "将关闭 {name} 并删除其关联路由，可能中断仍在运行的 VPN。确定继续？",
          { name: pendingUtun ?? "" },
        )}
        confirmLabel={t("清理")}
        busy={cleanupUtun.isPending}
        onConfirm={() => pendingUtun && cleanupUtun.mutate(pendingUtun)}
        onClose={() => setPendingUtun(null)}
      />
    </div>
  );
}

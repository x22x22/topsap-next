import { useEffect, useRef } from "react";
import { FolderOpen, ScrollText, Trash2 } from "lucide-react";
import { toast } from "sonner";

import { api } from "../lib/api";
import { firstLine } from "../lib/format";
import { useI18n } from "../lib/i18n";
import { Card, CardHeader, EmptyState, IconButton } from "./ui";
export function LogPanel({
  logs,
  onClear,
  className,
}: {
  logs: string[];
  onClear: () => void;
  className?: string;
}) {
  const { t } = useI18n();
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [logs]);

  const openLogDir = async () => {
    try {
      const path = await api.openLogDir();
      toast.success(t("已打开日志目录"), { description: path });
    } catch (error) {
      toast.error(`${t("打开日志目录失败")}：${firstLine(String(error))}`);
    }
  };

  return (
    <Card className={className}>
      <CardHeader
        title={t("运行日志")}
        description={t("密码与验证码不会写入日志")}
        actions={
          <>
            <IconButton label={t("打开日志目录")} onClick={openLogDir}>
              <FolderOpen className="h-4 w-4" />
            </IconButton>
            <IconButton label={t("清空日志")} onClick={onClear} disabled={logs.length === 0}>
              <Trash2 className="h-4 w-4" />
            </IconButton>
          </>
        }
      />
      <div
        ref={scrollRef}
        className="max-h-64 min-h-32 overflow-y-auto px-5 py-3 font-mono text-[11px] leading-relaxed"
      >
        {logs.length === 0 ? (
          <EmptyState
            icon={<ScrollText className="h-6 w-6" />}
            title={t("暂无日志")}
            description={t("连接流程的关键步骤会实时显示在这里")}
          />
        ) : (
          logs.map((line, index) => (
            <div
              key={`${index}-${line.slice(0, 12)}`}
              className="whitespace-pre-wrap break-all text-muted"
            >
              {line}
            </div>
          ))
        )}
      </div>
    </Card>
  );
}

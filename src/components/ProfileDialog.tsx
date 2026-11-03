import { useEffect, useState } from "react";
import { toast } from "sonner";

import { useI18n } from "../lib/i18n";
import type { ConnectionProfile } from "../types/engine";
import { Button, Field, Input, Modal, SegmentedControl, Switch, Textarea } from "./ui";

export function ProfileDialog({
  open,
  initial,
  onClose,
  onSubmit,
}: {
  open: boolean;
  initial: ConnectionProfile | null;
  onClose: () => void;
  onSubmit: (profile: ConnectionProfile) => void;
}) {
  const { t } = useI18n();
  const [name, setName] = useState("");
  const [mode, setMode] = useState("0");
  const [autoSelect, setAutoSelect] = useState(true);
  const [timeoutValue, setTimeoutValue] = useState("3");
  const [lines, setLines] = useState("");

  useEffect(() => {
    if (!open) return;
    setName(initial?.name ?? "");
    setMode(String(initial?.connect_type ?? 0));
    setAutoSelect(initial?.connect_select ?? true);
    setTimeoutValue(String(initial?.connect_timeout ?? 3));
    setLines((initial?.lines ?? []).join("\n"));
  }, [open, initial]);

  const submit = () => {
    const parsedLines = lines
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean);

    if (!name.trim()) {
      toast.error(t("请填写连接名称"));
      return;
    }
    if (parsedLines.length === 0) {
      toast.error(t("请至少填写一条地址（格式 host:port）"));
      return;
    }
    const timeout = Number(timeoutValue);
    if (!Number.isInteger(timeout) || timeout < 1 || timeout > 60) {
      toast.error(t("检测超时必须在 1-60 秒之间"));
      return;
    }

    onSubmit({
      name: name.trim(),
      login_way: initial?.login_way || "pwd",
      connect_type: Number(mode),
      connect_select: mode === "1" ? autoSelect : false,
      connect_timeout: timeout,
      lines: parsedLines,
    });
  };

  return (
    <Modal
      open={open}
      title={t(initial ? "编辑连接" : "新增连接")}
      description={t("地址列表每行一条，格式为 host:port（IPv6 使用 [addr]:port）")}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("取消")}
          </Button>
          <Button variant="primary" onClick={submit}>
            {t("保存")}
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <Field label={t("连接名称")}>
          <Input
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder={t("例如：办公室")}
            autoFocus
            disabled={Boolean(initial)}
          />
        </Field>

        <Field label={t("连接模式")}>
          <SegmentedControl
            value={mode}
            options={[
              { value: "0", label: t("单线路") },
              { value: "1", label: t("多线路优选") },
              { value: "2", label: t("多线路备份") },
            ]}
            onChange={setMode}
          />
        </Field>

        {mode === "1" ? (
          <div
            role="button"
            tabIndex={0}
            aria-pressed={autoSelect}
            onClick={() => setAutoSelect((value) => !value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                setAutoSelect((value) => !value);
              }
            }}
            className="flex cursor-pointer items-center justify-between rounded-xl border border-border bg-surface-2/40 px-3 py-2.5 transition hover:bg-hover"
          >
            <div>
              <div className="text-xs font-medium text-text">{t("自动选择线路")}</div>
              <div className="text-[11px] text-faint">
                {t("连接时自动探测并选择延迟最低的线路")}
              </div>
            </div>
            <span onClick={(event) => event.stopPropagation()}>
              <Switch
                checked={autoSelect}
                onChange={setAutoSelect}
                label={t("自动选择线路")}
              />
            </span>
          </div>
        ) : null}

        <Field label={t("检测超时（秒）")}>
          <Input
            type="number"
            min={1}
            max={60}
            value={timeoutValue}
            onChange={(event) => setTimeoutValue(event.target.value)}
            disabled={mode === "0"}
          />
        </Field>

        <Field label={t("地址列表")}>
          <Textarea
            rows={4}
            value={lines}
            onChange={(event) => setLines(event.target.value)}
            placeholder={"vpn1.example.com:8443\nvpn2.example.com:8443"}
            className="font-mono text-xs"
          />
        </Field>

      </div>
    </Modal>
  );
}

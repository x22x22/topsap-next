import { useEffect, useState } from "react";
import { Loader2, RefreshCw } from "lucide-react";
import { toast } from "sonner";

import { firstLine } from "../lib/format";
import { useI18n } from "../lib/i18n";
import { Button, Input, Modal } from "./ui";

export function SmsDialog({
  open,
  challenge,
  message,
  onSubmit,
  onResend,
  onCancel,
}: {
  open: boolean;
  challenge: boolean;
  message?: string;
  onSubmit: (code: string) => Promise<boolean>;
  onResend: () => Promise<number>;
  onCancel: () => void;
}) {
  const { t } = useI18n();
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [countdown, setCountdown] = useState(0);

  useEffect(() => {
    if (!open) return;
    setCode("");
    setCountdown(0);
  }, [open]);

  useEffect(() => {
    if (open && message) toast.error(message);
  }, [open, message]);

  useEffect(() => {
    if (countdown <= 0) return;
    const timer = setInterval(() => {
      setCountdown((value) => (value > 0 ? value - 1 : 0));
    }, 1000);
    return () => clearInterval(timer);
  }, [countdown]);

  const submit = async () => {
    const value = code.trim();
    if (!value) return;
    setBusy(true);
    try {
      const accepted = await onSubmit(value);
      if (accepted) {
        setCode("");
      } else {
        toast.error(t("验证码未被接受，请重新输入"));
      }
    } catch (error) {
      toast.error(firstLine(String(error)), { duration: 5000 });
    } finally {
      setBusy(false);
    }
  };

  const resend = async () => {
    if (countdown > 0) return;
    try {
      const seconds = await onResend();
      setCountdown(seconds || 60);
      toast.success(t("验证码已重新发送，有效期 {seconds} 秒", { seconds: seconds || 60 }));
    } catch (error) {
      toast.error(`${t("重新发送失败")}：${firstLine(String(error))}`, { duration: 5000 });
    }
  };

  return (
    <Modal
      open={open}
      title={t(challenge ? "输入动态口令" : "输入动态验证码")}
      description={t("网关已将验证码发送至账号绑定的企业微信")}
      onClose={onCancel}
      footer={
        <>
          <Button variant="ghost" onClick={onCancel}>
            {t("取消")}
          </Button>
          <Button variant="primary" onClick={submit} disabled={busy || !code.trim()}>
            {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : null}
            {busy ? t("提交中…") : t("确定")}
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <Input
          autoFocus
          value={code}
          maxLength={12}
          placeholder={t("输入企业微信收到的验证码")}
          onChange={(event) => setCode(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") void submit();
          }}
        />
        <button
          type="button"
          onClick={() => void resend()}
          disabled={countdown > 0}
          className="inline-flex items-center gap-1.5 text-xs text-accent hover:underline disabled:cursor-not-allowed disabled:text-faint"
        >
          <RefreshCw className="h-3.5 w-3.5" />
          {countdown > 0
            ? t("重新发送（{seconds} 秒）", { seconds: countdown })
            : t("重新发送验证码")}
        </button>
      </div>
    </Modal>
  );
}

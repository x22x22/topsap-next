import { useEffect, useState } from "react";

import { useI18n } from "../lib/i18n";
import { Button, Input, Modal } from "./ui";

export function CaptchaDialog({
  png,
  onSubmit,
  onCancel,
}: {
  png: string | null;
  onSubmit: (code: string) => void;
  onCancel: () => void;
}) {
  const { t } = useI18n();
  const [code, setCode] = useState("");
  const open = Boolean(png);

  useEffect(() => {
    setCode("");
  }, [png]);

  const submit = () => {
    const value = code.trim();
    if (!value) return;
    onSubmit(value);
    setCode("");
  };

  return (
    <Modal
      open={open}
      title={t("输入验证码")}
      description={t("验证码与本次登录会话绑定；取消后需要重新发起连接")}
      onClose={onCancel}
      footer={
        <>
          <Button variant="ghost" onClick={onCancel}>
            {t("取消")}
          </Button>
          <Button variant="primary" onClick={submit} disabled={!code.trim()}>
            {t("确定")}
          </Button>
        </>
      }
    >
      <div className="flex items-center gap-4">
        <div className="flex h-20 w-44 shrink-0 items-center justify-center overflow-hidden rounded-xl border border-border bg-white">
          {png && png !== "DEMO" ? (
            <img
              src={`data:image/png;base64,${png}`}
              alt={t("验证码")}
              className="h-full w-full object-contain"
            />
          ) : (
            <span className="text-xs text-slate-500">{t("演示模式")}</span>
          )}
        </div>
        <Input
          autoFocus
          value={code}
          maxLength={8}
          placeholder={t("输入图中字符")}
          onChange={(event) => setCode(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") submit();
          }}
          className="flex-1"
        />
      </div>
      <p className="mt-3 text-[11px] leading-relaxed text-faint">
        {t("看不清可以取消后重新连接，后端会重新获取一张验证码。")}
      </p>
    </Modal>
  );
}

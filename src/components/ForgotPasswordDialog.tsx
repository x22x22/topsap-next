import { useCallback, useEffect, useState } from "react";
import { Loader2 } from "lucide-react";
import { toast } from "sonner";

import { api } from "../lib/api";
import { firstLine } from "../lib/format";
import { useI18n } from "../lib/i18n";
import { toastPasswordSuccess } from "../lib/toast";
import type { CaptchaDto } from "../types/engine";
import { Button, Field, Input, Modal, PasswordInput } from "./ui";

const CAPTCHA_ERROR_PATTERN = /验证码错误|验证码.*(过期|失效)|-40018/;

export function ForgotPasswordDialog({
  open,
  gateway,
  port,
  defaultUser,
  onClose,
}: {
  open: boolean;
  gateway: string;
  port: number;
  defaultUser: string;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const [user, setUser] = useState(defaultUser);
  const [code, setCode] = useState("");
  const [captcha, setCaptcha] = useState<CaptchaDto | null>(null);
  const [loadingCaptcha, setLoadingCaptcha] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [step, setStep] = useState<"request" | "reset">("request");
  const [cookie, setCookie] = useState("");
  const [sms, setSms] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");

  const loadCaptcha = useCallback(async () => {
    if (!gateway.trim()) {
      toast.error(t("请填写网关地址"));
      return;
    }
    setLoadingCaptcha(true);
    try {
      setCaptcha(await api.fetchCaptcha(gateway.trim(), port));
      setCode("");
    } catch (error) {
      toast.error(`${t("获取验证码失败")}：${firstLine(String(error))}`);
    } finally {
      setLoadingCaptcha(false);
    }
  }, [gateway, port]);

  useEffect(() => {
    if (!open) return;
    setUser(defaultUser);
    setCode("");
    setCaptcha(null);
    setStep("request");
    setCookie("");
    setSms("");
    setNewPassword("");
    setConfirmPassword("");
    void loadCaptcha();
    // 每次打开时按当前网关重新获取，不跟随输入实时刷新
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const submit = async () => {
    if (!user.trim()) {
      toast.error(t("请输入用户名"));
      return;
    }

    if (step === "request") {
      if (!code.trim()) {
        toast.error(t("请输入图形验证码"));
        return;
      }
      setSubmitting(true);
      try {
        const result = await api.forgetPassword(
          gateway.trim(),
          port,
          user.trim(),
          code.trim(),
          captcha?.cookie ?? "",
        );
        if (result.need_sms) {
          setCookie(result.cookie);
          setStep("reset");
          setSms("");
          setNewPassword("");
          setConfirmPassword("");
          toast.success(t("动态验证码已发送，请查看绑定的邮箱"));
        } else {
          toast.success(
            result.target
              ? t("重置密码已发送至 {target}", { target: result.target })
              : t("请求已提交，请查看绑定的邮箱"),
          );
          onClose();
        }
      } catch (error) {
        const message = firstLine(String(error));
        toast.error(message, { duration: 5000 });
        if (!CAPTCHA_ERROR_PATTERN.test(message)) void loadCaptcha();
      } finally {
        setSubmitting(false);
      }
      return;
    }

    if (!sms.trim()) {
      toast.error(t("请输入动态验证码"));
      return;
    }
    if (!newPassword) {
      toast.error(t("请输入新密码"));
      return;
    }
    if (newPassword !== confirmPassword) {
      toast.error(t("两次输入的新密码不一致"));
      return;
    }

    setSubmitting(true);
    try {
      const result = await api.resetPassword(
        gateway.trim(),
        port,
        user.trim(),
        sms.trim(),
        newPassword,
        cookie,
      );
      toastPasswordSuccess(
        result.target
          ? t("密码已重置，通知已发送至 {target}", { target: result.target })
          : t("密码已重置，请使用新密码登录"),
      );
      onClose();
    } catch (error) {
      toast.error(firstLine(String(error)), { duration: 8000 });
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Modal
      open={open}
      title={t("忘记密码")}
      description={
        step === "request"
          ? t("网关会向账号绑定的邮箱发送重置信息")
          : t("请输入动态验证码并设置新密码")
      }
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("取消")}
          </Button>
          <Button variant="primary" onClick={submit} disabled={submitting}>
            {submitting ? <Loader2 className="h-4 w-4 animate-spin" /> : null}
            {submitting ?
              t("提交中…")
            : step === "request"
              ? t("发送")
              : t("确认重置")}
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <Field label={t("用户名")}>
          <Input
            value={user}
            onChange={(event) => setUser(event.target.value)}
            placeholder={t("登录账号")}
            autoFocus
            disabled={step === "reset"}
          />
        </Field>

        {step === "request" ? (
          <Field label={t("图形验证码")}>
            <div className="flex items-center gap-3">
              <Input
                value={code}
                onChange={(event) => setCode(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") void submit();
                }}
                maxLength={8}
                placeholder={t("输入图中字符")}
                className="flex-1"
              />
              <button
                type="button"
                onClick={() => void loadCaptcha()}
                className="flex h-10 w-36 shrink-0 items-center justify-center overflow-hidden rounded-xl border border-border bg-white"
                title={t("点击刷新验证码")}
              >
                {loadingCaptcha ? (
                  <Loader2 className="h-4 w-4 animate-spin text-slate-400" />
                ) : captcha && captcha.png_base64 !== "DEMO" ? (
                  <img
                    src={`data:image/png;base64,${captcha.png_base64}`}
                    alt={t("验证码")}
                    className="h-full w-full object-contain"
                  />
                ) : (
                  <span className="text-xs text-slate-500">{t("点击刷新")}</span>
                )}
              </button>
            </div>
          </Field>
        ) : (
          <>
            <Field label={t("动态验证码")}>
              <Input
                value={sms}
                onChange={(event) => setSms(event.target.value)}
                placeholder={t("输入邮箱收到的验证码")}
                autoFocus
                maxLength={12}
              />
            </Field>
            <Field label={t("新密码")}>
              <PasswordInput
                value={newPassword}
                onChange={(event) => setNewPassword(event.target.value)}
                placeholder={t("输入新密码")}
                autoComplete="new-password"
              />
            </Field>
            <Field label={t("确认新密码")}>
              <PasswordInput
                value={confirmPassword}
                onChange={(event) => setConfirmPassword(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") void submit();
                }}
                placeholder={t("再次输入新密码")}
                autoComplete="new-password"
              />
            </Field>
          </>
        )}
      </div>
    </Modal>
  );
}

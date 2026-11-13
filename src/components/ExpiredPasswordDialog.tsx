import { useEffect, useState } from "react";
import { Loader2 } from "lucide-react";
import { toast } from "sonner";

import { api } from "../lib/api";
import { firstLine } from "../lib/format";
import { useI18n } from "../lib/i18n";
import { toastPasswordSuccess } from "../lib/toast";
import { Button, Field, Input, Modal, PasswordInput } from "./ui";

/** 登录被网关要求先改密（密码到期、过弱、首次登录或管理员要求）时使用。 */
export function ExpiredPasswordDialog({
  open,
  host,
  port,
  user,
  message,
  onClose,
  onSaved,
}: {
  open: boolean;
  host: string;
  port: number;
  user: string;
  message: string;
  onClose: () => void;
  onSaved: (newPassword: string) => void;
}) {
  const { t } = useI18n();
  const [oldPassword, setOldPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    if (!open) return;
    setOldPassword("");
    setNewPassword("");
    setConfirmPassword("");
  }, [open]);

  const submit = async () => {
    if (!oldPassword) {
      toast.error(t("请输入原密码"));
      return;
    }
    if (!newPassword) {
      toast.error(t("请输入新密码"));
      return;
    }
    if (newPassword === oldPassword) {
      toast.error(t("新密码不能与旧密码一致"));
      return;
    }
    if (newPassword !== confirmPassword) {
      toast.error(t("两次输入的新密码不一致"));
      return;
    }

    setSubmitting(true);
    try {
      await api.changePasswordForLogin(host, port, user, oldPassword, newPassword);
      toastPasswordSuccess(t("密码修改成功（新密码已自动填入），正在自动连接"));
      onSaved(newPassword);
    } catch (error) {
      toast.error(firstLine(String(error)), { duration: 8000 });
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Modal
      open={open}
      title={t("需要修改密码")}
      description={message}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("取消")}
          </Button>
          <Button variant="primary" onClick={submit} disabled={submitting}>
            {submitting ? <Loader2 className="h-4 w-4 animate-spin" /> : null}
            {submitting ? t("提交中…") : t("确认修改")}
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <Field label={t("用户名")}>
          <Input value={user} readOnly className="bg-surface-2/60" />
        </Field>
        <Field label={t("原密码")}>
          <PasswordInput
            value={oldPassword}
            onChange={(event) => setOldPassword(event.target.value)}
            placeholder={t("请输入原密码")}
            autoFocus
            autoComplete="current-password"
          />
        </Field>
        <Field label={t("新密码")}>
          <PasswordInput
            value={newPassword}
            onChange={(event) => setNewPassword(event.target.value)}
            placeholder={t("请输入新密码")}
            autoComplete="new-password"
          />
        </Field>
        <Field label={t("确认密码")}>
          <PasswordInput
            value={confirmPassword}
            onChange={(event) => setConfirmPassword(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void submit();
            }}
            placeholder={t("请输入确认密码")}
            autoComplete="new-password"
          />
        </Field>
      </div>
    </Modal>
  );
}

import { Loader2 } from "lucide-react";

import { useI18n } from "../lib/i18n";
import { Button, Modal } from "./ui";

export function ConfirmDialog({
  open,
  title,
  description,
  confirmLabel,
  busy = false,
  onConfirm,
  onClose,
}: {
  open: boolean;
  title: string;
  description: string;
  confirmLabel: string;
  busy?: boolean;
  onConfirm: () => void;
  onClose: () => void;
}) {
  const { t } = useI18n();

  return (
    <Modal
      open={open}
      title={title}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("取消")}
          </Button>
          <Button variant="danger" onClick={onConfirm} disabled={busy}>
            {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : null}
            {confirmLabel}
          </Button>
        </>
      }
    >
      <div className="rounded-xl border border-danger/25 bg-danger-soft px-3 py-2.5 text-[11px] leading-relaxed text-danger">
        {description}
      </div>
    </Modal>
  );
}

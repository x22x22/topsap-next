import { toast } from "sonner";

/** 密码类成功提示统一走 Sonner 的 richColors success 样式。 */
export function toastPasswordSuccess(message: string, description?: string) {
  toast.success(message, { description });
}

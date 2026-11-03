import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { api } from "../lib/api";
import { firstLine } from "../lib/format";
import { translate, useI18n } from "../lib/i18n";
import { queryKeys } from "../lib/query";
import type { LoginForm } from "../types/engine";

/** 连接是长流程：登录 → 验证码 → 隧道 → 路由，进度由引擎事件驱动 */
export function useConnect() {
  const qc = useQueryClient();
  const { t } = useI18n();

  return useMutation({
    mutationFn: ({ form }: { form: LoginForm; passwordAutoFilled?: boolean }) =>
      api.connect(form),
    onSuccess: async (summary, variables) => {
      if (summary.resources.length === 0 || summary.routes === 0) {
        toast.error(t("用户未拥有任何资源地址，请联系管理员"), {
          description: t("已自动断开连接"),
          duration: 8000,
        });
        try {
          await api.disconnect();
        } catch {
          toast.error(t("自动断开失败，请手动点击断开"));
        }
        void qc.invalidateQueries({ queryKey: queryKeys.snapshot });
        void qc.invalidateQueries({ queryKey: queryKeys.residue });
        return;
      }

      void qc.invalidateQueries({ queryKey: queryKeys.settings });
      void qc.invalidateQueries({ queryKey: queryKeys.residue });
      const detail = t("网关 {gateway}:{port} · 路由 {routes} 条", {
        gateway: summary.gateway,
        port: summary.port,
        routes: summary.routes,
      });
      toast.success(t("已连接，虚拟 IP {vip}", { vip: summary.vip || t("未知") }), {
        description: variables?.passwordAutoFilled
          ? `${detail} · ${t("登录密码已自动填入")}`
          : detail,
        duration: 5000,
      });
    },
    onError: (error) => {
      toast.error(firstLine(String(error)), { duration: 5000 });
    },
  });
}

export function useDisconnect() {
  const qc = useQueryClient();
  const { t } = useI18n();

  return useMutation({
    mutationFn: () => api.disconnect(),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.snapshot });
      toast.success(t("已断开并恢复网络配置"));
    },
    onError: (error) => {
      toast.error(`${t("断开失败")}：${firstLine(String(error))}`, { duration: 5000 });
    },
  });
}

/** 提交验证码，成功唤醒后端等待中的登录流程 */
export async function submitCaptcha(code: string): Promise<boolean> {
  try {
    return await api.submitCaptcha(code);
  } catch (error) {
    toast.error(`${translate("提交验证码失败")}：${firstLine(String(error))}`);
    return false;
  }
}

/** 提交短信/企业微信动态验证码 */
export async function submitSms(code: string): Promise<boolean> {
  try {
    return await api.submitSms(code);
  } catch (error) {
    toast.error(`${translate("提交动态验证码失败")}：${firstLine(String(error))}`);
    return false;
  }
}

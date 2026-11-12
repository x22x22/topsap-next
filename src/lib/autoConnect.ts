/**
 * 启动自动连接的可用性判断（纯函数）。
 *
 * 只有同时具备「连接配置 + 账号 + 已保存密码」时才允许勾选；后端启动时
 * 会用同一组条件再校验一次，不满足则静默跳过并在运行日志说明原因。
 */

export type AutoConnectBlockReason = "profiles" | "account" | "password" | null;

export function autoConnectAvailability(input: {
  profileCount: number;
  user: string;
  hasPassword: boolean;
}): { available: boolean; reason: AutoConnectBlockReason } {
  if (input.profileCount <= 0) return { available: false, reason: "profiles" };
  if (!input.user.trim()) return { available: false, reason: "account" };
  if (!input.hasPassword) return { available: false, reason: "password" };
  return { available: true, reason: null };
}

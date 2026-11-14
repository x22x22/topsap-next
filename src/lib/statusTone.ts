import type { EnginePhase } from "../types/engine";

/** 连接状态色：与托盘图标保持一致 */
export type StatusTone = "ok" | "danger" | "info" | "closing" | "faint";

/**
 * 把引擎阶段映射为统一的五种状态色：
 * - ok：已连接（绿）
 * - info：连接中/重连中（蓝）
 * - closing：正在断开（浅蓝，人为操作，不按异常处理）
 * - danger：连接失败，或连接过之后异常断开（红）
 * - faint：从未连接，或人为断开完成（灰）
 */
export function statusTone(
  phase: EnginePhase,
  everConnected = false,
  abnormalDisconnect = false,
): StatusTone {
  if (phase === "connected") return "ok";
  if (phase === "disconnecting") return "closing";
  if (phase === "failed") return "danger";
  if (phase === "idle") {
    if (abnormalDisconnect && everConnected) return "danger";
    return "faint";
  }
  return "info";
}

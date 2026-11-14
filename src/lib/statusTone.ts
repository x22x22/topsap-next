import type { EnginePhase } from "../types/engine";

/** 连接状态色：与托盘图标保持一致 */
export type StatusTone = "ok" | "danger" | "info" | "faint";

/**
 * 把引擎阶段映射为统一的四种状态色：
 * - ok：已连接（绿）
 * - info：连接中/重连中（蓝）
 * - danger：连接失败，或成功连接过之后断开（红）
 * - faint：从未连接（灰）
 */
export function statusTone(phase: EnginePhase, everConnected = false): StatusTone {
  if (phase === "connected") return "ok";
  if (phase === "failed") return "danger";
  if (phase === "idle" || phase === "disconnecting") {
    return everConnected ? "danger" : "faint";
  }
  return "info";
}

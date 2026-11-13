import type { SessionSummary } from "../types/engine";

/** 会话摘要是否缺少可用资源（无门户资源或无可注入路由） */
export function isSessionEmpty(
  summary: Pick<SessionSummary, "resources" | "routes">,
): boolean {
  return summary.resources.length === 0 || summary.routes === 0;
}

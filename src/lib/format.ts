/** 纯格式化/校验函数，保持无副作用以便单测 */

import { localizeError } from "./errorText";

/** 字节数按 1024 进位，0 与负数归零 */
export function formatBytes(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let size = value;
  let index = 0;
  while (size >= 1024 && index < units.length - 1) {
    size /= 1024;
    index += 1;
  }
  return `${size.toFixed(index === 0 ? 0 : 1)} ${units[index]}`;
}

/** 秒 -> HH:MM:SS */
export function formatDuration(totalSeconds: number): string {
  const seconds = Math.max(0, Math.floor(Number.isFinite(totalSeconds) ? totalSeconds : 0));
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const rest = seconds % 60;
  return [hours, minutes, rest].map((v) => String(v).padStart(2, "0")).join(":");
}

/** 多行错误只取首行，并去掉 JS Error 的 "Error:" 前缀，避免 toast 重复 */
export function firstLine(text: string): string {
  const line = text
    .split(/\r?\n/)[0]
    ?.trim()
    .replace(/^error:\s*/i, "");
  return localizeError(line || text);
}

/** 网关地址只接受主机名或 IPv4，不接受协议前缀、路径与空白 */
export function isValidGateway(value: string): boolean {
  const host = value.trim();
  if (!host || host.length > 255) return false;
  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(host)) return false;
  if (/[/\\\s]/.test(host)) return false;

  // 纯数字与点必须构成完整 IPv4，否则容易被误当成主机名
  if (/^\d+(\.\d+)*$/.test(host)) {
    if (!/^\d{1,3}(\.\d{1,3}){3}$/.test(host)) return false;
    return host.split(".").every((part) => {
      const n = Number(part);
      return Number.isInteger(n) && n >= 0 && n <= 255;
    });
  }

  // 主机名
  return /^[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?)*$/.test(
    host,
  );
}

/** Unix 秒 -> 本地时间字符串；空值、0 或非数字原样返回 */
export function formatUnixTime(value: string): string {
  const seconds = Number(value);
  if (!Number.isFinite(seconds) || seconds <= 0) return value || "—";
  const date = new Date(seconds * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(
    date.getHours(),
  )}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

/** 端口范围校验 */
export function isValidPort(value: number): boolean {
  return Number.isInteger(value) && value >= 1 && value <= 65535;
}

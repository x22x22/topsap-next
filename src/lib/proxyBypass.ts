export const DEFAULT_PROXY_BYPASS = ["*.sunline.cn", "*.sunline-i.com"];

const MAX_ENTRIES = 128;
const MAX_LEN = 255;
const ALLOWED = /^[a-z0-9.*?:<>\[\]_-]+$/i;

/** 返回规范化条目；非法条目返回 null。 */
export function normalizeProxyEntry(raw: string): string | null {
  const entry = raw.trim().toLowerCase();
  if (!entry || entry.length > MAX_LEN || entry === "*") return null;
  if (!ALLOWED.test(entry)) return null;
  if (entry.replace(/[*?.-]/g, "").length === 0) return null;
  return entry;
}

export function addProxyEntry(
  entries: string[],
  raw: string,
): { entries: string[]; error?: string } {
  const entry = normalizeProxyEntry(raw);
  if (!entry) {
    return { entries, error: "请输入合法的域名、泛域名或 IP 地址" };
  }
  if (entries.some((item) => item.toLowerCase() === entry)) {
    return { entries, error: "该条目已存在" };
  }
  if (entries.length >= MAX_ENTRIES) {
    return { entries, error: "代理绕过条目不能超过 128 条" };
  }
  return { entries: [...entries, entry] };
}

/** 恢复默认时保留用户自定义条目，只补齐两个默认泛域名。 */
export function mergeDefaultProxyBypass(entries: string[]): string[] {
  const out = [...entries];
  for (const item of DEFAULT_PROXY_BYPASS) {
    if (!out.some((entry) => entry.toLowerCase() === item)) out.push(item);
  }
  return out;
}

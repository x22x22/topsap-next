/**
 * 隧道路由生效范围的纯函数（与后端 `route_policy.rs` 的归一化规则保持一致）。
 *
 * 设置里以 `目的地址/前缀` 保存被关闭的网段；这里只负责 key 生成、校验与
 * 勾选状态计算，真正的路由注入过滤在后端连接时完成。
 */

const MAX_DISABLED = 256;

function parseIpv4(raw: string): number[] | null {
  const parts = raw.trim().split(".");
  if (parts.length !== 4) return null;
  const octets: number[] = [];
  for (const part of parts) {
    if (!/^\d{1,3}$/.test(part)) return null;
    const value = Number(part);
    if (value > 255) return null;
    octets.push(value);
  }
  return octets;
}

function prefixFromMask(mask: number[]): number | null {
  const value =
    ((mask[0] << 24) | (mask[1] << 16) | (mask[2] << 8) | mask[3]) >>> 0;
  let prefix = 0;
  for (let bit = 31; bit >= 0; bit -= 1) {
    if ((value & (1 << bit)) === 0) break;
    prefix += 1;
  }
  const rebuilt = prefix === 0 ? 0 : (0xffffffff << (32 - prefix)) >>> 0;
  return rebuilt === value ? prefix : null;
}

function maskOctets(prefix: number): number[] {
  const value = prefix === 0 ? 0 : (0xffffffff << (32 - prefix)) >>> 0;
  return [
    (value >>> 24) & 0xff,
    (value >>> 16) & 0xff,
    (value >>> 8) & 0xff,
    value & 0xff,
  ];
}

function networkOctets(dest: number[], prefix: number): number[] {
  const mask = maskOctets(prefix);
  return dest.map((octet, index) => octet & mask[index]);
}

/** 把用户输入归一化为 `网络地址/前缀`；非法返回 null。 */
export function normalizeRouteKey(raw: string): string | null {
  const text = raw.trim();
  if (!text || text.length > 64) return null;
  let destText: string;
  let prefix: number;
  if (text.includes("/")) {
    const parts = text.split("/");
    if (parts.length !== 2) return null;
    destText = parts[0];
    // 严格校验十进制前缀：Number("") 为 0、Number("0x18") 为 24、Number("1e1") 为 10，
    // 都不能被当成合法掩码，规则需与后端 parse::<u8>() 保持一致
    const prefixText = parts[1].trim();
    if (!/^\d{1,2}$/.test(prefixText)) return null;
    prefix = Number(prefixText);
  } else {
    const parts = text.split(/\s+/);
    if (parts.length !== 2) return null;
    destText = parts[0];
    const mask = parseIpv4(parts[1]);
    if (!mask) return null;
    const parsed = prefixFromMask(mask);
    if (parsed === null) return null;
    prefix = parsed;
  }
  if (!Number.isInteger(prefix) || prefix < 0 || prefix > 32) return null;
  const dest = parseIpv4(destText);
  if (!dest) return null;
  return `${networkOctets(dest, prefix).join(".")}/${prefix}`;
}

/** 服务端返回的路由对应的设置 key。 */
export function routeKey(dest: string, prefix: number): string {
  return `${dest}/${prefix}`;
}

/** 判断一条路由是否生效（未出现在关闭列表中即生效）。 */
export function isRouteEnabled(disabled: string[], dest: string, prefix: number): boolean {
  const key = routeKey(dest, prefix);
  return !disabled.includes(key);
}

/** 切换单条路由的生效状态，返回新的关闭列表。 */
export function setRouteEnabled(
  disabled: string[],
  dest: string,
  prefix: number,
  enabled: boolean,
): string[] {
  const key = routeKey(dest, prefix);
  if (enabled) return disabled.filter((item) => item !== key);
  if (disabled.includes(key)) return disabled;
  if (disabled.length >= MAX_DISABLED) return disabled;
  return [...disabled, key];
}

/** 启用当前网关下发的全部路由；列表之外的手动排除项保持不变。 */
export function enableRoutes(
  disabled: string[],
  routes: Array<{ dest: string; prefix: number }>,
): string[] {
  const keys = new Set(routes.map((route) => routeKey(route.dest, route.prefix)));
  return disabled.filter((item) => !keys.has(item));
}

/** 关闭当前网关下发的全部路由，与已有的手动排除项合并，不丢弃用户配置。 */
export function disableRoutes(
  disabled: string[],
  routes: Array<{ dest: string; prefix: number }>,
): { entries: string[]; truncated: boolean } {
  const merged = new Set(disabled);
  for (const route of routes) {
    merged.add(routeKey(route.dest, route.prefix));
  }
  const entries = Array.from(merged);
  return {
    entries: entries.slice(0, MAX_DISABLED),
    truncated: entries.length > MAX_DISABLED,
  };
}

export const ROUTE_DISABLED_LIMIT = MAX_DISABLED;

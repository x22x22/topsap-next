/** 网卡自检明细的英文映射：后端明细为中文，英文模式下按已知句式翻译。 */

import { currentLanguage } from "./i18n";
import { localizeError } from "./errorText";

const DETAIL_EN: Array<[RegExp, string]> = [
  [/^已提权$/, "Elevated"],
  [/^未提权，无法创建虚拟网卡；请以管理员身份运行$/,
    "Not elevated; the virtual adapter cannot be created. Run as administrator."],
  [/^未提权，无法创建 TUN 接口；请重新启动并在系统授权框中允许管理员权限，或以 root\/sudo 运行$/,
    "Not elevated; the TUN interface cannot be created. Restart and allow administrator access, or run as root/sudo."],
  [/^当前未以 root 运行，无法创建 TUN 接口；请在终端执行 (.+) 启动应用$/,
    "Not running as root; the TUN interface cannot be created. Start the app from a terminal with $1."],
  [/^文件不存在：(.+)$/, "File not found: $1"],
  [/^找不到 wintun\.dll：(.+)$/, "wintun.dll not found: $1"],
  [/^加载 wintun\.dll 失败：(.+)$/, "Failed to load wintun.dll: $1"],
  [/^wintun\.dll 缺少导出函数 (.+)：(.+)$/, "wintun.dll is missing export $1: $2"],
  [/^创建 TUN 设备失败：当前用户没有创建 TUN 接口的权限；Linux 版需要以 root 运行，请在终端执行 (.+) 启动应用$/,
    "Failed to create the TUN device: the current user lacks permission to create a TUN interface. The Linux build must run as root; start it from a terminal with $1."],
  [/^创建 TUN 设备失败：网卡 (.+) 已被占用，可能是上次自检或连接未退出；请退出所有 TopSAP 实例，或执行 sudo ip link del (.+) 后重试$/,
    "Failed to create the TUN device: adapter $1 is already in use, likely left by a previous self-test or connection. Quit all TopSAP instances, or run sudo ip link del $1 and retry."],
  [/^创建 TUN 设备失败：(.+)（请检查特权 helper 或 root 权限）$/,
    "Failed to create the TUN device: $1 (check the privileged helper or root access)"],
  [/^创建虚拟网卡失败$/, "Failed to create the virtual adapter"],
  [/^使用系统 TUN，无需额外驱动$/, "Using the system TUN device; no extra driver is required"],
  [/^已删除遗留的 TopSAP 虚拟网卡$/, "Removed the leftover TopSAP virtual adapter"],
  [/^未发现遗留的 TopSAP 虚拟网卡$/, "No leftover TopSAP virtual adapter was found"],
  [/^已重置遗留的 TopSAP 网卡状态$/, "Reset the state of the leftover TopSAP adapter"],
  [/^已删除遗留的 topsap0 虚拟网卡$/, "Removed the leftover topsap0 TUN interface"],
  [/^未发现遗留的 TUN 虚拟网卡$/, "No leftover TUN interface was found"],
  [/^删除 topsap0 失败：(.+)$/, "Failed to delete topsap0: $1"],
  [/^删除 topsap0 失败$/, "Failed to delete topsap0"],
  [/^topsap0 仍存在$/, "topsap0 is still present"],
  [/^系统 TUN 随进程关闭自动释放，无需清理$/,
    "The system TUN device is released when the process exits; no cleanup is needed"],
  [/^删除虚拟网卡失败：网卡会话已存在或网卡状态异常，请清理残留网卡后重试（Win32 1247）；请先断开连接或手动删除网卡后重试$/,
    "Failed to delete the virtual adapter: the session already exists or the adapter is in a bad state (Win32 1247). Disconnect first or delete the adapter manually and retry."],
  [/^删除虚拟网卡失败：(.+)$/, "Failed to delete the virtual adapter: $1"],
  [/^需要管理员权限：(.+)$/, "Administrator privileges required: $1"],
  [/^虚拟网卡操作失败：(.+)$/, "Virtual adapter operation failed: $1"],
  [/^hosts 写入失败：(.+)$/, "Failed to write hosts: $1"],
  [/^地址已生效（(.+)）$/, "Address is active ($1)"],
  [/^地址未生效$/, "Address is not active"],
  [/^(.+) 已生效$/, "$1 is active"],
  [/^(.+) 未生效$/, "$1 is not active"],
  [/^已就绪$/, "Ready"],
  [/^执行超时（60 秒）$/, "Timed out after 60 seconds"],
  [/^ping 执行超时（60 秒）$/, "ping timed out after 60 seconds"],
  [/^已向网卡注入 (\d+) 字节 ICMP 请求（(.+) → (.+)）$/,
    "Injected a $1-byte ICMP request into the adapter ($2 → $3)"],
  [/^写入失败，网卡会话不可用$/, "Write failed; the adapter session is unavailable"],
  [/^读循环提前结束$/, "The read loop ended early"],
  [/^3 秒内未收到系统发往网卡的 ICMP 请求；请检查测试路由是否生效、网卡读路径是否可用$/,
    "No ICMP request from the system to the adapter was received within 3 seconds; check that the test route is active and the adapter read path works"],
  [/^ping (.+) 成功，系统已把包交给网卡$/,
    "ping $1 succeeded; the system delivered packets to the adapter"],
  [/^ping (.+) 未通（测试目标无对端，未收到回包属正常）$/,
    "ping $1 failed (the test target has no peer, so no reply is expected)"],
  [/^未发现同前缀路由$/, "No same-prefix route was found"],
  [/^(.+)\/(.+) 已被 (.+)（(.+)）占用；请先在诊断页清理 utun 残留或退出其他 VPN$/,
    "$1/$2 is already owned by $3 ($4); clean up utun residue on the Diagnostics page or quit the other VPN first"],
  [/^已跳过；处理冲突后重新运行自检$/,
    "Skipped; resolve the conflict and run the self-test again"],
  [/^网卡 (.+)$/, "Adapter $1"],
  [/^空报文$/, "Empty packet"],
  [/^IPv6 报文（(\d+) 字节）$/, "IPv6 packet ($1 bytes)"],
  [/^未知版本 (\d+)（(\d+) 字节）$/, "Unknown version $1 ($2 bytes)"],
];

/** 翻译自检步骤明细；中文模式与未匹配的句子原样返回 */
export function localizeSelfTestDetail(detail: string): string {
  if (currentLanguage() === "zh") return detail;
  const reply = detail.match(/^收到系统回包：(.+)$/);
  if (reply) return `System reply received: ${localizeSelfTestDetail(reply[1])}`;
  const request = detail.match(/^收到系统发往网卡的 ICMP 请求：(.+)$/);
  if (request) {
    return `Received an ICMP request from the system to the adapter: ${localizeSelfTestDetail(request[1])}`;
  }
  const retry = detail.match(
    /^(.+)；请退出其他 TopSAP 实例，或执行 sudo ip link del topsap0 后重试$/,
  );
  if (retry) {
    return `${localizeSelfTestDetail(retry[1])}. Quit other TopSAP instances, or run sudo ip link del topsap0 and retry.`;
  }
  for (const [pattern, replacement] of DETAIL_EN) {
    if (pattern.test(detail)) return detail.replace(pattern, replacement);
  }
  return localizeError(detail);
}

/**
 * 后端错误文案的英文映射。
 *
 * Rust 侧错误为中文，前端在英文模式下按已知句式做结构化翻译；
 * 未匹配的句子原样返回，避免吞掉诊断信息。
 */

import { currentLanguage } from "./i18n";

const REASON_EN: Array<[RegExp, string]> = [
  [/未知主机（错误码 (\d+)）/, "unknown host (error $1)"],
  [/DNS 解析暂时失败（错误码 (\d+)）/, "temporary DNS failure (error $1)"],
  [/域名没有可用的地址记录（错误码 (\d+)）/, "the domain has no address record (error $1)"],
  [/DNS 解析超时/, "DNS lookup timed out"],
  [/未解析到任何地址/, "no address was resolved"],
  [/连接被拒绝，目标端口未开放（错误码 (\d+)）/, "connection refused; the port is closed (error $1)"],
  [/连接超时（错误码 (\d+)）/, "connection timed out (error $1)"],
  [/网络不可达（错误码 (\d+)）/, "network unreachable (error $1)"],
  [/主机不可达（错误码 (\d+)）/, "host unreachable (error $1)"],
  [/访问被拒绝（错误码 (\d+)）/, "access denied (error $1)"],
  [/连接被拒绝，目标端口未开放/, "connection refused; the port is closed"],
  [/连接超时/, "connection timed out"],
  [/访问被拒绝/, "access denied"],
  [/目标端口未返回有效的 TLS 响应，可能不是 HTTPS 服务/,
    "the port did not return a valid TLS response and may not be HTTPS"],
  [/握手超时，网关端口可能不可达/, "the TLS handshake timed out; the gateway port may be unreachable"],
];

function localizeReason(reason: string): string {
  for (const [pattern, replacement] of REASON_EN) {
    if (pattern.test(reason)) return reason.replace(pattern, replacement);
  }
  return reason;
}

export function localizeError(message: string): string {
  if (currentLanguage() === "zh") return message;
  let text = message;

  text = text.replace(
    /^无法解析网关地址「(.+?)」：(.+?)，请检查域名拼写或网络连接$/,
    (_match, host: string, reason: string) =>
      `Unable to resolve gateway "${host}": ${localizeReason(reason)}. Check the domain spelling or network connection.`,
  );
  text = text.replace(
    /^无法连接网关 (.+?)：(.+?)，请检查端口是否正确以及网关是否可达$/,
    (_match, addr: string, reason: string) =>
      `Unable to connect to gateway ${addr}: ${localizeReason(reason)}. Check the port and network reachability.`,
  );
  text = text.replace(
    /^TLS 握手失败：(.+)$/,
    (_match, reason: string) => `TLS handshake failed: ${localizeReason(reason)}`,
  );

  const fixed: Array<[RegExp, string]> = [
    [/^网关响应超时（(\d+) 秒），请检查网络连接后重试$/,
      "The gateway timed out after $1s. Check the network connection and retry."],
    [/^网关探测超时（(\d+) 秒），请检查网络连接或端口是否正确$/,
      "Gateway probe timed out after $1s. Check the network connection and port."],
    [/^验证码输入超时（(\d+) 秒）$/, "Captcha input timed out after $1s."],
    [/^动态验证码输入超时（(\d+) 秒）$/, "One-time code input timed out after $1s."],
    [/^该端口返回的是网页而不是 VPN 网关接口（HTTP (\d+)），请检查端口是否正确$/,
      "This port returned a web page instead of the VPN gateway API (HTTP $1). Check the port."],
    [/^网关返回 HTTP (\d+)，请确认端口填写正确且该端口提供 VPN 网关服务$/,
      "The gateway returned HTTP $1. Verify that the port points to the VPN gateway."],
    [/^网关拒绝访问（HTTP (\d+)），请检查账号权限或网关配置$/,
      "The gateway denied access (HTTP $1). Check the account permissions or gateway configuration."],
    [/^网关接口不存在（HTTP 404），请检查网关地址与端口$/,
      "The gateway API was not found (HTTP 404). Check the gateway address and port."],
    [/^网关服务异常（HTTP (\d+)），请稍后重试或联系管理员$/,
      "The gateway service is unavailable (HTTP $1). Retry later or contact the administrator."],
    [/^网关返回异常状态（HTTP (\d+)），请检查网关地址与端口$/,
      "The gateway returned an unexpected status (HTTP $1). Check the gateway address and port."],
    [/^网关响应异常，请检查网关地址与端口$/, "Unexpected gateway response. Check the gateway address and port."],
    [/^网关返回内容无法识别，请检查网关地址与端口$/,
      "The gateway response could not be recognized. Check the gateway address and port."],
    [/^网关返回内容不完整，请确认该端口提供的是 VPN 网关服务$/,
      "The gateway response was incomplete. Confirm that the port serves the VPN gateway."],
    [/^账号或密码错误（Ret=-40081）$/, "Incorrect username or password (Ret=-40081)."],
    [/^需要修改密码后才能登录$/, "Change your password before signing in"],
    [/^登录用户密码有效期已到，请修改密码$/,
      "Your password has expired. Change it to continue."],
    [/^密码过于简单，请修改密码$/,
      "Your password is too weak. Change it to continue."],
    [/^首次登录，请修改初始密码$/, "First login: change the initial password."],
    [/^管理员要求修改密码$/, "The administrator requires a password change."],
    [/^验证码错误或已过期，请重新获取（Ret=-40018）$/,
      "The captcha is incorrect or expired. Fetch a new one (Ret=-40018)."],
    [/^账号已在别处登录，请确认官方客户端已完全退出后重试（Ret=-40077）$/,
      "This account is signed in elsewhere. Fully close the official client and retry (Ret=-40077)."],
    [/^账号已被锁定，请联系管理员（Ret=-40050）$/,
      "The account is locked. Contact the administrator (Ret=-40050)."],
    [/^重置密码过于频繁，请稍后再试（Ret=-40142）$/,
      "Too many password reset attempts. Try again later (Ret=-40142)."],
    [/^需要输入动态验证码（Ret=-40039）$/, "A one-time code is required (Ret=-40039)."],
    [/^动态验证码错误或已过期（Ret=-40045）$/, "The one-time code is incorrect or expired (Ret=-40045)."],
    [/^请求参数格式错误，请检查账号或密码是否包含特殊字符（Ret=-7）$/,
      "Invalid request parameters. Check the username or password for special characters (Ret=-7)."],
    [/^新密码不符合网关密码策略（Ret=-40226）$/,
      "The new password does not meet the gateway password policy (Ret=-40226)."],
    [/^网关版本不支持该功能，请联系管理员升级（Ret=-40227）$/,
      "The gateway version does not support this feature. Ask the administrator to upgrade (Ret=-40227)."],
    [/^网关版本不支持该功能，请联系管理员升级（Ret=-40057）$/,
      "The gateway version does not support this feature. Ask the administrator to upgrade (Ret=-40057)."],
    [/^网关参数获取失败，请稍后重试（Ret=-40228）$/,
      "Failed to obtain gateway parameters. Retry later (Ret=-40228)."],
    [/^登录被拒绝（Ret=(-?\d+)）$/, "Login rejected (Ret=$1)."],
    [/^连接已取消$/, "Connection cancelled."],
    [/^验证码输入已取消$/, "Captcha input cancelled."],
    [/^动态验证码输入已取消$/, "One-time code input cancelled."],
    [/^已有连接操作正在进行，请稍候$/, "A connection operation is already in progress. Please wait."],
    [/^请先断开连接后再运行自检，避免误删正在使用的虚拟网卡$/,
      "Disconnect before running the self-test so the adapter in use is not deleted."],
    [/^自检正在进行中，请稍候$/, "A self-test is already running. Please wait."],
    [/^自检超时（45 秒），已中止；下次自检会先清理上次残留的虚拟网卡$/,
      "The self-test timed out after 45 seconds and was stopped; the next run will clean up the leftover adapter first."],
    [/^路由注入全部失败，内网流量无法进入隧道$/,
      "All route injections failed; intranet traffic cannot enter the tunnel."],
    [/^所有线路均不可用$/, "No line is available."],
    [/^虚拟网卡已释放$/, "The virtual adapter has been released."],
    [/^登录会话已失效，请重新连接后再修改密码$/,
      "The session has expired. Reconnect before changing the password."],
    [/^无法确定当前登录账号$/, "Unable to determine the signed-in account."],
    [/^请先连接网关后再修改密码$/, "Connect to the gateway before changing the password."],
    [/^当前没有等待中的动态验证码$/, "There is no pending one-time code."],
    [/^请输入原密码$/, "Enter the current password."],
    [/^请输入新密码$/, "Enter a new password."],
    [/^没有可用的网关地址$/, "No gateway address is available."],
    [/^网络错误：(.+)$/, "Network error: $1"],
    [/^网络连接异常：(.+)$/, "Network connection error: $1"],
  ];

  for (const [pattern, replacement] of fixed) {
    if (pattern.test(text)) return text.replace(pattern, replacement);
  }
  return text;
}

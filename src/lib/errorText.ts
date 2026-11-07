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

const WIN32_HINTS: Array<[string, string]> = [
  ["拒绝访问，需要管理员权限运行", "access denied; administrator privileges are required"],
  ["无法连接到设备，驱动可能未加载", "cannot connect to the device; the driver may not be loaded"],
  ["参数错误", "invalid parameter"],
  ["驱动尚未安装，请重新安装客户端", "the driver is not installed; reinstall the client"],
  ["找不到设备元素", "device element not found"],
  [
    "网卡会话已存在或网卡状态异常，请清理残留网卡后重试",
    "the adapter session already exists or the adapter is in a bad state; clean up the leftover adapter and retry",
  ],
  ["未知 Win32 错误", "unknown Win32 error"],
];

function localizeWin32Hint(hint: string): string {
  const hit = WIN32_HINTS.find(([chinese]) => chinese === hint);
  return hit ? hit[1] : hint;
}

function localizeAddress(value: string): string {
  return value === "无地址" ? "no address" : value;
}

function localizeAttempts(value: string): string {
  return value.replace(/第(\d+)轮/g, "round $1").replace(/，/g, ", ");
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
  text = text.replace(
    /^已取消连接：检测到路由冲突$/,
    "Connection cancelled: route conflicts were detected",
  );
  text = text.replace(
    /^路由冲突确认已取消$/,
    "Route conflict confirmation was cancelled",
  );
  text = text.replace(
    /^路由冲突确认超时（(\d+) 秒）$/,
    "Route conflict confirmation timed out after $1 seconds",
  );
  text = text.replace(
    /^只允许清理 utun\* 虚拟网卡$/,
    "Only utun* virtual interfaces can be cleaned up",
  );
  text = text.replace(
    /^自检进行中，暂时无法清理 utun 残留$/,
    "The self-test is running; utun residue cannot be cleaned up right now",
  );
  text = text.replace(
    /^该虚拟网卡正在使用，请先断开连接$/,
    "This virtual interface is in use; disconnect first",
  );
  text = text.replace(
    /^当前平台不支持清理 utun 残留$/,
    "This platform does not support utun residue cleanup",
  );
  text = text.replace(
    /^(.+) 关闭失败，仍有 (\d+) 条路由：(.+)$/,
    "Failed to close $1; $2 routes remain: $3",
  );
  text = text.replace(
    /^(.+) 关闭失败，仍有 (\d+) 条路由$/,
    "Failed to close $1; $2 routes remain",
  );
  text = text.replace(/^(.+) 已不存在$/, "$1 no longer exists");
  text = text.replace(
    /^(.+) 已移除，关联路由已清理$/,
    "$1 was removed and its routes were cleaned up",
  );
  text = text.replace(
    /^(.+) 已关闭，关联路由已清理$/,
    "$1 was closed and its routes were cleaned up",
  );
  text = text.replace(
    /^(.+) 仍存在，残留 (\d+) 条路由；可能仍有进程持有该隧道$/,
    "$1 is still present with $2 routes left; a process may still own the tunnel",
  );
  text = text.replace(
    /^配置网卡地址失败（当前：(.+)）；(请确认特权 helper 已安装，或以 root\/sudo 运行|请以 root 运行（sudo \/usr\/bin\/topsap-client）)$/,
    (_match, current: string, hint: string) => {
      const action = hint.startsWith("请确认")
        ? "Confirm the privileged helper is installed, or run as root/sudo."
        : "Run the app as root with sudo /usr/bin/topsap-client.";
      return `Failed to configure the adapter address (current: ${localizeAddress(current)}). ${action}`;
    },
  );
  text = text.replace(
    /^设置网卡地址失败（当前：(.+)；(.+)）$/,
    (_match, current: string, attempts: string) =>
      `Failed to set the adapter address (current: ${localizeAddress(current)}; ${localizeAttempts(attempts)})`,
  );
  text = text.replace(
    /^(创建虚拟网卡失败|启动网卡会话失败)：(.+)（Win32 (\d+)）$/,
    (_match, action: string, hint: string, code: string) => {
      const label =
        action === "创建虚拟网卡失败"
          ? "Failed to create the virtual adapter"
          : "Failed to start the adapter session";
      return `${label}: ${localizeWin32Hint(hint)} (Win32 ${code})`;
    },
  );

  const fixed: Array<[RegExp, string]> = [
    [/^当前已是最新版本$/, "You're already up to date"],
    [/^当前平台不支持自动更新$/, "Auto update is not supported on this platform"],
    [/^检查更新失败：(.+)$/, "Failed to check for updates: $1"],
    [/^创建更新检查客户端失败：(.+)$/, "Failed to create the update client: $1"],
    [/^读取版本信息失败：(.+)$/, "Failed to read version information: $1"],
    [/^版本文件内容无法识别$/, "The version file content could not be recognized."],
    [/^当前版本号无法解析$/, "The current version number could not be parsed."],
    [/^创建更新临时目录失败：(.+)$/, "Failed to create the update temp directory: $1"],
    [/^创建下载客户端失败：(.+)$/, "Failed to create the download client: $1"],
    [/^连接下载源失败：(.+)$/, "Failed to connect to the update source: $1"],
    [/^下载更新包失败：HTTP (\d+)$/, "Failed to download the update: HTTP $1"],
    [/^下载更新包中断：(.+)$/, "Update download interrupted: $1"],
    [/^写入更新包失败：(.+)$/, "Failed to write the update package: $1"],
    [/^下载不完整，请重试$/, "The download is incomplete. Please retry."],
    [/^下载到的文件为空$/, "The downloaded update package is empty."],
    [/^下载到的文件不是有效的 ZIP 更新包$/, "The downloaded file is not a valid ZIP update package."],
    [/^更新包落盘失败：(.+)$/, "Failed to save the update package: $1"],
    [/^更新包缺少 topsap-client\.exe$/, "The update package is missing topsap-client.exe."],
    [/^更新包缺少 resources\/(.+)$/, "The update package is missing resources/$1."],
    [/^更新包包含非法路径$/, "The update package contains an invalid path."],
    [/^解压更新包失败：(.+)$/, "Failed to extract the update package: $1"],
    [/^创建解压目录失败：(.+)$/, "Failed to create the extraction directory: $1"],
    [/^读取更新包条目失败：(.+)$/, "Failed to read an update package entry: $1"],
    [/^写入更新文件失败：(.+)$/, "Failed to write an update file: $1"],
    [/^读取更新包失败：(.+)$/, "Failed to read the update package: $1"],
    [/^创建目录失败：(.+)$/, "Failed to create a directory: $1"],
    [/^创建临时目录失败：(.+)$/, "Failed to create the temp directory: $1"],
    [/^创建挂载目录失败：(.+)$/, "Failed to create the mount directory: $1"],
    [/^创建暂存目录失败：(.+)$/, "Failed to create the staging directory: $1"],
    [/^应用包名称不合法$/, "The application bundle name is invalid."],
    [/^未定位到当前应用包$/, "The current application bundle could not be located."],
    [/^挂载更新镜像失败：(.+)$/, "Failed to mount the update image: $1"],
    [/^更新镜像中未找到应用包$/, "No application bundle was found in the update image."],
    [/^复制新应用包失败：(.+)$/, "Failed to copy the new application bundle: $1"],
    [/^更新包签名校验失败：(.+)$/, "Update package signature verification failed: $1"],
    [/^更新包 ad-hoc 签名后仍无法通过校验：(.+)$/, "The update package still failed verification after ad-hoc signing: $1"],
    [/^当前应用从镜像或隔离位置运行，请先拖入「应用程序」后再更新$/, "The app is running from a disk image or translocated location. Move it to Applications before updating."],
    [/^管理员授权未完成：(.+)$/, "Administrator authorization was not completed: $1"],
    [/^启动更新助手失败：(.+)$/, "Failed to start the update helper: $1"],
    [/^写入更新脚本失败：(.+)$/, "Failed to write the update script: $1"],
    [/^写入授权脚本失败：(.+)$/, "Failed to write the authorization script: $1"],
    [/^启动管理员授权失败：(.+)$/, "Failed to start administrator authorization: $1"],
    [/^更新前断开连接失败：(.+)$/, "Failed to disconnect before updating: $1"],
    [/^更新前仍存在未清理的网络配置，已取消更新$/, "Network settings are still not fully cleaned up. The update was cancelled."],
    [/^获取程序路径失败：(.+)$/, "Failed to locate the application path: $1"],
    [/^获取安装目录失败$/, "Failed to locate the installation directory."],
    [/^读取脚本权限失败：(.+)$/, "Failed to read the script permissions: $1"],
    [/^设置脚本权限失败：(.+)$/, "Failed to set the script permissions: $1"],
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
    [/^网卡自检正在进行中，请稍候再连接$/,
      "The adapter self-test is running. Wait for it to finish before connecting."],
    [/^请先等待当前连接流程结束或断开连接后再运行自检$/,
      "Wait for the current connection attempt to finish, or disconnect, before running the self-test."],
    [/^创建 TUN 设备失败：当前用户没有创建 TUN 接口的权限；Linux 版需要以 root 运行，请在终端执行 (.+) 启动应用$/,
      "Failed to create the TUN device: the current user lacks permission to create a TUN interface. The Linux build must run as root; start it from a terminal with $1."],
    [/^创建 TUN 设备失败：网卡 (.+) 已被占用，可能是上次自检或连接未退出；请退出所有 TopSAP 实例，或执行 sudo ip link del (.+) 后重试$/,
      "Failed to create the TUN device: adapter $1 is already in use, likely left by a previous self-test or connection. Quit all TopSAP instances, or run sudo ip link del $1 and retry."],
    [/^创建 TUN 设备失败：(.+)（请检查特权 helper 或 root 权限）$/,
      "Failed to create the TUN device: $1 (check the privileged helper or root access)"],
    [/^创建 TUN 设备失败：(.+)（请检查 \/dev\/net\/tun 是否可用以及运行权限）$/,
      "Failed to create the TUN device: $1 (check that /dev/net/tun is available and the run permission)"],
    [/^创建 TUN 设备失败：(.+)$/, "Failed to create the TUN device: $1"],
    [/^配置 TUN 非阻塞模式失败：(.+)$/, "Failed to configure the TUN non-blocking mode: $1"],
    [/^接管 TUN 失败：(.+)$/, "Failed to take over the TUN device: $1"],
    [/^特权 helper 未运行，请重新启动应用以安装或修复 helper$/,
      "The privileged helper is not running. Restart the app to install or repair it."],
    [/^未找到虚拟网卡「(.+)」，驱动可能尚未就绪$/,
      'Adapter "$1" was not found; the driver may not be ready'],
    [/^掩码「(.+)」不是合法的连续掩码$/, 'Mask "$1" is not a valid contiguous mask'],
    [/^读取 hosts 失败：(.+)$/, "Failed to read hosts: $1"],
    [/^写入 hosts 失败：(.+)$/, "Failed to write hosts: $1"],
    [/^路由注入全部失败，内网流量无法进入隧道$/,
      "All route injections failed; intranet traffic cannot enter the tunnel."],
    [/^所有线路均不可用$/, "No line is available."],
    [/^虚拟网卡已释放$/, "The virtual adapter has been released."],    [/^登录会话已失效，请重新连接后再修改密码$/,
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

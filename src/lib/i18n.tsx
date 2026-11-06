/**
 * 轻量界面国际化。
 *
 * 以中文原文为 key，英文缺失时自动回退中文，避免漏翻导致空白。
 * 语言偏好写入 localStorage，重启后保持。
 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";

export type Language = "zh" | "en";
export type LanguageMode = "system" | Language;

const STORAGE_KEY = "topsap.lang";

const EN: Record<string, string> = {
  // 应用框架
  "Sunline VPN 客户端": "Sunline VPN client",
  "连接": "Connect",
  "网关登录与隧道": "Gateway login and tunnel",
  "诊断": "Diagnostics",
  "网卡自检与残留清理": "Adapter self-test and residue cleanup",
  "设置": "Settings",
  "连接管理与偏好": "Connections and preferences",
  "已连接": "Connected",
  "未连接": "Disconnected",
  "虚拟 IP": "Virtual IP",
  "版本": "Version",
  "展开侧栏": "Expand sidebar",
  "收起侧栏": "Collapse sidebar",
  "切换到浅色主题": "Switch to light theme",
  "切换到深色主题": "Switch to dark theme",
  "切换到英文": "Switch to English",
  "切换到中文": "Switch to Chinese",
  "关闭": "Close",
  "取消": "Cancel",
  "确定": "OK",
  "确认": "Confirm",
  "请选择": "Select",
  "显示密码": "Show password",
  "隐藏密码": "Hide password",
  "保存": "Save",
  "删除": "Delete",
  "编辑": "Edit",
  "新增": "Add",
  "刷新": "Refresh",
  "重试": "Retry",
  "加载中…": "Loading…",
  "未知": "Unknown",
  "未知模式": "Unknown mode",
  "已启用": "Enabled",
  "未启用": "Disabled",
  "已提权": "Elevated",
  "未提权": "Not elevated",
  "未设置": "Not set",
  "—": "—",

  // 连接状态
  "连接状态": "Connection status",
  "未连接，填写网关信息后开始连接": "Disconnected. Fill in the gateway to connect.",
  "内网流量已进入隧道": "Intranet traffic is routed through the tunnel",
  "填写网关信息后开始连接": "Fill in the gateway information to connect",
  "正在探测网关": "Probing gateway",
  "等待验证码": "Waiting for captcha",
  "等待动态验证码": "Waiting for one-time code",
  "正在登录": "Signing in",
  "正在创建网卡": "Creating adapter",
  "正在建立隧道": "Establishing tunnel",
  "正在配置网络": "Configuring network",
  "正在重连": "Reconnecting",
  "正在断开": "Disconnecting",
  "连接失败": "Connection failed",
  "断开": "Disconnect",
  "处理中…": "Working…",
  "连接中…": "Connecting…",

  // 登录表单
  "网关登录": "Gateway login",
  "账号密码通过 TLS 传输；验证码按网关要求弹出":
    "Credentials are sent over TLS; a captcha appears when required",
  "明文仅在 TLS 内": "Plaintext inside TLS only",
  "连接配置": "Connection profile",
  "选择已保存的配置可自动填充网关与线路":
    "Pick a saved profile to fill in the gateway and lines",
  "手动输入": "Manual input",
  "自行填写网关地址与端口": "Enter gateway address and port manually",
  "线路": "Lines",
  "连接模式": "Connection mode",
  "条线路": "lines",
  "单线路": "Single line",
  "多线路优选": "Multi-line preferred",
  "多线路备份": "Multi-line backup",
  "网关地址": "Gateway address",
  "域名或 IPv4，例如 vpn.example.com": "Domain or IPv4, e.g. vpn.example.com",
  "端口": "Port",
  "账号": "Username",
  "登录账号": "Username",
  "密码": "Password",
  "登录密码": "Password",
  "记住密码": "Remember password",
  "保存到系统安全存储，不可用时回退为机器绑定加密文件":
    "Stored in the system secure storage; falls back to a machine-bound encrypted file",
  "忘记密码？": "Forgot password?",
  "请填写网关地址": "Enter the gateway address",
  "网关地址格式不正确": "Invalid gateway address",
  "只填写域名或 IPv4，例如 vpn.example.com": "Enter a domain or IPv4 only, e.g. vpn.example.com",
  "端口必须在 1-65535 之间": "Port must be between 1 and 65535",
  "请填写账号": "Enter the username",
  "请填写密码": "Enter the password",
  "请输入有效的域名或 IPv4 地址，不要带协议前缀":
    "Enter a valid domain or IPv4 address without a protocol prefix",
  "端口范围 1-65535": "Port range 1-65535",
  "密码长度过短，请确认是否输入完整": "Password looks too short — check the input",

  // 统计
  "上行": "Upload",
  "下行": "Download",
  "接收包": "Packets in",
  "发送包": "Packets out",
  "路由": "Routes",
  "连接时长": "Duration",

  // 日志
  "运行日志": "Runtime log",
  "密码与验证码不会写入日志": "Passwords and verification codes are never logged",
  "打开日志目录": "Open log folder",
  "清空日志": "Clear log",
  "暂无日志": "No log entries",
  "连接流程的关键步骤会实时显示在这里":
    "Key connection steps appear here in real time",
  "已打开日志目录": "Log folder opened",
  "打开日志目录失败": "Failed to open log folder",

  // 验证码与二次认证
  "输入验证码": "Enter captcha",
  "验证码与本次登录会话绑定；取消后需要重新发起连接":
    "The captcha is bound to this login session; cancelling requires a new connection",
  "演示模式": "Demo mode",
  "输入图中字符": "Enter the characters",
  "看不清可以取消后重新连接，后端会重新获取一张验证码。":
    "If unreadable, cancel and reconnect to fetch a new captcha.",
  "点击刷新验证码": "Click to refresh captcha",
  "点击刷新": "Click to refresh",
  "验证码": "Captcha",
  "输入动态验证码": "Enter one-time code",
  "输入动态口令": "Enter one-time passcode",
  "网关已将验证码发送至账号绑定的企业微信":
    "The gateway sent the code to the bound WeCom account",
  "输入企业微信收到的验证码": "Enter the code from WeCom",
  "重新发送验证码": "Resend code",
  "重新发送（{seconds} 秒）": "Resend ({seconds}s)",
  "验证码错误或已过期，请重新输入": "The code is wrong or expired. Try again.",
  "验证码已失效，请重新输入": "The code is no longer valid. Try again.",
  "请输入动态口令": "Enter the one-time passcode",
  "验证码未被接受，请重新输入": "The code was not accepted. Try again.",
  "重新发送失败": "Resend failed",
  "验证码已重新发送，有效期 {seconds} 秒": "Code resent; valid for {seconds}s",
  "提交验证码失败": "Failed to submit captcha",
  "提交动态验证码失败": "Failed to submit one-time code",

  // 忘记密码
  "网关会向账号绑定的邮箱发送重置信息":
    "The gateway sends reset instructions to the bound email",
  "输入邮箱收到的验证码": "Enter the code sent to your email",
  "请输入动态验证码并设置新密码": "Enter the one-time code and set a new password",
  "用户名": "Username",
  "图形验证码": "Image captcha",
  "发送": "Send",
  "发送中…": "Sending…",
  "确认重置": "Reset password",
  "提交中…": "Submitting…",
  "动态验证码": "One-time code",
  "新密码": "New password",
  "确认新密码": "Confirm new password",
  "输入新密码": "Enter a new password",
  "再次输入新密码": "Enter the new password again",
  "请输入用户名": "Enter the username",
  "请输入图形验证码": "Enter the image captcha",
  "请输入动态验证码": "Enter the one-time code",
  "请输入新密码": "Enter a new password",
  "两次输入的新密码不一致": "The new passwords do not match",
  "动态验证码已发送，请查看绑定的邮箱":
    "One-time code sent. Check the bound email.",
  "重置密码已发送至 {target}": "Reset instructions sent to {target}",
  "请求已提交，请查看绑定的邮箱":
    "Request submitted. Check the bound email.",
  "密码已重置，通知已发送至 {target}（新密码已自动填入）":
    "Password reset; notification sent to {target} (the new password has been filled in automatically)",
  "密码已重置，请使用新密码登录（新密码已自动填入）":
    "Password reset. Sign in with the new password (it has been filled in automatically).",
  "获取验证码失败": "Failed to fetch captcha",

  // 连接配置
  "新增连接": "New profile",
  "编辑连接": "Edit profile",
  "地址列表每行一条，格式为 host:port（IPv6 使用 [addr]:port）":
    "One address per line as host:port (IPv6: [addr]:port)",
  "连接名称": "Profile name",
  "例如：办公室": "e.g. Office",
  "检测超时（秒）": "Probe timeout (s)",
  "地址列表": "Address list",
  "自动选择线路": "Auto-select line",
  "连接时自动探测并选择延迟最低的线路":
    "Probe all lines on connect and pick the lowest latency",
  "请填写连接名称": "Enter a profile name",
  "请至少填写一条地址（格式 host:port）": "Add at least one address (host:port)",
  "检测超时必须在 1-60 秒之间": "Probe timeout must be between 1 and 60 seconds",

  // 设置
  "通用": "General",
  "修改后点击底部保存生效": "Changes take effect after saving",
  "关闭窗口时最小化到托盘": "Minimize to tray on close",
  "关闭按钮不退出程序，连接保持；从托盘菜单可彻底退出":
    "The close button keeps the connection and hides the window; quit from the tray menu",
  "断线自动重连": "Auto reconnect",
  "隧道异常断开后按指数退避重试，最多 5 次；主动断开不触发":
    "Retries with exponential backoff (up to 5 times) after an unexpected drop; manual disconnects are ignored",
  "主题": "Theme",
  "深浅色跟随或固定": "Follow the system or choose a fixed theme",
  "跟随系统": "Follow system",
  "深色": "Dark",
  "浅色": "Light",
  "语言": "Language",
  "界面显示语言": "Interface language",
  "界面显示语言；跟随系统时按系统语言解析":
    "Interface language; Follow system resolves it from the OS language",
  "保存设置": "Save settings",
  "保存中…": "Saving…",
  "设置已保存": "Settings saved",
  "保存失败": "Save failed",
  "连接管理": "Connection profiles",
  "保存多个网关配置，支持多线路优选与备份":
    "Save multiple gateway profiles with preferred or backup lines",
  "暂无连接配置": "No connection profiles",
  "点击「新增」保存第一个网关配置":
    "Click Add to save your first gateway profile",
  "默认": "Default",
  "默认连接": "Default profile",
  "添加": "Add",
  "代理绕过": "Proxy bypass",
  "指定域名/IP 不走系统代理": "Keep selected domains/IPs out of the system proxy",
  "写入系统代理绕过列表，让指定域名/IP 不经过本地代理；仅对使用系统代理的程序生效，TUN/透明代理无法覆盖。":
    "Writes the entries into the system proxy bypass list. Only apps that use the system proxy are affected; TUN/transparent proxies are not covered.",
  "启用代理绕过": "Enable proxy bypass",
  "关闭后只移除本应用写入的条目，不修改其他代理设置":
    "When disabled, only entries written by this app are removed; other proxy settings stay untouched",
  "恢复默认": "Restore defaults",
  "代理绕过已包含默认条目":
    "The default proxy bypass entries are already present",
  "已恢复默认代理绕过条目": "Default proxy bypass entries restored",
  "*.example.com 或 192.0.2.10": "*.example.com or 192.0.2.10",
  "暂无代理绕过条目": "No proxy bypass entries",
  "默认会加入 *.sunline.cn 与 *.sunline-i.com":
    "By default *.sunline.cn and *.sunline-i.com are included",
  "当前系统不支持或未检测到可用的系统代理设置":
    "This system does not support, or no usable system proxy settings were detected",
  "代理绕过已关闭": "Proxy bypass is disabled",
  "未设置代理绕过条目": "No proxy bypass entries configured",
  "已写入系统代理绕过列表（{count} 条）":
    "Written to the system proxy bypass list ({count} entries)",
  "尚未写入系统代理绕过列表，保存设置后重试":
    "Not written to the system proxy bypass list yet; save the settings and retry",
  "请输入合法的域名、泛域名或 IP 地址":
    "Enter a valid domain, wildcard domain or IP address",
  "该条目已存在": "This entry already exists",
  "代理绕过条目不能超过 128 条":
    "No more than 128 proxy bypass entries are allowed",
  "设为默认": "Set as default",
  "已将「{name}」设为默认连接": "Set “{name}” as default profile",
  "连接配置已保存": "Profile saved",
  "连接配置已删除": "Profile deleted",
  "连接配置已清空": "Profiles cleared",
  "导入配置": "Import",
  "导出配置": "Export",
  "清空配置": "Clear",
  "已导入 {count} 条连接配置": "Imported {count} profiles",
  "已导出 {count} 条连接配置": "Exported {count} profiles",
  "导入失败": "Import failed",
  "导出失败": "Export failed",
  "确定删除连接「{name}」？": "Delete profile “{name}”?",
  "确定清空全部连接配置？此操作不可撤销。":
    "Clear all connection profiles? This cannot be undone.",
  "仅保存非敏感字段，密码单独存放在系统凭据中":
    "Only non-sensitive fields are stored; the password lives in the system credential store",
  "网关": "Gateway",
  "关于": "About",
  "平台": "Platform",
  "管理员权限": "Administrator rights",
  "外观": "Appearance",
  "深浅色主题与界面语言": "Theme and interface language",

  // 诊断
  "网卡自检": "Adapter self-test",
  "不接触网关，验证驱动、建卡、配址与双向收发":
    "Verifies the driver, adapter creation, address setup and bidirectional traffic without touching the gateway",
  "开始自检": "Run self-test",
  "自检中…": "Running…",
  "通过": "Passed",
  "失败": "Failed",
  "定位 wintun.dll": "Locate wintun.dll",
  "清理上次自检残留": "Clean up the previous self-test adapter",
  "TUN 设备": "TUN device",
  "创建虚拟网卡与会话": "Create virtual adapter and session",
  "配置网卡地址": "Configure adapter address",
  "回读网卡地址": "Read back adapter address",
  "注入测试路由": "Inject test route",
  "启动读循环": "Start read loop",
  "发送方向（应用 → 网卡）": "Send path (app → adapter)",
  "接收方向（网卡 → 应用）": "Receive path (adapter → app)",
  "系统路由（跨网段投递）": "System route (cross-subnet delivery)",
  "残留检测": "Residue check",
  "检查 hosts 中是否遗留本程序的标记段":
    "Check for leftover hosts entries created by this app",
  "检查残留": "Check residue",
  "清理残留": "Clean residue",
  "未发现残留": "No residue found",
  "发现 hosts 残留": "Hosts residue found",
  "已清理残留": "Residue cleaned",

  // 通用状态
  "已连接，虚拟 IP {vip}": "Connected, virtual IP {vip}",
  "网关 {gateway}:{port} · 路由 {routes} 条":
    "Gateway {gateway}:{port} · {routes} routes",
  "已断开并恢复网络配置": "Disconnected and network settings restored",
  "断开失败": "Disconnect failed",
  "连接已取消": "Connection cancelled",
  // 设置窗口与状态细节
  "托盘与自动重连": "Tray and auto-reconnect",
  "应用行为与连接偏好": "App behavior and connection preferences",
  "网关配置与线路": "Gateways and lines",
  "更改即时生效": "Changes apply immediately",
  "安全提示": "Security notice",
  "正在校验账号": "Verifying account",
  "正在检查上次会话": "Checking the previous session",
  "正在获取验证码": "Fetching captcha",
  "等待输入验证码": "Waiting for captcha input",
  "正在注入路由": "Injecting routes",
  "正在创建虚拟网卡": "Creating virtual adapter",
  "正在配置网卡地址": "Configuring adapter address",
  "{count} 条": "{count} routes",
  "网关证书普遍自签或已过期，本客户端按协议要求跳过证书链校验且不发送 SNI。":
    "Gateway certificates are often self-signed or expired; per protocol requirements this client skips chain validation and sends no SNI.",
  "这意味着 TLS 层无法识别中间人，请仅在可信网络中使用，并通过其他渠道核对网关地址。":
    "TLS therefore cannot detect a man-in-the-middle. Use it only on trusted networks and verify the gateway address through another channel.",
  "当前未以 root 运行，无法创建 TUN 接口与注入路由。请在终端执行 sudo open \"/Applications/TopSAP Client.app\"，或直接使用 sudo 启动应用。":
    "The app is not running as root, so it cannot create a TUN interface or inject routes. Run sudo open \"/Applications/TopSAP Client.app\" or launch it with sudo.",
  "当前未以 root 运行，无法创建 TUN 接口与注入路由。请在终端执行 sudo /usr/bin/topsap-client 启动应用。":
    "The app is not running as root, so it cannot create a TUN interface or inject routes. Start it from a terminal with sudo /usr/bin/topsap-client.",
  "当前未以管理员身份运行，创建虚拟网卡与注入路由会失败。请右键「以管理员身份运行」，或使用安装包的提权启动方式。":
    "The app is not running as administrator, so adapter creation and route injection will fail. Right-click and choose Run as administrator, or launch it from the installer.",
  "需要 root 权限": "Root access required",
  "Linux 版需要 root 权限才能创建 TUN 接口与修改路由":
    "The Linux build needs root access to create the TUN interface and modify routes",
  "当前以普通用户运行，创建 TUN 接口与修改路由会失败。请关闭窗口，在终端执行以下命令重新启动：":
    "The app is running as a regular user, so creating the TUN interface and modifying routes will fail. Close the window and restart from a terminal with:",
  "便携包请把路径替换为实际的 topsap-client 路径。":
    "For the portable build, replace the path with the actual topsap-client path.",
  "我知道了": "Got it",
  "尚未导入连接配置": "No connection profiles yet",
  "请先前往「设置」-「连接管理」中导入配置":
    "Import a profile first in Settings > Connection profiles.",
  "导入后可直接选择网关与线路，无需每次手动填写。":
    "After importing, you can select a gateway and lines instead of entering them manually each time.",
  "稍后": "Later",
  "前往设置": "Open settings",
  "可更新": "Update",
  "发现新版本": "New version available",
  "当前版本 v{current}，最新版本 v{latest}": "Current version v{current}, latest v{latest}",
  "建议更新到最新版本以获取修复与改进，点击「前往」打开下载页面。":
    "Update to the latest version for fixes and improvements. Click Download to open the download page.",
  "前往": "Download",
  "无法打开下载页面": "Unable to open the download page",
  "检查更新": "Check for updates",
  "已是最新版本": "You're up to date",
  "检查更新失败": "Failed to check for updates",
  // 诊断
  "残留检查": "Residue check",
  "旧版本异常退出可能留下 hosts 标记段，检测到后可一键清理":
    "Older versions could leave a hosts block after an abrupt exit; clean it here if detected",
  "重新检查": "Check again",
  "hosts 标记段": "Hosts block",
  "检查中": "Checking",
  "存在残留": "Residue found",
  "无残留": "No residue",
  "检测到旧版本写入的内网主机名映射，清理后会从 hosts 中移除该标记段，不影响其他条目。":
    "A hostname mapping written by an older version is present. Cleaning removes only this block and leaves other entries untouched.",
  "hosts 中未发现旧版本遗留的标记段。":
    "No legacy block written by this app was found in hosts.",
  "清理": "Clean",
  "已清理 hosts 残留": "Hosts residue cleaned",
  "没有需要清理的残留": "No residue to clean",
  "清理失败": "Cleanup failed",
  "检查失败": "Check failed",
  "检测到 hosts 残留，可点击清理":
    "Hosts residue detected; you can clean it now",
  "检查完成，未发现 hosts 残留":
    "Check complete; no hosts residue found",
  "强杀进程不会触发清理流程，可能残留路由；旧版本遗留的 hosts 标记段也可在此清理。":
    "Force-killing the process skips cleanup and may leave routes behind; any legacy hosts block can also be cleaned here.",
  // 路由冲突
  "检测到路由冲突": "Route conflicts detected",
  "以下网段已被其他接口占用（可能是其他 VPN、旧会话残留或物理网络同网段），继续连接需要先删除这些路由":
    "The following subnets are already owned by another interface (another VPN, a leftover session, or the physical network); connecting requires removing those routes",
  "取消连接": "Cancel connection",
  "接管并连接": "Take over and connect",
  "接管会删除这些冲突路由，可能中断官方 TopSAP 或其他 VPN；断开后不会自动恢复。":
    "Taking over deletes these conflicting routes and may interrupt the official TopSAP client or another VPN; they are not restored automatically after disconnecting.",
  "共 {total} 条": "{total} routes",
  "涉及接口：{interfaces}": "Interfaces: {interfaces}",
  "仅显示前 {n} 条，完整清单已写入运行日志。":
    "Showing the first {n} entries; the full list is written to the runtime log.",
  "连接流程已结束，无需再确认": "The connection flow has already ended; no confirmation is needed",
  "提交失败": "Submission failed",
  // 自检
  "提示": "Notice",
  "网卡自检通过（{passed}/{total}）": "Adapter self-test passed ({passed}/{total})",
  "数据面读写正常，可以尝试连接网关":
    "Data path reads and writes are healthy; try connecting to the gateway",
  "网卡自检未全部通过（{passed}/{total}）":
    "Adapter self-test did not fully pass ({passed}/{total})",
  "请查看失败项的具体原因": "Review the failed steps",
  "自检执行失败": "Self-test failed to run",
  "临时创建虚拟网卡并验证收发，不接触网关":
    "Creates a temporary adapter and verifies traffic without touching the gateway",
  "自检会依次验证：管理员权限、wintun.dll、创建网卡、配置地址、回读校验、测试路由、写入路径与读取路径。它能把「网卡层故障」和「协议层故障」区分开，排障时先跑一次。":
    "The self-test checks administrator rights, wintun.dll, adapter creation, address configuration, read-back, test route, write path and read path. It separates adapter faults from protocol faults; run it first when troubleshooting.",
  "自检通过": "Self-test passed",
  "存在失败项": "Some steps failed",
  "自检进行中": "Self-test running",
  "自检中断": "Self-test interrupted",
  "已完成 {n} 项": "{n} steps completed",
  "正在执行下一项…": "Running the next step…",
  "{passed}/{total} 项": "{passed}/{total} steps",
  "尚未运行自检": "Self-test not run yet",
  "建议在首次连接前或连接失败时执行一次":
    "Run it before the first connection or when a connection fails",
  // 线路选择
  "选择线路": "Select line",
  "已按延迟从低到高排序，点击线路开始连接":
    "Sorted by latency; click a line to connect",
  "正在探测线路…": "Probing lines…",
  "延迟 {ms} ms": "{ms} ms",
  "可用": "Available",
  "不可用": "Unavailable",
  "没有可用的线路": "No lines available",
  "忘记密码": "Forgot password",
  "保存主题偏好失败": "Failed to save theme preference",
  "版本与安全说明": "Version and security",
  "自动选路": "auto",
  "手动选路": "manual",
  // 修改密码
  "修改密码": "Change password",
  "修改当前登录账号的密码，需验证原密码":
    "Change the current account password; the old password is required",
  "原密码": "Current password",
  "请输入原密码": "Enter the current password",
  "确认密码": "Confirm password",
  "请输入确认密码": "Enter the password again",
  "密码修改成功，请重新登录（新密码已自动填入）":
    "Password changed. Please sign in again (the new password has been filled in).",
  "登录密码已自动填入": "Sign-in password filled in automatically",
  "新密码不能与旧密码一致":
    "The new password must be different from the old password",
  "用户未拥有任何资源地址，请联系管理员":
    "This user has no resource addresses. Please contact the administrator.",
  "已自动断开连接": "Disconnected automatically",
  "自动断开失败，请手动点击断开":
    "Automatic disconnect failed; please disconnect manually",
  "您的账号将于 {days} 天后到期，请联系管理员":
    "Your account will expire in {days} days. Please contact the administrator.",
  "您的密码将在 {days} 天后过期，请及时修改！":
    "Your password will expire in {days} days. Please change it in time.",
  "需要修改密码": "Password change required",
  "需要修改密码后才能登录": "Change your password before signing in",
  "登录用户密码有效期已到，请修改密码":
    "Your password has expired. Change it to continue.",
  "密码过于简单，请修改密码": "Your password is too weak. Change it to continue.",
  "首次登录，请修改初始密码": "First login: change the initial password.",
  "管理员要求修改密码": "The administrator requires a password change.",
  "密码修改成功，请重新连接": "Password changed. Reconnecting…",
  "确认修改": "Change password",
  "虚拟 IP：": "Virtual IP:",
  "当前账号": "Current account",
  "主导航": "Main navigation",
  "清空连接配置": "Clear connection profiles",
  "删除连接配置": "Delete connection profile",
  "基本信息": "Basic information",
  "当前用户": "Current user",
  "网关版本": "Gateway version",
  "工作状态": "Status",
  "隧道建立成功": "Tunnel established",
  "隧道类型": "Tunnel type",
  "连接时间": "Connected for",
  "收发数据": "Sent / received",
  "透明访问": "Transparent proxy",
  "网络隔离": "Network isolation",
  "网络完全代理": "Full proxy",
  "登录信息": "Login information",
  "地址": "Address",
  "本次登录成功IP": "Current login IP",
  "本次登录成功时间": "Current login time",
  "上次登录成功IP": "Previous login IP",
  "上次登录成功时间": "Previous login time",
  "上次登录失败IP": "Last failed login IP",
  "上次登录失败时间": "Last failed login time",
  "用户流量图": "Traffic chart",
  "发送流量": "Upload",
  "接收流量": "Download",
  "资源列表": "Resources",
  "搜索资源": "Search resources",
  "打开资源": "Open resource",
  "默认组": "Default group",
  "暂无资源": "No resources",
  "网关未下发资源，或搜索无匹配结果":
    "The gateway pushed no resources, or the search matched nothing",
}

function systemLanguage(): Language {
  if (typeof navigator === "undefined") return "zh";
  return navigator.language?.toLowerCase().startsWith("zh") ? "zh" : "en";
}

function storedLanguageMode(): LanguageMode {
  if (typeof window === "undefined") return "system";
  const value = window.localStorage.getItem(STORAGE_KEY);
  return value === "zh" || value === "en" ? value : "system";
}

/** 当前语言，供非组件上下文使用；跟随系统时按 navigator.language 解析 */
export function currentLanguage(): Language {
  const mode = storedLanguageMode();
  return mode === "system" ? systemLanguage() : mode;
}

/** 非组件上下文（如独立事件函数）使用的翻译入口 */
export function translate(
  zh: string,
  vars?: Record<string, string | number>,
): string {
  const lang = currentLanguage();
  let text = lang === "zh" ? zh : (EN[zh] ?? zh);
  if (vars) {
    for (const [key, value] of Object.entries(vars)) {
      text = text.replaceAll(`{${key}}`, String(value));
    }
  }
  return text;
}

interface I18nValue {
  lang: Language;
  languageMode: LanguageMode;
  setLanguageMode: (mode: LanguageMode) => void;
  toggleLang: () => void;
  /** 以中文原文为 key 取当前语言文案，支持 {name} 占位符 */
  t: (zh: string, vars?: Record<string, string | number>) => string;
}

const I18nContext = createContext<I18nValue | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  const [languageMode, setLanguageModeState] = useState<LanguageMode>(storedLanguageMode);
  const [systemLang, setSystemLang] = useState<Language>(systemLanguage);
  const lang: Language = languageMode === "system" ? systemLang : languageMode;

  useEffect(() => {
    document.documentElement.lang = lang === "zh" ? "zh-CN" : "en";
  }, [lang]);

  useEffect(() => {
    if (languageMode !== "system") return;
    const onChange = () => setSystemLang(systemLanguage());
    window.addEventListener("languagechange", onChange);
    return () => window.removeEventListener("languagechange", onChange);
  }, [languageMode]);

  const setLanguageMode = useCallback((next: LanguageMode) => {
    setLanguageModeState(next);
    window.localStorage.setItem(STORAGE_KEY, next);
  }, []);

  const toggleLang = useCallback(() => {
    setLanguageMode(lang === "zh" ? "en" : "zh");
  }, [lang, setLanguageMode]);

  const t = useCallback(
    (zh: string, vars?: Record<string, string | number>) => {
      let text = lang === "zh" ? zh : (EN[zh] ?? zh);
      if (vars) {
        for (const [key, value] of Object.entries(vars)) {
          text = text.replaceAll(`{${key}}`, String(value));
        }
      }
      return text;
    },
    [lang],
  );

  const value = useMemo(
    () => ({ lang, languageMode, setLanguageMode, toggleLang, t }),
    [lang, languageMode, setLanguageMode, toggleLang, t],
  );

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nValue {
  const ctx = useContext(I18nContext);
  if (!ctx) throw new Error("useI18n 必须在 I18nProvider 内使用");
  return ctx;
}

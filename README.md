<div align="center">

<h1>TopSAP Client</h1>

<p><b>天融信 NGVONE / TopSAP SSL VPN 的开源替代客户端</b> — 纯协议实现 · 不依赖厂商二进制 · Windows / macOS / Linux</p>

<p><b>简体中文</b> · <a href="README.en.md">English</a></p>

官方 TopSAP 客户端是一套 Electron 界面 + 常驻服务 + 内核驱动的组合，界面层问题较多。  
TopSAP Client 用 Tauri 2 重写了整条链路：登录、隧道、虚拟网卡、路由与 DNS 配置全部在应用内完成。  
下载一个安装包就能用，不附加常驻服务，不占用系统端口，行为可预期、可脚本化、有日志。  
账号密码只保存在本机（系统安全存储优先：Windows 凭据管理器 / macOS 钥匙串 / Linux Secret Service），断开时按账本撤销全部系统改动。

<p>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue?labelColor=1f2937" alt="MIT License"></a>
  <a href="https://tauri.app/"><img src="https://img.shields.io/badge/Tauri-2-24C8D8?logo=tauri&logoColor=white&labelColor=1f2937" alt="Tauri 2"></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-1.77%2B-DEA584?logo=rust&logoColor=white&labelColor=1f2937" alt="Rust 1.77+"></a>
  <a href="https://react.dev/"><img src="https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=black&labelColor=1f2937" alt="React 19"></a>
  <a href="https://tailwindcss.com/"><img src="https://img.shields.io/badge/Tailwind-4-06B6D4?logo=tailwindcss&logoColor=white&labelColor=1f2937" alt="Tailwind CSS 4"></a>
</p>

<p>
  <b><a href="#项目预览">项目预览</a></b> ·
  <a href="#它做什么">它做什么</a> ·
  <a href="#怎么工作">怎么工作</a> ·
  <a href="#技术栈">技术栈</a> ·
  <a href="#快速开始">快速开始</a> ·
  <a href="#构建与安装">构建与安装</a> ·
  <a href="#使用说明">使用说明</a> ·
  <a href="#数据与安全">安全</a> ·
  <a href="#常见问题">常见问题</a> ·
  <a href="#项目结构">项目结构</a> ·
  <a href="#文档">文档</a>
</p>

</div>

---

## 项目预览

![home](.github/images/topsap-client-home.png)

## 它做什么

- **账号密码 + 图形验证码登录** — 按网关要求自动获取验证码；验证码 Cookie 与登录会话复用，校验失败自动刷新并最多重试 3 轮；登录参数与官方客户端行为一致，兼容 `ngvone` 标准网关。
- **动态验证码二次认证** — 部分用户组密码校验后会触发短信/企业微信动态验证码（`Ret=-40039`）；自动弹出输入框，支持重新发送与倒计时，输错最多重试 3 次，验证码只走 TLS 且不写日志。
- **分流隧道** — 登录后另建一条 TLS 长连接，按 ACL 目标网段与资源 `ip/mask` 并集注入路由；只有匹配流量进入隧道，其余走物理网络。连接后若没有任何资源地址，会提示原因并自动断开。
- **虚拟网卡** — 使用微软签名的 Wintun 驱动创建 `TopSAP` 网卡，配虚拟 IP、MTU 1400，随包附带 `wintun.dll`。
- **内网域名信息** — 解析网关下发的 `dns_info.dns_list` 并保留在会话数据中；不修改系统 hosts 文件，与原客户端一致。
- **实时统计与连接信息** — 连接后展示资源列表、网关/账号/版本/隧道类型等基本信息、60 秒收发流量曲线，以及本次/上次登录 IP 与时间记录；运行日志保持可见。
- **断开清理** — 路由、DNS、网卡会话按账本逆序撤销；正常退出、系统关机与终止信号下会限时尝试断开；强杀或断电留下的会话标记会在下次启动/连接前补注销；macOS 还会按标记记录的 utun 名称与 VIP 清理自己残留的网卡（地址不一致时不触碰）。
- **断线自动重连** — 隧道异常断开后按指数退避重试（最多 5 次），主动断开不触发；可在设置中关闭。
- **已登录自动踢出** — 登录返回 `Ret=-40077` 时，按原客户端逻辑重新获取 `auth_param` 并调用 `user_pswd_r` 踢出上一次会话，成功后继续登录；无需重新输入账号密码，也不依赖本地会话标记。
- **网卡自检** — 不接触网关，几秒内验证管理员权限、驱动加载、建卡、配址、回读、测试路由与双向收发，把「网卡层故障」和「协议层故障」分开定位；macOS 接收方向由系统主动发包验证真实路由与读路径，测试路由冲突会单独提示。结束后自动清理测试网卡与路由。Linux 只删除带自检地址 `10.99.99.2` 的残留 `topsap0`，不会误删正在使用的连接网卡。
- **版本检查** — 启动后读取发布目录中的纯文本版本号；发现比当前版本更新时，在侧栏版本信息旁显示「可更新」，点击可查看当前/最新版本并前往下载页。点击版本号可随时手动检查：无更新提示「已是最新版本」，检查失败提示检查失败。Windows / macOS 的更新弹窗提供「自动更新」：下载对应平台安装包、断开当前连接、退出并替换程序，完成后自动重启并清理临时下载目录；失败时保留「前往」手动下载入口，Linux 保持「前往」。无法访问发布目录时不影响连接。
- **残留检测** — 诊断页检测旧版本 hosts 标记段；macOS 还会列出带 IPv4 地址的 utun 虚拟网卡与关联路由，确认后可一键关闭并清理。
- **连接配置管理** — 保存多个网关配置（名称、连接模式、检测超时、地址列表），支持一条配置内多条线路；选中配置后隐藏网关与端口，支持自动选最快线路或连接前手动选路；可导入、导出、清空配置，并在设置窗口切换默认连接。首次启动且没有任何导入配置、也没有保存过网关时，会提示前往「设置」-「连接管理」导入配置。
- **多线路接入** — 单线路、多线路优选（并发探测选最快）、多线路备份（按顺序重试）三种模式。
- **忘记密码** — 提交用户名与图形验证码；若网关要求二次验证，继续输入动态验证码并设置新密码，两步均复用同一次登录会话。账号不存在/被禁用与新密码不能与旧密码一致会给出明确提示。
- **修改密码** — 连接成功后可在当前账号旁修改密码，需输入原密码、新密码与确认密码；登录返回密码到期/过弱/首登/管理员要求时，也会弹出无会话改密窗口。改密成功后自动注销，新密码自动填入登录框，重新登录提示会注明这一点。
- **密码可见切换** — 所有密码框右侧提供固定的显示/隐藏按钮，并隐藏 WebView 自带的条件眼睛图标。
- **代理绕过** — 设置窗口可维护域名/泛域名/IP 绕过列表，默认写入 `*.sunline.cn` 与 `*.sunline-i.com`；Windows 使用 `ProxyOverride`，macOS 使用 `networksetup` 逐网络服务写入，Linux 使用 GNOME `gsettings ignore-hosts`。规则/全局系统代理模式下访问这些地址都不走代理；仅对使用系统代理的程序有效，TUN/透明代理模式无法覆盖。
- **系统托盘与单实例** — 关闭窗口默认最小化到托盘，连接保持；重复启动不会产生第二个实例。
- **中英文界面** — 顶栏一键切换中文 / English，语言偏好本地保存并支持跟随系统；设置窗口中的主题、语言与连接配置更改即时生效。
- **记住密码** — 优先写入系统安全存储（Windows 凭据管理器 / macOS 钥匙串 / Linux Secret Service）；不可用时回退为机器标识派生密钥的 AES-256-GCM 文件。
- **深浅色主题与可折叠侧栏** — 顶栏一键切换深浅色（记忆偏好）；侧栏可收起为图标模式，操作提示统一以顶部 toast 展示。

**它不是** VPN 网关、不是代理服务，也不绕过任何授权策略：TopSAP Client 只负责与**你自有的、已授权的**网关建立连接，访问控制仍由网关执行。

同一个接入场景，放在两种做法里大致是这样：

| 现场 | 官方客户端 | 用 TopSAP Client |
| --- | --- | --- |
| 进程模型 | Electron + 常驻服务 + 内核驱动 | 单进程，逻辑全部在应用内 |
| 界面问题 | 卡死、崩溃、字段错配 | 状态机驱动，每步有中文原因 |
| 断开清理 | 强杀会残留路由 | 清理账本 + 启动残留检测 |
| 排障 | 只有「连接失败」 | 网卡自检 + 分步日志 |
| 自动化 | 依赖界面点击 | 命令与事件分离，可脚本化 |
| 安装体积 | 约 200 MB | 安装包约 2 MB |

## 怎么工作

```text
  ┌────────────────────────────────────────────┐
│  Tauri 窗口（系统 WebView）                   │
│  React 19：连接 · 诊断 · 设置                  │
└────────────────┬───────────────────────────┘
                 │  Tauri IPC（invoke + 事件，不占网络端口）
                 ▼
┌────────────────────────────────────────────┐
│  Rust 核心（tokio）                          │
│  登录 · 隧道分帧 · Wintun · 路由 / DNS 账本  │
└───────┬───────────────────────┬────────────┘
        │ TLS 长连接             │ 用户态虚拟网卡
        ▼                       ▼
     网关:8443              Windows 网络栈
```

- **谁负责什么** — React 只负责表单、状态展示与日志；登录、隧道、拆帧、网卡收发、路由与 DNS 配置全部由 Rust 执行。
- **隧道是什么** — 登录走 HTTPS 短连接；隧道是另一条 TLS 长连接，之上承载 16 字节固定头的自定义帧，`type=15` 的载荷就是完整 IPv4 报文。
- **数据面在哪** — 系统把目标网段的包交给 `TopSAP` 虚拟网卡，读线程取出后写入隧道；网关下发的报文拆帧后写回网卡，注入系统网络栈。
- **不监听端口** — 前后端通过 Tauri IPC 通信，只有访问网关时才发起出站 TLS 连接。
- **网络配置怎么回滚** — 每条注入的路由、每次 DNS 变更都记账，断开时逆序撤销；网关 IP 单独加旁路路由，避免隧道连接自己走进隧道。

完整协议见 [docs/PROTOCOL.md](docs/PROTOCOL.md)，架构分层见 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)。

## 技术栈

| 层 | 选型 |
| --- | --- |
| 应用框架 | [Tauri 2](https://tauri.app/)（Rust 内核 + 系统 WebView） |
| 后端语言 | Rust 2021（tokio 异步运行时） |
| TLS | [rustls](https://github.com/rustls/rustls)（ring provider）+ tokio-rustls |
| 虚拟网卡 | [Wintun](https://www.wintun.net/)（微软签名驱动，libloading 绑定） |
| 前端框架 | [React 19](https://react.dev/) + TypeScript |
| 样式 | [Tailwind CSS 4](https://tailwindcss.com/) |
| 数据层 | [TanStack Query 5](https://tanstack.com/query) |
| 构建工具 | [Vite](https://vite.dev/) |
| 图标 / 通知 | [Lucide](https://lucide.dev/) / [sonner](https://sonner.emilkowal.ski/) |
| 安装包 | NSIS（Windows x64） |

## 快速开始

本地开发需要 **Rust 1.77+** 与 **Node.js 20+**。运行支持 Windows 7 SP1 / 10 / 11（x64）、macOS（Intel / Apple 芯片）与 Linux（amd64 / arm64）。

- **Windows**：使用 Wintun 虚拟网卡，安装包启动时请求管理员权限。
- **macOS / Linux**：使用系统 TUN 设备，需要 root 权限才能创建接口、修改路由与 DNS。macOS 首次启动会弹出系统授权框安装 root LaunchDaemon helper；GUI 保持当前用户身份，自检与建卡通过 helper 完成。取消授权则自检与连接会提示 helper 不可用。Linux 使用 `sudo ./topsap-client`；以普通用户启动时会弹窗提示用 `sudo /usr/bin/topsap-client` 重新启动。

```bash
git clone https://github.com/zyx3721/topsap-next.git
cd topsap-next
npm install
npm run tauri dev
```

首次运行会编译 Rust（数分钟），完成后自动打开应用窗口；前端与 Rust 改动均支持热重载。

只调前端可以用浏览器预览（内置假数据桥，不需要 Rust 与管理员权限）：

```bash
npm run dev            # http://localhost:5173
```

验证与测试：

```bash
npx tsc --noEmit                          # 前端类型检查
npm test                                  # 前端纯函数单测
cd src-tauri
cargo fmt --check                         # Rust 格式
cargo clippy --all-targets -- -D warnings # Rust 静态检查
cargo test --lib                          # Rust 单元测试
```

## 构建与安装

### 方式一：下载 Release 安装包（推荐）

前往 [GitHub Releases](https://github.com/zyx3721/topsap-next/releases) 页面下载：

| 你的机器 | 下载文件 |
| --- | --- |
| Windows x86_64 | `TopSAP Client_<版本>_x64-setup.exe` |
| Windows ARM64 | `TopSAP Client_<版本>_arm64-setup.exe` |
| macOS Intel 芯片 | `TopSAP Client_<版本>_x64.dmg` |
| macOS Apple 芯片 | `TopSAP Client_<版本>_aarch64.dmg` |
| Ubuntu / Debian 桌面 amd64 | `topsap-client_<版本>_amd64.deb` |
| Ubuntu / Debian 桌面 arm64 | `topsap-client_<版本>_arm64.deb` |
| Fedora / RHEL 桌面 amd64 | `topsap-client_<版本>_amd64.rpm` |
| Fedora / RHEL 桌面 arm64 | `topsap-client_<版本>_arm64.rpm` |
| Windows 便携版 amd64（免安装） | `topsap-client_<版本>_windows_amd64.zip` |
| Windows 便携版 arm64（免安装） | `topsap-client_<版本>_windows_arm64.zip` |
| macOS 便携版 Intel（免安装） | `topsap-client_<版本>_macos_amd64.tar.gz` |
| macOS 便携版 Apple 芯片（免安装） | `topsap-client_<版本>_macos_arm64.tar.gz` |
| Linux 便携版 amd64（免安装） | `topsap-client_<版本>_linux_amd64.tar.gz` |
| Linux 便携版 arm64（免安装） | `topsap-client_<版本>_linux_arm64.tar.gz` |

安装包启动时自动请求管理员权限（创建虚拟网卡、修改路由表、设置 DNS 都需要），支持 `/S` 静默安装。

> **Windows 7 用户**：需预先安装 WebView2 Runtime 109 与 Visual C++ 2015-2022 Redistributable。微软已停止 Windows 7 上的 WebView2 更新，更新的运行时版本可能无法安装。程序启动时会检测系统版本（6.1），自动切换到 Win7 兼容路径（配址走 netsh，资源路由优先 IPHLPAPI、失败回退 route），不影响 Win10/11 的原有行为。

便携版解压后直接运行 `topsap-client.exe`，需保持 `resources\wintun.dll` 的相对位置不变，同样需要以管理员身份运行。

> **macOS / Linux 用户**：应用需要 root 权限才能创建 TUN 接口与修改路由。macOS 首次启动会请求管理员授权并安装 root LaunchDaemon helper（`com.topsap.next.helper`），之后 GUI 以普通用户运行；也可以手动执行 `sudo open "/Applications/TopSAP Client.app"` 作为本地回退。Linux 直接 `sudo ./topsap-client`；如果双击以普通用户启动，应用会弹窗提示改用 `sudo /usr/bin/topsap-client`。macOS 应用未做 Apple 签名与公证，首次打开如提示「已损坏」，执行 `sudo xattr -rd com.apple.quarantine "/Applications/TopSAP Client.app"` 移除隔离属性。

### 方式二：本地源码构建

```bash
npm run tauri build
```

产物位于 `src-tauri/target/release/bundle/nsis/`。安装包主程序清单固定为 `requireAdministrator`，由 `src-tauri/build.rs` 注入。

> 若打包时报「另一个程序正在使用此文件」（os error 32），先结束残留的 `topsap-client.exe`，或使用独立 `CARGO_TARGET_DIR` 构建。

## 使用说明

1. **启动程序** — 安装版会自动请求 UAC；便携版需右键「以管理员身份运行」。首次启动若没有任何导入配置、也没有保存过网关，会提示先到「设置」-「连接管理」导入配置。
2. **填写网关** — 输入网关地址与端口（例如 `vpn.example.com` / `8443`）、账号与密码；勾选「记住密码」可保存到系统安全存储。
3. **输入验证码** — 网关需要验证码时会弹窗，输入图中字符后确定；取消等同于取消本次登录。
4. **输入动态验证码** — 部分用户组在密码校验后会收到短信或企业微信验证码（如「VPN登录验证码为：067143，有效期60秒」）。弹窗有效期与网关一致，可点「重新发送验证码」，输错可重试 3 次。
5. **等待连接** — 状态依次经过探测、登录、创建网卡、建立隧道、注入路由；成功后显示虚拟 IP、路由数与实时流量。
6. **断开** — 点「断开」或从托盘菜单断开；程序会注销会话、删除路由、恢复 DNS 与网卡。连接成功时可通过「修改密码」更新当前账号密码。
7. **连不上时** — 先到「诊断」页跑一次「网卡自检」，它不接触网关，能把驱动/网卡问题与协议问题分开；失败项会给出具体 Win32 错误码。
8. **异常退出后** — 重新启动程序会自动补注销上次未完成的会话；本地 TUN 网卡与路由随进程句柄关闭或系统重启回收。旧版本留下的 hosts 残留仍可在诊断页清理。

> 官方客户端必须完全退出，否则会触发单点登录冲突（`Ret=-40077`）。

## 数据与安全

```text
安装即用，无需注册
        +
密码存入系统安全存储，日志不含明文
        +
断开按账本撤销路由、DNS 与网卡
        +
前后端 Tauri IPC 通信，不监听任何 TCP 端口
```

- **证书不校验（协议要求）** — 网关证书普遍自签或已过期，官方客户端也不校验；本客户端同样跳过证书链校验且不发送 SNI。这意味着 TLS 层无法识别中间人，**请仅在可信网络中使用**，并通过其他渠道核对网关地址。
- **登录凭据以 Base64 编码后经 TLS 传输** — 网关要求 `U/P` 为账号与密码的 Base64；Base64 只是编码，安全边界完全依赖 TLS 通道。
- **版本检查** — 启动时访问发布目录读取纯文本版本号，只用于比较版本，不上传账号、设备或会话信息；自动更新只下载对应平台安装包，同样不上传账号或会话信息；无法访问时静默跳过，不影响连接与登录。
- **需要管理员权限** — 修改系统网络配置的前提，无法避免；程序只做路由、DNS 与虚拟网卡相关的改动。
- **密码存储** — 优先写入系统安全存储（Windows 凭据管理器 / macOS 钥匙串 / Linux Secret Service）；不可用时回退为机器标识派生密钥的 AES-256-GCM 文件。密码、验证码、`session_id` 不写入日志与事件；异常退出会话标记只保存网关、端口、`session_id` 与 macOS utun 名称/VIP，同样使用机器绑定 AES-256-GCM 加密。
- **Windows 便携目录** — 配置、日志与 WebView2 数据都写在程序目录：`config\`（设置、连接配置、凭据回退文件、异常退出会话标记）、`data\logs\`（`client.log`）、`webview\`（WebView2 用户数据，运行时生成 `EBWebView`）。请把程序放在可写目录；升级或重装会沿用这些目录，如需彻底清除请手动删除。
- **macOS / Linux 配置目录** — 设置、连接配置与凭据回退文件位于系统标准配置目录（Tauri `app_config_dir`），日志位于系统标准日志目录；使用 `sudo` 启动时以实际运行用户的 `$HOME` / `XDG_CONFIG_HOME` 为准。
- **日志位置** — 程序目录 `data\logs\client.log`（2 MB 轮转，保留一份 `client.old.log`），设置窗口可一键打开。macOS / Linux 仍使用系统标准日志目录。
- **残留清理** — 强杀或断电不会触发客户端清理，但 TUN 网卡与路由会随进程句柄关闭或系统重启回收；未完成注销的会话会加密记录，下次启动/连接前自动补注销，避免单点登录名额滞留。旧版本留下的 hosts 标记段仍可在诊断页清理。

## 常见问题

**连接失败怎么排查？**

按顺序检查：网关地址与端口是否正确、账号密码是否过期、验证码是否输错、官方客户端是否已完全退出；日志会标明失败发生在探测、登录、建卡、隧道还是路由阶段。

**提示账号已在别处登录（`Ret=-40077`）怎么办？**

说明同一账号还有另一个会话。本客户端会自动执行原客户端的踢出流程：重新获取 `auth_param`，再以 `user_pswd_r` 踢出旧会话并继续登录；即使旧会话来自官方客户端、当前客户端没有会话标记，也会自动处理。若踢出仍失败，确认两边都没有残留进程，等待几分钟让网关会话超时，或联系管理员释放。

**提示验证码错误（`Ret=-40018`）？**

验证码与本次会话绑定，输错后需要重新发起连接获取新图；注意 `+` 号等字符的输入。

**登录后要求输入动态验证码（`Ret=-40039`）？**

这是后台对部分用户组启用的二次认证：密码校验通过后，网关会把验证码发到账号绑定的手机或企业微信。弹窗内输入即可，失败可点「重新发送验证码」，输错最多重试 3 次；验证码 5 分钟内未提交会超时退出，需要重新连接。

**为什么必须管理员权限？**

创建虚拟网卡、修改路由表、设置 DNS 都需要管理员权限，这是 Windows 的限制，与客户端实现无关。

**强杀后路由残留、内网不通？**

重新启动本客户端，先到诊断页清理旧版本留下的 hosts 残留，再正常连接一次并断开；程序会按清理账本删除路由。

**支持国密网关吗？**

当前仅支持 `vpn_version=ngvone`、`cipher_type=0` 的标准网关；`sm2plusvone` 国密网关未实现。

**Windows 7 能用吗？**

可以。程序启动时通过 `RtlGetVersion` 检测系统版本，6.1 自动切换到 Win7 兼容路径：配址使用 `netsh`，资源路由优先 IPHLPAPI、失败回退 `route`，不依赖 PowerShell 3.0+ 的 NetTCPIP 模块；Win10/11 仍走原有路径。使用前需安装 WebView2 Runtime 109 与 Visual C++ 2015-2022 Redistributable，并建议在 Windows 7 SP1 上使用。

**macOS / Linux 怎么运行？**

macOS 与 Linux 使用系统 TUN 设备，需要 root 权限创建接口与修改路由。macOS 首次启动会请求管理员授权并安装 root LaunchDaemon helper，GUI 保持用户身份；也可手动执行 `sudo open "/Applications/TopSAP Client.app"` 作为本地回退。Linux 执行 `sudo ./topsap-client`；如果双击以普通用户启动，应用会弹窗提示改用 `sudo /usr/bin/topsap-client`。Linux 需要 WebKitGTK 4.1 运行时（Ubuntu/Debian: `sudo apt-get install libwebkit2gtk-4.1-0`）。两者的数据面基于 TUN，不依赖 Wintun 驱动；当前版本已实现并纳入 CI 构建，尚未在真机完成完整连接回归。

**Linux 连接时报 `Device or resource busy (os error 16)`？**

说明 `topsap0` 已被一个仍存活的进程占用，通常是上一次自检或连接没有正常退出。先在托盘选择「退出」结束所有 TopSAP 实例；确认没有正在使用的连接后，执行 `sudo ip link del topsap0` 删除残留网卡，再重新连接。新版自检开始前会先清理带测试地址 `10.99.99.2` 的残留网卡。

**能和官方客户端同时装吗？**

可以同时安装，但不能同时登录同一账号；建议二选一，避免单点登录冲突与重复路由。

**便携版为什么提示缺少 wintun.dll？**

便携包必须保持 `topsap-client.exe` 与 `resources\wintun.dll` 的相对位置；如果只复制了 exe，请把 `resources` 目录一并复制。

## 项目结构

```text
topsap-next/
├── src/                        React 19 前端
│   ├── pages/                  连接页 / 诊断页
│   ├── components/             设置窗口、业务弹窗与 ui.tsx 通用组件
│   ├── hooks/                  引擎事件订阅与连接 mutation
│   ├── lib/                    api.ts（唯一 IPC 边界）/ format.ts / theme.ts / mock.ts
│   ├── types/engine.ts         与 Rust DTO 一一对应
│   ├── App.tsx                 应用根组件
│   ├── main.tsx                前端入口
│   └── index.css               Tailwind 4 与主题变量
├── src-tauri/                  Rust 后端与 Tauri 配置
│   ├── src/
│   │   ├── auth/               登录三步骤与伪 JSON 解析
│   │   ├── protocol/           16 字节帧编解码与流式拆帧
│   │   ├── tunnel/             TLS 握手与双向数据泵
│   │   ├── tun/                Windows Wintun / Unix TUN、设备生命周期、自检
│   │   ├── netcfg/             网卡地址、路由注入、DNS 配置、旧版 hosts 残留清理（Windows / Unix 分实现）
│   │   ├── session/            连接生命周期、状态机、自动重连
│   │   ├── commands/           Tauri 命令层
│   │   ├── lib.rs              应用装配（状态 / 命令 / 托盘 / 单实例）
│   │   ├── platform.rs         运行环境探测（Windows 7 兼容分支）
│   │   ├── tls.rs              统一 TLS 配置
│   │   ├── update/{mod,download,install}.rs  版本检查、更新下载与多平台安装
│   │   ├── dto.rs / settings.rs / secret_store.rs / proxy_bypass.rs
│   │   ├── helper.rs            macOS root helper（TUN 创建与白名单命令）
│   │   ├── privileged.rs        macOS helper 客户端与 LaunchDaemon 安装
│   │   ├── session_guard.rs     异常退出会话标记与下次启动补注销
│   │   ├── shutdown.rs          系统关机/终止信号下的限时断开
│   ├── resources/wintun.dll    随包分发的虚拟网卡驱动
│   ├── icons/                  应用图标（源图 source.svg）
│   ├── build.rs                注入 requireAdministrator 清单
│   └── tauri.conf.json         Tauri 打包与窗口配置
├── docs/                       协议、架构与更新记录
├── .github/                    GitHub Actions 工作流与预览图
├── LICENSE
├── README.md                   中文说明（本文件）
└── README.en.md                English
```

## 文档

| 先看这个 | 再往下 |
| --- | --- |
| [快速开始](#快速开始) | 本地起开发环境，跑起来改代码 |
| [构建与安装](#构建与安装) | 下载安装包或本地源码构建 |
| [协议说明](docs/PROTOCOL.md) | 登录接口、帧格式、网络模型与限制 |
| [架构说明](docs/ARCHITECTURE.md) | 分层结构、模块职责与状态机 |
| [更新记录](docs/CHANGELOG.md) | 当前开发阶段的变更 |
| [English README](README.en.md) | 同样的内容，英文版 |

## 版本历史

| 版本 | 发布日期 | 更新日志 |
| --- | --- | --- |
| v1.4.1 | 2026-11-07 | [verchanglog/v1.4.1.md](verchanglog/v1.4.1.md) |
| v1.4.0 | 2026-11-07 | [verchanglog/v1.4.0.md](verchanglog/v1.4.0.md) |
| v1.3.3 | 2026-11-06 | [verchanglog/v1.3.3.md](verchanglog/v1.3.3.md) |
| v1.3.2 | 2026-11-06 | [verchanglog/v1.3.2.md](verchanglog/v1.3.2.md) |
| v1.3.1 | 2026-11-05 | [verchanglog/v1.3.1.md](verchanglog/v1.3.1.md) |
| v1.3.0 | 2026-11-05 | [verchanglog/v1.3.0.md](verchanglog/v1.3.0.md) |
| v1.2.2 | 2026-11-05 | [verchanglog/v1.2.2.md](verchanglog/v1.2.2.md) |
| v1.2.1 | 2026-11-04 | [verchanglog/v1.2.1.md](verchanglog/v1.2.1.md) |
| v1.2.0 | 2026-11-04 | [verchanglog/v1.2.0.md](verchanglog/v1.2.0.md) |
| v1.1.1 | 2026-11-04 | [verchanglog/v1.1.1.md](verchanglog/v1.1.1.md) |
| v1.1.0 | 2026-11-03 | [verchanglog/v1.1.0.md](verchanglog/v1.1.0.md) |
| v1.0.0 | 2026-11-03 | [verchanglog/v1.0.0.md](verchanglog/v1.0.0.md) |

各版本的构建产物与发布说明见 [GitHub Releases](https://github.com/zyx3721/topsap-next/releases)。

## 致谢

感谢以下开源项目和技术社区的支持：

- [Tauri](https://tauri.app/) — 跨平台桌面应用框架
- [tokio](https://tokio.rs/) — Rust 异步运行时
- [rustls](https://github.com/rustls/rustls) — 纯 Rust TLS 实现
- [Wintun](https://www.wintun.net/) — 用户态虚拟网卡驱动
- [React](https://react.dev/) — 前端界面框架
- [Vite](https://vite.dev/) — 快速前端构建工具
- [Tailwind CSS](https://tailwindcss.com/) — 原子化样式
- [TanStack Query](https://tanstack.com/query) — 前端数据层
- [Lucide](https://lucide.dev/) — 开源图标库

也感谢所有为本项目贡献代码、提出建议和报告问题的开发者。

## 许可证

本项目采用 [MIT License](LICENSE) 开源协议，可自由使用、复制、修改、合并、发布、分发、再许可与销售，只需在所有副本或重要部分中保留版权声明与许可声明。

## 联系方式

- **GitHub Issues**：[zyx3721/topsap-next/issues](https://github.com/zyx3721/topsap-next/issues)
- **项目主页**：[github.com/zyx3721/topsap-next](https://github.com/zyx3721/topsap-next)

---

**⭐ 如果这个项目对您有帮助，欢迎 Star 支持！**

# TopSAP / 天融信 NGVONE 协议说明

本文件记录的是**实测确认**的行为，不是推测。凡是标注"推断"的地方都单独说明了依据。

目标固件：TopSAP 客户端 3.6.6.19.64.1（天融信 NGVONE，`vpn_version=ngvone`、`cipher_type=0` 的网关）；与 3.6.4.16.64.1 的差异在正文单独标注。

---

## 1. 传输层前提

网关的 HTTPS 使用自签或已过期证书，官方客户端**不校验证书链**，且**不发送 SNI**。
因此本客户端连接时使用 `rejectUnauthorized: false` 并省略 `servername`。

> 安全性说明：这一层没有身份认证能力。在有中间人的网络里，任何人都能冒充网关。
> 这是原始实现的设计，本客户端为保持兼容而沿用，但在 README 中明确提示。

---

## 2. 登录接口

公共请求头：

```
User-Agent: VONE-AUTH,WINDOWS,NULL,TOPCLINET,NULL
```

> 官方客户端 3.6.6.19 会在末尾附加 `,VERSION=3.6.6.19.64.1`；该后缀未观察到参与登录校验，本客户端不发送，避免在 macOS/Linux 上声明为特定的 Windows 客户端版本。

### 2.1 能力探测

```
POST /vone/login/auth_param
```

无需认证。返回网关配置（伪 JSON，键名不带引号）：

| 字段 | 说明 |
| --- | --- |
| `auth_port` / `data_port` | 认证端口 / 数据端口 |
| `vpn_version` | `ngvone`（标准）或 `sm2plusvone`（国密） |
| `cipher_type` | `0` 表示未启用国密信道加密 |
| `captcha` | 旧版本数字型验证码开关，兼容保留 |
| `gid` | 图形验证码策略：`on`（每次）、`auto`（失败 3 次后）、`off`（不要求） |
| `login_check_verify` | 为 `on` 时额外强制图形验证码 |

### 2.2 取验证码

```
GET /vone/login/gid
```

返回 PNG，同时 `Set-Cookie: cgid=...`。后续登录必须带上该 cookie，验证码与 `cgid` 绑定。

### 2.3 提交登录

```
POST /vone/login/user_pswd
Content-Type: application/x-www-form-urlencoded
Cookie: cgid=...; topsecsvuilanguage=chinese

U=<Base64(账号)>&P=<Base64(密码)>&[Gid=<验证码>&]HASH=<64位十六进制>&S_CODE=<16字节随机数的Base64>
```

实测结论（官方客户端 3.6.6.19.64.1）：

- **`U` 和 `P` 都是 UTF-8 明文的 Base64，再按表单 URL 编码。** 例如账号 `demo` 发送为 `U=ZGVtbw%3D%3D`，密码字段同理为 `P=<Base64(密码)>`。早期 3.6.4 版本的抓包曾显示明文；当前网关按 Base64 解析，明文会被当作错误账号，本地账号尤其明显。
- **不需要图形验证码时不发送 `Gid` 字段。** 官方格式为 `U/P/HASH/S_CODE`；需要验证码时才使用 `U/P/Gid/HASH/S_CODE`。空 `Gid=` 不是官方行为。
- 登录 Cookie 的 `sv_un` 同样是 URL 编码后的账号 Base64，例如 `sv_un=ZGVtbw%3D%3D`。
- **`HASH` 的内容不被校验。** 传 64 个 `0` 也能通过。可以推断该字段只在某些加固配置下参与校验，标准网关不检查。
- `S_CODE` 传任意 16 字节随机数的 Base64 即可。
- 参数中的 `+` 必须转义为 `%2B`，否则网关按 URL 解码会把 `+` 变成空格（官方客户端自己也踩过这个坑，报 `Ret=-7`）。
- `auth_param`、`gid`、`user_pswd` 必须复用同一个 Cookie jar。`auth_param` 可能下发 `topsecsvkkk`，`gid` 下发 `cgid`；选路探测后另起客户端会丢 Cookie，表现为验证码正确但仍返回 `-40135`。
- 登录失败响应会返回 `login_err_num`，原客户端把它写回 Cookie；`gid=auto` 时达到 3 次后必须展示图形验证码。本客户端按账号跨连接保留该计数。
- 图形验证码用户返回 `-40135`/`-40018` 时，客户端自动刷新验证码并重新等待输入，最多 3 轮。

常见返回码：

| Ret | 含义 |
| --- | --- |
| `0` | 成功 |
| `-7` | 请求格式错误（多为 `+` 未转义） |
| `-40018` | 验证码错误 |
| `-40039` | 密码校验通过，需要动态验证码（短信 / 企业微信） |
| `-40045` | 动态验证码错误或已过期 |
| `-40077` | 账号已在别处登录（单点） |
| `-40081` | 账号或密码错误 |
| `-40123` | 不能获得用户权限（用户未拥有任何资源地址） |
| `-40135` | 认证因子校验失败（图形验证码 Cookie 丢失或验证码不匹配） |
| `-40142` | 重置密码过于频繁 |

### 2.4 动态验证码二次认证

后台可对指定用户组启用二次认证：`user_pswd` 返回 `Ret=-40039` 时，网关已向账号绑定的手机或企业微信发送动态验证码（例如「VPN登录验证码为：067143，有效期60秒」）。

重新发送：

```
POST /vone/login/send_sms
Cookie: sv_un=<账号>; login_err_num=0; cgid=...; topsecsvkkk=...; topsecsvuilanguage=chinese

U=<账号>&P=&Gid=<图形验证码>
```

密码登录分支的 `P` 为空（密码已在 `user_pswd` 阶段校验），`Gid` 为登录时的图形验证码。响应给出 `sms_valid_time`（界面倒计时使用，代码内限制不超过 600）。

提交验证码（官方客户端实测走 `user_pswd`，不是 `/vone/login/sms`）：

```
POST /vone/login/user_pswd
Referer: https://<host>/vone/login/user_pswd
Cookie: topsecsvuilanguage=chinese; login_err_num=2; topsecsvportalstyle=style1;
        topsecsvportalname=default; topsecsvkkk=...; topsecsvaaa=...;
        topsecsvccc=...; topsecsvddd=...; topsecsveee=60; topsecsvfff=3

U=<Base64(账号)>&SMS=<动态验证码>&SMS_TIME=0&HASH=<密码请求的 HASH>&S_CODE=<密码请求的 S_CODE>
```

关键点：

- `U` 为账号 Base64（URL 编码），与密码登录一致；官方日志示例 `U=aG9uZ3plbG9uZw%3D%3D`。
- `SMS_TIME` 固定为 `0`；官方日志中即使 `topsecsveee=60`，该字段仍为 0。
- `HASH`/`S_CODE` 必须与密码 `user_pswd` 请求一致；客户端在同一个 `AuthClient` 内复用。
- 请求不带 `sv_un`，但必须复用 `-40039` 响应种下的 `topsecsvkkk`、`topsecsvaaa`、`topsecsvccc`、`topsecsvddd`、`topsecsveee`、`topsecsvfff`。
- 官方原生层的 `chk_sms` 是状态机步骤，不单独发送 HTTP；真正提交验证码的是上面的 `user_pswd` 请求。

成功时返回与密码登录相同的响应；`-40045`/`-40039` 可重新输入，客户端最多重试 3 次。`-40135` 表示认证因子不匹配，常见原因是提交到了 `/vone/login/sms`、缺少挑战 Cookie、`SMS_TIME` 不是 0，或 `HASH/S_CODE` 与密码请求不一致。

### 2.5 登录响应

响应是伪 JSON（键名无引号），包含 9 个顶层节点：

```
user_param  会话与账号属性
net_param   网络参数（虚拟 IP、掩码、DNS、隧道类型）
route_list  网关聚合路由前缀（本客户端不使用）
acl_list    网关侧访问控制规则
dns_info    内网主机名映射
res_list    门户资源列表
sec_rule    终端合规规则
ErrMsg, Ret 结果
```

关键字段：

```
user_param.topafasfasfassession_id   会话 ID，形如 "100&demo0000000000"
user_param.spa_key                   SPA 单包授权密钥
net_param.vip_v4 / vip_mask          分配给本机的虚拟 IP 与掩码
net_param.dns1_v4 / dns2_v4          内网 DNS
net_param.tunnel_type                0/1=透明访问，2=网络隔离，3=网络完全代理
user_param.tos_version               网关版本（缺失时回退 auth_param.vpn_version）
user_param.cur_ok_ip / cur_ok_tm     本次登录成功 IP / Unix 秒
user_param.pre_ok_ip / pre_ok_tm     上次登录成功 IP / Unix 秒
user_param.pre_no_ip / pre_no_tm     上次登录失败 IP / Unix 秒
user_param.pwd_remind / account_end  密码/账号剩余天数（0=不提醒）
res_list[] {name, ip, mask, url, group_name, module, show}  门户资源；show=no 只隐藏不展示，module=va 跳过
route_list[] {dest, mask}            网关聚合网段（原客户端不使用）
dns_info.dns_list                    形如 "192.0.2.10 http://luban.example.com:3000"
acl_list[] {protocol, sport, dport, d_ip_v4, d_ip_v6, d_mask, action, week, start_tm, end_tm}
```

> 路由目标的字段位置因网关而异：部分网关把真实 IPv4 目标放在 `acl_list[].d_ip_v6`，同时把 `d_ip_v4` 固定为 `0.0.0.0`；必须优先取非零 `d_ip_v4`，否则回退 `d_ip_v6`。

### 2.6 找回密码

第一步：校验图形验证码并请求重置信息。

```
POST /vone/find_pswd
Content-Type: application/x-www-form-urlencoded
Cookie: cgid=...; topsecsvkkk=...

U=<账号>&Base64_U=<账号的 Base64>&Gid=<图形验证码>&S_Gid=<同一图形验证码>
```

`Gid` 与 `S_Gid` 使用同一个图形验证码值（原生实现中两处指向同一缓冲区）。

- `Ret=0`：网关直接发送重置信息，`Mail` / `Phone` 给出接收方。
- `Ret=-40230`：网关已发送动态验证码，需要进入第二步设置新密码。
- `Ret=-40142`：重置过于频繁，响应 Cookie 的 `reset_left_time` 给出剩余秒数。
- `Ret=-40075`（或 `ErrMsg` 含「禁止找回密码」）：原程序显示「禁止找回密码」，实际表示账号不存在、被禁用或未绑定找回方式；本客户端统一显示「该用户不存在或已被禁用」。

第二步：提交动态验证码与新密码。

```
POST /vone/find_pswd
Content-Type: application/x-www-form-urlencoded
Cookie: topsecsvkkk=...; topsecsvddd=...

U=<账号>&Base64_U=<账号的 Base64>&P=<Base64(新密码)>&SMS=<动态验证码>&S_Gid=
```

官方日志示例格式：`U=demo&Base64_U=ZGVtbw%3D%3D&P=<Base64(新密码)>&SMS=<动态验证码>&S_Gid=`。`U` 保持明文，`Base64_U` 与 `P` 都是账号/新密码的 Base64（URL 编码）。第二步必须复用第一步响应下发的 Cookie。

第二步 `Ret=-40052/-40071`（或 `ErrMsg` 含「找回密码失败」）在实测中对应新密码与旧密码相同；本客户端显示「新密码不能与旧密码一致」，其余策略错误仍优先展示网关 `ErrMsg`。第二步 `Ret=-40230` 同样表示重置成功（原客户端也在该分支关闭弹窗），不能按错误处理。

### 2.7 修改密码

已登录会话下调用（需携带登录响应的 Cookie）：

```
POST /vone/chg_pwd
Content-Type: application/x-www-form-urlencoded;charset=UTF-8
Cookie: ...; sv_un=<Base64(账号)>; seccheck=0; topafasfassession_id=<会话ID>

P1=<Base64(原密码)>&P2=<Base64(新密码)>
```

已登录会话下请求体**只发送 `P1/P2`**；`U` 只用于 Cookie 的 `sv_un`（账号 Base64，例如 `sv_un=ZGVtbw==`）。只有无会话的首登改密分支才在请求体追加 `U=<Base64(账号)>`。官方客户端 3.6.6.19 的 `P1/P2` 是**原/新密码的 Base64**（例如 `P1=<Base64(原密码)>&P2=<Base64(新密码)>`），不是明文。返回 `Ret=0` 表示修改成功；`-40226` 时原程序直接展示网关 `ErrMsg` 中的动态密码策略（长度、字符集、历史密码限制等）。新旧密码相同时网关返回 `-40037`（修改密码失败）或 `-40129`（新密码与旧密码不能相同），原前端在提交前就用 `difPassword` 拦截；本客户端同时做前端预检与返回码映射，统一提示「新密码不能与旧密码一致」。

### 2.7.1 密码状态与到期

`user_pswd` 除常规错误外，还会用以下返回码要求先改密：

| Ret | 含义 | 原客户端行为 |
| --- | --- | --- |
| `-40022` | 登录用户密码有效期已到 | 弹出「修改密码」窗口，标题「登录用户密码有效期已到」 |
| `-40023` | 密码过于简单 | 弹出「修改密码」窗口 |
| `-40024` | 首次登录，需修改初始密码 | 弹出「修改密码」窗口 |
| `-40199` | 管理员要求修改密码 | 弹出「修改密码」窗口 |

登录成功时还会返回 `pwd_remind`（密码剩余天数，0 表示不提醒）与 `account_end`（账号剩余天数）；原客户端把它们拼成登录后的提示文本，不影响登录。当前项目在连接建立后通过 `expiry_reminder` 事件分别弹出账号到期与密码到期提示，字段优先取 `user_param`，兼容响应顶层。

`-40081` 默认按凭据错误处理，不弹改密窗口。少数网关会把「密码已过期/首次登录/管理员要求改密」包在 `-40081` 的 `ErrMsg` 里；当前实现只有在文案明确指向密码状态时才映射为对应的 `-40022/-40024/-40023/-40199` 并进入改密流程，普通「用户名或密码错误」仍保持原提示。

原客户端在 `-40022` 后走无会话的 `/vone/chg_pwd`（`U/P1/P2`，Cookie `sv_un` + `seccheck=0`），成功后再由用户重新登录。当前项目同样弹出无会话改密窗口，成功后自动用新密码重试连接。

### 2.8 注销

```
POST /vone/login/logout
Cookie: topafasfasfassession_id=<会话ID>
```

返回 `{ Ret: 0 }`。不注销会占用账号的单点登录名额，下一次登录报 `-40077`。

### 2.9 已登录踢出（-40077）

原客户端登录返回 `-40077` 时会弹出确认框；确认后原生层执行：

```
POST /vone/login/auth_param            # 重新获取 auth_param 与 Cookie
POST /vone/login/user_pswd_r           # 踢出旧会话并重新取回 session_id
U=<账号 Base64>&HASH=<原密码请求的 HASH>&S_CODE=<原密码请求的 S_CODE>
```

`user_pswd_r` 的返回结构与 `user_pswd` 相同（`user_param` / `net_param` / `res_list`），成功时 `X-sv-ret: 0`，失败时 `Ret`/`X-sv-ret` 为负值。原客户端在 `kick_user` 之后仍可能返回 `-40039`（要求动态验证码），需要继续走短信/企业微信验证码流程。

当前项目在 `-40077` 时自动执行上述踢出流程并继续登录，不再直接报错；会话标记补注销与自动踢出同时保留，覆盖「上一次会话由官方客户端异常退出、当前客户端没有标记」的场景。

---

## 3. 隧道协议

登录走 HTTPS 短连接；隧道是**另开一条 TLS 长连接**到同一端口，之上承载自定义二进制帧。

### 3.1 握手

```
TCP/TLS 连接 gateway:port
→ "CONNECT 127.0.0.1:4430 HTTP/1.1\r\nhost:127.0.0.1\r\n
   User-Agent: VONE-NA,WINDOWS,NULL,TOPCLINET,NULL\r\n
   Proxy-Connection: Keep-Alive\r\n\r\n"
→ 登录帧（见下）
← 确认帧 type=2
→ 通知帧 type=7，载荷 8 字节零
↔ 数据帧 type=15
```

登录帧载荷 328 字节，布局：

```
偏移 0   4 字节   常量 0x00000004
偏移 4   68 字节  全零
偏移 72  19 字节  会话 ID（ASCII，例如 "100&demo0000000000"）
其余      补零
```

### 3.2 帧格式

固定 16 字节头 + 载荷，全部大端：

```
+0..3    保留，恒为 0
+4..7    消息类型   1=登录  2=确认  7=通知  15=IPv4 数据
+8..11   载荷长度
+12..15  方向       1=上行（客户端→网关）  0=下行
```

`type=15` 的载荷就是一个**完整的 IPv4 报文**（从版本/IHL 开始，含 IP 头）。
数据面**没有任何额外加密**：`cipher_type=0` 的网关上，国密 SM4 信道不会被启用。

### 3.3 保活与断线

- 网关侧空闲超时由 `net_param.no_flow_timeout` 控制，`0` 表示不主动断开。
- 未观察到独立的心跳帧；周期性流量（TCP keepalive 或应用层流量）即可维持。
- 连接断开后需重新握手，会话 ID 可复用（同一会话内重连无需重新登录）。
- 客户端在 TLS socket 上开启 TCP keepalive（空闲 30 秒、间隔 5 秒、3 次探测），用于发现 NAT 失效或半开连接；协议层不新增心跳帧。
- 数据面健康检查完全在客户端本地完成：观察 TUN 收包、隧道上行/下行包数与网卡投递增量，识别持续单向静默或上行卡死。命中后按隧道级重连恢复，并在重连后验证双向流量；不依赖网关的 `no_flow_timeout` 通知。

---

## 4. 客户端网络模型

### 4.1 地址

虚拟网卡使用 `net_param.vip_v4` / `vip_mask`。MTU 取 1400（官方客户端行为）。

macOS 的 utun 是点对点接口：本机地址为 VIP，对端地址按 **VIP-1** 推导（官方客户端实测 `ifconfig` 显示 `inet VIP --> VIP-1`），资源路由的下一跳同样使用 VIP-1。不能把 VIP 同时当作对端和网关：`route get` 回读会通过，但内核不会把系统流量真正投递到 utun。Windows 使用 Wintun，资源路由 NextHop 为 VIP-1（已与官方对齐）；Linux 使用 `ip route ... dev`，均不需要 utun 式的点对点对端配置。

### 4.2 路由

原客户端 `na_client.exe` 按 **`acl_list` 的目标网段**逐条注入路由（`AddVPNTunnelRoute` / `AddSingleRoute` / `CheckRouteTable`），不使用 `route_list` 聚合。目标地址优先取非零 `d_ip_v4`，否则取 `d_ip_v6`（本网关把 IPv4 目标写在该字段）。

当前客户端先按 ACL 目标生成路由，再与 `res_list[].ip` + `mask` 取并集，保证资源列表中的每个地址都能出现在系统路由表；`ip/mask` 缺失时回退 `name` 中的 IPv4/CIDR。`show=no` 的隐藏资源不展示，但仍参与路由。`route_list` 仍不作为路由来源。

原客户端使用 NDIS 虚拟网卡，资源路由的 NextHop 为虚拟网关（实测为 VIP-1）、metric 约 332。本客户端早期使用 Wintun on-link（`0.0.0.0`）路由；Windows 实现现已改为与官方一致的 `CreateIpForwardEntry`，NextHop 使用 VIP-1、Metric 保持 1，并通过 `GetIpForwardTable` 按 Dest/Mask/NextHop/IfIndex 查重与回读。Wintun 仍是 L3 设备，资源目的地址与掩码不变。

macOS 与 Windows 不同：utun 必须显式指定对端，本客户端与官方客户端一致，`ifconfig` 使用 `inet VIP VIP-1`，资源路由使用 `route -n add -net <dest> -netmask <mask> VIP-1`。

**必须**为网关自身的 IP 加一条旁路路由（走物理默认网关），否则隧道连接会自己走进隧道形成环路。

### 4.3 DNS

- `dns_info.dns_list` 中的内网主机名仅解析后保留在会话数据中，不写入系统 hosts；原客户端同样不修改 hosts。
- 网卡 DNS 设为 `dns1_v4`，让内网域名解析走隧道。不劫持全局 DNS，避免影响正常上网。

### 4.4 分流

`tunnel_type=0/1` 在原客户端均显示为「透明访问」，2 为「网络隔离」，3 为「网络完全代理」；实际流量范围由 ACL 目标网段与资源网段共同决定：只把这些地址打进隧道，其余流量走物理网络。
网关侧 `acl_list` 同时承担访客控制；即使本地路由放通，未授权的目标也会被网关丢弃。

---

## 5. 与官方实现的差异

| 方面 | 官方客户端 | 本客户端 |
| --- | --- | --- |
| 虚拟网卡 | NDIS 小端口驱动 `TopvDev.sys`（`\\.\TOPVNIC`，重叠 I/O） | Wintun（微软签名驱动，用户态 DLL） |
| 路由来源 | 按 ACL 目标网段逐条注入（`na_client.exe`） | ACL 目标 + 资源 `ip/mask` 并集；不使用 `route_list` |
| 进程模型 | Electron 界面 + `sv_service.exe`(LocalSystem) + `na_client.exe`，共享内存传 socket 句柄 | 单进程，全部逻辑在应用内 |
| 登录/改密字段 | 前端 RSA/Base64 后由原生层解密 | 账号/密码按官方 3.6.6.19 格式 Base64 编码后提交 |
| 证书校验 | 不校验证书链、不发 SNI | 相同（兼容性要求） |
| 国密 | 支持 SM2/SM4（`sm2plusvone` 网关） | 未实现，仅支持 `ngvone` |

---

## 6. 未实现 / 已知限制

1. **国密网关（`sm2plusvone`）不支持。** 只验证过 `vpn_version=ngvone`。
2. **SPA 单包授权未实现。** 登录响应会下发 `spa_key`，标准网关上不启用；若网关开启 SPA，需要先发 fwknop 敲门包。
3. **`route_list` 不参与本地路由生成。** 原客户端只使用 `acl_list` 目标与资源网段；`route_list` 的实际用途仍未确认。
4. **UDP over TCP 的封装细节未知。** 实测隧道直接承载原始 IP 报文，未观察到额外封装头；若某类网关启用了封装，需要重新抓包确认。
5. **断线重连**为重新握手，不做会话恢复。

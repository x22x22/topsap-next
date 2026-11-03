<div align="center">

<h1>TopSAP Client</h1>

<p><b>An open-source replacement for the Topsec NGVONE / TopSAP SSL VPN client</b> — pure protocol implementation · no vendor binaries · Windows / macOS / Linux</p>

<p><a href="README.md">简体中文</a> · <b>English</b></p>

The official TopSAP client is a combination of an Electron UI, a resident service and a kernel driver — and its UI layer is the fragile part.  
TopSAP Client rewrites the whole chain with Tauri 2: login, tunnel, virtual adapter, routing and DNS all live inside the app.  
One installer is all it takes — no extra resident service, no listening ports, predictable behaviour, scriptable, with logs.  
Credentials stay on your machine (system credential manager first), and disconnecting rolls back every system change from a ledger.

<p>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue?labelColor=1f2937" alt="MIT License"></a>
  <a href="https://tauri.app/"><img src="https://img.shields.io/badge/Tauri-2-24C8D8?logo=tauri&logoColor=white&labelColor=1f2937" alt="Tauri 2"></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-1.77%2B-DEA584?logo=rust&logoColor=white&labelColor=1f2937" alt="Rust 1.77+"></a>
  <a href="https://react.dev/"><img src="https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=black&labelColor=1f2937" alt="React 19"></a>
  <a href="https://tailwindcss.com/"><img src="https://img.shields.io/badge/Tailwind-4-06B6D4?logo=tailwindcss&logoColor=white&labelColor=1f2937" alt="Tailwind CSS 4"></a>
</p>

<p>
  <b><a href="#preview">Preview</a></b> ·
  <a href="#what-it-does">What it does</a> ·
  <a href="#how-it-works">How it works</a> ·
  <a href="#tech-stack">Tech stack</a> ·
  <a href="#quick-start">Quick start</a> ·
  <a href="#build--install">Build &amp; install</a> ·
  <a href="#usage">Usage</a> ·
  <a href="#data-and-security">Security</a> ·
  <a href="#faq">FAQ</a> ·
  <a href="#repository-layout">Layout</a> ·
  <a href="#documentation">Docs</a>
</p>

</div>

---

## Preview

![home](.github/images/topsap-client-home.png)

## What It Does

- **Username/password + image captcha login** — Fetches a captcha when the gateway requires one; the captcha cookie is reused for the login session, and a failed check refreshes the image and retries up to 3 rounds. Login parameters match the official client and work with `ngvone` gateways.
- **SMS / WeCom one-time code** — Some user groups require a second factor after the password check (`Ret=-40039`); a dialog collects the code sent to the bound phone or WeCom, with resend countdown and up to 3 attempts. The code only travels inside TLS and is never logged.
- **Split tunnel** — After login a second TLS long connection is opened and routes are injected from the union of ACL target networks and resource `ip/mask` values; only matching traffic enters the tunnel. If no resource addresses are returned, the app explains why and disconnects automatically.
- **Virtual adapter** — Creates a `TopSAP` adapter through Microsoft-signed Wintun, assigns the virtual IP and MTU 1400; `wintun.dll` ships with the app.
- **Intranet host information** — Parses the gateway-provided `dns_info.dns_list` and keeps it in session data; it never edits the system hosts file, matching the official client.
- **Live stats and session details** — After connecting, show the resource list, gateway/account/version/tunnel details, a 60-second traffic chart and current/previous login IP and time records; the runtime log stays visible.
- **Clean disconnect** — Routes, DNS and the adapter session are rolled back in reverse order from a ledger; normal quit, system shutdown and termination signals attempt a bounded disconnect; sessions left by a forced kill or power loss are logged out on the next launch/connect.
- **Auto reconnect** — Exponential backoff (up to 5 attempts) when the tunnel drops unexpectedly; manual disconnects never trigger it, and it can be disabled in settings.
- **Automatic kick-out** — When login returns `Ret=-40077`, the app follows the official flow: re-fetch `auth_param`, call `user_pswd_r` to kick the previous session, and continue logging in without re-entering credentials or relying on a local session marker.
- **Adapter self-test** — Without touching the gateway, verifies admin rights, driver loading, adapter creation, address assignment, read-back, test route and bidirectional packet flow in seconds — separating adapter faults from protocol faults.
- **Residue detection** — Older versions could leave a hosts block after an abrupt exit; the Diagnostics page can detect and remove it in one click.
- **Connection profiles** — Save multiple gateway profiles (name, connection mode, probe timeout, address list), with several lines per profile; selecting a profile hides the gateway and port, and multi-line profiles can auto-pick the fastest line or show a latency-sorted picker before connecting. Import, export or clear profiles from the Settings window.
- **Multi-line access** — Single line, multi-line preferred (probe all lines concurrently and pick the fastest) and multi-line backup (try in order).
- **Forgot password** — Submit a username and image captcha; when the gateway requires a second step, enter the one-time code and set a new password within the same session. Missing/disabled accounts and reuse of the old password get explicit messages.
- **Change password** — While connected, change the current account password with the old password, new password and confirmation. If login returns expired/weak/first-login/admin-required password status, a standalone change-password dialog is shown. After a successful change the session is logged out, the new password is filled into the login form, and the next successful login toast says so.
- **Password visibility** — Every password field has a fixed show/hide button on the right and hides the WebView's built-in conditional eye icon.
- **Proxy bypass** — The settings window manages a domain/wildcard/IP bypass list, preloaded with `*.sunline.cn` and `*.sunline-i.com`. Windows uses `ProxyOverride`, macOS writes per-service lists through `networksetup`, and Linux uses GNOME `gsettings ignore-hosts`. Entries bypass the local proxy in both rule and global system-proxy modes. Only apps that use the system proxy are affected; TUN/transparent proxies are not covered.
- **Tray and single instance** — Closing the window minimizes to tray by default and keeps the connection; launching again never creates a second instance.
- **Chinese / English UI** — Toggle languages from the header; the choice is saved locally and can follow the system. Theme, language and profile changes apply immediately in the Settings window.
- **Remember password** — Stored in the Windows Credential Manager first; falls back to a machine-bound AES-256-GCM file.
- **Light/dark theme and collapsible sidebar** — Toggle themes from the header (preference remembered); collapse the sidebar to icons, with all notices shown as top toasts.

**It is not** a VPN gateway, a proxy service, or a way to bypass authorization: TopSAP Client only connects to gateways **you own and are authorized to use**, and access control remains enforced by the gateway.

The same access scenario, side by side:

| Scenario | Official client | TopSAP Client |
| --- | --- | --- |
| Process model | Electron + resident service + kernel driver | Single process, all logic in-app |
| UI reliability | Freezes, crashes, field mismatches | State machine, every step has a reason |
| Disconnect cleanup | Forces kills leave routes behind | Cleanup ledger + startup residue check |
| Troubleshooting | Just "connection failed" | Adapter self-test + step-by-step log |
| Automation | UI clicking required | Commands and events separated |
| Installer size | ~200 MB | ~2 MB |

## How It Works

```text
  ┌────────────────────────────────────────────┐
  │  Tauri window (system WebView)             │
  │  React 19: connect · diagnostics · settings│
  └────────────────┬───────────────────────────┘
                   │  Tauri IPC (invoke + events, no ports)
                   ▼
┌────────────────────────────────────────────┐
│  Rust core (tokio)                          │
│  login · framing · Wintun · route/DNS       │
└───────┬───────────────────────┬────────────┘
        │ TLS long connection   │ user-mode adapter
        ▼                       ▼
   gateway:8443            Windows network stack
```

- **Who does what** — React only handles forms, state display and logs; login, tunneling, framing, adapter I/O and network configuration all run in Rust.
- **What the tunnel is** — Login uses HTTPS short connections; the tunnel is a separate TLS long connection carrying custom frames with a 16-byte header, where `type=15` payloads are complete IPv4 packets.
- **Where the data plane is** — Windows hands packets for the target networks to the `TopSAP` adapter; the reader thread forwards them into the tunnel, and inbound frames are written back to the adapter.
- **No listening ports** — Frontend and backend talk over Tauri IPC; outbound TLS happens only when contacting the gateway.
- **How rollback works** — Every injected route and DNS change is recorded, then reversed on disconnect; the gateway IP gets a bypass route so the tunnel connection never loops into itself.

See [docs/PROTOCOL.md](docs/PROTOCOL.md) for the full protocol and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the architecture.

## Tech Stack

| Layer | Choice |
| --- | --- |
| App framework | [Tauri 2](https://tauri.app/) (Rust core + system WebView) |
| Backend language | Rust 2021 (tokio async runtime) |
| TLS | [rustls](https://github.com/rustls/rustls) (ring provider) + tokio-rustls |
| Virtual adapter | [Wintun](https://www.wintun.net/) (Microsoft-signed driver, libloading) |
| Frontend | [React 19](https://react.dev/) + TypeScript |
| Styling | [Tailwind CSS 4](https://tailwindcss.com/) |
| Data layer | [TanStack Query 5](https://tanstack.com/query) |
| Build tool | [Vite](https://vite.dev/) |
| Icons / Toasts | [Lucide](https://lucide.dev/) / [sonner](https://sonner.emilkowal.ski/) |
| Installers | NSIS (Windows x64) |

## Quick Start

Local development requires **Rust 1.77+** and **Node.js 20+**. Runtime support covers Windows 7 SP1 / 10 / 11 (x64), macOS (Intel / Apple silicon) and Linux (amd64 / arm64).

- **Windows**: uses the Wintun virtual adapter; the installer requests administrator rights on launch.
- **macOS / Linux**: uses the system TUN device and needs root privileges to create the interface and edit routes/DNS. On macOS the first launch installs a root LaunchDaemon helper after administrator authorization; the GUI stays user-level and uses the helper for the self-test and adapter creation. Cancelling leaves the self-test and connection reporting that the helper is unavailable. On Linux run `sudo ./topsap-client`.

```bash
git clone https://github.com/zyx3721/topsap-next.git
cd topsap-next
npm install
npm run tauri dev
```

The first run compiles the Rust core (a few minutes), then opens the app window; both frontend and Rust changes hot-reload.

For frontend-only work, a browser preview is available (built-in mock bridge, no Rust or admin rights needed):

```bash
npm run dev            # http://localhost:5173
```

Checks and tests:

```bash
npx tsc --noEmit                          # Frontend type check
npm test                                  # Frontend pure-function tests
cd src-tauri
cargo fmt --check                         # Rust formatting
cargo clippy --all-targets -- -D warnings # Rust lints
cargo test --lib                          # Rust unit tests
```

## Build & Install

### Option 1: Download a Release installer (recommended)

Grab the artifact from the [GitHub Releases](https://github.com/zyx3721/topsap-next/releases) page:

| Your machine | Download |
| --- | --- |
| Windows x86_64 | `TopSAP Client_<version>_x64-setup.exe` |
| Windows ARM64 | `TopSAP Client_<version>_arm64-setup.exe` |
| macOS Intel | `TopSAP Client_<version>_x64.dmg` |
| macOS Apple silicon | `TopSAP Client_<version>_aarch64.dmg` |
| Ubuntu / Debian desktop amd64 | `topsap-client_<version>_amd64.deb` |
| Ubuntu / Debian desktop arm64 | `topsap-client_<version>_arm64.deb` |
| Fedora / RHEL desktop amd64 | `topsap-client_<version>_amd64.rpm` |
| Fedora / RHEL desktop arm64 | `topsap-client_<version>_arm64.rpm` |
| Windows portable amd64 | `topsap-client_<version>_windows_amd64.zip` |
| Windows portable arm64 | `topsap-client_<version>_windows_arm64.zip` |
| macOS portable Intel | `topsap-client_<version>_macos_amd64.tar.gz` |
| macOS portable Apple silicon | `topsap-client_<version>_macos_arm64.tar.gz` |
| Linux portable amd64 | `topsap-client_<version>_linux_amd64.tar.gz` |
| Linux portable arm64 | `topsap-client_<version>_linux_arm64.tar.gz` |

The installer requests administrator rights on launch (creating the virtual adapter, editing the route table and setting DNS all require it) and supports `/S` silent installation.

> **Windows 7 users**: install WebView2 Runtime 109 and the Visual C++ 2015-2022 Redistributable first. Microsoft no longer updates WebView2 on Windows 7, so newer runtimes may fail to install. On startup the app detects the OS version (6.1) and switches to the netsh compatibility path automatically, leaving Windows 10/11 behaviour untouched.

The portable build runs `topsap-client.exe` directly; keep `resources\wintun.dll` at its relative path, and run it as administrator as well.

> **macOS / Linux users**: the app needs root privileges to create the TUN interface and edit routes. On macOS the first launch requests administrator authorization and installs a root LaunchDaemon helper (`com.topsap.next.helper`); afterwards the GUI runs as the current user. You can also run `sudo open "/Applications/TopSAP Client.app"` as a local fallback. On Linux run `sudo ./topsap-client`. The macOS app is not signed or notarized by Apple — if macOS reports it as damaged on first launch, run `sudo xattr -rd com.apple.quarantine "/Applications/TopSAP Client.app"` to remove the quarantine attribute.

### Option 2: Build from source

```bash
npm run tauri build
```

Artifacts land in `src-tauri/target/release/bundle/nsis/`. The main binary manifest is pinned to `requireAdministrator` by `src-tauri/build.rs`.

> If packaging fails with "another program is using this file" (os error 32), terminate any leftover `topsap-client.exe` process or use a separate `CARGO_TARGET_DIR`.

## Usage

1. **Launch** — The installer build prompts for UAC; for the portable build, right-click and run as administrator.
2. **Fill in the gateway** — Enter address and port (e.g. `vpn.example.com` / `8443`), username and password; enable "Remember password" to store it in the system credential manager.
3. **Enter the captcha** — A dialog appears when the gateway requires one; cancelling is treated as cancelling the login.
4. **Enter the one-time code** — Some user groups receive an SMS or WeCom code after the password check (e.g. "VPN login code: 067143, valid for 60 seconds"). The dialog follows the gateway validity window, offers resend and allows up to 3 attempts.
5. **Wait for connection** — States go through probing, login, adapter creation, tunnel handshake and route injection; the app then shows the virtual IP, route count and live traffic.
6. **Disconnect** — Use the Disconnect button or the tray menu; the app logs out, removes routes and restores DNS and the adapter. While connected, use Change password to update the current account password.
7. **When it fails** — Run the adapter self-test on the Diagnostics page first; it never touches the gateway and separates driver/adapter issues from protocol issues, reporting concrete Win32 error codes.
8. **After an abrupt exit** — Relaunch; the app retries the unfinished logout automatically, and the TUN adapter and routes are released when the process handles close or the system reboots. Legacy hosts residue can still be removed on the Diagnostics page.

> The official client must be fully closed, otherwise single-sign-on conflicts (`Ret=-40077`) occur.

## Data & Security

```text
Install and use — no sign-up
        +
Passwords in the system credential manager, never in logs
        +
Disconnect rolls back routes, DNS and the adapter from a ledger
        +
Frontend and backend talk over Tauri IPC — no TCP ports listened on
```

- **Certificate validation is skipped (protocol requirement)** — Gateway certificates are usually self-signed or expired, and the official client does not validate them either; this client also skips chain validation and sends no SNI. That means TLS cannot identify a man-in-the-middle — **use it only on trusted networks** and verify the gateway address through another channel.
- **Credentials travel in plaintext inside TLS** — The gateway requires plaintext `U/P`; the security boundary is the TLS channel itself.
- **Administrator rights are required** — Unavoidable for changing system network configuration; the app only touches routes, DNS and the virtual adapter.
- **Password storage** — Windows Credential Manager first; falls back to a machine-bound AES-256-GCM file. Passwords, captchas and `session_id` never enter logs or events; the abnormal-exit session marker stores only the gateway, port and `session_id`, encrypted with the same machine-bound AES-256-GCM key.
- **Portable layout on Windows** — Settings, logs and WebView2 data live next to the executable: `config\` (settings, connection profiles, credential fallback, abnormal-exit session marker), `data\logs\` (`client.log`) and `webview\` (WebView2 user data, containing `EBWebView`). Keep the app in a writable folder; upgrades and reinstalls reuse these directories, so delete them manually for a full cleanup.
- **Log location** — `data\logs\client.log` next to the executable (2 MB rotation, one `client.old.log` kept); open it from the Settings window. macOS and Linux keep using the standard system log directory.
- **Residue cleanup** — A forced kill or power loss skips client-side cleanup, but the TUN adapter and routes are released when the process handles close or the system reboots; an unfinished logout is recorded encrypted and retried on the next launch/connect so the SSO slot is not left occupied. Legacy hosts blocks can still be removed on the Diagnostics page.

## FAQ

**How do I troubleshoot a failed connection?**

Check in order: gateway address and port, account/password validity, captcha correctness, and whether the official client is fully closed. The log states whether the failure happened during probing, login, adapter creation, tunneling or route injection.

**It says the account is logged in elsewhere (`Ret=-40077`).**

Another session still holds the account. This client automatically follows the official kick-out flow: re-fetch `auth_param`, call `user_pswd_r` to terminate the old session, and continue logging in. It also works when the old session came from the official client and this client has no local marker. If the kick still fails, make sure neither side has a leftover process, wait for the gateway session to time out, or ask the administrator to release it.

**It says the captcha is wrong (`Ret=-40018`).**

The captcha is bound to the current session; reconnect to fetch a new one. Watch out for characters like `+`.

**Login asks for a one-time code (`Ret=-40039`).**

Some user groups have a second factor enabled: after the password check the gateway sends a code to the bound phone or WeCom account. Enter it in the dialog, use "Resend" if needed, and note that a wrong code can be retried up to 3 times; the dialog times out after 5 minutes and the connection must be restarted.

**Why does it need administrator rights?**

Creating a virtual adapter, editing the route table and setting DNS all require administrator rights — a Windows constraint unrelated to the client implementation.

**Routes are left behind after a forced kill and the intranet is unreachable.**

Relaunch the client, clean the legacy hosts residue on the Diagnostics page, then connect and disconnect once — routes are removed from the cleanup ledger.

**Does it support national-cryptography gateways?**

Currently only `vpn_version=ngvone` / `cipher_type=0` gateways are supported; `sm2plusvone` is not implemented.

**Does it run on Windows 7?**

Yes. On startup the app reads the real OS version through `RtlGetVersion`; version 6.1 switches to the netsh compatibility path that avoids the PowerShell 3.0+ NetTCPIP module, while Windows 10/11 keep the original path. Install WebView2 Runtime 109 and the Visual C++ 2015-2022 Redistributable first, and prefer Windows 7 SP1.

**How do I run it on macOS / Linux?**

macOS and Linux use the system TUN device and need root privileges to create the interface and edit routes. On macOS the first launch requests administrator authorization and installs a root LaunchDaemon helper; the GUI stays user-level. You can also run `sudo open "/Applications/TopSAP Client.app"` as a local fallback. On Linux run `sudo ./topsap-client`. Linux needs the WebKitGTK 4.1 runtime (`sudo apt-get install libwebkitgtk-4.1-0` on Ubuntu/Debian). Both platforms use a TUN-based data plane instead of Wintun; the implementation is in place and built by CI, but a full connection regression on real hardware is still pending.

**Can it coexist with the official client?**

Both can be installed, but they cannot log in with the same account simultaneously; pick one to avoid single-sign-on conflicts and duplicate routes.

**The portable build says wintun.dll is missing.**

Keep `topsap-client.exe` and `resources\wintun.dll` at their relative paths; copying only the exe is not enough.

## Repository Layout

```text
topsap-next/
├── src/                        React 19 frontend
│   ├── pages/                  Connect / Diagnostics
│   ├── components/             Settings window, dialogs, ui.tsx and feature components
│   ├── hooks/                  Engine event subscription & connection mutations
│   ├── lib/                    api.ts (single IPC boundary) / format.ts / theme.ts / mock.ts
│   ├── types/engine.ts         Mirrors the Rust DTOs
│   ├── App.tsx                 Root component
│   ├── main.tsx                Frontend entry
│   └── index.css               Tailwind 4 & theme variables
├── src-tauri/                  Rust backend & Tauri configuration
│   ├── src/
│   │   ├── auth/               Login steps & loose JSON parsing
│   │   ├── protocol/           16-byte framing & streaming parser
│   │   ├── tunnel/             TLS handshake & bidirectional pump
│   │   ├── tun/                Windows Wintun / Unix TUN, device lifecycle, self-test
│   │   ├── netcfg/             Adapter address, route injection, DNS, legacy hosts cleanup
│   │   ├── session/            Connection lifecycle, state machine, reconnect
│   │   ├── commands/           Tauri command layer
│   │   ├── lib.rs              App assembly (state / commands / tray / single instance)
│   │   ├── platform.rs         Runtime detection (Windows 7 compatibility branch)
│   │   ├── tls.rs              Shared TLS configuration
│   │   ├── dto.rs / settings.rs / secret_store.rs / proxy_bypass.rs
│   │   ├── helper.rs            macOS root helper (TUN creation + allowlisted commands)
│   │   ├── privileged.rs        macOS helper client and LaunchDaemon installer
│   │   ├── session_guard.rs     Abnormal-exit session marker and deferred logout
│   │   ├── shutdown.rs          Bounded disconnect on shutdown/termination
│   ├── resources/wintun.dll    Bundled virtual adapter driver
│   ├── icons/                  App icons (source: source.svg)
│   ├── build.rs                Injects the requireAdministrator manifest
│   └── tauri.conf.json         Tauri bundling & window configuration
├── docs/                       Protocol, architecture & changelog
├── .github/                    GitHub Actions workflows & preview image
├── LICENSE
├── README.md                   简体中文
└── README.en.md                English (this file)
```

## Documentation

| Start here | Then read on |
| --- | --- |
| [Quick start](#quick-start) | Get a dev environment running |
| [Build & install](#build--install) | Download installers or build from source |
| [Protocol](docs/PROTOCOL.md) | Login endpoints, frame format, network model, limits (Chinese) |
| [Architecture](docs/ARCHITECTURE.md) | Layers, module responsibilities, state machine (Chinese) |
| [Changelog](docs/CHANGELOG.md) | Changes during the current development phase (Chinese) |
| [中文 README](README.md) | 同样的内容，中文版 |

## Release Notes

| Version | Release date | Changelog |
| --- | --- | --- |
| v1.1.0 | 2026-11-03 | [verchanglog/v1.1.0.md](verchanglog/v1.1.0.md) |
| v1.0.0 | 2026-11-03 | [verchanglog/v1.0.0.md](verchanglog/v1.0.0.md) |

Build artifacts and release notes for every version are on the [GitHub Releases](https://github.com/zyx3721/topsap-next/releases) page.

## Acknowledgements

Thanks to the following open-source projects and communities:

- [Tauri](https://tauri.app/) — Cross-platform desktop app framework
- [tokio](https://tokio.rs/) — Asynchronous runtime for Rust
- [rustls](https://github.com/rustls/rustls) — TLS implementation in pure Rust
- [Wintun](https://www.wintun.net/) — User-mode virtual adapter driver
- [React](https://react.dev/) — Frontend UI framework
- [Vite](https://vite.dev/) — Fast frontend build tool
- [Tailwind CSS](https://tailwindcss.com/) — Utility-first styling
- [TanStack Query](https://tanstack.com/query) — Frontend data layer
- [Lucide](https://lucide.dev/) — Open-source icon library

Thanks also to everyone who contributed code, suggestions and bug reports.

## License

This project is released under the [MIT License](LICENSE). You are free to use, copy, modify, merge, publish, distribute, sublicense and sell it, provided the copyright and permission notices are included in all copies or substantial portions.

## Contact

- **GitHub Issues**: [zyx3721/topsap-next/issues](https://github.com/zyx3721/topsap-next/issues)
- **Project Home**: [github.com/zyx3721/topsap-next](https://github.com/zyx3721/topsap-next)

---

**⭐ If this project helps you, please consider giving it a star!**

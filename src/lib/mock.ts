/**
 * 浏览器预览用的假桥。
 *
 * 仅在非 Tauri 环境（`npm run dev` 直接用浏览器打开）启用，
 * 让纯前端改动可以在没有 Rust 后端时预览与截图。
 */

import type {
  AppInfo,
  AppSettings,
  EngineEvent,
  LoginForm,
  ProxyBypassStatus,
  ResidueReport,
  RoutePolicySnapshot,
  SelfTestReport,
  SessionSummary,
  Stats,
  UpdateCheck,
} from "../types/engine";
import { DEFAULT_PROXY_BYPASS } from "./proxyBypass";

// 演示用验证码：160x64 白底，内容 DBCi，带三条干扰线
const CAPTCHA_PNG = "DEMO";

const listeners = new Set<(event: EngineEvent) => void>();
let stats: Stats = {
  vip: "",
  routes: 0,
  duration_secs: 0,
  bytes_up: 0,
  bytes_down: 0,
  packets_in: 0,
  packets_out: 0,
  dropped_out: 0,
};
let timer: ReturnType<typeof setInterval> | null = null;
let pendingForm: LoginForm | null = null;
let connectResolve: ((summary: SessionSummary) => void) | null = null;
let connectReject: ((error: Error) => void) | null = null;

export function isMockMode(): boolean {
  return (
    typeof window !== "undefined" &&
    !("__TAURI_INTERNALS__" in (window as unknown as Record<string, unknown>))
  );
}

function emit(event: EngineEvent) {
  for (const listener of listeners) listener(event);
}

function log(message: string) {
  const now = new Date();
  const stamp = [now.getHours(), now.getMinutes(), now.getSeconds()]
    .map((v) => String(v).padStart(2, "0"))
    .join(":");
  emit({ kind: "log", line: `[${stamp}] ${message}` });
}

function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function startStats() {
  stopStats();
  const startedAt = Date.now();
  timer = setInterval(() => {
    stats = {
      ...stats,
      duration_secs: Math.floor((Date.now() - startedAt) / 1000),
      bytes_up: stats.bytes_up + Math.floor(400 + Math.random() * 5000),
      bytes_down: stats.bytes_down + Math.floor(1200 + Math.random() * 12000),
      packets_in: stats.packets_in + 2,
      packets_out: stats.packets_out + 2,
    };
    emit({ kind: "stats", stats });
  }, 1000);
}

function stopStats() {
  if (timer) clearInterval(timer);
  timer = null;
}

export function mockListen(handler: (event: EngineEvent) => void) {
  listeners.add(handler);
  // 初始状态补发，模拟后端启动事件
  handler({ kind: "state", state: "idle", label: "未连接", detail: "" });
  return () => {
    listeners.delete(handler);
  };
}

export async function mockInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const result = await handle(cmd, args);
  return result as T;
}

async function handle(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
  switch (cmd) {
    case "app_info":
      return {
        version: "0.1.0",
        platform: "windows",
        arch: "x86_64",
        elevated: true,
      } satisfies AppInfo;

    case "check_update":
      return {
        update: {
          current: "0.1.0",
          latest: "0.1.1",
          url: "https://pan.sunline.cn/index.php?share/folder&user=123&sid=ra6BDZXP",
        },
        checked: true,
      } satisfies UpdateCheck;

    case "engine_snapshot":
      return stats;

    case "session_summary":
      return null;

    case "list_connections":
      return {
        active: "办公室",
        connections: [
          {
            name: "办公室",
            login_way: "pwd",
            connect_type: 1,
            connect_select: true,
            connect_timeout: 3,
            lines: ["vpn.example.com:8443", "vpn2.example.com:8443"],
          },
          {
            name: "备用线路",
            login_way: "pwd",
            connect_type: 2,
            connect_select: false,
            connect_timeout: 5,
            lines: ["vpn3.example.com:8443"],
          },
          {
            name: "手动选路",
            login_way: "pwd",
            connect_type: 1,
            connect_select: false,
            connect_timeout: 3,
            lines: ["vpn1.example.com:8443", "vpn2.example.com:8443"],
          },
        ],
      };

    case "save_connection":
      return args?.profile;

    case "delete_connection":
    case "set_active_connection":
    case "clear_connections":
      return null;

    case "import_connections":
      return 2;

    case "export_connections":
      return 2;

    case "fetch_captcha":
      return { png_base64: "DEMO", cookie: "demo-cookie" };

    case "probe_lines": {
      const lines = (args?.lines as string[]) ?? [];
      await sleep(700);
      return lines.map((line, index) => {
        const [host, portText] = line.split(":");
        return {
          line,
          host,
          port: Number(portText) || 443,
          latency_ms: 18 + index * 37,
          ok: index < 2,
          error: index < 2 ? undefined : "连接超时",
        };
      });
    }

    case "forget_password":
      // 演示动态验证码路径：用户名以 sms 开头
      if (String(args?.user ?? "").startsWith("sms")) {
        return { target: "", need_sms: true, cookie: "demo-reset-cookie" };
      }
      return { target: "user@example.com", need_sms: false, cookie: "demo-reset-cookie" };

    case "reset_password":
      return { target: "user@example.com", need_sms: false, cookie: "demo-reset-cookie" };

    case "load_settings":
      return {
        gateway: "vpn.example.com",
        port: 8443,
        user: "demo",
        remember: false,
        auto_reconnect: true,
        auto_connect_on_start: false,
        close_to_tray: true,
        theme: "system",
        proxy_bypass_enabled: true,
        proxy_bypass: [...DEFAULT_PROXY_BYPASS],
        hide_dock_when_closed: false,
        disabled_routes: ["172.16.0.0/12"],
      } satisfies AppSettings;

    case "save_settings":
      return null;

    case "proxy_bypass_status":
      return {
        supported: true,
        enabled: true,
        entries: [...DEFAULT_PROXY_BYPASS],
        defaults: [...DEFAULT_PROXY_BYPASS],
        applied: true,
        message: "演示模式",
      } satisfies ProxyBypassStatus;

    case "route_policy":
      return {
        online: false,
        source: "cache",
        routes: [
          { dest: "10.8.0.0", mask: "255.255.0.0", prefix: 16, enabled: true },
          { dest: "10.20.0.0", mask: "255.255.0.0", prefix: 16, enabled: true },
          { dest: "172.16.0.0", mask: "255.240.0.0", prefix: 12, enabled: false },
          { dest: "192.168.10.0", mask: "255.255.255.0", prefix: 24, enabled: true },
        ],
      } satisfies RoutePolicySnapshot;

    case "load_password":
      return null;

    case "has_saved_password":
      return true;

    case "open_url":
      return null;

    case "open_log_dir":
      return "C:\\Program Files\\TopSAP Client\\data\\logs";

    case "check_residue":
      return {
        hosts: false,
        utun_supported: false,
        utuns: [],
      } satisfies ResidueReport;

    case "purge_residue":
      return false;

    case "cleanup_utun_residue":
      return {
        routes_removed: 0,
        still_present: false,
        message: "当前平台不支持清理 utun 残留",
      };

    case "run_self_test": {
      await sleep(1400);
      return {
        ok: true,
        passed: 9,
        total: 9,
        steps: [
          { name: "管理员权限", status: "ok", detail: "已提权" },
          { name: "定位 wintun.dll", status: "ok", detail: "resources/wintun.dll" },
          { name: "创建虚拟网卡与会话", status: "ok", detail: "网卡 TopSAP" },
          { name: "配置网卡地址", status: "ok", detail: "10.99.99.2/255.255.255.0" },
          { name: "回读网卡地址", status: "ok", detail: "地址已生效（10.99.99.2）" },
          { name: "注入测试路由", status: "ok", detail: "10.98.0.0/255.255.0.0 已生效" },
          { name: "启动读循环", status: "ok", detail: "已就绪" },
          { name: "发送方向（应用 → 网卡）", status: "ok", detail: "已写入 44 字节 ICMP 请求" },
          { name: "接收方向（网卡 → 应用）", status: "ok", detail: "收到 IPv6 ICMPv6（64 字节）" },
        ],
      } satisfies SelfTestReport;
    }

    case "disconnect":
      stopStats();
      pendingForm = null;
      if (connectReject) {
        connectReject(new Error("连接已取消"));
        connectReject = null;
        connectResolve = null;
      }
      stats = {
        vip: "",
        routes: 0,
        duration_secs: 0,
        bytes_up: 0,
        bytes_down: 0,
        packets_in: 0,
        packets_out: 0,
        dropped_out: 0,
      };
      emit({ kind: "state", state: "disconnecting", label: "正在断开", detail: "" });
      await sleep(400);
      log("已断开并恢复网络配置");
      emit({ kind: "state", state: "idle", label: "未连接", detail: "" });
      return null;

    case "submit_captcha": {
      const code = String(args?.code ?? "");
      if (!pendingForm || !code) return false;
      // 演示二次认证：用户名以 sms 开头时触发动态验证码
      if (pendingForm.user.startsWith("sms")) {
        emit({
          kind: "state",
          state: "sms_pending",
          label: "等待动态验证码",
          detail: "请输入动态验证码",
        });
        emit({ kind: "sms_challenge", challenge: false });
        return true;
      }
      const form = pendingForm;
      pendingForm = null;
      emit({ kind: "state", state: "authenticating", label: "正在登录", detail: "正在校验账号" });
      await sleep(600);
      const summary = await finishConnect(form);
      connectResolve?.(summary);
      connectResolve = null;
      connectReject = null;
      return true;
    }

    case "submit_sms": {
      const code = String(args?.code ?? "");
      if (!pendingForm || !code) return false;
      // 演示错误路径：smsdemo 只有输入 067143 才通过
      if (pendingForm.user.startsWith("sms") && code !== "067143") {
        // 先让 submit_sms 返回、弹窗按正常流程关闭，再模拟后端重试
        void (async () => {
          await sleep(400);
          emit({
            kind: "state",
            state: "sms_pending",
            label: "等待动态验证码",
            detail: "验证码错误或已过期，请重新输入",
          });
          emit({
            kind: "sms_challenge",
            challenge: false,
            message: "验证码错误或已过期，请重新输入",
          });
        })();
        return true;
      }
      const form = pendingForm;
      pendingForm = null;
      emit({ kind: "state", state: "authenticating", label: "正在登录", detail: "正在校验账号" });
      await sleep(600);
      const summary = await finishConnect(form);
      connectResolve?.(summary);
      connectResolve = null;
      connectReject = null;
      return true;
    }

    case "resend_sms":
      return 60;

    case "change_password":
      await sleep(500);
      return null;

    case "connect": {
      const form = args?.form as LoginForm;
      // 演示错误提示：用户名为 faildemo 时模拟域名解析失败
      if (form.user.startsWith("fail")) {
        emit({ kind: "state", state: "probing", label: "正在探测网关", detail: "正在探测网关" });
        await sleep(500);
        const message =
          "无法解析网关地址「vpn4.example.com」：未知主机（错误码 11001），请检查域名拼写或网络连接";
        emit({ kind: "state", state: "failed", label: "连接失败", detail: message });
        emit({ kind: "error", message });
        throw new Error(message);
      }
      pendingForm = form;
      emit({ kind: "state", state: "probing", label: "正在探测网关", detail: "正在探测网关" });
      await sleep(500);
      log("网关能力探测完成（ngvone，验证码必需）");
      emit({
        kind: "state",
        state: "captcha_pending",
        label: "等待验证码",
        detail: "等待输入验证码",
      });
      emit({ kind: "captcha", png_base64: CAPTCHA_PNG });

      // 与真机一致：connect 会一直等待验证码提交或取消
      return new Promise<SessionSummary>((resolve, reject) => {
        connectResolve = resolve;
        connectReject = reject;
      });
    }

    default:
      throw new Error(`演示模式不支持命令：${cmd}`);
  }
}

async function finishConnect(form: LoginForm | null) {
  emit({ kind: "state", state: "creating_tun", label: "正在创建网卡", detail: "" });
  await sleep(500);
  log("虚拟网卡已就绪：TopSAP");
  emit({ kind: "state", state: "handshaking", label: "正在建立隧道", detail: "" });
  await sleep(700);
  log("隧道 TLS 已建立（TLSv1.2）");
  emit({ kind: "state", state: "configuring", label: "正在配置网络", detail: "正在注入路由" });
  await sleep(500);
  log("已注入 7/7 条资源路由，DNS 198.51.100.5");

  stats = {
    vip: "10.8.0.10",
    routes: 7,
    duration_secs: 0,
    bytes_up: 0,
    bytes_down: 0,
    packets_in: 0,
    packets_out: 0,
    dropped_out: 0,
  };
  emit({ kind: "stats", stats });
  emit({ kind: "state", state: "connected", label: "已连接", detail: "" });
  log("登录成功：demo，虚拟 IP 10.8.0.10，下发路由 5 条");
  startStats();

  const summary: SessionSummary = {
    user: form?.user ?? "demo",
    vip: "10.8.0.10",
    mask: "255.255.248.0",
    dns1: "198.51.100.5",
    dns2: "198.51.100.6",
    routes: 5,
    hosts: 1,
    acl: 22,
    tunnel_type: 1,
    gateway: form?.gateway ?? "vpn.example.com",
    port: form?.port ?? 8443,
    gateway_version: "v3.220094.0061_NGVONE.1",
    login_info: {
      current_ip: "203.0.113.24",
      current_time: "1762100000",
      previous_ip: "203.0.113.18",
      previous_time: "1762010000",
      failed_ip: "203.0.113.9",
      failed_time: "1759900000",
    },
    resources: [
      { name: "10.0.1.0", url: "", group: "办公区", module: "na" },
      { name: "10.0.2.0", url: "", group: "办公区", module: "na" },
      { name: "10.0.3.0", url: "", group: "办公区", module: "cifs" },
      { name: "192.168.10.0", url: "", group: "服务器", module: "na" },
      { name: "192.168.20.0", url: "", group: "服务器", module: "na" },
      { name: "172.16.0.0", url: "", group: "服务器", module: "na" },
      { name: "10.10.10.10", url: "", group: "默认组", module: "na" },
    ],
  };
  // connect 命令在真机上返回 summary；mock 中通过事件即可驱动界面
  return summary;
}

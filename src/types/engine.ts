/** 与 Rust DTO 一一对应（字段保持 snake_case） */

export type EnginePhase =
  | "idle"
  | "probing"
  | "captcha_pending"
  | "sms_pending"
  | "authenticating"
  | "creating_tun"
  | "handshaking"
  | "configuring"
  | "connected"
  | "reconnecting"
  | "disconnecting"
  | "failed";

export interface AppInfo {
  version: string;
  platform: string;
  arch: string;
  elevated: boolean;
}

export interface UpdateInfo {
  current: string;
  latest: string;
  url: string;
}

export interface UpdateCheck {
  update: UpdateInfo | null;
  checked: boolean;
}

export interface UpdateProgress {
  stage: string;
  downloaded: number;
  total: number;
}

export interface LoginForm {
  gateway: string;
  port?: number;
  user: string;
  password: string;
  remember: boolean;
  lines?: string[];
  connect_type?: number;
  connect_timeout?: number;
}

export interface ConnectOptions {
  passwordAutoFilled?: boolean;
}

export interface ConnectionProfile {
  name: string;
  login_way: string;
  connect_type: number;
  connect_select: boolean;
  connect_timeout: number;
  lines: string[];
}

export interface ProfileStore {
  active: string;
  connections: ConnectionProfile[];
}

export interface CaptchaDto {
  png_base64: string;
  cookie: string;
}

export interface ForgetPasswordResult {
  target: string;
  need_sms: boolean;
  cookie: string;
}

export interface LineProbe {
  line: string;
  host: string;
  port: number;
  latency_ms: number;
  ok: boolean;
  error?: string;
}

export interface SessionSummary {
  user: string;
  vip: string;
  mask: string;
  dns1: string;
  dns2: string;
  routes: number;
  hosts: number;
  acl: number;
  tunnel_type: number;
  gateway: string;
  port: number;
  gateway_version: string;
  login_info: LoginInfo;
  resources: ResourceEntry[];
}

export interface LoginInfo {
  current_ip: string;
  current_time: string;
  previous_ip: string;
  previous_time: string;
  failed_ip: string;
  failed_time: string;
}

export interface ResourceEntry {
  name: string;
  url: string;
  group: string;
  module: string;
}

export interface Stats {
  vip: string;
  routes: number;
  duration_secs: number;
  bytes_up: number;
  bytes_down: number;
  packets_in: number;
  packets_out: number;
  dropped_out: number;
}

export type RouteConflictKind = "tunnel" | "physical" | "loopback" | "other";

export interface RouteConflict {
  dest: string;
  mask: string;
  prefix: number;
  gateway: string;
  ifname: string;
  kind: RouteConflictKind;
}

export interface RouteConflictReport {
  total: number;
  conflicts: RouteConflict[];
}

export type SelfTestStatus = "ok" | "fail" | "info";

export interface SelfTestStep {
  name: string;
  status: SelfTestStatus;
  detail: string;
}

export interface SelfTestReport {
  ok: boolean;
  passed: number;
  total: number;
  steps: SelfTestStep[];
}

export type ThemeMode = "system" | "dark" | "light";

export interface AppSettings {
  gateway: string;
  port: number;
  user: string;
  remember: boolean;
  auto_reconnect: boolean;
  /** 断线自动重连的最大次数；0 表示不限次数 */
  auto_reconnect_max_attempts: number;
  /** 启动软件后自动使用默认配置与已保存密码连接 */
  auto_connect_on_start: boolean;
  close_to_tray: boolean;
  theme: ThemeMode;
  proxy_bypass_enabled: boolean;
  proxy_bypass: string[];
  /** macOS：关闭窗口后同时隐藏 Dock 图标，仅保留菜单栏托盘 */
  hide_dock_when_closed: boolean;
  /** 不写入系统路由表的网段，格式 `目的地址/前缀` */
  disabled_routes: string[];
}

/** 设置页可勾选的一条隧道路由 */
export interface RouteRule {
  dest: string;
  mask: string;
  prefix: number;
  /** false 表示该网段不写入系统路由表 */
  enabled: boolean;
}

export interface RoutePolicySnapshot {
  routes: RouteRule[];
  /** 当前是否存在已登录会话 */
  online: boolean;
  /** session=当前会话，cache=上次登录 */
  source: string;
}

export interface ProxyBypassStatus {
  supported: boolean;
  enabled: boolean;
  entries: string[];
  defaults: string[];
  applied: boolean;
  message: string;
}

export interface UtunResidue {
  name: string;
  address: string;
  peer: string;
  mtu: number;
  route_count: number;
  active: boolean;
}

export interface ResidueReport {
  hosts: boolean;
  utun_supported: boolean;
  utuns: UtunResidue[];
}

export interface UtunCleanupResult {
  routes_removed: number;
  still_present: boolean;
  message: string;
}

export type EngineEvent =
  | { kind: "state"; state: EnginePhase; label: string; detail: string }
  | { kind: "log"; line: string }
  | { kind: "stats"; stats: Stats }
  | { kind: "captcha"; png_base64: string }
  | { kind: "sms_challenge"; challenge: boolean; message?: string }
  | {
      kind: "password_change_required";
      code: number;
      user: string;
      host: string;
      port: number;
      message: string;
    }
  | { kind: "expiry_reminder"; account_end: number; password_remind: number }
  | { kind: "route_conflict"; total: number; conflicts: RouteConflict[] }
  | { kind: "error"; message: string };

export const EMPTY_STATS: Stats = {
  vip: "",
  routes: 0,
  duration_secs: 0,
  bytes_up: 0,
  bytes_down: 0,
  packets_in: 0,
  packets_out: 0,
  dropped_out: 0,
};

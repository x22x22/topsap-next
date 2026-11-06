import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  AppInfo,
  AppSettings,
  CaptchaDto,
  ConnectionProfile,
  EngineEvent,
  ForgetPasswordResult,
  LineProbe,
  LoginForm,
  ProfileStore,
  ProxyBypassStatus,
  ResidueReport,
  SelfTestReport,
  SelfTestStep,
  SessionSummary,
  Stats,
  UpdateCheck,
} from "../types/engine";
import { isMockMode, mockInvoke, mockListen } from "./mock";

const call = <T>(cmd: string, args?: Record<string, unknown>): Promise<T> =>
  isMockMode() ? mockInvoke<T>(cmd, args) : invoke<T>(cmd, args);

/** 前端唯一的 IPC 边界：组件禁止直接 invoke */
export const api = {
  appInfo: () => call<AppInfo>("app_info"),
  checkUpdate: () => call<UpdateCheck>("check_update"),
  openUrl: (url: string) => call<void>("open_url", { url }),

  connect: (form: LoginForm) => call<SessionSummary>("connect", { form }),
  disconnect: () => call<void>("disconnect"),
  snapshot: () => call<Stats>("engine_snapshot"),
  submitCaptcha: (code: string) => call<boolean>("submit_captcha", { code }),
  submitSms: (code: string) => call<boolean>("submit_sms", { code }),
  submitRouteConflict: (accept: boolean) =>
    call<boolean>("submit_route_conflict", { accept }),
  resendSms: () => call<number>("resend_sms"),
  changePassword: (oldPassword: string, newPassword: string) =>
    call<void>("change_password", { oldPassword, newPassword }),
  changePasswordForLogin: (
    gateway: string,
    port: number,
    user: string,
    oldPassword: string,
    newPassword: string,
  ) =>
    call<void>("change_password_for_login", {
      gateway,
      port,
      user,
      oldPassword,
      newPassword,
    }),
  fetchCaptcha: (gateway: string, port: number) =>
    call<CaptchaDto>("fetch_captcha", { gateway, port }),
  probeLines: (lines: string[], timeout: number) =>
    call<LineProbe[]>("probe_lines", { lines, timeout }),
  forgetPassword: (gateway: string, port: number, user: string, code: string, cookie: string) =>
    call<ForgetPasswordResult>("forget_password", { gateway, port, user, code, cookie }),
  resetPassword: (
    gateway: string,
    port: number,
    user: string,
    sms: string,
    newPassword: string,
    cookie: string,
  ) =>
    call<ForgetPasswordResult>("reset_password", {
      gateway,
      port,
      user,
      sms,
      newPassword,
      cookie,
    }),

  listConnections: () => call<ProfileStore>("list_connections"),
  saveConnection: (profile: ConnectionProfile) =>
    call<ConnectionProfile>("save_connection", { profile }),
  deleteConnection: (name: string) => call<void>("delete_connection", { name }),
  setActiveConnection: (name: string) => call<void>("set_active_connection", { name }),
  importConnections: (path: string) => call<number>("import_connections", { path }),
  exportConnections: (path: string) => call<number>("export_connections", { path }),
  clearConnections: () => call<void>("clear_connections"),

  selfTest: () => call<SelfTestReport>("run_self_test"),
  checkResidue: () => call<ResidueReport>("check_residue"),
  purgeResidue: () => call<boolean>("purge_residue"),

  loadSettings: () => call<AppSettings>("load_settings"),
  saveSettings: (settings: AppSettings) => call<void>("save_settings", { settings }),
  proxyBypassStatus: () => call<ProxyBypassStatus>("proxy_bypass_status"),
  loadPassword: (user: string) => call<string | null>("load_password", { user }),

  openLogDir: () => call<string>("open_log_dir"),

  onEngineEvent: (handler: (event: EngineEvent) => void): Promise<UnlistenFn> => {
    if (isMockMode()) {
      const unsubscribe = mockListen(handler);
      return Promise.resolve(unsubscribe);
    }
    return listen<EngineEvent>("engine://event", (event) => handler(event.payload));
  },

  onSelfTestStep: (handler: (step: SelfTestStep) => void): Promise<UnlistenFn> => {
    if (isMockMode()) {
      return Promise.resolve(() => {});
    }
    return listen<SelfTestStep>("self_test://step", (event) => handler(event.payload));
  },
};

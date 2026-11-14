import { useCallback, useEffect, useState } from "react";

import { api } from "../lib/api";
import {
  EMPTY_STATS,
  type EngineEvent,
  type EnginePhase,
  type RouteConflictReport,
  type Stats,
} from "../types/engine";

const MAX_LOG_LINES = 800;

export interface EngineView {
  phase: EnginePhase;
  label: string;
  detail: string;
  /** 最近一次状态变化是否来自异常断开（用于区分人为断开） */
  abnormalDisconnect: boolean;
  logs: string[];
  stats: Stats;
  captcha: string | null;
  smsChallenge: { challenge: boolean; message?: string } | null;
  passwordChange: {
    code: number;
    user: string;
    host: string;
    port: number;
    message: string;
  } | null;
  expiryReminder: { accountEnd: number; passwordRemind: number } | null;
  routeConflict: RouteConflictReport | null;
  clearPasswordChange?: () => void;
  clearExpiryReminder?: () => void;
  clearRouteConflict?: () => void;
}

const INITIAL: EngineView = {
  phase: "idle",
  label: "未连接",
  detail: "",
  abnormalDisconnect: false,
  logs: [],
  stats: EMPTY_STATS,
  captcha: null,
  smsChallenge: null,
  passwordChange: null,
  expiryReminder: null,
  routeConflict: null,
};

/** 订阅引擎事件并维护界面侧的视图状态 */
export function useEngineEvents() {
  const [view, setView] = useState<EngineView>(INITIAL);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    api
      .onEngineEvent((event: EngineEvent) => {
        if (disposed) return;
        setView((prev) => applyEvent(prev, event));
      })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => {
        // 事件通道不可用时，界面仍可通过快照与命令结果工作
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  // 首次挂载补一次快照，避免错过启动早期事件
  useEffect(() => {
    let cancelled = false;
    api
      .snapshot()
      .then((stats) => {
        if (!cancelled) setView((prev) => ({ ...prev, stats }));
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  const clearCaptcha = useCallback(() => {
    setView((prev) => (prev.captcha ? { ...prev, captcha: null } : prev));
  }, []);

  const clearSms = useCallback(() => {
    setView((prev) => (prev.smsChallenge ? { ...prev, smsChallenge: null } : prev));
  }, []);

  const clearLogs = useCallback(() => {
    setView((prev) => (prev.logs.length ? { ...prev, logs: [] } : prev));
  }, []);

  const clearPasswordChange = useCallback(() => {
    setView((prev) => (prev.passwordChange ? { ...prev, passwordChange: null } : prev));
  }, []);

  const clearExpiryReminder = useCallback(() => {
    setView((prev) => (prev.expiryReminder ? { ...prev, expiryReminder: null } : prev));
  }, []);

  const clearRouteConflict = useCallback(() => {
    setView((prev) => (prev.routeConflict ? { ...prev, routeConflict: null } : prev));
  }, []);

  return {
    ...view,
    clearCaptcha,
    clearSms,
    clearLogs,
    clearPasswordChange,
    clearExpiryReminder,
    clearRouteConflict,
  };
}

function applyEvent(prev: EngineView, event: EngineEvent): EngineView {
  switch (event.kind) {
    case "state": {
      // 连接失败、取消或已连上时，动态验证码弹窗必须收起，避免残留在旧流程上
      const closed =
        event.state === "idle" ||
        event.state === "connected" ||
        event.state === "failed" ||
        event.state === "disconnecting";
      return {
        ...prev,
        phase: event.state,
        abnormalDisconnect: event.abnormal ?? false,
        label: event.label,
        detail: event.detail,
        smsChallenge: closed ? null : prev.smsChallenge,
        routeConflict: closed ? null : prev.routeConflict,
      };
    }
    case "log":
      return { ...prev, logs: trimLogs([...prev.logs, event.line]) };
    case "stats":
      return { ...prev, stats: event.stats };
    case "captcha":
      return { ...prev, captcha: event.png_base64 };
    case "sms_challenge":
      return {
        ...prev,
        smsChallenge: { challenge: event.challenge, message: event.message },
      };
    case "password_change_required":
      return {
        ...prev,
        passwordChange: {
          code: event.code,
          user: event.user,
          host: event.host,
          port: event.port,
          message: event.message,
        },
      };
    case "expiry_reminder":
      return {
        ...prev,
        expiryReminder: {
          accountEnd: event.account_end,
          passwordRemind: event.password_remind,
        },
      };
    case "route_conflict":
      return {
        ...prev,
        routeConflict: { total: event.total, conflicts: event.conflicts },
      };
    case "error":
      return { ...prev, logs: trimLogs([...prev.logs, `错误：${event.message}`]) };
    default:
      return prev;
  }
}

function trimLogs(lines: string[]): string[] {
  return lines.length > MAX_LOG_LINES ? lines.slice(lines.length - MAX_LOG_LINES) : lines;
}

/** 判断当前是否处于「流程进行中」，用于禁用重复操作 */
export function isBusyPhase(phase: EnginePhase): boolean {
  return (
    phase === "probing" ||
    phase === "captcha_pending" ||
    phase === "sms_pending" ||
    phase === "authenticating" ||
    phase === "creating_tun" ||
    phase === "handshaking" ||
    phase === "configuring" ||
    phase === "disconnecting"
  );
}

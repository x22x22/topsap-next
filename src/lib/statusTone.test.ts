import { describe, expect, it } from "vitest";

import { statusTone } from "./statusTone";
import type { EnginePhase } from "../types/engine";

const CONNECTING: EnginePhase[] = [
  "probing",
  "captcha_pending",
  "sms_pending",
  "authenticating",
  "creating_tun",
  "handshaking",
  "configuring",
  "reconnecting",
];

describe("statusTone", () => {
  it("已连接为绿色", () => {
    expect(statusTone("connected")).toBe("ok");
    expect(statusTone("connected", true)).toBe("ok");
  });

  it("连接中与重连中为蓝色", () => {
    for (const phase of CONNECTING) {
      expect(statusTone(phase)).toBe("info");
      expect(statusTone(phase, true)).toBe("info");
    }
  });

  it("从未连接为灰色，连接过之后断开为红色", () => {
    expect(statusTone("idle")).toBe("faint");
    expect(statusTone("idle", false)).toBe("faint");
    expect(statusTone("idle", true)).toBe("danger");
    expect(statusTone("disconnecting", true)).toBe("danger");
    expect(statusTone("disconnecting")).toBe("faint");
  });

  it("失败始终为红色", () => {
    expect(statusTone("failed")).toBe("danger");
    expect(statusTone("failed", true)).toBe("danger");
  });
});

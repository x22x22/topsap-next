import { describe, expect, it } from "vitest";

import { autoConnectAvailability } from "./autoConnect";

describe("autoConnectAvailability", () => {
  it("配置、账号、密码齐全时可用", () => {
    expect(
      autoConnectAvailability({ profileCount: 1, user: "demo", hasPassword: true }),
    ).toEqual({ available: true, reason: null });
  });

  it("没有连接配置时不可用", () => {
    expect(
      autoConnectAvailability({ profileCount: 0, user: "demo", hasPassword: true }),
    ).toEqual({ available: false, reason: "profiles" });
  });

  it("没有账号（含仅空白）时不可用", () => {
    expect(
      autoConnectAvailability({ profileCount: 1, user: "", hasPassword: true }),
    ).toEqual({ available: false, reason: "account" });
    expect(
      autoConnectAvailability({ profileCount: 1, user: "   ", hasPassword: true }),
    ).toEqual({ available: false, reason: "account" });
  });

  it("没有已保存密码时不可用", () => {
    expect(
      autoConnectAvailability({ profileCount: 1, user: "demo", hasPassword: false }),
    ).toEqual({ available: false, reason: "password" });
  });

  it("优先报告配置缺失", () => {
    expect(
      autoConnectAvailability({ profileCount: 0, user: "", hasPassword: false }),
    ).toEqual({ available: false, reason: "profiles" });
  });
});

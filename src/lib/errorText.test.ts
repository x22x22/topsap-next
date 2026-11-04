import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { localizeError } from "./errorText";

function useLanguage(lang: "zh" | "en") {
  (globalThis as unknown as { window: unknown }).window = {
    localStorage: { getItem: () => lang },
  };
}

describe("localizeError", () => {
  beforeEach(() => useLanguage("en"));

  afterEach(() => {
    delete (globalThis as unknown as { window?: unknown }).window;
  });

  it("翻译 DNS 解析失败", () => {
    const text = localizeError(
      "无法解析网关地址「vpn.example.com」：未知主机（错误码 11001），请检查域名拼写或网络连接",
    );
    expect(text).toContain('Unable to resolve gateway "vpn.example.com"');
    expect(text).toContain("unknown host (error 11001)");
  });

  it("翻译端口返回网页的提示", () => {
    const text = localizeError(
      "该端口返回的是网页而不是 VPN 网关接口（HTTP 400），请检查端口是否正确",
    );
    expect(text).toContain("web page instead of the VPN gateway API");
    expect(text).toContain("HTTP 400");
  });

  it("翻译网关返回码", () => {
    expect(localizeError("账号或密码错误（Ret=-40081）")).toBe(
      "Incorrect username or password (Ret=-40081).",
    );
  });

  it("翻译登录时要求改密的提示", () => {
    expect(localizeError("登录用户密码有效期已到，请修改密码")).toBe(
      "Your password has expired. Change it to continue.",
    );
    expect(localizeError("密码过于简单，请修改密码")).toBe(
      "Your password is too weak. Change it to continue.",
    );
    expect(localizeError("首次登录，请修改初始密码")).toBe(
      "First login: change the initial password.",
    );
    expect(localizeError("管理员要求修改密码")).toBe(
      "The administrator requires a password change.",
    );
    expect(localizeError("需要修改密码后才能登录")).toBe(
      "Change your password before signing in",
    );
  });

  it("翻译自检与连接的互斥提示", () => {
    expect(localizeError("网卡自检正在进行中，请稍候再连接")).toBe(
      "The adapter self-test is running. Wait for it to finish before connecting.",
    );
    expect(localizeError("请先等待当前连接流程结束或断开连接后再运行自检")).toBe(
      "Wait for the current connection attempt to finish, or disconnect, before running the self-test.",
    );
  });

  it("翻译会话与连接状态错误", () => {
    expect(localizeError("已有连接操作正在进行，请稍候")).toBe(
      "A connection operation is already in progress. Please wait.",
    );
    expect(localizeError("所有线路均不可用")).toBe("No line is available.");
  });

  it("未匹配的文案保持原样", () => {
    expect(localizeError("未知的新错误")).toBe("未知的新错误");
  });

  it("中文模式不翻译", () => {
    useLanguage("zh");
    expect(localizeError("连接已取消")).toBe("连接已取消");
  });
});

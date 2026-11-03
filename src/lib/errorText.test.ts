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

  it("未匹配的文案保持原样", () => {
    expect(localizeError("未知的新错误")).toBe("未知的新错误");
  });

  it("中文模式不翻译", () => {
    useLanguage("zh");
    expect(localizeError("连接已取消")).toBe("连接已取消");
  });
});

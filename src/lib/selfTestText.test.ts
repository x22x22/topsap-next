import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { localizeSelfTestDetail } from "./selfTestText";

function useLanguage(lang: "zh" | "en") {
  (globalThis as unknown as { window: unknown }).window = {
    localStorage: { getItem: () => lang },
  };
}

describe("localizeSelfTestDetail", () => {
  beforeEach(() => useLanguage("en"));

  afterEach(() => {
    delete (globalThis as unknown as { window?: unknown }).window;
  });

  it("翻译静态结果与带变量句式", () => {
    expect(localizeSelfTestDetail("已提权")).toBe("Elevated");
    expect(localizeSelfTestDetail("网卡 TopSAP")).toBe("Adapter TopSAP");
    expect(localizeSelfTestDetail("地址已生效（10.99.99.2）")).toBe(
      "Address is active (10.99.99.2)",
    );
    expect(localizeSelfTestDetail("10.98.0.0/255.255.0.0 已生效")).toBe(
      "10.98.0.0/255.255.0.0 is active",
    );
    expect(localizeSelfTestDetail("收到报文：IPv6 报文（64 字节）")).toBe(
      "Packet received: IPv6 packet (64 bytes)",
    );
  });

  it("未匹配的句子原样返回", () => {
    expect(localizeSelfTestDetail("未知错误")).toBe("未知错误");
  });

  it("中文模式不做转换", () => {
    useLanguage("zh");
    expect(localizeSelfTestDetail("已提权")).toBe("已提权");
  });
});

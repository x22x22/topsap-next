import { describe, expect, it } from "vitest";

import {
  DEFAULT_PROXY_BYPASS,
  addProxyEntry,
  mergeDefaultProxyBypass,
  normalizeProxyEntry,
} from "./proxyBypass";

describe("proxyBypass", () => {
  it("默认只包含两个泛域名", () => {
    expect(DEFAULT_PROXY_BYPASS).toEqual(["*.sunline.cn", "*.sunline-i.com"]);
  });

  it("规范化为小写并拒绝危险条目", () => {
    expect(normalizeProxyEntry(" *.Sunline.CN ")).toBe("*.sunline.cn");
    expect(normalizeProxyEntry("192.0.2.10")).toBe("192.0.2.10");
    expect(normalizeProxyEntry("[2001:db8::1]")).toBe("[2001:db8::1]");
    expect(normalizeProxyEntry("*")).toBeNull();
    expect(normalizeProxyEntry("a b")).toBeNull();
    expect(normalizeProxyEntry("http://example.com")).toBeNull();
    expect(normalizeProxyEntry("*.sunline.cn/")).toBeNull();
  });

  it("添加时去重并校验", () => {
    const first = addProxyEntry(["*.sunline.cn"], "*.sunline-i.com");
    expect(first.error).toBeUndefined();
    expect(first.entries).toEqual(["*.sunline.cn", "*.sunline-i.com"]);

    const duplicate = addProxyEntry(first.entries, "*.SUNLINE.CN");
    expect(duplicate.error).toBe("该条目已存在");

    const invalid = addProxyEntry(first.entries, "a;b");
    expect(invalid.error).toBe("请输入合法的域名、泛域名或 IP 地址");
  });

  it("恢复默认时补齐缺失项且保留自定义项", () => {
    expect(mergeDefaultProxyBypass(["10.0.0.1"])).toEqual([
      "10.0.0.1",
      "*.sunline.cn",
      "*.sunline-i.com",
    ]);
  });
});

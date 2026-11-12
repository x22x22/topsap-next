import { describe, expect, it } from "vitest";

import {
  isRouteEnabled,
  normalizeRouteKey,
  routeKey,
  setAllRoutes,
  setRouteEnabled,
} from "./routePolicy";

describe("normalizeRouteKey", () => {
  it("归一化 CIDR 到网络地址", () => {
    expect(normalizeRouteKey("10.1.2.3/8")).toBe("10.0.0.0/8");
    expect(normalizeRouteKey(" 192.168.10.5/24 ")).toBe("192.168.10.0/24");
    expect(normalizeRouteKey("0.0.0.0/0")).toBe("0.0.0.0/0");
    expect(normalizeRouteKey("10.0.0.0/32")).toBe("10.0.0.0/32");
  });

  it("支持点分掩码", () => {
    expect(normalizeRouteKey("192.168.10.5 255.255.255.0")).toBe(
      "192.168.10.0/24",
    );
    expect(normalizeRouteKey("10.0.0.0 255.0.0.0")).toBe("10.0.0.0/8");
  });

  it("拒绝非法输入", () => {
    expect(normalizeRouteKey("")).toBeNull();
    expect(normalizeRouteKey("10.0.0.0/33")).toBeNull();
    expect(normalizeRouteKey("10.0.0.256/8")).toBeNull();
    expect(normalizeRouteKey("10.0.0.0")).toBeNull();
    expect(normalizeRouteKey("10.0.0.0/8/16")).toBeNull();
    expect(normalizeRouteKey("10.0.0.0 255.0.255.0")).toBeNull();
  });
});

describe("路由勾选", () => {
  it("生成设置 key", () => {
    expect(routeKey("10.0.0.0", 8)).toBe("10.0.0.0/8");
  });

  it("默认生效，关闭后写入列表", () => {
    const disabled: string[] = [];
    expect(isRouteEnabled(disabled, "10.0.0.0", 8)).toBe(true);
    const next = setRouteEnabled(disabled, "10.0.0.0", 8, false);
    expect(next).toEqual(["10.0.0.0/8"]);
    expect(isRouteEnabled(next, "10.0.0.0", 8)).toBe(false);
  });

  it("重复关闭不产生重复条目", () => {
    const first = setRouteEnabled([], "10.0.0.0", 8, false);
    expect(setRouteEnabled(first, "10.0.0.0", 8, false)).toEqual(first);
  });

  it("重新启用会移除该网段且保留其它条目", () => {
    const disabled = ["10.0.0.0/8", "192.168.0.0/16"];
    expect(setRouteEnabled(disabled, "10.0.0.0", 8, true)).toEqual([
      "192.168.0.0/16",
    ]);
  });

  it("全部启用清空列表，全部关闭覆盖所有路由", () => {
    const routes = [
      { dest: "10.0.0.0", prefix: 8 },
      { dest: "192.168.0.0", prefix: 16 },
    ];
    expect(setAllRoutes(routes, false)).toEqual([
      "10.0.0.0/8",
      "192.168.0.0/16",
    ]);
    expect(setAllRoutes(routes, true)).toEqual([]);
  });
});

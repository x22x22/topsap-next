import { describe, expect, it } from "vitest";

import {
  disableRoutes,
  enableRoutes,
  isRouteEnabled,
  normalizeRouteKey,
  routeKey,
  setRouteEnabled,
} from "./routePolicy";

describe("normalizeRouteKey", () => {
  it("归一化 CIDR 到网络地址", () => {
    expect(normalizeRouteKey("10.1.2.3/8")).toBe("10.0.0.0/8");
    expect(normalizeRouteKey(" 192.168.10.5/24 ")).toBe("192.168.10.0/24");
    expect(normalizeRouteKey("0.0.0.0/0")).toBe("0.0.0.0/0");
    expect(normalizeRouteKey("10.0.0.0/32")).toBe("10.0.0.0/32");
    expect(normalizeRouteKey("10.0.0.0/08")).toBe("10.0.0.0/8");
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

  it("拒绝空或非十进制前缀", () => {
    // 这些在 Number() 下会被静默转换："" -> 0、0x18 -> 24、1e1 -> 10
    expect(normalizeRouteKey("10.0.0.0/")).toBeNull();
    expect(normalizeRouteKey("10.0.0.0/  ")).toBeNull();
    expect(normalizeRouteKey("10.0.0.0/0x18")).toBeNull();
    expect(normalizeRouteKey("10.0.0.0/1e1")).toBeNull();
    expect(normalizeRouteKey("10.0.0.0/+8")).toBeNull();
    expect(normalizeRouteKey("10.0.0.0/24.0")).toBeNull();
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

  it("全部关闭与手动排除项合并，不丢用户配置", () => {
    const routes = [
      { dest: "10.0.0.0", prefix: 8 },
      { dest: "192.168.0.0", prefix: 16 },
    ];
    const result = disableRoutes(["172.16.0.0/12"], routes);
    expect(result.entries).toEqual([
      "172.16.0.0/12",
      "10.0.0.0/8",
      "192.168.0.0/16",
    ]);
    expect(result.truncated).toBe(false);
  });

  it("全部启用只移除列表内网段，保留手动排除项", () => {
    const routes = [
      { dest: "10.0.0.0", prefix: 8 },
      { dest: "192.168.0.0", prefix: 16 },
    ];
    expect(
      enableRoutes(["10.0.0.0/8", "172.16.0.0/12"], routes),
    ).toEqual(["172.16.0.0/12"]);
  });

  it("合并后超过上限时截断并标记", () => {
    const routes = Array.from({ length: 300 }, (_, index) => ({
      dest: `10.${Math.floor(index / 256)}.${index % 256}.0`,
      prefix: 24,
    }));
    const result = disableRoutes(["192.168.0.0/16"], routes);
    expect(result.entries).toHaveLength(256);
    expect(result.truncated).toBe(true);
    expect(result.entries[0]).toBe("192.168.0.0/16");
  });
});

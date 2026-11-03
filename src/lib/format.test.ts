import { describe, expect, it } from "vitest";

import {
  firstLine,
  formatBytes,
  formatDuration,
  formatUnixTime,
  isValidGateway,
  isValidPort,
} from "./format";

describe("formatBytes", () => {
  it("按 1024 进位", () => {
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(1024 * 1024)).toBe("1.0 MB");
  });

  it("零值与负数归零", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(-5)).toBe("0 B");
  });

  it("不足 1 KB 不带小数", () => {
    expect(formatBytes(512)).toBe("512 B");
  });

  it("非有限数值不产生 NaN 文案", () => {
    expect(formatBytes(Number.NaN)).toBe("0 B");
    expect(formatBytes(Number.POSITIVE_INFINITY)).toBe("0 B");
  });
});

describe("formatDuration", () => {
  it("按 时分秒 补零", () => {
    expect(formatDuration(0)).toBe("00:00:00");
    expect(formatDuration(65)).toBe("00:01:05");
    expect(formatDuration(3661)).toBe("01:01:01");
  });

  it("负数与小数向下取整", () => {
    expect(formatDuration(-10)).toBe("00:00:00");
    expect(formatDuration(59.9)).toBe("00:00:59");
  });
});

describe("firstLine", () => {
  it("取多行文本首行", () => {
    expect(firstLine("第一行\n第二行")).toBe("第一行");
  });

  it("去掉 JS Error 前缀", () => {
    expect(firstLine("Error: 无法解析网关地址")).toBe("无法解析网关地址");
    expect(firstLine("error: 连接失败")).toBe("连接失败");
  });

  it("空文本回退原文", () => {
    expect(firstLine("")).toBe("");
  });
});

describe("formatUnixTime", () => {
  it("0 与空值原样回退", () => {
    expect(formatUnixTime("0")).toBe("0");
    expect(formatUnixTime("")).toBe("—");
  });

  it("Unix 秒转为本地时间", () => {
    expect(formatUnixTime("1762100000")).toMatch(/^2025-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/);
  });
});

describe("isValidGateway", () => {
  it("接受域名与 IPv4", () => {
    expect(isValidGateway("vpn.example.com")).toBe(true);
    expect(isValidGateway("192.0.2.10")).toBe(true);
    expect(isValidGateway("  vpn.example.com  ")).toBe(true);
  });

  it("拒绝协议前缀、路径与空白", () => {
    expect(isValidGateway("https://vpn.example.com")).toBe(false);
    expect(isValidGateway("vpn.example.com/path")).toBe(false);
    expect(isValidGateway("vpn example")).toBe(false);
    expect(isValidGateway("")).toBe(false);
  });

  it("拒绝非法 IPv4 段", () => {
    expect(isValidGateway("192.0.2.256")).toBe(false);
    expect(isValidGateway("192.0.2")).toBe(false);
  });

  it("拒绝以连字符开头或结尾的主机名", () => {
    expect(isValidGateway("-vpn.example.com")).toBe(false);
    expect(isValidGateway("vpn-.example.com")).toBe(false);
  });
});

describe("isValidPort", () => {
  it("接受常见端口", () => {
    expect(isValidPort(443)).toBe(true);
    expect(isValidPort(8443)).toBe(true);
    expect(isValidPort(65535)).toBe(true);
  });

  it("拒绝越界与非整数", () => {
    expect(isValidPort(0)).toBe(false);
    expect(isValidPort(65536)).toBe(false);
    expect(isValidPort(443.5)).toBe(false);
  });
});

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

  it("区分 TUN 权限不足与网卡占用", () => {
    expect(
      localizeError(
        "创建 TUN 设备失败：网卡 topsap0 已被占用，可能是上次自检或连接未退出；请退出所有 TopSAP 实例，或执行 sudo ip link del topsap0 后重试",
      ),
    ).toBe(
      "Failed to create the TUN device: adapter topsap0 is already in use, likely left by a previous self-test or connection. Quit all TopSAP instances, or run sudo ip link del topsap0 and retry.",
    );
  });

  it("翻译 Linux 网卡配置权限错误", () => {
    expect(
      localizeError(
        "配置网卡地址失败（当前：10.99.99.2）；请以 root 运行（sudo /usr/bin/topsap-client）",
      ),
    ).toBe(
      "Failed to configure the adapter address (current: 10.99.99.2). Run the app as root with sudo /usr/bin/topsap-client.",
    );
  });

  it("翻译会话与连接状态错误", () => {
    expect(localizeError("已有连接操作正在进行，请稍候")).toBe(
      "A connection operation is already in progress. Please wait.",
    );
    expect(localizeError("所有线路均不可用")).toBe("No line is available.");
  });

  it("翻译 macOS helper 与网卡配置失败", () => {
    expect(
      localizeError("特权 helper 未运行，请重新启动应用以安装或修复 helper"),
    ).toBe(
      "The privileged helper is not running. Restart the app to install or repair it.",
    );
    expect(
      localizeError(
        "配置网卡地址失败（当前：无地址）；请确认特权 helper 已安装，或以 root/sudo 运行",
      ),
    ).toBe(
      "Failed to configure the adapter address (current: no address). Confirm the privileged helper is installed, or run as root/sudo.",
    );
    expect(localizeError("未找到虚拟网卡「TopSAP」，驱动可能尚未就绪")).toBe(
      'Adapter "TopSAP" was not found; the driver may not be ready',
    );
  });

  it("翻译 Windows 网卡与 hosts 错误", () => {
    expect(localizeError("创建虚拟网卡失败：拒绝访问，需要管理员权限运行（Win32 5）")).toBe(
      "Failed to create the virtual adapter: access denied; administrator privileges are required (Win32 5)",
    );
    expect(
      localizeError("设置网卡地址失败（当前：无地址；第1轮 netsh=fail，第2轮 powershell=fail）"),
    ).toBe(
      "Failed to set the adapter address (current: no address; round 1 netsh=fail, round 2 powershell=fail)",
    );
    expect(localizeError("读取 hosts 失败：permission denied")).toBe(
      "Failed to read hosts: permission denied",
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

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
    expect(localizeSelfTestDetail("收到系统回包：IPv6 报文（64 字节）")).toBe(
      "System reply received: IPv6 packet (64 bytes)",
    );
    expect(
      localizeSelfTestDetail(
        "已向网卡注入 44 字节 ICMP 请求（10.98.0.5 → 10.99.99.2）",
      ),
    ).toBe("Injected a 44-byte ICMP request into the adapter (10.98.0.5 → 10.99.99.2)");
    expect(
      localizeSelfTestDetail(
        "收到系统发往网卡的 ICMP 请求：IPv4 10.99.99.2 -> 10.98.0.5 proto=1 len=44",
      ),
    ).toBe(
      "Received an ICMP request from the system to the adapter: IPv4 10.99.99.2 -> 10.98.0.5 proto=1 len=44",
    );
    expect(
      localizeSelfTestDetail(
        "3 秒内未收到系统发往网卡的 ICMP 请求；请检查测试路由是否生效、网卡读路径是否可用",
      ),
    ).toBe(
      "No ICMP request from the system to the adapter was received within 3 seconds; check that the test route is active and the adapter read path works",
    );
    expect(localizeSelfTestDetail("执行超时（60 秒）")).toBe(
      "Timed out after 60 seconds",
    );
    expect(localizeSelfTestDetail("ping 执行超时（60 秒）")).toBe(
      "ping timed out after 60 seconds",
    );
  });

  it("区分 TUN 权限不足与网卡占用", () => {
    expect(
      localizeSelfTestDetail(
        "创建 TUN 设备失败：当前用户没有创建 TUN 接口的权限；Linux 版需要以 root 运行，请在终端执行 sudo /usr/bin/topsap-client 启动应用",
      ),
    ).toBe(
      "Failed to create the TUN device: the current user lacks permission to create a TUN interface. The Linux build must run as root; start it from a terminal with sudo /usr/bin/topsap-client.",
    );
    expect(
      localizeSelfTestDetail(
        "创建 TUN 设备失败：网卡 topsap0 已被占用，可能是上次自检或连接未退出；请退出所有 TopSAP 实例，或执行 sudo ip link del topsap0 后重试",
      ),
    ).toBe(
      "Failed to create the TUN device: adapter topsap0 is already in use, likely left by a previous self-test or connection. Quit all TopSAP instances, or run sudo ip link del topsap0 and retry.",
    );
  });

  it("翻译 Linux 未提权提示", () => {
    expect(
      localizeSelfTestDetail(
        "当前未以 root 运行，无法创建 TUN 接口；请在终端执行 sudo /usr/bin/topsap-client 启动应用",
      ),
    ).toBe(
      "Not running as root; the TUN interface cannot be created. Start the app from a terminal with sudo /usr/bin/topsap-client.",
    );
  });

  it("翻译测试路由冲突与后续测试", () => {
    expect(localizeSelfTestDetail("未发现同前缀路由")).toBe(
      "No same-prefix route was found",
    );
    expect(
      localizeSelfTestDetail(
        "10.98.0.0/255.255.0.0 已被 utun8（198.51.100.107）占用；请先在诊断页清理 utun 残留或退出其他 VPN",
      ),
    ).toBe(
      "10.98.0.0/255.255.0.0 is already owned by utun8 (198.51.100.107); clean up utun residue on the Diagnostics page or quit the other VPN first",
    );
    expect(localizeSelfTestDetail("已跳过；处理冲突后重新运行自检")).toBe(
      "Skipped; resolve the conflict and run the self-test again",
    );
  });

  it("翻译 Linux 残留清理结果", () => {
    expect(localizeSelfTestDetail("已删除遗留的 topsap0 虚拟网卡")).toBe(
      "Removed the leftover topsap0 TUN interface",
    );
    expect(localizeSelfTestDetail("未发现遗留的 TUN 虚拟网卡")).toBe(
      "No leftover TUN interface was found",
    );
    expect(
      localizeSelfTestDetail(
        "topsap0 仍存在；请退出其他 TopSAP 实例，或执行 sudo ip link del topsap0 后重试",
      ),
    ).toBe(
      "topsap0 is still present. Quit other TopSAP instances, or run sudo ip link del topsap0 and retry.",
    );
  });

  it("未匹配的句子回退到后端错误映射", () => {
    expect(
      localizeSelfTestDetail(
        "配置网卡地址失败（当前：无地址）；请确认特权 helper 已安装，或以 root/sudo 运行",
      ),
    ).toBe(
      "Failed to configure the adapter address (current: no address). Confirm the privileged helper is installed, or run as root/sudo.",
    );
    expect(
      localizeSelfTestDetail("创建虚拟网卡失败：拒绝访问，需要管理员权限运行（Win32 5）"),
    ).toBe(
      "Failed to create the virtual adapter: access denied; administrator privileges are required (Win32 5)",
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

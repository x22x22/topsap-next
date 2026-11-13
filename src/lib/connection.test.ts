import { describe, expect, it } from "vitest";

import { isSessionEmpty } from "./connection";

function summary(resources: number, routes: number) {
  return {
    resources: Array.from({ length: resources }, () => ({
      name: "资源",
      url: "https://example.com",
      group: "",
      module: "",
    })),
    routes,
  };
}

describe("isSessionEmpty", () => {
  it("有门户资源且有路由时不为空", () => {
    expect(isSessionEmpty(summary(1, 1))).toBe(false);
  });

  it("没有门户资源时为空", () => {
    expect(isSessionEmpty(summary(0, 1))).toBe(true);
  });

  it("没有可注入路由时为空", () => {
    expect(isSessionEmpty(summary(1, 0))).toBe(true);
  });
});

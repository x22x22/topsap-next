import type { ThemeMode } from "../types/engine";

/** 应用主题到 html 元素，返回清理函数（system 模式下监听系统变化） */
export function applyTheme(mode: ThemeMode): () => void {
  const root = document.documentElement;
  const media = window.matchMedia("(prefers-color-scheme: light)");

  const apply = () => {
    const resolved: "dark" | "light" =
      mode === "system" ? (media.matches ? "light" : "dark") : mode;
    root.classList.toggle("light", resolved === "light");
    root.classList.toggle("dark", resolved === "dark");
    root.style.colorScheme = resolved;
  };

  apply();

  if (mode === "system") {
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }
  return () => {};
}

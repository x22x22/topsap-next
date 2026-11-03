import { useEffect, useRef, useState, type MouseEvent } from "react";

import { formatBytes } from "../lib/format";
import { useI18n } from "../lib/i18n";
import type { Stats } from "../types/engine";
import { Card, CardHeader } from "./ui";

interface Sample {
  up: number;
  down: number;
  time: number;
}

type SeriesKey = "up" | "down";

const POINTS = 60;
const WIDTH = 640;
const HEIGHT = 200;
const PLOT_LEFT = 64;
const BASELINE = HEIGHT - 24;

export function TrafficChartCard({ stats }: { stats: Stats }) {
  const { t } = useI18n();
  const [history, setHistory] = useState<Sample[]>([
    { up: 0, down: 0, time: Date.now() },
  ]);
  const [hoverIndex, setHoverIndex] = useState<number | null>(null);
  const lastRef = useRef<{ up: number; down: number } | null>(null);
  const statsRef = useRef(stats);
  statsRef.current = stats;

  // 固定每秒采样一次：即使这一秒没有流量，曲线也会继续前进
  useEffect(() => {
    const timer = setInterval(() => {
      const current = statsRef.current;
      const previous = lastRef.current;
      if (!previous) {
        lastRef.current = { up: current.bytes_up, down: current.bytes_down };
        return;
      }
      const up = Math.max(0, current.bytes_up - previous.up);
      const down = Math.max(0, current.bytes_down - previous.down);
      lastRef.current = { up: current.bytes_up, down: current.bytes_down };
      setHistory((items) => [
        ...items.slice(-(POINTS - 1)),
        { up, down, time: Date.now() },
      ]);
    }, 1000);
    return () => clearInterval(timer);
  }, []);

  const maxValue = Math.max(
    1024,
    ...history.map((item) => Math.max(item.up, item.down)),
  );
  const scale = (value: number) => BASELINE - (value / maxValue) * (HEIGHT - 48);
  const offset = POINTS - history.length;
  const xAt = (index: number) =>
    PLOT_LEFT + ((offset + index) / (POINTS - 1)) * (WIDTH - PLOT_LEFT - 12);
  const points = (key: SeriesKey) =>
    history.map((item, index) => `${xAt(index)},${scale(item[key])}`).join(" ");
  const area = (key: SeriesKey) => {
    const line = history
      .map((item, index) => `${xAt(index)} ${scale(item[key])}`)
      .join(" L ");
    return `M ${xAt(0)} ${BASELINE} L ${line} L ${xAt(history.length - 1)} ${BASELINE} Z`;
  };

  const onMove = (event: MouseEvent<SVGSVGElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    // SVG 默认按比例居中（letterbox），需先还原 viewBox 的实际显示区域
    const scale = Math.min(rect.width / WIDTH, rect.height / HEIGHT);
    const contentWidth = WIDTH * scale;
    const left = rect.left + (rect.width - contentWidth) / 2;
    if (event.clientX < left || event.clientX > left + contentWidth) {
      setHoverIndex(null);
      return;
    }
    const x = ((event.clientX - left) / contentWidth) * WIDTH;
    if (x < PLOT_LEFT - 4 || x > WIDTH - 12) {
      setHoverIndex(null);
      return;
    }
    let best = 0;
    let distance = Number.POSITIVE_INFINITY;
    history.forEach((_item, index) => {
      const current = Math.abs(xAt(index) - x);
      if (current < distance) {
        distance = current;
        best = index;
      }
    });
    setHoverIndex(best);
  };

  const hovered = hoverIndex === null ? null : history[hoverIndex];
  const hoverX = hoverIndex === null ? 0 : xAt(hoverIndex);

  return (
    <Card>
      <CardHeader title={t("用户流量图")} />
      <div className="p-5">
        <div className="relative">
          <svg
            viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
            className="h-48 w-full cursor-crosshair"
            role="img"
            aria-label={t("用户流量图")}
            onMouseMove={onMove}
            onMouseLeave={() => setHoverIndex(null)}
          >
            {[0, 1, 2, 3, 4].map((row) => {
              const y = 24 + (row / 4) * (HEIGHT - 48);
              return (
                <g key={row}>
                  <line
                    x1={PLOT_LEFT}
                    x2={WIDTH - 12}
                    y1={y}
                    y2={y}
                    stroke="var(--border-strong)"
                    strokeDasharray="4 6"
                    strokeWidth="1"
                  />
                  <text
                    x={PLOT_LEFT - 6}
                    y={y - 4}
                    textAnchor="end"
                    fill="var(--text-faint)"
                    fontSize="9"
                  >
                    {formatBytes(maxValue * (1 - row / 4))}/s
                  </text>
                </g>
              );
            })}

            <path d={area("down")} fill="var(--ok)" opacity="0.12" />
            <path d={area("up")} fill="var(--warn)" opacity="0.12" />

            <polyline
              points={points("down")}
              fill="none"
              stroke="var(--ok)"
              strokeWidth="2"
              strokeLinejoin="round"
              strokeLinecap="round"
            />
            <polyline
              points={points("up")}
              fill="none"
              stroke="var(--warn)"
              strokeWidth="2"
              strokeLinejoin="round"
              strokeLinecap="round"
            />

            {hovered ? (
              <>
                <line
                  x1={hoverX}
                  x2={hoverX}
                  y1={24}
                  y2={BASELINE}
                  stroke="var(--text-faint)"
                  strokeDasharray="3 4"
                  strokeWidth="1"
                />
                <circle
                  cx={hoverX}
                  cy={scale(hovered.down)}
                  r="3"
                  fill="var(--ok)"
                  stroke="var(--surface)"
                  strokeWidth="1.5"
                />
                <circle
                  cx={hoverX}
                  cy={scale(hovered.up)}
                  r="3"
                  fill="var(--warn)"
                  stroke="var(--surface)"
                  strokeWidth="1.5"
                />
              </>
            ) : null}

            {hovered ? (
              <g
                transform={`translate(${
                  hoverX > WIDTH / 2 ? hoverX - 182 : hoverX + 10
                }, 26)`}
              >
                <rect width="172" height="62" rx="6" fill="#0f172a" opacity="0.92" />
                <text x="10" y="17" fill="#cbd5e1" fontSize="9">
                  {formatTime(hovered.time)}
                </text>
                <circle cx="12" cy="31" r="3" fill="var(--warn)" />
                <text x="20" y="34" fill="#ffffff" fontSize="9">
                  {t("发送流量")}: {formatBytes(hovered.up)}/s
                </text>
                <circle cx="12" cy="48" r="3" fill="var(--ok)" />
                <text x="20" y="51" fill="#ffffff" fontSize="9">
                  {t("接收流量")}: {formatBytes(hovered.down)}/s
                </text>
              </g>
            ) : null}

            <text x={PLOT_LEFT} y={HEIGHT - 2} fill="var(--text-faint)" fontSize="9">
              60s
            </text>
            <text x={WIDTH - 16} y={HEIGHT - 2} fill="var(--text-faint)" fontSize="9">
              0
            </text>
          </svg>

        </div>

        <div className="mt-1 flex items-center justify-center gap-5 text-[11px] text-muted">
          <Legend color="var(--warn)" label={t("发送流量")} />
          <Legend color="var(--ok)" label={t("接收流量")} />
        </div>
      </div>
    </Card>
  );
}

function formatTime(timestamp: number): string {
  const date = new Date(timestamp);
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}/${date.getMonth() + 1}/${date.getDate()} ${pad(
    date.getHours(),
  )}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

function Legend({ color, label }: { color: string; label: string }) {
  return (
    <span className="inline-flex items-center gap-1.5">
      <span className="h-2 w-2 rounded-full" style={{ backgroundColor: color }} />
      {label}
    </span>
  );
}

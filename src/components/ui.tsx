import type {
  ButtonHTMLAttributes,
  InputHTMLAttributes,
  ReactNode,
  TextareaHTMLAttributes,
} from "react";
import { useEffect, useRef, useState } from "react";
import { AlertCircle, ChevronDown, Eye, EyeOff, Loader2, X } from "lucide-react";

import type { EnginePhase } from "../types/engine";
import { useI18n } from "../lib/i18n";

export function cn(...values: Array<string | false | null | undefined>): string {
  return values.filter(Boolean).join(" ");
}

/* ---------------- Button ---------------- */

type ButtonVariant = "primary" | "ghost" | "danger" | "subtle";
type ButtonSize = "sm" | "md" | "lg";

const BUTTON_VARIANT: Record<ButtonVariant, string> = {
  primary:
    "bg-gradient-to-r from-primary-from to-primary-to text-primary-text shadow-lg shadow-accent/20 hover:brightness-105 active:brightness-95",
  ghost: "border border-border-strong bg-surface-2/70 text-text hover:bg-hover",
  danger: "border border-danger/40 bg-danger-soft text-danger hover:bg-danger/15",
  subtle: "text-muted hover:bg-hover hover:text-text",
};

const BUTTON_SIZE: Record<ButtonSize, string> = {
  sm: "h-8 gap-1.5 rounded-lg px-3 text-xs",
  md: "h-10 gap-2 rounded-xl px-4 text-sm",
  lg: "h-12 gap-2 rounded-xl px-6 text-sm",
};

export function Button({
  variant = "ghost",
  size = "md",
  className,
  children,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: ButtonVariant;
  size?: ButtonSize;
}) {
  return (
    <button
      className={cn(
        "inline-flex items-center justify-center font-medium transition-all",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60",
        "disabled:cursor-not-allowed disabled:opacity-50",
        BUTTON_VARIANT[variant],
        BUTTON_SIZE[size],
        className,
      )}
      {...props}
    >
      {children}
    </button>
  );
}

export function IconButton({
  label,
  className,
  children,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { label: string }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      className={cn(
        "inline-flex h-8 w-8 items-center justify-center rounded-lg text-muted transition",
        "hover:bg-hover hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60",
        className,
      )}
      {...props}
    >
      {children}
    </button>
  );
}

/* ---------------- Input ---------------- */

const FIELD_BASE =
  "w-full rounded-xl border border-border-strong bg-surface-2/60 text-text placeholder:text-faint " +
  "outline-none transition focus:border-accent/60 focus:bg-surface-2 focus:ring-2 focus:ring-accent/20 " +
  "disabled:cursor-not-allowed disabled:opacity-50";

export function Input({ className, ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return <input className={cn("h-10 px-3 text-sm", FIELD_BASE, className)} {...props} />;
}

/** 密码输入框：右侧固定眼睛按钮切换明文，同时隐藏 WebView 原生眼睛 */
export function PasswordInput({
  className,
  ...props
}: Omit<InputHTMLAttributes<HTMLInputElement>, "type">) {
  const { t } = useI18n();
  const [visible, setVisible] = useState(false);
  const label = visible ? t("隐藏密码") : t("显示密码");

  return (
    <div className="relative">
      <Input
        {...props}
        type={visible ? "text" : "password"}
        className={cn("pr-10", className)}
      />
      <button
        type="button"
        tabIndex={-1}
        aria-label={label}
        title={label}
        disabled={props.disabled}
        onClick={() => setVisible((prev) => !prev)}
        className="absolute right-2 top-1/2 flex h-7 w-7 -translate-y-1/2 items-center justify-center rounded-md text-faint transition hover:bg-hover hover:text-text disabled:cursor-not-allowed disabled:opacity-50"
      >
        {visible ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
      </button>
    </div>
  );
}

export function Textarea({ className, ...props }: TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return <textarea className={cn("px-3 py-2 text-sm", FIELD_BASE, className)} {...props} />;
}

export interface SelectOption<T extends string = string> {
  value: T;
  label: string;
  hint?: string;
}

/** 自绘下拉框：可控制展开箭头旋转与菜单样式，避免系统原生菜单割裂主题 */
export function Select<T extends string>({
  value,
  options,
  onChange,
  disabled,
  ariaLabel,
  placeholder = "请选择",
  className,
}: {
  value: T;
  options: Array<SelectOption<T>>;
  onChange: (value: T) => void;
  disabled?: boolean;
  ariaLabel?: string;
  placeholder?: string;
  className?: string;
}) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const selected = options.find((option) => option.value === value);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div ref={rootRef} className={cn("relative", className)}>
      <button
        type="button"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={ariaLabel}
        disabled={disabled}
        onClick={() => setOpen((prev) => !prev)}
        className={cn(
          "flex h-10 w-full items-center justify-between gap-2 rounded-xl border bg-surface-2/60 px-3 text-left text-sm text-text",
          "outline-none transition focus-visible:border-accent/60 focus-visible:ring-2 focus-visible:ring-accent/20",
          "disabled:cursor-not-allowed disabled:opacity-50",
          open ? "border-accent/50 bg-surface-2" : "border-border-strong hover:bg-hover",
        )}
      >
        <span className={cn("truncate", selected ? "text-text" : "text-faint")}>
          {selected?.label ?? placeholder}
        </span>
        <ChevronDown
          className={cn(
            "h-4 w-4 shrink-0 text-muted transition-transform duration-200",
            open && "rotate-180",
          )}
        />
      </button>

      {open ? (
        <div
          role="listbox"
          aria-label={ariaLabel}
          className="absolute left-0 right-0 top-[calc(100%+6px)] z-50 max-h-64 overflow-y-auto rounded-xl border border-border-strong bg-surface p-1 shadow-2xl"
        >
          {options.map((option) => {
            const isSelected = option.value === value;
            return (
              <button
                key={option.value}
                type="button"
                role="option"
                aria-selected={isSelected}
                onClick={() => {
                  onChange(option.value);
                  setOpen(false);
                }}
                className={cn(
                  "flex w-full items-center justify-between gap-3 rounded-lg px-3 py-2 text-left text-sm transition",
                  isSelected
                    ? "bg-accent-soft font-medium text-accent"
                    : "text-text hover:bg-hover",
                )}
              >
                <span className="min-w-0">
                  <span className="block truncate">{option.label}</span>
                  {option.hint ? (
                    <span className="mt-0.5 block truncate text-[11px] text-faint">
                      {option.hint}
                    </span>
                  ) : null}
                </span>
                {isSelected ? <span className="text-accent">✓</span> : null}
              </button>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}

export function Field({
  label,
  hint,
  error,
  children,
  className,
}: {
  label: string;
  hint?: string;
  error?: string | null;
  children: ReactNode;
  className?: string;
}) {
  return (
    <label className={cn("flex flex-col gap-1.5", className)}>
      <span className="text-xs font-medium text-muted">{label}</span>
      {children}
      {error ? (
        <span className="text-xs text-danger">{error}</span>
      ) : hint ? (
        <span className="text-xs text-faint">{hint}</span>
      ) : null}
    </label>
  );
}

/* ---------------- Card ---------------- */

export function Card({
  className,
  children,
}: {
  className?: string;
  children: ReactNode;
}) {
  return (
    <section
      className={cn(
        "rounded-2xl border border-border bg-surface/80 shadow-[var(--shadow)] backdrop-blur-sm",
        className,
      )}
    >
      {children}
    </section>
  );
}

export function CardHeader({
  title,
  description,
  actions,
  className,
}: {
  title: string;
  description?: string;
  actions?: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex items-start justify-between gap-3 border-b border-border px-5 py-4", className)}>
      <div className="min-w-0">
        <h2 className="text-sm font-semibold text-text">{title}</h2>
        {description ? <p className="mt-0.5 text-xs text-muted">{description}</p> : null}
      </div>
      {actions ? <div className="flex shrink-0 items-center gap-2">{actions}</div> : null}
    </div>
  );
}

/* ---------------- Badge / Status ---------------- */

type BadgeTone = "neutral" | "accent" | "ok" | "warn" | "danger";

const BADGE_TONE: Record<BadgeTone, string> = {
  neutral: "border-border-strong bg-surface-3 text-muted",
  accent: "border-accent/30 bg-accent-soft text-accent",
  ok: "border-ok/30 bg-ok-soft text-ok",
  warn: "border-warn/30 bg-warn-soft text-warn",
  danger: "border-danger/30 bg-danger-soft text-danger",
};

export function Badge({
  tone = "neutral",
  className,
  children,
}: {
  tone?: BadgeTone;
  className?: string;
  children: ReactNode;
}) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[11px] font-medium",
        BADGE_TONE[tone],
        className,
      )}
    >
      {children}
    </span>
  );
}

export function StatusDot({ phase, className }: { phase: EnginePhase; className?: string }) {
  const tone =
    phase === "connected"
      ? "bg-ok"
      : phase === "failed"
        ? "bg-danger"
        : phase === "idle"
          ? "bg-faint"
          : "bg-warn";
  const animated = phase !== "idle" && phase !== "connected" && phase !== "failed";

  return (
    <span className={cn("inline-flex items-center justify-center", className)}>
      <span className={cn("h-2 w-2 rounded-full", tone, animated && "pulse-ring")} />
    </span>
  );
}

export function Spinner({ className }: { className?: string }) {
  return <Loader2 className={cn("h-4 w-4 animate-spin", className)} />;
}

/* ---------------- Empty / Error ---------------- */

export function EmptyState({
  icon,
  title,
  description,
}: {
  icon?: ReactNode;
  title: string;
  description?: string;
}) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 px-6 py-10 text-center">
      {icon ? <div className="text-faint">{icon}</div> : null}
      <div className="text-sm font-medium text-muted">{title}</div>
      {description ? <div className="max-w-sm text-xs text-faint">{description}</div> : null}
    </div>
  );
}

export function ErrorNote({ message, onRetry }: { message: string; onRetry?: () => void }) {
  return (
    <div className="flex items-start gap-2 rounded-xl border border-danger/30 bg-danger-soft px-3 py-2.5">
      <AlertCircle className="mt-0.5 h-4 w-4 shrink-0 text-danger" />
      <div className="min-w-0 flex-1 text-xs text-danger">
        <p className="break-words">{message}</p>
        {onRetry ? (
          <button
            type="button"
            onClick={onRetry}
            className="mt-1 font-medium underline underline-offset-2 hover:opacity-80"
          >
            重试
          </button>
        ) : null}
      </div>
    </div>
  );
}

/* ---------------- Switch ---------------- */

export function Switch({
  checked,
  onChange,
  disabled,
  label,
}: {
  checked: boolean;
  onChange: (next: boolean) => void;
  disabled?: boolean;
  label: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cn(
        "relative inline-flex h-6 w-11 shrink-0 items-center rounded-full border transition",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60",
        "disabled:cursor-not-allowed disabled:opacity-50",
        checked
          ? "border-accent/50 bg-accent/80"
          : "border-border-strong bg-surface-3",
      )}
    >
      <span
        className={cn(
          "inline-block h-4 w-4 rounded-full bg-white shadow transition-transform",
          checked ? "translate-x-6" : "translate-x-1",
        )}
      />
    </button>
  );
}

/* ---------------- SegmentedControl ---------------- */

export function SegmentedControl<T extends string>({
  value,
  options,
  onChange,
  className,
}: {
  value: T;
  options: Array<{ value: T; label: string }>;
  onChange: (next: T) => void;
  className?: string;
}) {
  return (
    <div
      role="tablist"
      className={cn(
        "inline-flex items-center gap-1 rounded-xl border border-border-strong bg-surface-2/60 p-1",
        className,
      )}
    >
      {options.map((option) => {
        const active = option.value === value;
        return (
          <button
            key={option.value}
            type="button"
            role="tab"
            aria-selected={active}
            onClick={() => onChange(option.value)}
            className={cn(
              "rounded-lg px-3 py-1 text-xs font-medium transition",
              active
                ? "bg-hover text-text shadow-sm ring-1 ring-border-strong"
                : "text-muted hover:bg-hover/60 hover:text-text",
            )}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}

/* ---------------- Modal ---------------- */

export function Modal({
  open,
  title,
  description,
  onClose,
  children,
  footer,
}: {
  open: boolean;
  title: string;
  description?: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
}) {
  const { t } = useI18n();
  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-6">
      <div
        className="absolute inset-0 bg-black/60 backdrop-blur-sm"
        onClick={onClose}
        aria-hidden="true"
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className="fade-up relative w-full max-w-md rounded-2xl border border-border bg-surface p-6 shadow-2xl"
      >
        <div className="mb-4 flex items-start justify-between gap-4">
          <div>
            <h3 className="text-base font-semibold text-text">{title}</h3>
            {description ? <p className="mt-1 text-xs text-muted">{description}</p> : null}
          </div>
          <IconButton label={t("关闭")} onClick={onClose}>
            <X className="h-4 w-4" />
          </IconButton>
        </div>
        {children}
        {footer ? <div className="mt-5 flex justify-end gap-2">{footer}</div> : null}
      </div>
    </div>
  );
}

/* ---------------- Logo ---------------- */

export function Logo({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 64 64" className={cn("h-9 w-9", className)} aria-hidden="true">
      <defs>
        <linearGradient id="topsap-logo" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#22d3ee" />
          <stop offset="1" stopColor="#0ea5e9" />
        </linearGradient>
      </defs>
      <path
        d="M32 5 L55 13 V34 C55 47 44 56 32 60 C20 56 9 47 9 34 V13 Z"
        fill="url(#topsap-logo)"
      />
      <circle cx="32" cy="32" r="9.5" fill="none" stroke="#0b1220" strokeWidth="3.4" />
      <circle cx="32" cy="22.5" r="3.2" fill="#0b1220" />
      <circle cx="32" cy="41.5" r="3.2" fill="#0b1220" />
      <circle cx="22.5" cy="32" r="3.2" fill="#0b1220" />
      <circle cx="41.5" cy="32" r="3.2" fill="#0b1220" />
    </svg>
  );
}

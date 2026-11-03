import { useMemo, useState } from "react";
import { ChevronDown, ChevronUp, Globe2, Search } from "lucide-react";
import { toast } from "sonner";

import { api } from "../lib/api";
import { firstLine } from "../lib/format";
import { useI18n } from "../lib/i18n";
import type { ResourceEntry } from "../types/engine";
import { Badge, Card, CardHeader, EmptyState, Input, cn } from "./ui";

export function ResourceListCard({ resources }: { resources: ResourceEntry[] }) {
  const { t } = useI18n();
  const [query, setQuery] = useState("");
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());

  const groups = useMemo(() => {
    const keyword = query.trim().toLowerCase();
    const filtered = resources.filter((item) => {
      if (!keyword) return true;
      return `${item.name} ${item.url} ${item.group}`.toLowerCase().includes(keyword);
    });
    const map = new Map<string, ResourceEntry[]>();
    for (const item of filtered) {
      const group = item.group || "default";
      map.set(group, [...(map.get(group) ?? []), item]);
    }
    return Array.from(map.entries())
      .map(([group, items]) => ({
        group,
        label:
          group === "default" || group === "defaultGroup" ? t("默认组") : group,
        items: items.slice().sort((a, b) => a.name.localeCompare(b.name)),
      }))
      .sort((a, b) => {
        const aDefault = a.group === "default" || a.group === "defaultGroup";
        const bDefault = b.group === "default" || b.group === "defaultGroup";
        if (aDefault !== bDefault) return aDefault ? -1 : 1;
        return a.group.localeCompare(b.group, "zh-CN");
      });
  }, [resources, query, t]);

  const toggle = (group: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(group)) next.delete(group);
      else next.add(group);
      return next;
    });
  };

  const canOpen = (item: ResourceEntry) =>
    /^(https?|smb):\/\//i.test(item.url.trim());

  const openResource = async (item: ResourceEntry) => {
    if (!canOpen(item)) return;
    try {
      await api.openUrl(item.url.trim());
    } catch (error) {
      toast.error(firstLine(String(error)));
    }
  };

  return (
    <Card>
      <CardHeader
        title={t("资源列表")}
        actions={
          <div className="relative w-52">
            <Search className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-faint" />
            <Input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={t("搜索资源")}
              className="h-8 pl-8 text-xs"
            />
          </div>
        }
      />
      <div className="max-h-[420px] space-y-4 overflow-y-auto p-5">
        {groups.length === 0 ? (
          <EmptyState
            icon={<Globe2 className="h-6 w-6" />}
            title={t("暂无资源")}
            description={t("网关未下发资源，或搜索无匹配结果")}
          />
        ) : (
          groups.map((group) => {
            const isCollapsed = collapsed.has(group.group);
            return (
              <section key={group.group}>
                <button
                  type="button"
                  onClick={() => toggle(group.group)}
                  className="flex w-full items-center justify-between gap-2 rounded-lg py-1 text-left text-sm font-medium text-text transition hover:text-accent"
                >
                  <span className="flex items-center gap-2">
                    {group.label}
                    <Badge tone="neutral">{group.items.length}</Badge>
                  </span>
                  {isCollapsed ? (
                    <ChevronDown className="h-3.5 w-3.5 text-faint" />
                  ) : (
                    <ChevronUp className="h-3.5 w-3.5 text-faint" />
                  )}
                </button>
                {isCollapsed ? null : (
                  <div className="mt-2 grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
                    {group.items.map((item) => (
                      <div
                        key={`${group.group}-${item.name}-${item.url}`}
                        role={canOpen(item) ? "button" : undefined}
                        tabIndex={canOpen(item) ? 0 : undefined}
                        title={canOpen(item) ? t("打开资源") : undefined}
                        onClick={
                          canOpen(item) ? () => void openResource(item) : undefined
                        }
                        onKeyDown={(event) => {
                          if (!canOpen(item)) return;
                          if (event.key === "Enter" || event.key === " ") {
                            event.preventDefault();
                            void openResource(item);
                          }
                        }}
                        className={cn(
                          "flex items-center gap-2.5 rounded-xl border border-border bg-surface-2/40 px-3 py-2",
                          "transition hover:border-accent/40 hover:bg-hover",
                          canOpen(item) ? "cursor-pointer" : "cursor-default",
                        )}
                      >
                        <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-accent-soft text-accent">
                          <Globe2 className="h-4 w-4" />
                        </span>
                        <span className="min-w-0">
                          <span className="block truncate text-xs font-medium text-text">
                            {item.name}
                          </span>
                          {item.url ? (
                            <span className="block truncate font-mono text-[10px] text-faint">
                              {item.url}
                            </span>
                          ) : null}
                        </span>
                      </div>
                    ))}
                  </div>
                )}
              </section>
            );
          })
        )}
      </div>
    </Card>
  );
}

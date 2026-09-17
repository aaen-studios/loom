import { useEffect, useRef } from "react";
import { cn } from "../lib/cn";
import { LiquidSurface } from "./LiquidSurface";
import { formatReset, relativeTime } from "../lib/format";
import { useMenu } from "../lib/menu";
import {
  metricBadge,
  metricSummary,
  metricTone,
  percentOf,
  primaryMetric,
} from "../lib/usage";
import { useChat } from "../stores/chat";
import { currentModel, useProviders } from "../stores/providers";
import { useSettings } from "../stores/settings";
import { useUsage } from "../stores/usage";

const TONE_CLASS = {
  ok: "border-[var(--glass-border)] text-soft",
  warn: "border-[var(--accent)] text-[var(--accent)]",
  hot: "border-[var(--danger)] text-[var(--danger)]",
} as const;

/** Bar fill colours, one step louder than the quiet badge. */
const TONE_BAR = {
  ok: "text-soft",
  warn: "text-[var(--accent)]",
  hot: "text-[var(--danger)]",
} as const;

/**
 * Badge for the current model's provider: the tightest subscription window
 * (or remaining credit) when the vendor exposes a usage endpoint. Clicking
 * opens a small popover with the already-loaded numbers — no fetch, no trip
 * to Settings. Lives in the chat header, so the popover drops down.
 */
export function UsageBadge({ align = "up" }: { align?: "up" | "down" }) {
  const models = useProviders((state) => state.models);
  const session = useChat((state) =>
    state.sessions.find((item) => item.id === state.activeId),
  );
  const chatDefaults = useSettings((state) => state.config.chat);
  const capable = useUsage((state) => state.capable);
  const byProvider = useUsage((state) => state.byProvider);
  const refreshProvider = useUsage((state) => state.refreshProvider);

  const containerRef = useRef<HTMLDivElement>(null);
  const { open, setOpen } = useMenu("usage", containerRef);

  const model = currentModel(models, session, chatDefaults);
  const providerId = model?.providerId ?? null;
  const isCapable = providerId
    ? capable.some((entry) => entry.providerId === providerId)
    : false;
  const usage = providerId ? byProvider[providerId] : undefined;

  // A key can be added (or the capable list can arrive late), so ask once when
  // the active model points at a provider we have no reading for.
  useEffect(() => {
    if (!providerId || !isCapable || usage) return;
    void refreshProvider(providerId);
  }, [providerId, isCapable, usage, refreshProvider]);

  const metric = primaryMetric(usage);
  const label = metricBadge(metric);
  if (!providerId || !isCapable || !label || !metric || !usage) return null;

  const reset = formatReset(metric.resetsAt, metric.resetsAtMs);

  return (
    <div className="relative" ref={containerRef}>
      <button
        type="button"
        aria-expanded={open}
        title={`${usage.source} · ${metric.label}: ${metricSummary(metric)}`}
        onClick={() => setOpen(!open)}
        className={cn(
          "flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[12.5px]",
          TONE_CLASS[metricTone(metric)],
        )}
      >
        <span className="h-1.5 w-1.5 rounded-full bg-current" aria-hidden />
        {label}
      </button>

      {open && (
        <LiquidSurface
          surface="popovers"
          layout="block"
          className={cn(
            "absolute z-50 w-64 max-w-[calc(100vw-2rem)] rounded-sheet border border-[var(--glass-border)]",
            align === "down" ? "right-0 top-full mt-2" : "bottom-full left-0 mb-2",
          )}
          contentClassName="p-2.5"
          tint="var(--panel-bg-strong)"
        >
          <p className="flex items-center gap-1.5 text-[10.5px] font-semibold tracking-[0.06em] text-faint uppercase">
            <span
              className={cn("h-1.5 w-1.5 rounded-full bg-current", TONE_CLASS[metricTone(metric)])}
              aria-hidden
            />
            {usage.source}
          </p>
          <div className="mt-2 space-y-2">
            {usage.metrics.map((entry) => {
              const percent = percentOf(entry);
              // Keep a sliver of colour visible for tiny non-zero windows.
              const width =
                percent === null || percent <= 0
                  ? 0
                  : Math.min(100, Math.max(percent, 3));
              return (
                <div key={entry.id}>
                  <div className="flex items-baseline justify-between gap-3">
                    <span
                      className="min-w-0 truncate text-[12px] text-soft"
                      title={entry.detail ?? undefined}
                    >
                      {entry.label}
                    </span>
                    <span className="shrink-0 text-[11.5px] text-faint tabular-nums">
                      {metricSummary(entry)}
                    </span>
                  </div>
                  {percent !== null && (
                    <div className="mt-1 h-1 overflow-hidden rounded-full bg-[var(--ink-ghost)]">
                      <div
                        className={cn(
                          "h-full rounded-full bg-current transition-[width] duration-300",
                          TONE_BAR[metricTone(entry)],
                        )}
                        style={{ width: `${width}%` }}
                      />
                    </div>
                  )}
                </div>
              );
            })}
          </div>
          <div className="mt-2 flex items-center justify-between gap-3 border-t border-[var(--glass-border)] pt-1.5 text-[10.5px] text-faint">
            {reset ? <span>{reset}</span> : <span />}
            <span>updated {relativeTime(usage.fetchedAt)}</span>
          </div>
        </LiquidSurface>
      )}
    </div>
  );
}

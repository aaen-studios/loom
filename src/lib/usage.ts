import type { ProviderUsage, UsageMetric } from "../types";
import { formatMoney } from "./format";

/**
 * The metric worth showing at a glance: the tightest percentage window, or the
 * first meaningful amount for credit-style cards.
 */
export function primaryMetric(usage: ProviderUsage | undefined): UsageMetric | null {
  if (!usage || usage.metrics.length === 0) return null;
  const windows = usage.metrics.filter(
    (metric): metric is UsageMetric & { percent: number } => metric.percent !== null,
  );
  if (windows.length > 0) {
    return windows.reduce((worst, metric) =>
      metric.percent > worst.percent ? metric : worst,
    );
  }
  return (
    usage.metrics.find(
      (metric) => metric.remaining !== null || metric.used !== null,
    ) ?? usage.metrics[0]
  );
}

/** Compact label for the composer badge, e.g. "62%" or "$74.50". */
export function metricBadge(metric: UsageMetric | null): string | null {
  if (!metric) return null;
  if (metric.percent !== null) return `${Math.round(metric.percent)}%`;
  if (metric.remaining !== null) return formatMoney(metric.remaining, metric.unit);
  if (metric.used !== null) return formatMoney(metric.used, metric.unit);
  return null;
}

/** How close the primary metric is to its cap, for colour coding. */
export function metricTone(metric: UsageMetric | null): "ok" | "warn" | "hot" {
  const percent = percentOf(metric);
  if (percent === null) return "ok";
  if (percent >= 90) return "hot";
  if (percent >= 75) return "warn";
  return "ok";
}

/** Percentage consumed, derived from used/limit when no percent is reported. */
export function percentOf(metric: UsageMetric | null): number | null {
  if (!metric) return null;
  if (metric.percent !== null) return metric.percent;
  if (metric.used !== null && metric.limit !== null && metric.limit > 0) {
    return Math.min(100, (metric.used / metric.limit) * 100);
  }
  return null;
}

/** Human line for one metric, used in card rows and the badge tooltip. */
export function metricSummary(metric: UsageMetric): string {
  const percent = percentOf(metric);
  if (percent !== null) {
    return `${Math.round(percent)}% used`;
  }
  if (metric.remaining !== null) {
    const amount = formatMoney(metric.remaining, metric.unit);
    return amount ? `${amount} left` : `${metric.remaining} left`;
  }
  if (metric.used !== null) {
    const amount = formatMoney(metric.used, metric.unit);
    return amount ? `${amount} used` : `${metric.used} used`;
  }
  return "";
}

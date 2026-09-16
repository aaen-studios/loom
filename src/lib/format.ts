import type { MetadataSource } from "../types";

/** "now", "4m", "3h", "Aug 12" — small and dependency-free. */
export function relativeTime(ms: number): string {
  const delta = Date.now() - ms;
  if (delta < 60_000) return "now";
  if (delta < 3_600_000) return `${Math.floor(delta / 60_000)}m`;
  if (delta < 86_400_000) return `${Math.floor(delta / 3_600_000)}h`;
  if (delta < 7 * 86_400_000) return `${Math.floor(delta / 86_400_000)}d`;

  return new Date(ms).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
  });
}

/** 128000 → "128k", 1048576 → "1M", 2500000 → "2.5M". */
export function compactTokens(tokens: number | null | undefined): string | null {
  if (!tokens) return null;
  if (tokens >= 1_000_000) {
    const millions = Math.round((tokens / 1_000_000) * 10) / 10;
    return `${Number.isInteger(millions) ? millions : millions.toFixed(1)}M`;
  }
  if (tokens >= 1_000) return `${Math.round(tokens / 1_000)}k`;
  return String(tokens);
}

export function shortModelName(providerName: string, modelId: string): string {
  const tail = modelId.split("/").pop() ?? modelId;
  return tail || providerName;
}

/** `formatMoney(25.5, "usd")` → "$25.50"; unknown units get a suffix. */
export function formatMoney(
  value: number | null | undefined,
  unit: string,
): string | null {
  if (value === null || value === undefined || !Number.isFinite(value)) {
    return null;
  }
  const symbol = unit === "usd" ? "$" : unit === "cny" ? "¥" : "";
  return symbol
    ? `${symbol}${value.toFixed(2)}`
    : `${value.toFixed(2)} ${unit.toUpperCase()}`;
}

/**
 * "in 3h 12m" until a vendor reset stamp. Accepts either an ISO string or
 * epoch milliseconds; `null` when the vendor gave neither.
 */
export function formatReset(
  resetsAt: string | null | undefined,
  resetsAtMs: number | null | undefined,
  now: number = Date.now(),
): string | null {
  const target =
    resetsAtMs ?? (resetsAt ? Date.parse(resetsAt) : Number.NaN);
  if (!Number.isFinite(target)) return null;
  const delta = target - now;
  if (delta <= 0) return "now";
  const minutes = Math.floor(delta / 60_000);
  if (minutes < 60) return `in ${Math.max(1, minutes)}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `in ${hours}h ${minutes % 60}m`;
  return `in ${Math.floor(hours / 24)}d ${hours % 24}h`;
}

/**
 * Where a model's metadata came from, in words, for tooltips. "catalog@N"
 * covers every bundled-catalogue guess, whichever version produced it.
 */
export function metadataSourceLabel(source: MetadataSource): string {
  if (source === "user") return "your edit";
  if (source === "api") return "from gateway";
  if (source === "unknown") return "unknown";
  return "guessed from name";
}

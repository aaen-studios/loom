import { describe, expect, it } from "vitest";
import type { ProviderUsage, UsageMetric } from "../types";
import {
  metricBadge,
  metricSummary,
  metricTone,
  percentOf,
  primaryMetric,
} from "./usage";

function metric(partial: Partial<UsageMetric>): UsageMetric {
  return {
    id: "m",
    label: "Window",
    percent: null,
    used: null,
    limit: null,
    remaining: null,
    unit: "percent",
    resetsAt: null,
    resetsAtMs: null,
    status: null,
    detail: null,
    ...partial,
  };
}

function usage(metrics: UsageMetric[]): ProviderUsage {
  return { providerId: "p", source: "Test", fetchedAt: 0, metrics };
}

describe("usage helpers", () => {
  it("picks the tightest window as the primary metric", () => {
    const picked = primaryMetric(
      usage([
        metric({ id: "rolling", percent: 12 }),
        metric({ id: "weekly", percent: 81 }),
        metric({ id: "monthly", percent: 40 }),
      ]),
    );
    expect(picked?.id).toBe("weekly");
  });

  it("falls back to amounts when there are no windows", () => {
    const picked = primaryMetric(
      usage([metric({ id: "balance", unit: "usd", remaining: 1.5 })]),
    );
    expect(picked?.id).toBe("balance");
    expect(primaryMetric(undefined)).toBeNull();
    expect(primaryMetric(usage([]))).toBeNull();
  });

  it("labels badges and derives percentages from used/limit", () => {
    expect(metricBadge(metric({ percent: 61.4 }))).toBe("61%");
    expect(metricBadge(metric({ unit: "usd", remaining: 74.5 }))).toBe("$74.50");
    expect(metricBadge(null)).toBeNull();

    const derived = metric({ used: 25, limit: 100 });
    expect(percentOf(derived)).toBe(25);
    expect(metricSummary(derived)).toBe("25% used");
    expect(metricSummary(metric({ unit: "usd", remaining: 3 }))).toBe("$3.00 left");
  });

  it("grades tones by how close the cap is", () => {
    expect(metricTone(metric({ percent: 20 }))).toBe("ok");
    expect(metricTone(metric({ percent: 80 }))).toBe("warn");
    expect(metricTone(metric({ percent: 95 }))).toBe("hot");
    expect(metricTone(metric({ unit: "usd", remaining: 3 }))).toBe("ok");
  });
});

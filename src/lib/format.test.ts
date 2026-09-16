import { describe, expect, it } from "vitest";
import {
  compactTokens,
  formatMoney,
  formatReset,
  relativeTime,
  shortModelName,
} from "./format";
import { parseAttachments, parseToolCalls } from "./messageExtra";

describe("format", () => {
  it("formats recent times compactly", () => {
    const now = Date.now();
    expect(relativeTime(now)).toBe("now");
    expect(relativeTime(now - 5 * 60_000)).toBe("5m");
    expect(relativeTime(now - 3 * 3_600_000)).toBe("3h");
    expect(relativeTime(now - 2 * 86_400_000)).toBe("2d");
  });

  it("falls back to a date for old timestamps", () => {
    const old = Date.now() - 30 * 86_400_000;
    expect(relativeTime(old)).not.toMatch(/^\d+[mhd]$/);
  });

  it("compacts token counts", () => {
    expect(compactTokens(1_000_000)).toBe("1M");
    expect(compactTokens(1_048_576)).toBe("1M");
    expect(compactTokens(128_000)).toBe("128k");
    expect(compactTokens(512)).toBe("512");
    expect(compactTokens(null)).toBeNull();
  });

  it("shortens provider-qualified model ids", () => {
    expect(shortModelName("OpenRouter", "anthropic/claude-sonnet-4")).toBe(
      "claude-sonnet-4",
    );
    expect(shortModelName("Local", "qwen3:8b")).toBe("qwen3:8b");
  });

  it("formats money with a currency symbol", () => {
    expect(formatMoney(25.5, "usd")).toBe("$25.50");
    expect(formatMoney(100, "cny")).toBe("¥100.00");
    expect(formatMoney(3, "eur")).toBe("3.00 EUR");
    expect(formatMoney(null, "usd")).toBeNull();
    expect(formatMoney(Number.NaN, "usd")).toBeNull();
  });

  it("counts down to a vendor reset stamp", () => {
    const now = Date.now();
    expect(formatReset(new Date(now + 45 * 60_000).toISOString(), null, now)).toBe(
      "in 45m",
    );
    expect(
      formatReset(null, now + (3 * 60 + 12) * 60_000, now),
    ).toBe("in 3h 12m");
    expect(
      formatReset(null, now + (2 * 24 + 5) * 3_600_000, now),
    ).toBe("in 2d 5h");
    expect(formatReset(null, now - 1_000, now)).toBe("now");
    expect(formatReset(null, null, now)).toBeNull();
    expect(formatReset("not a date", null, now)).toBeNull();
  });
});

describe("message extra parsing", () => {
  it("reads attachment arrays on user messages", () => {
    const extra = JSON.stringify([
      {
        id: "a1",
        kind: "image",
        name: "shot.png",
        mime: "image/png",
        size: 10,
        path: "C:/loom/a.png",
      },
    ]);
    const attachments = parseAttachments(extra);
    expect(attachments).toHaveLength(1);
    expect(attachments[0].name).toBe("shot.png");
  });

  it("reads tool calls on assistant messages", () => {
    const extra = JSON.stringify({
      toolCalls: [
        {
          id: "c1",
          name: "read_file",
          arguments: '{"path":"a.txt"}',
          status: "ok",
          output: "hi",
        },
      ],
    });
    expect(parseToolCalls(extra)).toHaveLength(1);
    // Attachment parsing must not mistake tool calls for attachments.
    expect(parseAttachments(extra)).toEqual([]);
    expect(parseToolCalls(extra.replace("toolCalls", "other"))).toEqual([]);
  });

  it("survives malformed and empty input", () => {
    expect(parseAttachments(null)).toEqual([]);
    expect(parseToolCalls("not json")).toEqual([]);
    expect(parseAttachments('{"toolCalls":[]}')).toEqual([]);
  });
});

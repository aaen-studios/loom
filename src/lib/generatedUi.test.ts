// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import {
  MAX_ACTION_VALUE_CHARS,
  MAX_GENERATED_UI_CHARS,
  readGeneratedAction,
  safeExternalUrl,
  sanitizeGeneratedUi,
} from "./generatedUi";

function element(html: string): Element {
  const host = document.createElement("div");
  host.innerHTML = html;
  return host.firstElementChild as Element;
}

describe("sanitizeGeneratedUi", () => {
  it("keeps themed markup, classes, inline styles and actions", () => {
    const clean = sanitizeGeneratedUi(
      '<div class="card" style="padding:8px"><button data-loom-action="send" data-loom-value="Go">Go</button></div>',
    );
    expect(clean).toContain('class="card"');
    expect(clean).toContain("padding:8px");
    expect(clean).toContain('data-loom-action="send"');
    expect(clean).toContain('data-loom-value="Go"');
  });

  it("drops scripts, handlers and hosting elements", () => {
    const clean = sanitizeGeneratedUi(
      '<div onclick="steal()"><script>steal()</script><iframe src="https://x.test"></iframe><object></object><form action="https://x.test"></form><link rel="stylesheet" href="https://x.test/a.css"><img src="x.png" onerror="steal()"></div>',
    );
    expect(clean).not.toContain("script");
    expect(clean).not.toContain("iframe");
    expect(clean).not.toContain("object");
    expect(clean).not.toContain("form");
    expect(clean).not.toContain("link");
    expect(clean).not.toContain("onclick");
    expect(clean).not.toContain("onerror");
    expect(clean).toContain('src="x.png"');
  });

  it("allows relative, https, asset and data:image URLs only", () => {
    expect(sanitizeGeneratedUi('<img src="/chart.png">')).toContain("/chart.png");
    expect(sanitizeGeneratedUi('<a href="https://example.com">x</a>')).toContain(
      "https://example.com",
    );
    expect(sanitizeGeneratedUi('<img src="asset://localhost/chart.png">')).toContain(
      "asset://localhost/chart.png",
    );
    expect(sanitizeGeneratedUi('<img src="data:image/png;base64,AAAA">')).toContain(
      "data:image/png",
    );
    expect(sanitizeGeneratedUi('<a href="javascript:steal()">x</a>')).not.toContain(
      "javascript:",
    );
    expect(sanitizeGeneratedUi('<a href="file:///C:/secrets.txt">x</a>')).not.toContain(
      "file:",
    );
    expect(sanitizeGeneratedUi('<img src="data:text/html,<b>x</b>">')).not.toContain(
      "data:text/html",
    );
  });

  it("keeps a chart-sized subset of SVG and scoped style rules", () => {
    const clean = sanitizeGeneratedUi(
      '<svg viewBox="0 0 10 10"><style>.bar{fill:var(--accent)}</style><rect class="bar" x="1" y="1" width="4" height="4"/><animate attributeName="x"/></svg>',
    );
    expect(clean).toContain("<svg");
    expect(clean).toContain("viewBox");
    expect(clean).toContain("<rect");
    expect(clean).toContain("var(--accent)");
    expect(clean).not.toContain("animate");
  });

  it("treats empty and oversized blocks as unrenderable", () => {
    expect(sanitizeGeneratedUi("   ")).toBe("");
    expect(sanitizeGeneratedUi("a".repeat(MAX_GENERATED_UI_CHARS + 1))).toBe("");
  });
});

describe("readGeneratedAction", () => {
  it("reads an action from the element or an ancestor", () => {
    const button = element(
      '<button data-loom-action="send" data-loom-value="Expand">Expand</button>',
    );
    expect(readGeneratedAction(button)).toEqual({ action: "send", value: "Expand" });

    const nested = element(
      '<div data-loom-action="draft" data-loom-value="Rewrite"><span><b>x</b></span></div>',
    );
    const inner = nested.querySelector("b") as Element;
    expect(readGeneratedAction(inner)).toEqual({ action: "draft", value: "Rewrite" });
  });

  it("normalizes open targets and rejects unsafe ones", () => {
    const link = element('<a data-loom-action="open" href="https://example.com/docs">docs</a>');
    expect(readGeneratedAction(link)).toEqual({
      action: "open",
      value: "https://example.com/docs",
    });
    const bad = element('<a data-loom-action="open" href="javascript:steal()">x</a>');
    expect(readGeneratedAction(bad)).toBeNull();
  });

  it("ignores unknown actions, missing values and plain elements", () => {
    expect(readGeneratedAction(element('<button data-loom-action="rm -rf">x</button>'))).toBeNull();
    expect(readGeneratedAction(element('<button data-loom-action="send">x</button>'))).toBeNull();
    expect(readGeneratedAction(element("<button>x</button>"))).toBeNull();
    expect(readGeneratedAction(null)).toBeNull();
  });

  it("clamps long values", () => {
    const button = element(
      `<button data-loom-action="copy" data-loom-value="${"x".repeat(MAX_ACTION_VALUE_CHARS + 10)}">x</button>`,
    );
    expect(readGeneratedAction(button)?.value).toHaveLength(MAX_ACTION_VALUE_CHARS);
  });
});

describe("safeExternalUrl", () => {
  it("accepts http, https and mailto", () => {
    expect(safeExternalUrl("https://example.com")).toBe("https://example.com/");
    expect(safeExternalUrl("mailto:a@b.co")).toBe("mailto:a@b.co");
  });

  it("rejects other schemes and garbage", () => {
    expect(safeExternalUrl("javascript:alert(1)")).toBeNull();
    expect(safeExternalUrl("file:///C:/x")).toBeNull();
    expect(safeExternalUrl("/relative")).toBeNull();
    expect(safeExternalUrl("  ")).toBeNull();
  });
});

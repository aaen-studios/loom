import DOMPurify, { type Config } from "dompurify";

/**
 * Model-generated UI: a fenced `loom-ui` block is sanitized, then mounted in a
 * shadow root inside the message. No scripts ever run; the declarative
 * `data-loom-action` attributes are the only way a block talks back.
 */

/** Fence language that switches a code block to the live renderer. */
export const GENERATED_UI_LANGUAGE = "loom-ui";

/** Above this the block renders as a plain code block instead. */
export const MAX_GENERATED_UI_CHARS = 120_000;

/** Clicks carry at most this much text into an app action. */
export const MAX_ACTION_VALUE_CHARS = 4_000;

export type GeneratedUiActionKind = "send" | "draft" | "copy" | "open";

export interface GeneratedUiAction {
  action: GeneratedUiActionKind;
  value: string;
}

const HTML_TAGS = [
  "a",
  "abbr",
  "b",
  "blockquote",
  "br",
  "caption",
  "cite",
  "code",
  "col",
  "colgroup",
  "dd",
  "del",
  "details",
  "dfn",
  "div",
  "dl",
  "dt",
  "em",
  "figcaption",
  "figure",
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
  "hr",
  "i",
  "img",
  "ins",
  "kbd",
  "label",
  "li",
  "mark",
  "ol",
  "p",
  "pre",
  "q",
  "s",
  "samp",
  "section",
  "small",
  "span",
  "strong",
  "sub",
  "summary",
  "sup",
  "table",
  "tbody",
  "td",
  "tfoot",
  "th",
  "thead",
  "time",
  "tr",
  "u",
  "ul",
  "var",
  "wbr",
  "style",
];

/** Form controls minus `<form>`: nothing here can navigate or submit. */
const CONTROL_TAGS = [
  "button",
  "fieldset",
  "input",
  "legend",
  "meter",
  "optgroup",
  "option",
  "output",
  "progress",
  "select",
  "textarea",
];

/** A chart-sized subset of SVG. `lineargradient` etc. stay lowercase: that is
 * how DOMPurify compares tag names. */
const SVG_TAGS = [
  "svg",
  "g",
  "title",
  "desc",
  "defs",
  "symbol",
  "use",
  "path",
  "rect",
  "circle",
  "ellipse",
  "line",
  "polyline",
  "polygon",
  "text",
  "tspan",
  "lineargradient",
  "radialgradient",
  "stop",
  "clippath",
  "mask",
  "pattern",
];

const ATTRS = [
  "alt",
  "checked",
  "cite",
  "class",
  "cols",
  "colspan",
  "data-loom-action",
  "data-loom-href",
  "data-loom-value",
  "datetime",
  "dir",
  "disabled",
  "for",
  "height",
  "href",
  "id",
  "label",
  "lang",
  "max",
  "maxlength",
  "min",
  "multiple",
  "name",
  "open",
  "optimum",
  "pattern",
  "placeholder",
  "readonly",
  "rel",
  "rows",
  "rowspan",
  "selected",
  "size",
  "span",
  "src",
  "start",
  "step",
  "style",
  "title",
  "type",
  "value",
  "width",
];

const SVG_ATTRS = [
  "viewbox",
  "d",
  "fill",
  "fill-opacity",
  "fill-rule",
  "stroke",
  "stroke-width",
  "stroke-linecap",
  "stroke-linejoin",
  "stroke-dasharray",
  "stroke-opacity",
  "points",
  "x",
  "y",
  "x1",
  "x2",
  "y1",
  "y2",
  "cx",
  "cy",
  "r",
  "rx",
  "ry",
  "transform",
  "offset",
  "stop-color",
  "stop-opacity",
  "gradienttransform",
  "preserveaspectratio",
  "opacity",
  "text-anchor",
  "dominant-baseline",
  "font-family",
  "font-size",
  "font-weight",
];

/**
 * Relative URLs, or an allowlisted scheme. Anything with a scheme that is not
 * in the list (javascript:, file:, blob:, data:text/html, ...) is rejected.
 */
const SAFE_URI =
  /^(?:(?![a-z][a-z0-9+.-]*:)[^\s\x00-\x20]|$)|^(?:https?:|mailto:|tel:|asset:|data:image\/)/i;

/** Attributes whose value is fetched or navigated to. */
const URI_ATTRS = new Set(["href", "src", "cite"]);

const SANITIZE_OPTIONS: Config = {
  ALLOWED_TAGS: [...HTML_TAGS, ...CONTROL_TAGS, ...SVG_TAGS],
  ALLOWED_ATTR: [...ATTRS, ...SVG_ATTRS],
  // `data-loom-*` is matched explicitly in ALLOWED_ATTR, so everything else
  // that starts with `data-` is dropped.
  ALLOW_DATA_ATTR: false,
  ALLOW_ARIA_ATTR: true,
  ALLOWED_URI_REGEXP: SAFE_URI,
};

/** Sanitized HTML, or `""` when the input is empty or oversized. */
export function sanitizeGeneratedUi(html: string): string {
  const trimmed = html.trim();
  if (!trimmed || trimmed.length > MAX_GENERATED_UI_CHARS) return "";
  return DOMPurify.sanitize(trimmed, SANITIZE_OPTIONS);
}

/**
 * DOMPurify allows any `data:` payload on media tags (`DATA_URI_TAGS`), which
 * is harmless inside `<img>` but looser than this feature's contract, so the
 * scheme rule is enforced uniformly here.
 *
 * Guarded: a non-DOM import (tests, tooling) gets the unsupported stub, which
 * has no hook support.
 */
if (typeof DOMPurify.addHook === "function") {
  DOMPurify.addHook("uponSanitizeAttribute", (_node, event) => {
    if (URI_ATTRS.has(event.attrName.toLowerCase()) && !SAFE_URI.test(event.attrValue)) {
      event.keepAttr = false;
    }
  });
}

/** An absolute external URL the app is willing to hand to the OS. */
export function safeExternalUrl(url: string): string | null {
  const trimmed = url.trim();
  if (!trimmed) return null;
  let parsed: URL;
  try {
    parsed = new URL(trimmed);
  } catch {
    return null;
  }
  if (parsed.protocol !== "http:" && parsed.protocol !== "https:" && parsed.protocol !== "mailto:") {
    return null;
  }
  return parsed.href;
}

/**
 * Reads the declarative action off a clicked element (or the nearest ancestor
 * that carries one). Returns null for anything unrecognized, so a stray
 * attribute can never cause an action.
 */
export function readGeneratedAction(target: Element | null): GeneratedUiAction | null {
  const element = target?.closest("[data-loom-action]");
  if (!element) return null;

  const action = element.getAttribute("data-loom-action");
  if (action === "open") {
    const url = safeExternalUrl(
      element.getAttribute("data-loom-href") ?? element.getAttribute("href") ?? "",
    );
    return url ? { action, value: url } : null;
  }
  if (action !== "send" && action !== "draft" && action !== "copy") return null;

  const value = (element.getAttribute("data-loom-value") ?? "").slice(
    0,
    MAX_ACTION_VALUE_CHARS,
  );
  return value ? { action, value } : null;
}

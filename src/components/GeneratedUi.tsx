import { useLayoutEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import type { CustomRendererProps } from "streamdown";
import sheet from "../generatedUi.css?inline";
import { cn } from "../lib/cn";
import {
  MAX_GENERATED_UI_CHARS,
  readGeneratedAction,
  safeExternalUrl,
  sanitizeGeneratedUi,
} from "../lib/generatedUi";
import { openExternal } from "../lib/tauri";
import { useChat } from "../stores/chat";
import { useSettings } from "../stores/settings";
import { CheckIcon, CopyIcon, FileIcon, SparkIcon } from "./icons";

/** The fallback for blocks that must not render: oversized, or nothing left
 *  after sanitizing. The source stays readable and copyable. */
function RawBlock({ code }: { code: string }) {
  return (
    <pre className="max-h-72 overflow-auto rounded-[14px] border border-[var(--glass-border)] bg-[var(--panel-bg-strong)] px-3.5 py-3 text-[13px] leading-relaxed whitespace-pre-wrap">
      {code.length > MAX_GENERATED_UI_CHARS ? `${code.slice(0, MAX_GENERATED_UI_CHARS)}\n…` : code}
    </pre>
  );
}

/**
 * Renders a ```loom-ui fence as a live, themed widget.
 *
 * The markup lives in a shadow root styled by `generatedUi.css`, so it inherits
 * the app's design tokens (dark/light included) while its own CSS stays scoped
 * and cannot reach the app. JavaScript never runs: the only way a widget talks
 * back is the declarative `data-loom-action` attributes, handled here.
 */
export function GeneratedUi({ code, isIncomplete }: CustomRendererProps) {
  const compact = useSettings((state) => state.config.interface.compact);
  const hostRef = useRef<HTMLDivElement>(null);
  const [sourceOpen, setSourceOpen] = useState(false);
  const [copied, setCopied] = useState(false);

  const sanitized = useMemo(() => {
    try {
      return sanitizeGeneratedUi(code);
    } catch {
      return "";
    }
  }, [code]);
  const renderable = sanitized.length > 0;

  // Ensure the shadow root, sheet and container exist, then swap the markup.
  // A layout effect keeps the transcript's follow-the-output scroll in step:
  // the height is real before the canvas measures it.
  useLayoutEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const root = host.shadowRoot ?? host.attachShadow({ mode: "open" });

    if (!root.querySelector("style[data-loom-ui]")) {
      const style = document.createElement("style");
      style.setAttribute("data-loom-ui", "");
      style.textContent = sheet;
      root.append(style);
    }

    let container = root.querySelector<HTMLDivElement>(".loom-ui");
    if (!container) {
      container = document.createElement("div");
      container.className = "loom-ui";
      root.append(container);
    }

    if (container.innerHTML !== sanitized) container.innerHTML = sanitized;
  }, [sanitized]);

  /** Widget markup never navigates the webview; links open in the OS browser. */
  const onClick = (event: MouseEvent<HTMLDivElement>) => {
    const path = event.nativeEvent.composedPath();
    const target = path.find((node): node is Element => node instanceof Element) ?? null;

    const action = readGeneratedAction(target);
    if (action) {
      event.preventDefault();
      if (action.action === "send") {
        void useChat.getState().send(action.value);
      } else if (action.action === "draft") {
        useChat.getState().setDraft(action.value);
      } else if (action.action === "copy") {
        void navigator.clipboard.writeText(action.value).catch(() => {});
        const element = target?.closest("[data-loom-action]");
        if (element) {
          element.setAttribute("data-loom-copied", "");
          window.setTimeout(() => element.removeAttribute("data-loom-copied"), 900);
        }
      } else if (action.action === "open") {
        openExternal(action.value);
      }
      return;
    }

    const anchor = path.find(
      (node): node is HTMLAnchorElement => node instanceof HTMLAnchorElement,
    );
    if (anchor) {
      event.preventDefault();
      const url = safeExternalUrl(anchor.getAttribute("href") ?? "");
      if (url) openExternal(url);
    }
  };

  const copySource = () => {
    void navigator.clipboard.writeText(code).catch(() => {});
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1200);
  };

  return (
    <div className="mt-0.5">
      <div
        ref={hostRef}
        data-streaming={isIncomplete ? "true" : undefined}
        data-density={compact ? "compact" : undefined}
        onClick={onClick}
        style={sourceOpen ? { display: "none" } : undefined}
        className={cn(!renderable && "hidden")}
      />
      {!renderable && <RawBlock code={code} />}
      {sourceOpen && renderable && <RawBlock code={code} />}
      {renderable && !isIncomplete && (
        <div
          className={cn(
            "mt-1 flex items-center gap-1 transition",
            "opacity-0 group-hover:opacity-100 focus-within:opacity-100",
            sourceOpen && "opacity-100",
          )}
        >
          <button
            type="button"
            title={copied ? "Copied" : "Copy HTML"}
            aria-label="Copy generated UI source"
            onClick={copySource}
            className="hover-surface grid h-6 w-6 place-items-center rounded-control text-faint hover:text-[var(--ink)]"
          >
            {copied ? <CheckIcon size={13} /> : <CopyIcon size={13} />}
          </button>
          <button
            type="button"
            title={sourceOpen ? "Hide source" : "View source"}
            aria-label="View generated UI source"
            aria-pressed={sourceOpen}
            onClick={() => setSourceOpen((value) => !value)}
            className={cn(
              "hover-surface grid h-6 w-6 place-items-center rounded-control text-faint hover:text-[var(--ink)]",
              sourceOpen && "text-[var(--accent)]",
            )}
          >
            <FileIcon size={13} />
          </button>
          <span className="flex items-center gap-1 text-[11.5px] text-faint">
            <SparkIcon size={11} />
            Generated UI
          </span>
        </div>
      )}
    </div>
  );
}

import { lazy, Suspense, type MouseEvent } from "react";
import type { CustomRenderer, CustomRendererProps, PluginConfig } from "streamdown";
import { GENERATED_UI_LANGUAGE, safeExternalUrl } from "../lib/generatedUi";
import { openExternal } from "../lib/tauri";
import { CheckIcon, CopyIcon, ExternalLinkIcon } from "./icons";

/** Fence languages a document is expected to arrive in. */
export const DOCUMENT_LANGUAGES = ["markdown", "md"];

/**
 * A link in a reply, drawn as a badge rather than a bare underline.
 *
 * The host is the useful part: a wall of `https://…` underlines reads as noise,
 * while `github.com` in a pill tells you where the link goes at a glance. The
 * full URL is kept in `title`, so nothing is hidden — only de-emphasised.
 *
 * Rendered as a real `<a href>` on purpose. The delegated click handler in
 * `Markdown` is what stops the webview navigating away, and it keys off the
 * anchor, so this must stay one.
 */
function LinkBadge({
  href,
  children,
  ...rest
}: React.AnchorHTMLAttributes<HTMLAnchorElement>) {
  const url = typeof href === "string" ? href : "";
  let host: string | null = null;
  try {
    const parsed = new URL(url);
    host = parsed.host.replace(/^www\./, "");
  } catch {
    // A relative or malformed href: fall back to the plain anchor, which is
    // still clickable and still sanitized on click.
    host = null;
  }

  if (!host) {
    return (
      <a href={href} {...rest}>
        {children}
      </a>
    );
  }

  return (
    <a href={href} title={url} className="loom-link" {...rest}>
      <span className="loom-link-host">{host}</span>
      <ExternalLinkIcon size={11} className="loom-link-glyph" />
    </a>
  );
}

// Loaded on demand: markdown, syntax highlighting and the rich renderers are by
// far the heaviest part of the bundle and only matter once a reply arrives.
const Streamdown = lazy(async () => {
  const [streamdown, shiki] = await Promise.all([
    import("streamdown"),
    import("@streamdown/code"),
  ]);

  const code = shiki.createCodePlugin({
    themes: ["github-light-default", "github-dark-default"],
  });

  // A markdown fence is a document, not a widget: it renders in reasoning too.
  // The `loom-ui` renderer is the model's HTML, so it follows the setting.
  const documents: CustomRenderer[] = [
    { language: DOCUMENT_LANGUAGES, component: MarkdownDocSlot },
  ];
  const plain: PluginConfig = { code, renderers: documents };
  const rich: PluginConfig = {
    code,
    renderers: [
      ...documents,
      { language: GENERATED_UI_LANGUAGE, component: GeneratedUiSlot },
    ],
  };

  // Stable references: Streamdown memoizes on them.
  const linkage = { enabled: false };
  const controls = { code: { download: false }, table: false, image: false };
  const icons = { CheckIcon, CopyIcon };
  // Module scope, not created per render: Streamdown memoizes on this
  // reference, so an inline object would remount every link on every delta.
  const components = { a: LinkBadge };

  return {
    default: function MarkdownView({
      content,
      rich: allowRich,
      streaming,
    }: {
      content: string;
      rich: boolean;
      streaming: boolean;
    }) {
      return (
        <streamdown.Streamdown
          plugins={allowRich ? rich : plain}
          controls={controls}
          icons={icons}
          components={components}
          linkSafety={linkage}
          isAnimating={streaming}
        >
          {content}
        </streamdown.Streamdown>
      );
    },
  };
});

// Model-generated widgets carry the sanitizer, so they load with the first
// ```loom-ui block instead of with the app shell.
const GeneratedUi = lazy(() =>
  import("./GeneratedUi").then((module) => ({ default: module.GeneratedUi })),
);

function GeneratedUiSlot(props: CustomRendererProps) {
  return (
    <Suspense
      fallback={
        <pre className="overflow-x-auto rounded-[14px] border border-[var(--glass-border)] bg-[var(--panel-bg-strong)] px-3.5 py-3 text-[13px] leading-relaxed whitespace-pre-wrap">
          {props.code}
        </pre>
      }
    >
      <GeneratedUi {...props} />
    </Suspense>
  );
}

const MarkdownDoc = lazy(() =>
  import("./MarkdownDoc").then((module) => ({ default: module.MarkdownDoc })),
);

function MarkdownDocSlot(props: CustomRendererProps) {
  return (
    <Suspense
      fallback={
        <pre className="overflow-x-auto rounded-[14px] border border-[var(--glass-border)] bg-[var(--panel-bg-strong)] px-3.5 py-3 text-[13px] leading-relaxed whitespace-pre-wrap">
          {props.code}
        </pre>
      }
    >
      <MarkdownDoc {...props} />
    </Suspense>
  );
}

/** Links leave the app: the webview must never navigate away from Loom. */
function handleLinkClick(event: MouseEvent<HTMLDivElement>) {
  const anchor = (event.target as Element | null)?.closest("a[href]");
  if (!anchor) return;
  event.preventDefault();
  const url = safeExternalUrl(anchor.getAttribute("href") ?? "");
  if (url) openExternal(url);
}

/**
 * Streaming-safe markdown. Streamdown tolerates partial markdown while a
 * response is still arriving, so nothing flickers or re-flows badly.
 *
 * `allowGeneratedUi` is off for reasoning text: the inner monologue should read
 * as prose, not mount widgets. Markdown documents are not widgets, so they
 * render either way.
 */
export function Markdown({
  content,
  allowGeneratedUi = true,
  streaming = false,
}: {
  content: string;
  allowGeneratedUi?: boolean;
  streaming?: boolean;
}) {
  return (
    <div className="loom-markdown" onClick={handleLinkClick}>
      <Suspense fallback={<div className="whitespace-pre-wrap">{content}</div>}>
        <Streamdown content={content} rich={allowGeneratedUi} streaming={streaming} />
      </Suspense>
    </div>
  );
}

import { lazy, Suspense } from "react";

// Loaded on demand: markdown + syntax highlighting is by far the heaviest part
// of the bundle and only matters once a reply arrives.
const Streamdown = lazy(() =>
  import("streamdown").then((module) => ({ default: module.Streamdown })),
);

/**
 * Streaming-safe markdown. Streamdown tolerates partial markdown while a
 * response is still arriving, so nothing flickers or re-flows badly.
 */
export function Markdown({ content }: { content: string }) {
  return (
    <div className="loom-markdown">
      <Suspense fallback={<div className="whitespace-pre-wrap">{content}</div>}>
        <Streamdown>{content}</Streamdown>
      </Suspense>
    </div>
  );
}

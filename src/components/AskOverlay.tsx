import { useCallback, useEffect, useRef, useState } from "react";
import { currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
import { LogicalSize, PhysicalPosition } from "@tauri-apps/api/dpi";
import { ipc } from "../lib/ipc";
import { useEngineEvents } from "../lib/events";
import { parseAttachments } from "../lib/messageExtra";
import { isTauri } from "../lib/tauri";
import type { Attachment } from "../types";
import { useChat } from "../stores/chat";
import { useSettings } from "../stores/settings";
import { AttachmentStrip } from "./AttachmentChips";
import { Markdown } from "./Markdown";
import { PermissionCard } from "./ToolCalls";
import { QuestionCard } from "./QuestionCard";
import {
  ArrowUpIcon,
  CameraIcon,
  ExternalLinkIcon,
  LoomMark,
  PlusIcon,
  StopIcon,
} from "./icons";

const WIDTH = 640;
const MAX_HEIGHT = 620;
/** Window padding, and the gap between the transcript and the composer blob. */
const PAD = 12;
const GAP = 10;
/** The column hangs a little below the middle of the monitor, so replies grow
 *  upward into open space instead of pushing the composer around. */
const ANCHOR_Y = 0.62;
/** Height of the fade at a clipped edge of the transcript. */
const FADE = 22;

interface Anchor {
  x: number;
  /** The window's bottom edge, and the ceiling its top edge may not pass. */
  bottom: number;
  top: number;
}

/**
 * Quick-ask overlay (Ctrl+Shift+Space): loose blobs on the desktop, not a
 * window. Your prompts are surfaces on the right, replies are bare text on the
 * left, and the composer stays put while answers grow upward from it. Nothing
 * tool-shaped is shown — the full transcript waits in the main window.
 *
 * Clicking outside (or Esc) hides the window; the turn keeps running.
 */
export function AskOverlay() {
  useEngineEvents();

  const [value, setValue] = useState("");
  const [edges, setEdges] = useState({ top: false, bottom: false });
  const [capturing, setCapturing] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const messages = useChat((state) => state.messages);
  const activeId = useChat((state) => state.activeId);
  const busy = useChat(
    (state) => (state.activeId ? state.busy[state.activeId] : false) ?? false,
  );
  const question = useChat((state) =>
    state.activeId ? state.questions[state.activeId] : undefined,
  );
  const permission = useChat((state) =>
    state.activeId ? state.permissions[state.activeId] : undefined,
  );
  const error = useChat((state) =>
    state.activeId ? state.errors[state.activeId] : undefined,
  );
  const send = useChat((state) => state.send);
  const stop = useChat((state) => state.stop);
  const newSession = useChat((state) => state.newSession);
  const ensureSession = useChat((state) => state.ensureSession);
  const captureOnSend = useSettings(
    (state) => state.config.interface.captureOnSend,
  );
  const generatedUi = useSettings((state) => state.config.interface.generatedUi);
  const loadSettings = useSettings((state) => state.load);

  const rootRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const footerRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const pinned = useRef(true);
  /** True until this summon's first message has used the opening frame. */
  const firstSend = useRef(true);
  const sized = useRef(0);
  const placed = useRef(false);
  const anchor = useRef<Anchor | null>(null);
  const focusedOnce = useRef(false);
  const shownEdges = useRef(edges);

  const started = messages.length > 0 || busy || !!question;

  /** Resolves (once per summon) where the column's bottom edge should sit. */
  const pin = useCallback(async (): Promise<Anchor | null> => {
    if (!anchor.current) {
      const monitor = await currentMonitor();
      if (!monitor) return null;
      anchor.current = {
        x:
          monitor.position.x +
          (monitor.size.width - WIDTH * monitor.scaleFactor) / 2,
        bottom: monitor.position.y + monitor.size.height * ANCHOR_Y,
        top: monitor.position.y + 8,
      };
    }
    return anchor.current;
  }, []);

  /** Sizes the window to the stack, then re-hangs it from the pinned edge. */
  const layout = useCallback(async () => {
    if (!isTauri) return;
    const content = contentRef.current;
    const footer = footerRef.current;
    if (!content || !footer) return;

    const desired = Math.round(
      Math.min(
        MAX_HEIGHT,
        Math.max(56, content.offsetHeight + footer.offsetHeight + GAP + PAD * 2),
      ),
    );

    const window_ = getCurrentWindow();
    const moved = desired !== sized.current;
    if (moved) {
      sized.current = desired;
      try {
        await window_.setSize(new LogicalSize(WIDTH, desired));
      } catch {
        return;
      }
    }
    if (!moved && placed.current) return;

    const spot = await pin();
    if (!spot) return;
    placed.current = true;
    try {
      const size = await window_.innerSize();
      await window_.setPosition(
        new PhysicalPosition(
          Math.round(spot.x),
          Math.round(Math.max(spot.top, spot.bottom - size.height)),
        ),
      );
    } catch {
      placed.current = false;
    }
  }, [pin]);

  /** Fades whichever end of the transcript is currently cut off. */
  const syncEdges = useCallback(() => {
    const element = scrollRef.current;
    if (!element) return;
    const next = {
      top: element.scrollTop > 4,
      bottom: element.scrollTop + element.clientHeight < element.scrollHeight - 4,
    };
    if (next.top === shownEdges.current.top && next.bottom === shownEdges.current.bottom) {
      return;
    }
    shownEdges.current = next;
    setEdges(next);
  }, []);

  /** Replays the summon animation on a root class, without remounting children. */
  const replay = useCallback(() => {
    const element = rootRef.current;
    if (!element) return;
    element.classList.remove("animate-summon");
    void element.offsetWidth;
    element.classList.add("animate-summon");
  }, []);

  // The overlay owns its own webview, so it reads the config itself: focus
  // events alone can fire before this view has subscribed.
  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  useEffect(() => {
    if (!isTauri) return;
    let frame = 0;
    const schedule = () => {
      window.cancelAnimationFrame(frame);
      frame = window.requestAnimationFrame(() => {
        void layout();
        syncEdges();
      });
    };
    const observer = new ResizeObserver(schedule);
    if (contentRef.current) observer.observe(contentRef.current);
    if (footerRef.current) observer.observe(footerRef.current);
    schedule();
    return () => {
      observer.disconnect();
      window.cancelAnimationFrame(frame);
    };
  }, [layout, syncEdges]);

  // Losing focus (a click anywhere else) puts the overlay away.
  useEffect(() => {
    if (!isTauri) return;
    let dispose: (() => void) | undefined;
    void getCurrentWindow()
      .onFocusChanged(({ payload }) => {
        if (payload) {
          // A summon: re-pin in case the window landed on another monitor,
          // and pick up settings changed in the main window meanwhile.
          focusedOnce.current = true;
          firstSend.current = true;
          anchor.current = null;
          placed.current = false;
          replay();
          void layout();
          void loadSettings();
        } else if (focusedOnce.current) {
          void ipc.hideOverlay();
        }
      })
      .then((unlisten) => {
        dispose = unlisten;
      })
      .catch(() => {});
    return () => dispose?.();
  }, [layout, loadSettings, replay]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") void ipc.hideOverlay();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Keep the tail of the conversation in view unless the user scrolled up.
  useEffect(() => {
    const element = scrollRef.current;
    if (element && pinned.current) element.scrollTop = element.scrollHeight;
    syncEdges();
  }, [messages, question, permission, busy, syncEdges]);

  // Give the input back once a turn ends.
  useEffect(() => {
    if (!busy) inputRef.current?.focus();
  }, [busy]);

  /**
   * Sends the prompt, with a screenshot when enabled. The summon's first
   * message carries the frame taken as the overlay opened (the screen the user
   * summoned it over); later messages capture the screen at send time. A
   * failed capture never costs the user their text: the prompt goes anyway,
   * with a one-line notice where the error would sit.
   */
  const submit = async () => {
    const text = value.trim();
    if (!text || busy || capturing) return;
    setValue("");
    setNotice(null);
    pinned.current = true;

    let attachments: Attachment[] = [];
    if (captureOnSend) {
      setCapturing(true);
      try {
        const sessionId = await ensureSession();
        if (sessionId) {
          // The first message of a summon carries the screen as it was when
          // the overlay opened; later ones capture at send time.
          const opening = firstSend.current
            ? await ipc.claimScreen(sessionId)
            : null;
          firstSend.current = false;
          const shot = opening ?? (await ipc.captureScreen(sessionId));
          if (shot) attachments = [shot];
        }
      } catch (error) {
        setNotice(
          `Screenshot skipped — ${error instanceof Error ? error.message : String(error)}`,
        );
      } finally {
        setCapturing(false);
      }
    }

    await send(text, { attachments });
  };

  const reset = () => {
    setValue("");
    setNotice(null);
    pinned.current = true;
    void newSession();
    inputRef.current?.focus();
  };

  const latest = messages[messages.length - 1];
  const waiting =
    busy && !(latest?.role === "assistant" && latest.content.trim().length > 0);

  let lastAnswerId: string | null = null;
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (message.role === "assistant" && message.content.trim().length > 0) {
      lastAnswerId = message.id;
      break;
    }
  }

  let mask: string | undefined;
  if (edges.top && edges.bottom) {
    mask = `linear-gradient(to bottom, transparent 0, #000 ${FADE}px, #000 calc(100% - ${FADE}px), transparent 100%)`;
  } else if (edges.top) {
    mask = `linear-gradient(to bottom, transparent 0, #000 ${FADE}px)`;
  } else if (edges.bottom) {
    mask = `linear-gradient(to bottom, #000 calc(100% - ${FADE}px), transparent 100%)`;
  }
  const maskStyle = mask
    ? { maskImage: mask, WebkitMaskImage: mask }
    : undefined;

  return (
    <div
      ref={rootRef}
      className="animate-summon flex h-screen w-screen flex-col overflow-hidden bg-transparent px-3 pt-3 pb-3"
    >
      <div
        ref={scrollRef}
        onScroll={() => {
          const element = scrollRef.current;
          if (!element) return;
          pinned.current =
            element.scrollHeight - element.scrollTop - element.clientHeight < 40;
          syncEdges();
        }}
        style={maskStyle}
        className="no-scrollbar min-h-0 flex-1 overflow-y-auto"
      >
        <div ref={contentRef} className="flex flex-col gap-2">
          {messages.map((message) =>
            message.role === "user" ? (
              <div key={message.id} className="animate-fade-up flex justify-end">
                <div className="blob max-w-[78%] rounded-sheet px-3.5 py-2 text-[13.5px] leading-5 whitespace-pre-wrap select-text">
                  <AttachmentStrip
                    attachments={parseAttachments(message.extra)}
                    compact
                  />
                  {message.content}
                </div>
              </div>
            ) : message.content ? (
              <div
                key={message.id}
                className="group animate-fade-up relative select-text"
              >
                <div className="loom-markdown blob-text text-[13.5px] leading-6">
                  <Markdown
                    content={message.content}
                    allowGeneratedUi={generatedUi}
                    streaming={busy && message.id === latest?.id}
                  />
                </div>
                {message.id === lastAnswerId && (
                  <div className="pointer-events-none mt-1.5 flex h-6 items-center gap-1 opacity-0 transition-opacity duration-150 group-hover:pointer-events-auto group-hover:opacity-100">
                    <button
                      type="button"
                      onClick={() => void ipc.showMain(activeId)}
                      title="Open in chat"
                      className="blob flex h-6 items-center gap-1.5 rounded-capsule px-2.5 text-[11.5px] text-soft hover:text-[var(--ink)]"
                    >
                      <ExternalLinkIcon size={12} />
                      Open in chat
                    </button>
                  </div>
                )}
              </div>
            ) : null,
          )}

          {waiting && !question && (
            <div className="blob animate-fade-up flex w-fit items-center gap-1 self-start rounded-capsule px-3.5 py-3">
              {[0, 1, 2].map((index) => (
                <span
                  key={index}
                  className="cursor-blink h-1.5 w-1.5 rounded-full bg-[var(--ink-faint)]"
                  style={{ animationDelay: `${index * 0.16}s` }}
                />
              ))}
            </div>
          )}
        </div>
      </div>

      <div ref={footerRef} className="shrink-0 pt-2.5">
        {notice && (
          <p className="blob animate-fade-up mb-2 rounded-sheet px-3.5 py-2 text-center text-[12px] text-soft">
            {notice}
          </p>
        )}
        {error && (
          <p className="blob animate-fade-up mb-2 rounded-sheet px-3.5 py-2 text-center text-[12px] text-[var(--danger)]">
            {error}
          </p>
        )}

        {question ? (
          <QuestionCard question={question} />
        ) : permission ? (
          <PermissionCard permission={permission} />
        ) : (
          <div className="blob flex items-center gap-2 rounded-sheet px-3 py-1">
            {capturing ? (
              <span className="flex shrink-0 items-center gap-1.5 text-faint">
                <CameraIcon size={14} className="cursor-blink" />
                <span className="text-[12px]">Screen</span>
              </span>
            ) : (
              <LoomMark size={14} className="shrink-0 text-[var(--ink-faint)]" />
            )}
            <input
              ref={inputRef}
              autoFocus
              value={value}
              placeholder={
                capturing ? "Capturing screen…" : started ? "Reply…" : "Ask anything…"
              }
              onChange={(event) => setValue(event.currentTarget.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && !event.nativeEvent.isComposing) {
                  event.preventDefault();
                  void submit();
                }
              }}
              className="h-7 min-w-0 flex-1 bg-transparent text-[14px] text-[var(--ink)] placeholder:text-[var(--ink-faint)]"
            />
            {messages.length > 0 && (
              <button
                type="button"
                onClick={reset}
                title="New chat"
                aria-label="New chat"
                className="grid h-7 w-7 shrink-0 place-items-center rounded-full text-[var(--ink-faint)] hover:bg-[var(--hover-bg)] hover:text-[var(--ink)]"
              >
                <PlusIcon size={15} />
              </button>
            )}
            <button
              type="button"
              onClick={busy ? () => void stop() : () => void submit()}
              disabled={capturing || (!busy && !value.trim())}
              aria-label={busy ? "Stop" : "Ask"}
              className="grid h-7 w-7 shrink-0 place-items-center rounded-full bg-[var(--control-bg)] text-[var(--control-ink)] disabled:opacity-40"
            >
              {busy ? <StopIcon size={13} /> : <ArrowUpIcon size={14} />}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

import { useRef, useState } from "react";
import { ipc } from "../lib/ipc";
import { useChat } from "../stores/chat";
import { ArrowUpIcon, LoomMark } from "./icons";

/**
 * Quick-ask overlay (Ctrl+Shift+Space): a compact always-on-top input that
 * starts a new chat, sends the message, then reveals the main window so the
 * reply is visible.
 */
export function AskOverlay() {
  const [value, setValue] = useState("");
  const sending = useRef(false);
  const newSession = useChat((state) => state.newSession);
  const send = useChat((state) => state.send);

  const submit = async () => {
    const text = value.trim();
    if (!text || sending.current) return;
    sending.current = true;
    setValue("");
    try {
      const session = await newSession();
      if (session) await send(text);
      await ipc.showMain();
    } finally {
      await ipc.hideOverlay();
      sending.current = false;
    }
  };

  return (
    <div className="chrome h-screen w-screen overflow-hidden bg-transparent p-2">
      <div className="panel-strong flex h-full w-full items-center gap-3 rounded-sheet px-3.5">
        <span className="text-soft">
          <LoomMark size={18} />
        </span>

        <input
          autoFocus
          value={value}
          placeholder="Ask anything…"
          onChange={(event) => setValue(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.nativeEvent.isComposing) {
              event.preventDefault();
              void submit();
            }
            if (event.key === "Escape") void ipc.hideOverlay();
          }}
          className="h-full min-w-0 flex-1 bg-transparent text-[15px] text-[var(--ink)] placeholder:text-[var(--ink-faint)]"
        />

        <button
          type="button"
          onClick={() => void submit()}
          disabled={!value.trim()}
          aria-label="Ask"
          className="grid h-9 w-9 shrink-0 place-items-center rounded-full bg-[var(--control-bg)] text-[var(--control-ink)] disabled:opacity-40"
        >
          <ArrowUpIcon size={16} />
        </button>
      </div>
    </div>
  );
}

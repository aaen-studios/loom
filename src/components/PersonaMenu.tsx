import { useEffect, useRef, useState } from "react";
import { cn } from "../lib/cn";
import { useChat } from "../stores/chat";
import { useSettings } from "../stores/settings";
import { CheckIcon, ChevronDownIcon, PersonIcon } from "./icons";

/**
 * Persona chip + menu. Selecting a persona snapshots its system prompt onto
 * the session, so later edits to the persona don't rewrite existing chats.
 */
export function PersonaMenu() {
  const personas = useSettings((state) => state.config.personas);
  const session = useChat((state) =>
    state.sessions.find((item) => item.id === state.activeId),
  );
  const setPersona = useChat((state) => state.setPersona);
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: MouseEvent) => {
      if (!containerRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("mousedown", onPointerDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onPointerDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const persona = personas.find((item) => item.id === session?.personaId);

  const select = async (personaId: string | null) => {
    const chosen = personas.find((item) => item.id === personaId);
    await setPersona(personaId, chosen?.systemPrompt ?? null);
    setOpen(false);
  };

  return (
    <div className="relative" ref={containerRef}>
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        title={persona ? `Persona: ${persona.name}` : "Choose persona"}
        className={cn(
          "flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[12.5px]",
          persona
            ? "border-[var(--glass-border)] text-soft"
            : "border-[var(--glass-border)] text-faint",
        )}
      >
        <PersonIcon size={14} />
        <span className="max-w-[140px] truncate">
          {persona?.name ?? "Persona"}
        </span>
        <ChevronDownIcon size={13} />
      </button>

      {open && (
        <div className="panel-strong absolute bottom-full left-0 z-40 mb-2 w-[280px] overflow-hidden rounded-sheet p-1.5">
          <button
            type="button"
            onClick={() => void select(null)}
            className="hover-surface flex w-full items-center gap-2 rounded-row px-2 py-1.5 text-left text-[13px] text-soft"
          >
            <span className="flex-1">No persona</span>
            {!session?.personaId && <CheckIcon size={14} />}
          </button>

          {personas.length === 0 && (
            <p className="px-2 py-2 text-[12px] text-faint">
              Create personas in Settings → Personas.
            </p>
          )}

          {personas.map((item) => (
            <button
              key={item.id}
              type="button"
              onClick={() => void select(item.id)}
              className={cn(
                "hover-surface flex w-full items-center gap-2 rounded-row px-2 py-1.5 text-left text-[13px]",
                session?.personaId === item.id
                  ? "text-[var(--ink)]"
                  : "text-soft",
              )}
            >
              <span className="flex-1 truncate">{item.name}</span>
              {session?.personaId === item.id && <CheckIcon size={14} />}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

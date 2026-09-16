import { useEffect, useRef, useState } from "react";
import { cn } from "../lib/cn";
import { useMenu } from "../lib/menu";
import { ipc } from "../lib/ipc";
import { useChat } from "../stores/chat";
import { useSettings } from "../stores/settings";
import { CheckIcon, ChevronDownIcon, PersonIcon } from "./icons";

/**
 * Persona chip + menu. Selecting a persona snapshots its system prompt onto
 * the session, so later edits to the persona don't rewrite existing chats — a
 * chat whose snapshot has fallen behind gets an "updated" marker.
 *
 * The same menu manages a multi-persona cast and picks who answers next.
 */
export function PersonaMenu({ align = "up" }: { align?: "up" | "down" }) {
  const personas = useSettings((state) => state.config.personas);
  const session = useChat((state) =>
    state.sessions.find((item) => item.id === state.activeId),
  );
  const setPersona = useChat((state) => state.setPersona);
  const newSession = useChat((state) => state.newSession);
  const speakerId = useChat((state) => state.speakerId);
  const setSpeaker = useChat((state) => state.setSpeaker);
  const setCastIds = useChat((state) => state.setCastIds);
  const containerRef = useRef<HTMLDivElement>(null);
  const { open, setOpen, close } = useMenu("persona", containerRef);
  const [cast, setCast] = useState<string[]>([]);

  const persona = personas.find((item) => item.id === session?.personaId);
  const speaker = personas.find((item) => item.id === speakerId);

  useEffect(() => {
    if (!open || !session) {
      if (!session) setCastIds([]);
      return;
    }
    let live = true;
    void ipc.sessionCast(session.id).then((list) => {
      if (live && list) {
        const ids = list.map((member) => member.id);
        setCast(ids);
        setCastIds(ids);
      }
    });
    return () => {
      live = false;
    };
  }, [open, session?.id]);

  const select = async (personaId: string | null) => {
    const chosen = personas.find((item) => item.id === personaId);
    await setPersona(personaId, chosen?.systemPrompt ?? null);
    close();
  };

  const toggleCast = async (personaId: string) => {
    if (!session) return;
    const next = cast.includes(personaId)
      ? cast.filter((id) => id !== personaId)
      : [...cast, personaId];
    setCast(next);
    setCastIds(next);
    if (speakerId && !next.includes(speakerId)) setSpeaker(null);
    await ipc.setSessionCast(session.id, next);
  };

  const ordered = [...personas].sort((left, right) => {
    if (left.favorite !== right.favorite) return left.favorite ? -1 : 1;
    return left.name.localeCompare(right.name);
  });

  const stale =
    Boolean(persona && session?.systemPrompt) &&
    session?.systemPrompt !== persona?.systemPrompt;

  return (
    <div className="relative" ref={containerRef}>
      <button
        type="button"
        onClick={() => setOpen(!open)}
        title={
          persona
            ? stale
              ? `Persona: ${persona.name} (updated since this chat started)`
              : `Persona: ${persona.name}`
            : "Choose persona"
        }
        className={cn(
          "hover-surface flex h-8 max-w-[190px] items-center gap-1.5 rounded-full px-2.5 text-[12.5px]",
          persona ? "text-soft" : "text-faint",
        )}
      >
        {persona?.emoji ? (
          <span className="text-[14px] leading-none">{persona.emoji}</span>
        ) : (
          <PersonIcon size={15} />
        )}
        <span className="max-w-[110px] truncate">
          {speaker ? `${speaker.name} →` : (persona?.name ?? "Persona")}
        </span>
        {stale && <span className="text-[10px] text-[var(--accent)]">•</span>}
        <ChevronDownIcon size={13} />
      </button>

      {open && (
        <div
          className={cn(
            "panel-strong absolute z-40 max-h-[70vh] w-[300px] overflow-y-auto rounded-sheet p-1.5",
            align === "down" ? "right-0 top-full mt-2" : "bottom-full left-0 mb-2",
          )}
        >
          <button
            type="button"
            onClick={() => void select(null)}
            className="hover-surface flex w-full items-center gap-2 rounded-row px-2 py-1.5 text-left text-[13px] text-soft"
          >
            <span className="flex-1">No persona</span>
            {!session?.personaId && <CheckIcon size={14} />}
          </button>

          {stale && persona && (
            <button
              type="button"
              onClick={() => void select(persona.id)}
              className="hover-surface flex w-full items-center gap-2 rounded-row px-2 py-1.5 text-left text-[12px] text-soft"
            >
              <span className="flex-1">
                {persona.name} changed since this chat started — update it
              </span>
            </button>
          )}

          {personas.length === 0 && (
            <p className="px-2 py-2 text-[12px] text-faint">
              Create personas in Settings → Personas.
            </p>
          )}

          {ordered.map((item) => (
            <div key={item.id} className="group flex items-center">
              <button
                type="button"
                onClick={() => void select(item.id)}
                className={cn(
                  "hover-surface flex min-w-0 flex-1 items-center gap-2 rounded-row px-2 py-1.5 text-left text-[13px]",
                  session?.personaId === item.id
                    ? "text-[var(--ink)]"
                    : "text-soft",
                )}
              >
                <span className="w-5 shrink-0 text-center text-[14px]">
                  {item.emoji ?? "·"}
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate">
                    {item.favorite ? "★ " : ""}
                    {item.name}
                  </span>
                  {item.description && (
                    <span className="block truncate text-[11.5px] text-faint">
                      {item.description}
                    </span>
                  )}
                </span>
                {session?.personaId === item.id && <CheckIcon size={14} />}
              </button>
              <button
                type="button"
                title={`New chat as ${item.name}`}
                onClick={() => {
                  void newSession(item.id);
                  close();
                }}
                className="hover-surface mr-0.5 rounded-row px-1.5 py-1 text-[12px] text-faint"
              >
                New
              </button>
            </div>
          ))}

          {session && personas.length > 0 && (
            <div className="mt-1 border-t border-[var(--glass-border)] px-2 pb-1 pt-2">
              <p className="text-[11px] uppercase tracking-wide text-faint">
                Group cast
              </p>
              <p className="mt-0.5 text-[11.5px] leading-4 text-faint">
                Tick two or more personas, then pick who answers next.
              </p>
              <div className="mt-1 flex flex-wrap gap-1">
                {ordered.map((item) => (
                  <button
                    key={item.id}
                    type="button"
                    onClick={() => void toggleCast(item.id)}
                    className={cn(
                      "chip px-2 py-0.5 text-[11.5px]",
                      cast.includes(item.id)
                        ? "text-[var(--ink)]"
                        : "text-faint",
                    )}
                  >
                    {item.emoji ?? ""} {item.name}
                  </button>
                ))}
              </div>
              {cast.length > 1 && (
                <div className="mt-2">
                  <p className="text-[11px] uppercase tracking-wide text-faint">
                    Next reply
                  </p>
                  <div className="mt-1 flex flex-wrap gap-1">
                    <button
                      type="button"
                      onClick={() => setSpeaker(null)}
                      className={cn(
                        "chip px-2 py-0.5 text-[11.5px]",
                        speakerId ? "text-faint" : "text-[var(--ink)]",
                      )}
                    >
                      Default
                    </button>
                    {cast.map((id) => {
                      const member = personas.find((item) => item.id === id);
                      if (!member) return null;
                      return (
                        <button
                          key={id}
                          type="button"
                          onClick={() => setSpeaker(id)}
                          className={cn(
                            "chip px-2 py-0.5 text-[11.5px]",
                            speakerId === id
                              ? "text-[var(--ink)]"
                              : "text-faint",
                          )}
                        >
                          {member.emoji ?? ""} {member.name}
                        </button>
                      );
                    })}
                  </div>
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

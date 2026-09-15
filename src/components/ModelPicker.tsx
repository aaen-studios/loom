import { useEffect, useMemo, useRef, useState } from "react";
import { cn } from "../lib/cn";
import { compactTokens, shortModelName } from "../lib/format";
import { ipc } from "../lib/ipc";
import type { ModelEntry, ModelRef } from "../types";
import { findModel, useProviders } from "../stores/providers";
import { useChat } from "../stores/chat";
import { useSettings } from "../stores/settings";
import { ChevronDownIcon, SparkIcon } from "./icons";

/**
 * Model chip + picker. Selecting a model sets it for the active chat and as
 * the default; models with reasoning variants offer a second step to choose
 * the effort variant.
 */
export function ModelPicker() {
  const models = useProviders((state) => state.models);
  const setModel = useChat((state) => state.setModel);
  const session = useChat((state) =>
    state.sessions.find((item) => item.id === state.activeId),
  );
  const applyRemote = useSettings((state) => state.applyRemote);
  const refreshModels = useProviders((state) => state.refresh);

  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [variantTarget, setVariantTarget] = useState<ModelEntry | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  const current = findModel(models, session?.providerId, session?.modelId);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: MouseEvent) => {
      if (!containerRef.current?.contains(event.target as Node)) {
        setOpen(false);
        setVariantTarget(null);
      }
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpen(false);
        setVariantTarget(null);
      }
    };
    window.addEventListener("mousedown", onPointerDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onPointerDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const grouped = useMemo(() => {
    const needle = query.trim().toLowerCase();
    const filtered = models.filter((entry) => {
      if (!entry.enabled) return false;
      if (!needle) return true;
      return (
        entry.modelId.toLowerCase().includes(needle) ||
        entry.providerName.toLowerCase().includes(needle)
      );
    });
    const map = new Map<string, ModelEntry[]>();
    for (const entry of filtered) {
      const list = map.get(entry.providerId) ?? [];
      list.push(entry);
      map.set(entry.providerId, list);
    }
    for (const list of map.values()) {
      // Favourites first, then alphabetical.
      list.sort((left, right) => {
        if (left.spec.favorite !== right.spec.favorite) {
          return left.spec.favorite ? -1 : 1;
        }
        return left.modelId.localeCompare(right.modelId);
      });
    }
    return [...map.entries()];
  }, [models, query]);

  const choose = async (entry: ModelEntry) => {
    if (entry.spec.reasoning?.variants.length) {
      setVariantTarget(entry);
      return;
    }
    await setModel(modelRef(entry));
    setOpen(false);
  };

  const chooseVariant = async (entry: ModelEntry, variant: string | null) => {
    await setModel(modelRef(entry), variant);
    setVariantTarget(null);
    setOpen(false);
  };

  const toggleFavorite = async (entry: ModelEntry) => {
    const updated = await ipc.setModelFavorite(
      entry.providerId,
      entry.modelId,
      !entry.spec.favorite,
    );
    if (updated) applyRemote(updated);
    await refreshModels();
  };

  const label = current
    ? shortModelName(current.providerName, current.modelId)
    : "Select model";

  return (
    <div className="relative" ref={containerRef}>
      <button
        type="button"
        onClick={() => {
          setOpen((value) => !value);
          setVariantTarget(null);
          setQuery("");
        }}
        title={current ? `${current.providerName} · ${current.modelId}` : "Choose a model"}
        className={cn(
          "flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[12.5px] transition",
          current
            ? "border-[var(--glass-border)] text-soft hover:text-[var(--ink)]"
            : "border-[var(--accent)] text-[var(--ink)]",
        )}
      >
        <SparkIcon size={14} />
        <span className="max-w-[180px] truncate">{label}</span>
        <ChevronDownIcon size={13} />
      </button>

      {open && (
        <div className="panel-strong absolute bottom-full left-0 z-40 mb-2 w-[420px] overflow-hidden rounded-2xl">
          {variantTarget ? (
            <div className="p-3">
              <button
                type="button"
                onClick={() => setVariantTarget(null)}
                className="mb-2 text-[12px] text-faint hover:text-[var(--ink)]"
              >
                ← {variantTarget.modelId}
              </button>
              <p className="mb-2 text-[12.5px] text-soft">
                Reasoning effort for this chat
              </p>
              <div className="flex flex-wrap gap-1.5">
                <button
                  type="button"
                  onClick={() => void chooseVariant(variantTarget, null)}
                  className="rounded-full border border-[var(--glass-border)] px-3 py-1 text-[12.5px] text-soft hover:text-[var(--ink)]"
                >
                  Model default
                </button>
                {variantTarget.spec.reasoning?.variants.map((variant) => (
                  <button
                    key={variant}
                    type="button"
                    onClick={() => void chooseVariant(variantTarget, variant)}
                    className="rounded-full border border-[var(--glass-border)] px-3 py-1 text-[12.5px] text-soft hover:text-[var(--ink)]"
                  >
                    {variant}
                  </button>
                ))}
              </div>
            </div>
          ) : (
            <>
              <div className="border-b border-[var(--glass-border)] p-2">
                <input
                  autoFocus
                  value={query}
                  onChange={(event) => setQuery(event.currentTarget.value)}
                  placeholder="Search models…"
                  className="w-full bg-transparent px-2 py-1 text-[13px] text-[var(--ink)] placeholder:text-[var(--ink-faint)]"
                />
              </div>

              <div className="max-h-[320px] overflow-y-auto p-1.5">
                {grouped.length === 0 && (
                  <p className="px-2 py-3 text-[12.5px] text-faint">
                    No models yet. Add a provider in Settings and fetch its
                    models.
                  </p>
                )}

                {grouped.map(([providerId, entries]) => (
                  <div key={providerId} className="mb-1">
                    <p className="px-2 py-1 text-[11px] font-semibold tracking-[0.06em] text-faint uppercase">
                      {entries[0].providerName}
                      {!entries[0].keyReady && entries[0].keyRequired && (
                        <span className="ml-2 normal-case text-[var(--danger)]">
                          no API key
                        </span>
                      )}
                    </p>
                    {entries.map((entry) => (
                      <div
                        key={`${entry.providerId}/${entry.modelId}`}
                        className="hover-surface group flex items-center rounded-xl pr-1"
                      >
                        <button
                          type="button"
                          onClick={() => void choose(entry)}
                          className="flex min-w-0 flex-1 items-center gap-2 px-2 py-1.5 text-left"
                        >
                          <span className="truncate text-[13px]">
                            {shortModelName(entry.providerName, entry.modelId)}
                          </span>
                          <span className="ml-auto flex shrink-0 items-center gap-1.5 text-[11px] text-faint">
                            {entry.spec.inputModalities.includes("image") && (
                              <span className="rounded border border-[var(--glass-border)] px-1">
                                img
                              </span>
                            )}
                            {entry.spec.reasoning && (
                              <span className="rounded border border-[var(--glass-border)] px-1">
                                think
                              </span>
                            )}
                            {compactTokens(entry.spec.context) && (
                              <span>{compactTokens(entry.spec.context)}</span>
                            )}
                          </span>
                        </button>
                        <button
                          type="button"
                          title={entry.spec.favorite ? "Unfavourite" : "Favourite"}
                          aria-label="Toggle favourite"
                          onClick={() => void toggleFavorite(entry)}
                          className={cn(
                            "grid h-7 w-7 shrink-0 place-items-center rounded-lg",
                            entry.spec.favorite
                              ? "text-[var(--accent)]"
                              : "text-faint opacity-0 group-hover:opacity-100 hover:text-[var(--ink)]",
                          )}
                        >
                          {entry.spec.favorite ? "★" : "☆"}
                        </button>
                      </div>
                    ))}
                  </div>
                ))}
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}

function modelRef(entry: ModelEntry): ModelRef {
  return { providerId: entry.providerId, modelId: entry.modelId };
}

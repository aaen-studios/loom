import { useEffect, useMemo, useRef, useState } from "react";
import { cn } from "../lib/cn";
import { compactTokens, metadataSourceLabel, shortModelName } from "../lib/format";
import { ipc } from "../lib/ipc";
import { useMenu } from "../lib/menu";
import type { ModelEntry, ModelRef } from "../types";
import { currentModel, findModel, useProviders } from "../stores/providers";
import { useChat } from "../stores/chat";
import { useSettings } from "../stores/settings";
import { ChevronDownIcon, PlusIcon, SparkIcon } from "./icons";

/**
 * Model chip + picker. Selecting a model sets it for the active chat and as
 * the default; models with reasoning variants offer a second step to choose
 * the effort variant.
 *
 * Providers whose `/models` endpoint fails can still be used: type a model id
 * at the bottom of the list and it is added by hand.
 */
export function ModelPicker() {
  const models = useProviders((state) => state.models);
  const setModel = useChat((state) => state.setModel);
  const session = useChat((state) =>
    state.sessions.find((item) => item.id === state.activeId),
  );
  const providers = useSettings((state) => state.config.providers);
  const chatDefaults = useSettings((state) => state.config.chat);
  const applyRemote = useSettings((state) => state.applyRemote);
  const refreshModels = useProviders((state) => state.refresh);

  const [query, setQuery] = useState("");
  const [variantTarget, setVariantTarget] = useState<ModelEntry | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [draftModel, setDraftModel] = useState("");
  const [draftProvider, setDraftProvider] = useState("");
  const [busy, setBusy] = useState(false);
  const [drop, setDrop] = useState<{ up: boolean; maxHeight: number }>({
    up: true,
    maxHeight: 480,
  });
  const containerRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const { open, setOpen, close } = useMenu("model", containerRef);

  const providerIds = Object.keys(providers);
  const recents = chatDefaults.recentModels
    .map((model) => findModel(models, model.providerId, model.modelId))
    .filter((entry): entry is ModelEntry => !!entry && entry.enabled);
  const current = currentModel(models, session, chatDefaults);
  const variant = session?.variant ?? chatDefaults.variant ?? null;

  // The variant step is transient: leaving the picker forgets it.
  useEffect(() => {
    if (!open) setVariantTarget(null);
  }, [open]);

  useEffect(() => {
    if (!open || draftProvider || providerIds.length === 0) return;
    setDraftProvider(
      session?.providerId && providers[session.providerId]
        ? session.providerId
        : providerIds[0],
    );
  }, [open, draftProvider, providerIds, providers, session?.providerId]);

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

  /** Every async action goes through here so failures are visible. */
  const guard = async (action: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  };

  const choose = (entry: ModelEntry) =>
    guard(async () => {
      if (entry.spec.reasoning?.variants.length) {
        setVariantTarget(entry);
        return;
      }
      await setModel(modelRef(entry));
      close();
    });

  const chooseVariant = (entry: ModelEntry, variant: string | null) =>
    guard(async () => {
      await setModel(modelRef(entry), variant);
      close();
    });

  const toggleFavorite = (entry: ModelEntry) =>
    guard(async () => {
      const updated = await ipc.setModelFavorite(
        entry.providerId,
        entry.modelId,
        !entry.spec.favorite,
      );
      if (updated) applyRemote(updated);
      await refreshModels();
    });

  const addManual = () =>
    guard(async () => {
      const modelId = draftModel.trim();
      if (!modelId || !draftProvider) return;
      const updated = await ipc.addModel(draftProvider, modelId);
      if (updated) applyRemote(updated);
      await refreshModels();
      setDraftModel("");
      setQuery(modelId);
      setError(null);
    });

  const label = current
    ? shortModelName(current.providerName, current.modelId)
    : "Select model";
  const chipLabel = current && variant ? `${label} · ${variant}` : label;

  /** Opens toward whichever side has more room, capped to the window. */
  const toggle = () => {
    if (open) {
      close();
      return;
    }
    const rect = triggerRef.current?.getBoundingClientRect();
    const gap = 12;
    const above = (rect?.top ?? 0) - gap;
    const below = window.innerHeight - (rect?.bottom ?? 0) - gap;
    const up = above > below;
    setDrop({
      up,
      maxHeight: Math.max(240, Math.min(620, (up ? above : below) - 4)),
    });
    setOpen(true);
    setVariantTarget(null);
    setQuery("");
    setError(null);
  };

  return (
    <div className="relative" ref={containerRef}>
      <button
        type="button"
        ref={triggerRef}
        onClick={toggle}
        title={
          current ? `${current.providerName} · ${current.modelId}` : "Choose a model"
        }
        className={cn(
          "flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[12.5px] transition",
          current
            ? "border-[var(--glass-border)] text-soft hover:text-[var(--ink)]"
            : "border-[var(--accent)] text-[var(--ink)]",
        )}
      >
        <SparkIcon size={14} />
        <span className="max-w-[200px] truncate">{chipLabel}</span>
        <ChevronDownIcon size={13} />
      </button>

      {open && (
        <div
          className={cn(
            "panel-strong absolute left-0 z-50 flex w-[420px] max-w-[calc(100vw-2rem)] flex-col overflow-hidden rounded-sheet",
            drop.up ? "bottom-full mb-2" : "top-full mt-2",
          )}
          style={{ maxHeight: drop.maxHeight }}
        >
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
                  disabled={busy}
                  onClick={() => void chooseVariant(variantTarget, null)}
                  className="rounded-full border border-[var(--glass-border)] px-3 py-1 text-[12.5px] text-soft hover:text-[var(--ink)] disabled:opacity-50"
                >
                  Model default
                </button>
                {variantTarget.spec.reasoning?.variants.map((variant) => (
                  <button
                    key={variant}
                    type="button"
                    disabled={busy}
                    onClick={() => void chooseVariant(variantTarget, variant)}
                    className={cn(
                      "rounded-full border px-3 py-1 text-[12.5px] disabled:opacity-50",
                      variant === variant
                        ? "border-[var(--accent)] text-[var(--ink)]"
                        : "border-[var(--glass-border)] text-soft hover:text-[var(--ink)]",
                    )}
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

              <div className="min-h-0 flex-1 overflow-y-auto p-1.5">
                {grouped.length === 0 && (
                  <p className="px-2 py-3 text-[12.5px] leading-5 text-faint">
                    {models.length === 0
                      ? "No models yet. Add a provider in Settings, or type a model id below if you already know it."
                      : "No models match that search."}
                  </p>
                )}

                {/* Recently used models first: no search needed for the usual
                    suspects. */}
                {!query.trim() && recents.length > 0 && (
                  <div className="mb-1">
                    <p className="px-2 py-1 text-[11px] font-semibold tracking-[0.06em] text-faint uppercase">
                      Recent
                    </p>
                    {recents.map((entry) => (
                      <div
                        key={`recent-${entry.providerId}/${entry.modelId}`}
                        className="hover-surface group flex items-center rounded-row pr-1"
                      >
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => void choose(entry)}
                          className="flex min-w-0 flex-1 items-center gap-2 px-2 py-1.5 text-left disabled:opacity-60"
                        >
                          <span
                            className={cn(
                              "truncate text-[13px]",
                              current?.providerId === entry.providerId &&
                                current?.modelId === entry.modelId &&
                                "font-medium text-[var(--accent)]",
                            )}
                          >
                            {shortModelName(entry.providerName, entry.modelId)}
                          </span>
                          <span className="ml-auto shrink-0 text-[11px] text-faint">
                            {entry.providerName}
                          </span>
                        </button>
                      </div>
                    ))}
                  </div>
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
                    {entries.map((entry) => {
                      const context = compactTokens(entry.spec.context);
                      // Empty modalities mean "unknown", not "text only": say
                      // so instead of rendering nothing.
                      const unknown = [
                        entry.spec.inputModalities.length === 0
                          ? "input types"
                          : null,
                        context ? null : "context window",
                      ].filter((bit): bit is string => !!bit);
                      return (
                      <div
                        key={`${entry.providerId}/${entry.modelId}`}
                        className="hover-surface group flex items-center rounded-row pr-1"
                      >
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => void choose(entry)}
                          className="flex min-w-0 flex-1 items-center gap-2 px-2 py-1.5 text-left disabled:opacity-60"
                        >
                          <span
                            className={cn(
                              "truncate text-[13px]",
                              current?.providerId === entry.providerId &&
                                current?.modelId === entry.modelId &&
                                "font-medium text-[var(--accent)]",
                            )}
                          >
                            {shortModelName(entry.providerName, entry.modelId)}
                            {current?.providerId === entry.providerId &&
                              current?.modelId === entry.modelId &&
                              " ✓"}
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
                            {context && <span>{context}</span>}
                            {unknown.length > 0 && (
                              <span
                                className="cursor-help"
                                title={`${unknown.join(" and ")} unknown — ${metadataSourceLabel(entry.spec.source)}`}
                              >
                                —
                              </span>
                            )}
                          </span>
                        </button>
                        <button
                          type="button"
                          title={entry.spec.favorite ? "Unfavourite" : "Favourite"}
                          aria-label="Toggle favourite"
                          onClick={() => void toggleFavorite(entry)}
                          className={cn(
                            "grid h-7 w-7 shrink-0 place-items-center rounded-control",
                            entry.spec.favorite
                              ? "text-[var(--accent)]"
                              : "text-faint opacity-0 group-hover:opacity-100 hover:text-[var(--ink)]",
                          )}
                        >
                          {entry.spec.favorite ? "★" : "☆"}
                        </button>
                      </div>
                      );
                    })}
                  </div>
                ))}
              </div>

              {providerIds.length > 0 && (
                <div className="border-t border-[var(--glass-border)] p-2">
                  <p className="mb-1.5 px-1 text-[11px] tracking-[0.06em] text-faint uppercase">
                    Add a model id
                  </p>
                  <div className="flex items-center gap-1.5">
                    <select
                      value={draftProvider}
                      onChange={(event) => setDraftProvider(event.currentTarget.value)}
                      className="min-w-0 rounded-control border border-[var(--glass-border)] bg-[var(--hover-bg)] px-1.5 py-1 text-[12px]"
                    >
                      {providerIds.map((id) => (
                        <option key={id} value={id}>
                          {providers[id].name || id}
                        </option>
                      ))}
                    </select>
                    <input
                      value={draftModel}
                      placeholder="model id"
                      onChange={(event) => setDraftModel(event.currentTarget.value)}
                      onKeyDown={(event) => {
                        if (event.key === "Enter") {
                          event.preventDefault();
                          void addManual();
                        }
                      }}
                      className="min-w-0 flex-1 rounded-control border border-[var(--glass-border)] bg-[var(--hover-bg)] px-2 py-1 text-[12px] placeholder:text-[var(--ink-faint)]"
                    />
                    <button
                      type="button"
                      disabled={busy || !draftModel.trim()}
                      onClick={() => void addManual()}
                      aria-label="Add model"
                      className="grid h-7 w-7 shrink-0 place-items-center rounded-control border border-[var(--glass-border)] text-soft disabled:opacity-40"
                    >
                      <PlusIcon size={14} />
                    </button>
                  </div>
                </div>
              )}

              {error && (
                <p className="border-t border-[var(--glass-border)] px-3 py-2 text-[12px] leading-5 text-[var(--danger)]">
                  {error}
                </p>
              )}
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

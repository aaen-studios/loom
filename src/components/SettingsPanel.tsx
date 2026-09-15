import { useEffect, useState, type ReactNode } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { cn } from "../lib/cn";
import { BACKGROUND_PRESETS } from "../lib/background";
import { call, isTauri, tryCall } from "../lib/tauri";
import { ipc } from "../lib/ipc";
import type {
  AppInfo,
  ModelEntry,
  ModelSpec,
  Persona,
  PermissionMode,
  ProviderConfig,
  ProviderKind,
  ProviderPreset,
  Theme,
} from "../types";
import { useProviders } from "../stores/providers";
import { useSettings } from "../stores/settings";
import { useUi } from "../stores/ui";
import {
  CheckIcon,
  ChevronDownIcon,
  CloseIcon,
  KeyIcon,
  MoonIcon,
  PlusIcon,
  RefreshIcon,
  SunIcon,
  TrashIcon,
} from "./icons";

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="border-b border-[var(--glass-border)] px-4 py-4 last:border-b-0">
      <h3 className="mb-3 text-[11.5px] font-semibold tracking-[0.08em] text-faint uppercase">
        {title}
      </h3>
      {children}
    </section>
  );
}

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4 py-1.5">
      <span className="shrink-0 text-[13px] text-soft">{label}</span>
      {children}
    </div>
  );
}

const inputClass =
  "w-full rounded-row border border-[var(--glass-border)] bg-[var(--hover-bg)] px-2.5 py-1.5 text-[13px] text-[var(--ink)] placeholder:text-[var(--ink-faint)] focus:border-[var(--accent)]";

const THEME_OPTIONS: { id: Theme; label: string; icon: ReactNode }[] = [
  { id: "light", label: "Light", icon: <SunIcon size={15} /> },
  { id: "dark", label: "Dark", icon: <MoonIcon size={15} /> },
];



function ModelMetaList({
  providerId,
  models,
}: {
  providerId: string;
  models: Record<string, ModelSpec>;
}) {
  const applyRemote = useSettings((state) => state.applyRemote);
  const refreshModels = useProviders((state) => state.refresh);
  const entries = Object.entries(models);

  if (entries.length === 0) return null;

  const save = async (
    modelId: string,
    spec: ModelSpec,
    context: string,
    output: string,
  ) => {
    const nextContext = context.trim() ? Number(context) : null;
    const nextOutput = output.trim() ? Number(output) : null;
    if (nextContext === spec.context && nextOutput === spec.output) return;

    const updated = await ipc.setModelSpec(
      providerId,
      modelId,
      nextContext,
      nextOutput,
    );
    if (updated) applyRemote(updated);
    await refreshModels();
  };

  return (
    <details className="mt-2">
      <summary className="cursor-pointer text-[12px] text-faint hover:text-[var(--ink)]">
        Models ({entries.length}) — edit context windows
      </summary>
      <div className="mt-1.5 max-h-56 space-y-1 overflow-y-auto pr-0.5">
        {entries.map(([modelId, spec]) => (
          <div key={modelId} className="flex items-center gap-1.5">
            <span className="min-w-0 flex-1 truncate font-mono text-[11.5px] text-soft">
              {modelId}
            </span>
            <input
              defaultValue={spec.context ?? ""}
              placeholder="ctx"
              title="Context window (tokens)"
              onBlur={(event) =>
                void save(modelId, spec, event.currentTarget.value, String(spec.output ?? ""))
              }
              className="w-20 rounded-control border border-[var(--glass-border)] bg-[var(--hover-bg)] px-1.5 py-0.5 text-[11.5px]"
            />
            <input
              defaultValue={spec.output ?? ""}
              placeholder="out"
              title="Max output (tokens)"
              onBlur={(event) =>
                void save(modelId, spec, String(spec.context ?? ""), event.currentTarget.value)
              }
              className="w-20 rounded-control border border-[var(--glass-border)] bg-[var(--hover-bg)] px-1.5 py-0.5 text-[11.5px]"
            />
          </div>
        ))}
      </div>
    </details>
  );
}

/**
 * Settings categories. Each section registers itself here so the search box
 * can find a setting by name ("hotkey", "thinking", "theme") rather than
 * making you hunt through tabs.
 */
export const SETTINGS_CATEGORIES = [
  { id: "general", label: "General", keywords: "notification toast hotkey shortcut density compact scroll" },
  { id: "appearance", label: "Appearance", keywords: "theme dark light background wallpaper dim blur" },
  { id: "chat", label: "Chat", keywords: "thinking reasoning effort send key enter title token models context" },
  { id: "tools", label: "Tools", keywords: "permission ask auto approve workspace index search" },
  { id: "providers", label: "Providers", keywords: "api key base url openai anthropic opencode ollama model" },
  { id: "personas", label: "Personas", keywords: "system prompt role character" },
  { id: "mcp", label: "MCP", keywords: "server tools stdio external" },
  { id: "skills", label: "Skills", keywords: "markdown slash prompts snippets" },
  { id: "data", label: "Data", keywords: "folder database files version" },
  { id: "updates", label: "Updates", keywords: "version release download restart" },
] as const;

export type SettingsCategoryId = (typeof SETTINGS_CATEGORIES)[number]["id"];

/** Small switch used across the sections. */
function Toggle({
  checked,
  onChange,
  label,
  hint,
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
  label: string;
  hint?: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      onClick={() => onChange(!checked)}
      className="flex w-full items-start justify-between gap-4 rounded-control px-1 py-1.5 text-left"
    >
      <span className="min-w-0">
        <span className="block text-[13px] text-soft">{label}</span>
        {hint && <span className="block text-[11.5px] leading-4 text-faint">{hint}</span>}
      </span>
      <span
        className={cn(
          "mt-0.5 flex h-5 w-9 shrink-0 items-center rounded-capsule border p-0.5 transition",
          checked
            ? "justify-end border-[var(--accent)] bg-[var(--accent-soft)]"
            : "justify-start border-[var(--glass-border)]",
        )}
      >
        <span
          className={cn(
            "h-3.5 w-3.5 rounded-capsule transition",
            checked ? "bg-[var(--accent)]" : "bg-[var(--ink-faint)]",
          )}
        />
      </span>
    </button>
  );
}

/** Two or three mutually exclusive options, e.g. thinking display. */
function Segmented<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { id: T; label: string; title?: string }[];
  onChange: (value: T) => void;
}) {
  return (
    <div className="flex rounded-capsule border border-[var(--glass-border)] p-0.5">
      {options.map((option) => (
        <button
          key={option.id}
          type="button"
          title={option.title}
          onClick={() => onChange(option.id)}
          className={cn(
            "rounded-capsule px-2.5 py-1 text-[12px] transition",
            value === option.id
              ? "bg-[var(--control-bg)] text-[var(--control-ink)]"
              : "text-soft hover:text-[var(--ink)]",
          )}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

// ------------------------------------------------------------------ sections

// ---------------------------------------------------------------------------
// Providers
// ---------------------------------------------------------------------------

interface ProviderFormState {
  id: string | null;
  preset: string | null;
  name: string;
  kind: ProviderKind;
  baseUrl: string;
  apiKey: string;
  keyRequired: boolean;
  sessionHeader: string;
}

const EMPTY_FORM: ProviderFormState = {
  id: null,
  preset: null,
  name: "",
  kind: "openai-compatible",
  baseUrl: "",
  apiKey: "",
  keyRequired: true,
  sessionHeader: "",
};

function ProvidersSection() {
  const config = useSettings((state) => state.config);
  const applyRemote = useSettings((state) => state.applyRemote);
  const presets = useProviders((state) => state.presets);
  const refreshModels = useProviders((state) => state.refresh);

  const [form, setForm] = useState<ProviderFormState | null>(null);
  const [keyDrafts, setKeyDrafts] = useState<Record<string, string>>({});
  const [keyStatus, setKeyStatus] = useState<Record<string, boolean>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isTauri) return;
    void (async () => {
      const ids = Object.keys(config.providers);
      const entries = await Promise.all(
        ids.map(async (id) => [id, (await tryCall<boolean>("provider_key_status", { id })) ?? false] as const),
      );
      setKeyStatus(Object.fromEntries(entries));
    })();
  }, [config.providers]);

  const startAdd = (preset: ProviderPreset | null) => {
    setNotice(null);
    setError(null);
    setForm(
      preset
        ? {
            id: preset.id,
            preset: preset.id,
            name: preset.name,
            kind: preset.kind,
            baseUrl: preset.baseUrl,
            apiKey: "",
            keyRequired: preset.keyRequired,
            sessionHeader: preset.sessionHeader ?? "",
          }
        : { ...EMPTY_FORM },
    );
  };

  const startEdit = (id: string) => {
    const provider = config.providers[id];
    setNotice(null);
    setError(null);
    setForm({
      id,
      preset: null,
      name: provider.name,
      kind: provider.kind,
      baseUrl: provider.baseUrl,
      apiKey: "",
      keyRequired: provider.keyRequired,
      sessionHeader: provider.sessionHeader ?? "",
    });
  };

  const save = async () => {
    if (!form) return;
    const name = form.name.trim();
    const baseUrl = form.baseUrl.trim();
    if (!name || !baseUrl) {
      setError("Name and base URL are required.");
      return;
    }

    const id = form.id ?? slugify(name);
    setBusy(id);
    setError(null);
    setNotice(null);
    try {
      const provider: ProviderConfig = {
        name,
        kind: form.kind,
        baseUrl,
        headers: config.providers[id]?.headers ?? {},
        enabled: config.providers[id]?.enabled ?? true,
        models: config.providers[id]?.models ?? {},
        modelsSource: config.providers[id]?.modelsSource ?? "manual",
        lastFetchedAt: config.providers[id]?.lastFetchedAt ?? null,
        keyRequired: form.keyRequired,
        sessionHeader: form.sessionHeader.trim() || null,
      };

      const saved = await ipc.upsertProvider(id, provider);
      if (saved) applyRemote(saved);

      if (form.apiKey.trim()) {
        await ipc.setProviderKey(id, form.apiKey.trim());
        setKeyStatus((state) => ({ ...state, [id]: true }));
      }
      if (!form.keyRequired || form.apiKey.trim() || keyStatus[id]) {
        await fetchModels(id);
      }
      setForm(null);
    } catch (cause) {
      setError(messageOf(cause));
    } finally {
      setBusy(null);
    }
  };

  const fetchModels = async (id: string) => {
    setBusy(id);
    setError(null);
    setNotice(null);
    try {
      const updated = await ipc.refreshProviderModels(id);
      if (updated) applyRemote(updated);
      await refreshModels();
      const count = Object.keys(updated?.providers[id]?.models ?? {}).length;
      setNotice(`Fetched ${count} model${count === 1 ? "" : "s"}.`);
    } catch (cause) {
      setError(messageOf(cause));
    } finally {
      setBusy(null);
    }
  };

  const remove = async (id: string) => {
    setBusy(id);
    try {
      const updated = await ipc.deleteProvider(id);
      if (updated) applyRemote(updated);
      await refreshModels();
    } catch (cause) {
      setError(messageOf(cause));
    } finally {
      setBusy(null);
    }
  };

  const saveKey = async (id: string) => {
    const key = (keyDrafts[id] ?? "").trim();
    if (!key) return;
    setBusy(id);
    try {
      await ipc.setProviderKey(id, key);
      setKeyStatus((state) => ({ ...state, [id]: true }));
      setKeyDrafts((state) => ({ ...state, [id]: "" }));
    } catch (cause) {
      setError(messageOf(cause));
    } finally {
      setBusy(null);
    }
  };

  const providerIds = Object.keys(config.providers);

  return (
    <Section title="Providers">
      {providerIds.length === 0 && !form && (
        <p className="mb-3 text-[12.5px] leading-5 text-faint">
          Add a provider to start chatting. Local providers (Ollama, LM Studio)
          need no key.
        </p>
      )}

      <div className="space-y-2">
        {providerIds.map((id) => {
          const provider = config.providers[id];
          const hasKey = keyStatus[id] ?? provider.keyRequired === false;
          return (
            <div
              key={id}
              className="rounded-row border border-[var(--glass-border)] p-2.5"
            >
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  title={provider.enabled ? "Disable" : "Enable"}
                  onClick={() =>
                    void ipc
                      .setProviderEnabled(id, !provider.enabled)
                      .then((updated) => {
                        if (updated) applyRemote(updated);
                        return refreshModels();
                      })
                  }
                  className={cn(
                    "grid h-5 w-5 shrink-0 place-items-center rounded-md border",
                    provider.enabled
                      ? "border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]"
                      : "border-[var(--glass-border)] text-transparent",
                  )}
                >
                  <CheckIcon size={12} />
                </button>

                <div className="min-w-0 flex-1">
                  <p className="truncate text-[13.5px]">{provider.name}</p>
                  <p className="truncate text-[11.5px] text-faint">
                    {provider.kind === "anthropic" ? "Anthropic" : "OpenAI-compatible"} ·{" "}
                    {Object.keys(provider.models).length} models
                    {provider.sessionHeader ? ` · ${provider.sessionHeader}` : ""}
                  </p>
                </div>

                <button
                  type="button"
                  title="Fetch models"
                  onClick={() => void fetchModels(id)}
                  disabled={busy === id}
                  className="grid h-7 w-7 place-items-center rounded-control text-faint hover:text-[var(--ink)]"
                >
                  <RefreshIcon size={15} className={busy === id ? "animate-spin" : ""} />
                </button>
                <button
                  type="button"
                  title="Edit"
                  onClick={() => startEdit(id)}
                  className="grid h-7 w-7 place-items-center rounded-control text-faint hover:text-[var(--ink)]"
                >
                  <ChevronDownIcon size={15} />
                </button>
                <button
                  type="button"
                  title="Delete provider"
                  onClick={() => void remove(id)}
                  className="grid h-7 w-7 place-items-center rounded-control text-faint hover:text-[var(--danger)]"
                >
                  <TrashIcon size={15} />
                </button>
              </div>

              {provider.keyRequired && (
                <div className="mt-2 flex items-center gap-1.5">
                  <KeyIcon
                    size={14}
                    className={hasKey ? "text-[var(--accent)]" : "text-faint"}
                  />
                  <input
                    type="password"
                    value={keyDrafts[id] ?? ""}
                    placeholder={hasKey ? "API key saved — replace…" : "API key"}
                    onChange={(event) =>
                      setKeyDrafts((state) => ({
                        ...state,
                        [id]: event.currentTarget.value,
                      }))
                    }
                    onKeyDown={(event) => {
                      if (event.key === "Enter") void saveKey(id);
                    }}
                    className={inputClass}
                  />
                  <button
                    type="button"
                    onClick={() => void saveKey(id)}
                    className="shrink-0 rounded-control border border-[var(--glass-border)] px-2 py-1 text-[12px] text-soft"
                  >
                    Save
                  </button>
                </div>
              )}

              <ModelMetaList providerId={id} models={provider.models} />
            </div>
          );
        })}
      </div>

      {form ? (
        <div className="mt-3 rounded-row border border-[var(--glass-border)] p-3">
          <div className="mb-2 flex items-center justify-between">
            <p className="text-[13px] font-medium">
              {form.id && config.providers[form.id] ? "Edit provider" : "New provider"}
            </p>
            <button
              type="button"
              onClick={() => setForm(null)}
              className="text-faint hover:text-[var(--ink)]"
            >
              <CloseIcon size={15} />
            </button>
          </div>

          <div className="space-y-2">
            <input
              value={form.name}
              placeholder="Name"
              onChange={(event) =>
                setForm({ ...form, name: event.currentTarget.value })
              }
              className={inputClass}
            />
            <div className="flex gap-2">
              <select
                value={form.kind}
                onChange={(event) =>
                  setForm({ ...form, kind: event.currentTarget.value as ProviderKind })
                }
                className={inputClass}
              >
                <option value="openai-compatible">OpenAI-compatible</option>
                <option value="anthropic">Anthropic</option>
              </select>
            </div>
            <input
              value={form.baseUrl}
              placeholder="Base URL (e.g. https://api.openai.com/v1)"
              onChange={(event) =>
                setForm({ ...form, baseUrl: event.currentTarget.value })
              }
              className={inputClass}
            />
            <input
              type="password"
              value={form.apiKey}
              placeholder={form.keyRequired ? "API key" : "API key (optional)"}
              onChange={(event) =>
                setForm({ ...form, apiKey: event.currentTarget.value })
              }
              className={inputClass}
            />
            <input
              value={form.sessionHeader}
              placeholder="Session header (e.g. x-opencode-session)"
              title="Some gateways require a stable per-chat id in a header"
              onChange={(event) =>
                setForm({ ...form, sessionHeader: event.currentTarget.value })
              }
              className={inputClass}
            />
            <label className="flex items-center gap-2 text-[12.5px] text-soft">
              <input
                type="checkbox"
                checked={form.keyRequired}
                onChange={(event) =>
                  setForm({ ...form, keyRequired: event.currentTarget.checked })
                }
              />
              Requires an API key
            </label>
          </div>

          <button
            type="button"
            onClick={() => void save()}
            disabled={busy !== null}
            className="mt-3 w-full rounded-row bg-[var(--control-bg)] px-3 py-2 text-[13px] font-medium text-[var(--control-ink)] disabled:opacity-50"
          >
            {busy ? "Working…" : "Save & test connection"}
          </button>
        </div>
      ) : (
        <div className="mt-3">
          <p className="mb-1.5 text-[12px] text-faint">Add provider</p>
          <div className="flex flex-wrap gap-1.5">
            {presets.map((preset) => (
              <button
                key={preset.id}
                type="button"
                title={preset.note}
                onClick={() => startAdd(preset)}
                className="rounded-full border border-[var(--glass-border)] px-2.5 py-1 text-[12px] text-soft hover:text-[var(--ink)]"
              >
                {preset.name}
              </button>
            ))}
            <button
              type="button"
              onClick={() => startAdd(null)}
              className="flex items-center gap-1 rounded-full border border-[var(--glass-border)] px-2.5 py-1 text-[12px] text-soft hover:text-[var(--ink)]"
            >
              <PlusIcon size={13} />
              Custom
            </button>
          </div>
        </div>
      )}

      {notice && (
        <p className="mt-2 text-[12px] text-[var(--accent)]">{notice}</p>
      )}
      {error && <p className="mt-2 text-[12px] text-[var(--danger)]">{error}</p>}
    </Section>
  );
}

// ---------------------------------------------------------------------------
// Personas
// ---------------------------------------------------------------------------

function PersonasSection() {
  const config = useSettings((state) => state.config);
  const applyRemote = useSettings((state) => state.applyRemote);
  const [editing, setEditing] = useState<Persona | null>(null);

  const save = async () => {
    if (!editing) return;
    const name = editing.name.trim();
    if (!name) return;
    const updated = await ipc.upsertPersona({
      ...editing,
      id: editing.id || crypto.randomUUID(),
      name,
    });
    if (updated) applyRemote(updated);
    setEditing(null);
  };

  const remove = async (id: string) => {
    const updated = await ipc.deletePersona(id);
    if (updated) applyRemote(updated);
  };

  return (
    <Section title="Personas">
      <div className="space-y-1.5">
        {config.personas.map((persona) => (
          <div
            key={persona.id}
            className="flex items-center gap-2 rounded-row border border-[var(--glass-border)] px-2.5 py-1.5"
          >
            <span className="min-w-0 flex-1 truncate text-[13px]">
              {persona.name}
            </span>
            <button
              type="button"
              onClick={() => setEditing(persona)}
              className="text-[12px] text-faint hover:text-[var(--ink)]"
            >
              Edit
            </button>
            <button
              type="button"
              onClick={() => void remove(persona.id)}
              className="text-faint hover:text-[var(--danger)]"
            >
              <TrashIcon size={14} />
            </button>
          </div>
        ))}
      </div>

      {editing ? (
        <div className="mt-2 space-y-2 rounded-row border border-[var(--glass-border)] p-2.5">
          <input
            value={editing.name}
            placeholder="Persona name"
            onChange={(event) =>
              setEditing({ ...editing, name: event.currentTarget.value })
            }
            className={inputClass}
          />
          <textarea
            value={editing.systemPrompt}
            placeholder="System prompt"
            rows={4}
            onChange={(event) =>
              setEditing({ ...editing, systemPrompt: event.currentTarget.value })
            }
            className={cn(inputClass, "resize-none")}
          />
          <div className="flex gap-2">
            <button
              type="button"
              onClick={() => void save()}
              className="flex-1 rounded-row bg-[var(--control-bg)] px-3 py-1.5 text-[12.5px] font-medium text-[var(--control-ink)]"
            >
              Save
            </button>
            <button
              type="button"
              onClick={() => setEditing(null)}
              className="rounded-row border border-[var(--glass-border)] px-3 py-1.5 text-[12.5px] text-soft"
            >
              Cancel
            </button>
          </div>
        </div>
      ) : (
        <button
          type="button"
          onClick={() =>
            setEditing({
              id: "",
              name: "",
              systemPrompt: "",
              modelRef: null,
              variant: null,
            })
          }
          className="mt-2 flex items-center gap-1.5 rounded-full border border-[var(--glass-border)] px-2.5 py-1 text-[12px] text-soft hover:text-[var(--ink)]"
        >
          <PlusIcon size={13} />
          New persona
        </button>
      )}
    </Section>
  );
}

// ---------------------------------------------------------------------------
// Chat defaults
// ---------------------------------------------------------------------------

function ChatSection() {
  const config = useSettings((state) => state.config);
  const applyRemote = useSettings((state) => state.applyRemote);
  const models = useProviders((state) => state.models);

  const saveInterface = async (patch: Partial<typeof config.interface>) => {
    const updated = await ipc.setInterfaceSettings({ ...config.interface, ...patch });
    if (updated) applyRemote(updated);
  };

  const setChatSettings = async (args: {
    permissionMode?: PermissionMode;
    historyLimit?: number;
    maxOutputTokens?: number;
  }) => {
    const updated = await ipc.setChatSettings(args);
    if (updated) applyRemote(updated);
  };

  const setLite = async (value: string) => {
    const [providerId, ...rest] = value.split("|");
    const modelId = rest.join("|");
    const updated = await ipc.setDefaultModel({
      providerId: config.chat.providerId,
      modelId: config.chat.modelId,
      variant: config.chat.variant,
      liteProviderId: value ? providerId : null,
      liteModelId: value ? modelId : null,
    });
    if (updated) applyRemote(updated);
  };

  const liteValue = config.chat.lite
    ? `${config.chat.lite.providerId}|${config.chat.lite.modelId}`
    : "";

  return (
    <Section title="Chat">
      <Row label="Model thinking">
        <Segmented
          value={config.interface.showThinking}
          options={[
            { id: "collapsed", label: "Collapsed", title: "A header you expand when you want it" },
            { id: "hidden", label: "Hidden", title: "Never rendered unless opened from the message actions" },
            { id: "expanded", label: "Expanded", title: "Open from the start" },
          ]}
          onChange={(value) => void saveInterface({ showThinking: value })}
        />
      </Row>

      <Row label="Send with">
        <Segmented
          value={config.interface.sendKey}
          options={[
            { id: "enter", label: "Enter" },
            { id: "ctrl-enter", label: "Ctrl+Enter" },
          ]}
          onChange={(value) => void saveInterface({ sendKey: value })}
        />
      </Row>

      <Row label="Auto-title chats">
        <button
          type="button"
          role="switch"
          aria-checked={config.chat.autoTitle}
          onClick={() =>
            void ipc
              .setChatSettings({ autoTitle: !config.chat.autoTitle })
              .then((updated) => {
                if (updated) applyRemote(updated);
              })
          }
          className={cn(
            "flex h-5 w-9 items-center rounded-capsule border p-0.5 transition",
            config.chat.autoTitle
              ? "justify-end border-[var(--accent)] bg-[var(--accent-soft)]"
              : "justify-start border-[var(--glass-border)]",
          )}
        >
          <span
            className={cn(
              "h-3.5 w-3.5 rounded-capsule",
              config.chat.autoTitle ? "bg-[var(--accent)]" : "bg-[var(--ink-faint)]",
            )}
          />
        </button>
      </Row>

      <Row label="Titles model">
        <select
          value={liteValue}
          onChange={(event) => void setLite(event.currentTarget.value)}
          className={cn(inputClass, "w-44")}
        >
          <option value="">Same as chat</option>
          {models.map((entry: ModelEntry) => (
            <option
              key={`${entry.providerId}|${entry.modelId}`}
              value={`${entry.providerId}|${entry.modelId}`}
            >
              {entry.providerName} · {entry.modelId}
            </option>
          ))}
        </select>
      </Row>

      <Row label="History limit">
        <input
          type="number"
          min={2}
          max={500}
          value={config.chat.historyLimit}
          onChange={(event) =>
            void setChatSettings({ historyLimit: Number(event.currentTarget.value) })
          }
          className={cn(inputClass, "w-24")}
        />
      </Row>

      <Row label="Max output">
        <input
          type="number"
          min={256}
          max={200000}
          step={256}
          value={config.chat.maxOutputTokens}
          onChange={(event) =>
            void setChatSettings({
              maxOutputTokens: Number(event.currentTarget.value),
            })
          }
          className={cn(inputClass, "w-28")}
        />
      </Row>



      <Row label="Image model">
        <input
          defaultValue={config.chat.imageModel ?? ""}
          placeholder="gpt-image-1"
          onBlur={(event) =>
            void ipc.setImageModel(event.currentTarget.value).then((updated) => {
              if (updated) applyRemote(updated);
            })
          }
          className={cn(inputClass, "w-44")}
        />
      </Row>

      <Row label="Embedding model">
        <input
          defaultValue={config.chat.embeddingModel ?? ""}
          placeholder="text-embedding-3-small"
          title="Used by the workspace index and semantic search"
          onBlur={(event) =>
            void ipc
              .setEmbeddingModel(event.currentTarget.value)
              .then((updated) => {
                if (updated) applyRemote(updated);
              })
          }
          className={cn(inputClass, "w-44")}
        />
      </Row>
    </Section>
  );
}

// ---------------------------------------------------------------------------
// MCP servers
// ---------------------------------------------------------------------------

function McpSection() {
  const config = useSettings((state) => state.config);
  const applyRemote = useSettings((state) => state.applyRemote);
  const [form, setForm] = useState<{
    id: string;
    name: string;
    command: string;
    args: string;
    env: string;
  } | null>(null);
  const [tools, setTools] = useState<{ server: string; modelName: string }[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const save = async () => {
    if (!form) return;
    setBusy(true);
    setError(null);
    try {
      const env: Record<string, string> = {};
      for (const line of form.env.split("\n")) {
        const [key, ...rest] = line.split("=");
        if (key?.trim() && rest.length) env[key.trim()] = rest.join("=").trim();
      }
      const updated = await ipc.upsertMcpServer(form.id.trim() || slugify(form.name), {
        name: form.name.trim(),
        command: form.command.trim(),
        args: form.args.split(/\s+/).filter(Boolean),
        env,
        enabled: true,
      });
      if (updated) applyRemote(updated);
      setTools(await ipc.mcpTools());
      setForm(null);
    } catch (cause) {
      setError(messageOf(cause));
    } finally {
      setBusy(false);
    }
  };

  const discover = async () => {
    setBusy(true);
    setError(null);
    try {
      setTools(await ipc.mcpTools());
    } catch (cause) {
      setError(messageOf(cause));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Section title="MCP servers">
      <div className="space-y-1.5">
        {Object.entries(config.mcpServers ?? {}).map(([id, server]) => (
          <div
            key={id}
            className="flex items-center gap-2 rounded-row border border-[var(--glass-border)] px-2.5 py-1.5"
          >
            <span className="min-w-0 flex-1">
              <span className="block truncate text-[13px]">{server.name || id}</span>
              <span className="block truncate font-mono text-[11px] text-faint">
                {server.command} {server.args.join(" ")}
              </span>
            </span>
            <button
              type="button"
              onClick={() =>
                void ipc.deleteMcpServer(id).then((updated) => {
                  if (updated) applyRemote(updated);
                  setTools(null);
                })
              }
              className="text-faint hover:text-[var(--danger)]"
            >
              <TrashIcon size={14} />
            </button>
          </div>
        ))}
      </div>

      {form ? (
        <div className="mt-2 space-y-2 rounded-row border border-[var(--glass-border)] p-2.5">
          <input
            value={form.id}
            placeholder="Server id (e.g. filesystem)"
            onChange={(event) => setForm({ ...form, id: event.currentTarget.value })}
            className={inputClass}
          />
          <input
            value={form.name}
            placeholder="Display name"
            onChange={(event) => setForm({ ...form, name: event.currentTarget.value })}
            className={inputClass}
          />
          <input
            value={form.command}
            placeholder="Command (e.g. npx)"
            onChange={(event) =>
              setForm({ ...form, command: event.currentTarget.value })
            }
            className={inputClass}
          />
          <input
            value={form.args}
            placeholder="Arguments (space separated)"
            onChange={(event) => setForm({ ...form, args: event.currentTarget.value })}
            className={inputClass}
          />
          <textarea
            value={form.env}
            placeholder={"Environment variables, one KEY=value per line"}
            rows={2}
            onChange={(event) => setForm({ ...form, env: event.currentTarget.value })}
            className={cn(inputClass, "resize-none font-mono text-[11.5px]")}
          />
          <div className="flex gap-2">
            <button
              type="button"
              disabled={busy}
              onClick={() => void save()}
              className="flex-1 rounded-row bg-[var(--control-bg)] px-3 py-1.5 text-[12.5px] font-medium text-[var(--control-ink)] disabled:opacity-50"
            >
              {busy ? "Connecting…" : "Save & connect"}
            </button>
            <button
              type="button"
              onClick={() => setForm(null)}
              className="rounded-row border border-[var(--glass-border)] px-3 py-1.5 text-[12.5px] text-soft"
            >
              Cancel
            </button>
          </div>
        </div>
      ) : (
        <button
          type="button"
          onClick={() =>
            setForm({ id: "", name: "", command: "", args: "", env: "" })
          }
          className="mt-2 flex items-center gap-1.5 rounded-full border border-[var(--glass-border)] px-2.5 py-1 text-[12px] text-soft hover:text-[var(--ink)]"
        >
          <PlusIcon size={13} />
          Add MCP server
        </button>
      )}

      <div className="mt-2 flex items-center gap-2">
        <button
          type="button"
          onClick={() => void discover()}
          disabled={busy}
          className="rounded-full border border-[var(--glass-border)] px-2.5 py-1 text-[12px] text-soft hover:text-[var(--ink)]"
        >
          Discover tools
        </button>
        {tools && (
          <span className="text-[11.5px] text-faint">
            {tools.length} tool{tools.length === 1 ? "" : "s"}
          </span>
        )}
      </div>

      {tools && tools.length > 0 && (
        <div className="mt-2 flex flex-wrap gap-1">
          {tools.map((tool) => (
            <span
              key={tool.modelName}
              className="rounded-full border border-[var(--glass-border)] px-2 py-0.5 font-mono text-[10.5px] text-faint"
            >
              {tool.server}:{tool.modelName.split("__").pop()}
            </span>
          ))}
        </div>
      )}

      {error && <p className="mt-2 text-[12px] text-[var(--danger)]">{error}</p>}
    </Section>
  );
}

// ---------------------------------------------------------------------------
// Skills
// ---------------------------------------------------------------------------

function SkillsSection() {
  const [skills, setSkills] = useState<
    { id: string; name: string; description: string; path: string }[]
  >([]);

  useEffect(() => {
    void ipc.listSkills().then((result) => setSkills(result ?? []));
  }, []);

  return (
    <Section title="Skills">
      {skills.length === 0 ? (
        <p className="text-[12.5px] leading-5 text-faint">
          Drop markdown files into <span className="font-mono">~/.loom/skills/</span>{" "}
          and they appear here and in the composer&apos;s <span className="font-mono">/</span>{" "}
          menu.
        </p>
      ) : (
        <div className="space-y-1.5">
          {skills.map((skill) => (
            <div
              key={skill.id}
              className="rounded-row border border-[var(--glass-border)] px-2.5 py-1.5"
            >
              <p className="text-[13px]">
                <span className="font-mono text-[12px] text-faint">/{skill.id}</span>{" "}
                {skill.name}
              </p>
              {skill.description && (
                <p className="text-[11.5px] text-faint">{skill.description}</p>
              )}
            </div>
          ))}
        </div>
      )}
    </Section>
  );
}

// ---------------------------------------------------------------------------
// Updates
// ---------------------------------------------------------------------------

function UpdatesSection({ version }: { version: string | undefined }) {
  const [status, setStatus] = useState<string | null>(null);
  const [manifest, setManifest] = useState<
    { version: string; notes: string } | null
  >(null);
  const [staged, setStaged] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const check = async () => {
    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      const result = await ipc.checkForUpdates();
      if (!result) {
        setStatus("Update checks need the packaged app.");
      } else if (result.available && result.manifest) {
        setManifest(result.manifest);
        setStatus(`Version ${result.manifest.version} is available.`);
      } else {
        setStatus(`You are on the latest version (${result.currentVersion}).`);
      }
    } catch (cause) {
      setError(messageOf(cause));
    } finally {
      setBusy(false);
    }
  };

  const download = async () => {
    if (!manifest) return;
    setBusy(true);
    setError(null);
    try {
      const result = await ipc.checkForUpdates();
      if (!result?.manifest) return;
      const path = await ipc.downloadUpdate(result.manifest);
      setStaged(path);
      setStatus("Downloaded and verified. Restart to apply.");
    } catch (cause) {
      setError(messageOf(cause));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Section title="Updates">
      <Row label="Installed">
        <span className="text-[13px] text-soft">{version ?? "—"}</span>
      </Row>
      <div className="mt-1.5 flex flex-wrap gap-1.5">
        <button
          type="button"
          onClick={() => void check()}
          disabled={busy}
          className="rounded-full border border-[var(--glass-border)] px-2.5 py-1 text-[12px] text-soft hover:text-[var(--ink)] disabled:opacity-50"
        >
          {busy ? "Working…" : "Check for updates"}
        </button>
        {manifest && !staged && (
          <button
            type="button"
            onClick={() => void download()}
            disabled={busy}
            className="rounded-full bg-[var(--control-bg)] px-2.5 py-1 text-[12px] font-medium text-[var(--control-ink)] disabled:opacity-50"
          >
            Download {manifest.version}
          </button>
        )}
        {staged && (
          <button
            type="button"
            onClick={() => void ipc.applyUpdate(staged)}
            className="rounded-full bg-[var(--control-ink)] px-2.5 py-1 text-[12px] font-medium text-[var(--control-bg)]"
          >
            Restart &amp; install
          </button>
        )}
      </div>
      {status && <p className="mt-2 text-[12px] text-[var(--accent)]">{status}</p>}
      {manifest?.notes && (
        <p className="mt-2 whitespace-pre-wrap text-[12px] text-soft">
          {manifest.notes}
        </p>
      )}
      {error && <p className="mt-2 text-[12px] text-[var(--danger)]">{error}</p>}
    </Section>
  );
}


function GeneralSection() {
  const config = useSettings((state) => state.config);
  const applyRemote = useSettings((state) => state.applyRemote);
  const [hotkeyDraft, setHotkeyDraft] = useState(config.interface.hotkey);
  const [hotkeyNote, setHotkeyNote] = useState<string | null>(null);

  const saveInterface = async (patch: Partial<typeof config.interface>) => {
    const updated = await ipc.setInterfaceSettings({ ...config.interface, ...patch });
    if (updated) applyRemote(updated);
  };

  const applyHotkey = async () => {
    try {
      await ipc.setHotkey(config.interface.hotkeyEnabled, hotkeyDraft);
      await saveInterface({ hotkey: hotkeyDraft });
      setHotkeyNote("Hotkey active.");
    } catch (error) {
      setHotkeyNote(error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <Section title="General">
      <Toggle
        label="Notify when a reply finishes"
        hint="Windows toast when the window is not focused."
        checked={config.interface.notifyOnCompletion}
        onChange={(value) => void saveInterface({ notifyOnCompletion: value })}
      />
      <Toggle
        label="Quick-ask overlay hotkey"
        hint="Ctrl+Shift+Space summons the overlay from anywhere in Windows."
        checked={config.interface.hotkeyEnabled}
        onChange={(value) => void saveInterface({ hotkeyEnabled: value })}
      />
      {config.interface.hotkeyEnabled && (
        <div className="flex items-center gap-1.5 pt-1">
          <input
            value={hotkeyDraft}
            onChange={(event) => setHotkeyDraft(event.currentTarget.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void applyHotkey();
            }}
            placeholder="Ctrl+Shift+Space"
            className={inputClass}
          />
          <button
            type="button"
            onClick={() => void applyHotkey()}
            className="shrink-0 rounded-control border border-[var(--glass-border)] px-2.5 py-1.5 text-[12px] text-soft"
          >
            Apply
          </button>
        </div>
      )}
      {hotkeyNote && <p className="pt-1 text-[12px] text-faint">{hotkeyNote}</p>}

      <div className="mt-2">
        <Toggle
          label="Always follow new text"
          hint="Keep the transcript pinned even while you read older messages."
          checked={config.interface.alwaysFollow}
          onChange={(value) => void saveInterface({ alwaysFollow: value })}
        />
        <Toggle
          label="Compact density"
          hint="Tighter spacing and smaller text for long sessions."
          checked={config.interface.compact}
          onChange={(value) => void saveInterface({ compact: value })}
        />
      </div>
    </Section>
  );
}

// ------------------------------------------------------------------ sections

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

function AppearanceSection({ info }: { info: AppInfo | null }) {
  const config = useSettings((state) => state.config);
  const setTheme = useSettings((state) => state.setTheme);
  const setBackground = useSettings((state) => state.setBackground);
  const applyRemote = useSettings((state) => state.applyRemote);

  const pickBackground = async (kind: "image" | "video") => {
    if (!isTauri) return;
    const filters =
      kind === "image"
        ? [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif", "bmp"] }]
        : [{ name: "Video", extensions: ["mp4", "webm", "mkv", "mov"] }];
    const picked = await openDialog({ multiple: false, filters });
    if (!picked || typeof picked !== "string") return;
    const updated = await ipc.setBackgroundFile(kind, picked);
    if (updated) applyRemote(updated);
  };

  return (
    <>
      <Section title="Appearance">
        <Row label="Theme">
          <div className="flex rounded-capsule border border-[var(--glass-border)] p-0.5">
            {THEME_OPTIONS.map((option) => (
              <button
                key={option.id}
                type="button"
                onClick={() => setTheme(option.id)}
                className={cn(
                  "flex items-center gap-1.5 rounded-capsule px-3 py-1 text-[12.5px] transition",
                  config.theme === option.id
                    ? "bg-[var(--control-bg)] text-[var(--control-ink)]"
                    : "text-soft hover:text-[var(--ink)]",
                )}
              >
                {option.icon}
                {option.label}
              </button>
            ))}
          </div>
        </Row>

        <div className="mt-3 grid grid-cols-3 gap-2">
          {BACKGROUND_PRESETS.map((preset) => (
            <button
              key={preset.id}
              type="button"
              title={preset.name}
              onClick={() =>
                setBackground({ kind: "builtin", preset: preset.id, path: null })
              }
              className={cn(
                "group relative h-16 overflow-hidden rounded-row border transition",
                config.background.preset === preset.id &&
                  config.background.kind === "builtin"
                  ? "border-[var(--accent)] ring-2 ring-[var(--accent-soft)]"
                  : "border-[var(--glass-border)] hover:border-[var(--ink-faint)]",
              )}
              style={{ background: preset.swatch }}
            >
              <span className="absolute inset-x-0 bottom-0 bg-black/25 py-0.5 text-[10.5px] text-white/90 opacity-0 transition group-hover:opacity-100">
                {preset.name}
              </span>
            </button>
          ))}
        </div>

        <div className="mt-3 flex flex-wrap gap-1.5">
          <button
            type="button"
            onClick={() => void pickBackground("image")}
            className="rounded-capsule border border-[var(--glass-border)] px-2.5 py-1 text-[12px] text-soft hover:text-[var(--ink)]"
          >
            Choose image…
          </button>
          <button
            type="button"
            onClick={() => void pickBackground("video")}
            className="rounded-capsule border border-[var(--glass-border)] px-2.5 py-1 text-[12px] text-soft hover:text-[var(--ink)]"
          >
            Choose video…
          </button>
          {config.background.kind !== "builtin" && (
            <button
              type="button"
              onClick={() => setBackground({ kind: "builtin", path: null })}
              className="rounded-capsule border border-[var(--glass-border)] px-2.5 py-1 text-[12px] text-faint hover:text-[var(--danger)]"
            >
              Remove custom
            </button>
          )}
        </div>

        {config.background.kind !== "builtin" && config.background.path && (
          <p
            className="mt-2 truncate font-mono text-[11px] text-faint"
            title={config.background.path}
          >
            {config.background.path}
          </p>
        )}

        <div className="mt-3">
          <Row label="Dim">
            <input
              type="range"
              min={0}
              max={100}
              value={config.background.dim}
              onChange={(event) =>
                setBackground({ dim: Number(event.currentTarget.value) })
              }
              className="w-40"
            />
          </Row>
          <Row label="Blur">
            <input
              type="range"
              min={0}
              max={64}
              value={config.background.blur}
              onChange={(event) =>
                setBackground({ blur: Number(event.currentTarget.value) })
              }
              className="w-40"
            />
          </Row>
        </div>
      </Section>

      <Section title="Data">
        <Row label="App version">
          <span className="text-[13px] text-soft">{info?.version ?? "—"}</span>
        </Row>
        <div className="pt-1.5">
          <p className="text-[13px] text-soft">Data folder</p>
          <p
            className="mt-1 truncate font-mono text-[11.5px] text-faint"
            title={info?.loomHome ?? ""}
          >
            {info?.loomHome ?? "—"}
          </p>
        </div>
      </Section>
    </>
  );
}

export function SettingsPanel() {
  const open = useUi((state) => state.settingsOpen);
  const setOpen = useUi((state) => state.setSettingsOpen);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [category, setCategory] = useState<SettingsCategoryId>("general");
  const [query, setQuery] = useState("");

  useEffect(() => {
    if (open && !info) {
      void call<AppInfo>("app_info").then((result) => {
        if (result) setInfo(result);
      });
    }
  }, [open, info]);

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, setOpen]);

  if (!open) return null;

  const needle = query.trim().toLowerCase();
  const matches = (id: SettingsCategoryId) => {
    if (!needle) return id === category;
    const entry = SETTINGS_CATEGORIES.find((item) => item.id === id);
    return (
      (entry?.label.toLowerCase().includes(needle) ?? false) ||
      (entry?.keywords.includes(needle) ?? false)
    );
  };
  const searching = needle.length > 0;

  return (
    <div className="absolute inset-0 z-40 flex justify-end p-3 pt-16">
      <button
        type="button"
        aria-label="Close settings"
        onClick={() => setOpen(false)}
        className="absolute inset-0 cursor-default bg-black/10"
      />

      <div className="animate-fade-up panel-strong relative flex h-full w-[620px] flex-col overflow-hidden rounded-sheet">
        <div className="flex items-center gap-3 px-4 py-3">
          <h2 className="text-[14.5px] font-semibold">Settings</h2>
          <input
            value={query}
            onChange={(event) => setQuery(event.currentTarget.value)}
            placeholder="Search settings…"
            className="min-w-0 flex-1 rounded-control border border-[var(--glass-border)] bg-[var(--hover-bg)] px-2.5 py-1.5 text-[12.5px] placeholder:text-[var(--ink-faint)]"
          />
          <button
            type="button"
            aria-label="Close settings"
            onClick={() => setOpen(false)}
            className="hover-surface grid h-8 w-8 place-items-center rounded-control text-soft"
          >
            <CloseIcon size={16} />
          </button>
        </div>

        <div className="flex min-h-0 flex-1">
          <nav className="w-[160px] shrink-0 overflow-y-auto border-r border-[var(--glass-border)] p-2">
            {SETTINGS_CATEGORIES.map((entry) => (
              <button
                key={entry.id}
                type="button"
                onClick={() => {
                  setCategory(entry.id);
                  setQuery("");
                }}
                className={cn(
                  "hover-surface w-full rounded-row px-2.5 py-1.5 text-left text-[13px]",
                  entry.id === category && !searching
                    ? "bg-[var(--hover-bg)] text-[var(--ink)]"
                    : "text-soft",
                  searching && matches(entry.id) && "text-[var(--accent)]",
                )}
              >
                {entry.label}
              </button>
            ))}
          </nav>

          <div className="min-h-0 flex-1 overflow-y-auto">
            {searching ? (
              <>
                {SETTINGS_CATEGORIES.filter((entry) => matches(entry.id)).map(
                  (entry) => (
                    <p
                      key={entry.id}
                      className="px-4 pt-3 text-[11.5px] tracking-[0.06em] text-faint uppercase"
                    >
                      {entry.label}
                    </p>
                  ),
                )}
                <p className="px-4 py-6 text-[12.5px] text-faint">
                  {SETTINGS_CATEGORIES.some((entry) => matches(entry.id))
                      ? "Open the highlighted category to change it."
                      : "Nothing matches your search."}
                </p>
              </>
            ) : (
              <>
                {category === "general" && <GeneralSection />}
                {category === "appearance" && <AppearanceSection info={info} />}
                {category === "chat" && <ChatSection />}
                {category === "tools" && <ToolsSection />}
                {category === "providers" && <ProvidersSection />}
                {category === "personas" && <PersonasSection />}
                {category === "mcp" && <McpSection />}
                {category === "skills" && <SkillsSection />}
                {category === "data" && <AppearanceSection info={info} />}
                {category === "updates" && <UpdatesSection version={info?.version} />}
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

function slugify(name: string): string {
  const slug = name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return `${slug || "provider"}-${Math.random().toString(36).slice(2, 6)}`;
}

function messageOf(error: unknown): string {
  if (error instanceof Error) return error.message;
  return String(error);
}

/** Tool permissions and the tools the model can actually call. */
function ToolsSection() {
  const config = useSettings((state) => state.config);
  const applyRemote = useSettings((state) => state.applyRemote);
  const [tools, setTools] = useState<{ name: string; description: string; readOnly: boolean }[]>([]);

  useEffect(() => {
    void ipc.listTools().then((result) => setTools(result ?? []));
  }, []);

  const setMode = async (mode: PermissionMode) => {
    const updated = await ipc.setChatSettings({ permissionMode: mode });
    if (updated) applyRemote(updated);
  };

  return (
    <>
      <Section title="Permissions">
        <Row label="Default mode">
          <Segmented
            value={config.chat.permissionMode}
            options={[
              { id: "ask", label: "Ask", title: "Confirm every tool call" },
              { id: "auto-read-only", label: "Auto read", title: "Read-only tools run silently" },
              { id: "auto-all", label: "Auto all", title: "Run every tool without asking" },
            ]}
            onChange={(value) => void setMode(value)}
          />
        </Row>
        <p className="pt-1.5 text-[12px] leading-5 text-faint">
          Each chat can override this from the composer. Write-file and command
          tools always ask unless the mode is “Auto all”.
        </p>
      </Section>

      <Section title="Tools">
        <div className="space-y-1.5">
          {tools.map((tool) => (
            <div key={tool.name} className="rounded-row border border-[var(--glass-border)] px-2.5 py-1.5">
              <p className="font-mono text-[12px] text-soft">
                {tool.name}
                {tool.readOnly && (
                  <span className="ml-2 rounded-capsule border border-[var(--glass-border)] px-1.5 py-0.5 text-[10px] text-faint">
                    read-only
                  </span>
                )}
              </p>
              <p className="text-[11.5px] leading-4 text-faint">{tool.description}</p>
            </div>
          ))}
          {tools.length === 0 && (
            <p className="text-[12.5px] text-faint">No tools reported.</p>
          )}
        </div>
      </Section>
    </>
  );
}
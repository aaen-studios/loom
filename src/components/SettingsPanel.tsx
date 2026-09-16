import { useEffect, useRef, useState, type ComponentType } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { cn } from "../lib/cn";
import { BACKGROUND_PRESETS } from "../lib/background";
import { metadataSourceLabel, formatReset, compactTokens } from "../lib/format";
import { GLOBAL_PERMISSION_MODES } from "../lib/modes";
import type { SettingsCategoryId } from "../lib/settingsCategories";
import { call, isTauri, tryCall } from "../lib/tauri";
import { metricSummary, metricTone, percentOf } from "../lib/usage";
import { ipc } from "../lib/ipc";
import type {
  AgentMode,
  AppInfo,
  MemoryEntry,
  ModelEntry,
  Modality,
  ReasoningSpec,
  StorageUsage,
  ModelSpec,
  Persona,
  PermissionMode,
  ProviderConfig,
  ProviderKind,
  ProviderPreset,
  SearchProvider,
  StoredMemory,
  ToolScope,
  UsageMetric,
} from "../types";
import { useProviders } from "../stores/providers";
import { useSettings } from "../stores/settings";
import { useSkills } from "../stores/skills";
import { useUi } from "../stores/ui";
import { useUsage } from "../stores/usage";
import { VoiceSettings } from "./VoiceSettings";
import {
  BrainIcon,
  CheckIcon,
  ChevronDownIcon,
  CloseIcon,
  DatabaseIcon,
  EditIcon,
  GaugeIcon,
  KeyIcon,
  MessageIcon,
  MoonIcon,
  PaletteIcon,
  PersonIcon,
  PlugIcon,
  PlusIcon,
  RefreshIcon,
  SearchIcon,
  ServerIcon,
  SettingsIcon,
  SoundIcon,
  SparkIcon,
  SunIcon,
  TrashIcon,
  WrenchIcon,
} from "./icons";
import {
  EmptyState,
  IconButton,
  Row,
  SearchField,
  Section,
  Segmented,
  Toggle,
  fieldBase,
  inputClass,
} from "./ui";

function ModelMetaList({
  providerId,
  models,
}: {
  providerId: string;
  models: Record<string, ModelSpec>;
}) {
  const applyRemote = useSettings((state) => state.applyRemote);
  const refreshModels = useProviders((state) => state.refresh);
  const [error, setError] = useState<string | null>(null);
  const entries = Object.entries(models);

  if (entries.length === 0) return null;

  const save = async (modelId: string, spec: ModelSpec, edit: ModelEdit) => {
    const sameModalities =
      [...edit.inputModalities].sort().join() ===
      [...spec.inputModalities].sort().join();
    const sameReasoning =
      JSON.stringify(edit.reasoning) === JSON.stringify(spec.reasoning);
    if (
      edit.context === spec.context &&
      edit.output === spec.output &&
      sameModalities &&
      sameReasoning
    ) {
      return;
    }

    try {
      const updated = await ipc.setModelSpec(
        providerId,
        modelId,
        edit.context,
        edit.output,
        edit.inputModalities,
        edit.reasoning,
      );
      if (updated) applyRemote(updated);
      await refreshModels();
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  const reset = async (modelId: string) => {
    try {
      const updated = await ipc.resetModelSpec(providerId, modelId);
      if (updated) applyRemote(updated);
      await refreshModels();
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  return (
    <details className="mt-2">
      <summary className="cursor-pointer text-[12px] text-faint hover:text-[var(--ink)]">
        Models ({entries.length}) — edit windows and capabilities
      </summary>
      <div className="mt-1.5 max-h-72 space-y-1 overflow-y-auto pr-0.5">
        {entries.map(([modelId, spec]) => (
          <ModelMetaRow
            // Remount when the stored spec changes from under the editor
            // (a refresh, a reset) so the inputs show the new truth.
            key={`${modelId}:${spec.source}:${spec.context ?? "-"}:${spec.output ?? "-"}:${spec.inputModalities.join("+")}:${spec.reasoning?.variants.join("+") ?? "-"}:${spec.reasoning?.defaultVariant ?? "-"}`}
            modelId={modelId}
            spec={spec}
            onSave={(edit) => void save(modelId, spec, edit)}
            onReset={() => void reset(modelId)}
          />
        ))}
      </div>
      {error && (
        <p className="mt-1 text-[11.5px] leading-4 text-[var(--danger)]">{error}</p>
      )}
    </details>
  );
}

interface ModelEdit {
  context: number | null;
  output: number | null;
  inputModalities: Modality[];
  reasoning: ReasoningSpec | null;
}

const MODALITY_CHOICES: { id: Modality; label: string }[] = [
  { id: "text", label: "txt" },
  { id: "image", label: "img" },
  { id: "pdf", label: "pdf" },
  { id: "audio", label: "audio" },
  { id: "video", label: "video" },
];

const DEFAULT_REASONING: ReasoningSpec = {
  enabled: true,
  variants: ["off", "on"],
  defaultVariant: "on",
};

function tokenCount(text: string): number | null {
  const trimmed = text.trim();
  if (!trimmed) return null;
  const value = Number(trimmed);
  if (!Number.isFinite(value) || value < 0) return null;
  return Math.floor(value);
}

function ModelMetaRow({
  modelId,
  spec,
  onSave,
  onReset,
}: {
  modelId: string;
  spec: ModelSpec;
  onSave: (edit: ModelEdit) => void;
  onReset: () => void;
}) {
  const [context, setContext] = useState(
    spec.context == null ? "" : String(spec.context),
  );
  const [output, setOutput] = useState(
    spec.output == null ? "" : String(spec.output),
  );
  const [reasoning, setReasoning] = useState<ReasoningSpec | null>(
    spec.reasoning,
  );
  const [variants, setVariants] = useState(
    spec.reasoning ? spec.reasoning.variants.join(", ") : "",
  );

  const edit = (overrides: Partial<ModelEdit>): ModelEdit => ({
    context: tokenCount(context),
    output: tokenCount(output),
    inputModalities: spec.inputModalities,
    reasoning,
    ...overrides,
  });

  const toggleModality = (id: Modality) => {
    const next = spec.inputModalities.includes(id)
      ? spec.inputModalities.filter((entry) => entry !== id)
      : [...spec.inputModalities, id];
    onSave(edit({ inputModalities: next }));
  };

  const toggleReasoning = () => {
    const next = reasoning ? null : DEFAULT_REASONING;
    setReasoning(next);
    setVariants(next ? next.variants.join(", ") : "");
    onSave(edit({ reasoning: next }));
  };

  const commitVariants = (text: string) => {
    setVariants(text);
    if (!reasoning) return;
    const parsed = text
      .split(",")
      .map((value) => value.trim())
      .filter(Boolean);
    const list = parsed.length ? parsed : DEFAULT_REASONING.variants;
    const next: ReasoningSpec = {
      enabled: true,
      variants: list,
      defaultVariant:
        reasoning.defaultVariant && list.includes(reasoning.defaultVariant)
          ? reasoning.defaultVariant
          : list[0],
    };
    setReasoning(next);
    onSave(edit({ reasoning: next }));
  };

  return (
    <div className="rounded-row border border-[var(--glass-border)] px-2 py-1.5">
      <div className="flex items-center gap-1.5">
        <span className="min-w-0 flex-1 truncate font-mono text-[11.5px] text-soft">
          {modelId}
        </span>
        <span
          className="shrink-0 text-[10.5px] text-faint"
          title={`Metadata source: ${metadataSourceLabel(spec.source)}`}
        >
          {spec.source}
        </span>
        <button
          type="button"
          title="Reset to detected"
          aria-label="Reset to detected"
          onClick={onReset}
          className="grid h-6 w-6 shrink-0 place-items-center rounded-control text-faint hover:text-[var(--ink)]"
        >
          ↺
        </button>
      </div>
      <div className="mt-1 flex flex-wrap items-center gap-1">
        <input
          value={context}
          placeholder="ctx"
          title="Context window (tokens); empty means unknown"
          onChange={(event) => setContext(event.currentTarget.value)}
          onBlur={(event) => {
            const next = event.currentTarget.value;
            setContext(next);
            onSave(edit({ context: tokenCount(next) }));
          }}
          className="w-20 rounded-control border border-[var(--glass-border)] bg-[var(--hover-bg)] px-1.5 py-0.5 text-[11.5px]"
        />
        <input
          value={output}
          placeholder="out"
          title="Max output (tokens); empty means unknown"
          onChange={(event) => setOutput(event.currentTarget.value)}
          onBlur={(event) => {
            const next = event.currentTarget.value;
            setOutput(next);
            onSave(edit({ output: tokenCount(next) }));
          }}
          className="w-20 rounded-control border border-[var(--glass-border)] bg-[var(--hover-bg)] px-1.5 py-0.5 text-[11.5px]"
        />
        {MODALITY_CHOICES.map((choice) => (
          <button
            key={choice.id}
            type="button"
            title={`Input: ${choice.id}`}
            onClick={() => toggleModality(choice.id)}
            className={cn(
              "rounded-full border px-1.5 py-0.5 text-[10.5px]",
              spec.inputModalities.includes(choice.id)
                ? "border-[var(--accent)] text-[var(--ink)]"
                : "border-[var(--glass-border)] text-faint hover:text-[var(--ink)]",
            )}
          >
            {choice.label}
          </button>
        ))}
        <button
          type="button"
          title="Reasoning support"
          onClick={toggleReasoning}
          className={cn(
            "rounded-full border px-1.5 py-0.5 text-[10.5px]",
            reasoning
              ? "border-[var(--accent)] text-[var(--ink)]"
              : "border-[var(--glass-border)] text-faint hover:text-[var(--ink)]",
          )}
        >
          think
        </button>
        {reasoning && (
          <input
            value={variants}
            placeholder="off, on"
            title="Reasoning variants, comma-separated"
            onChange={(event) => setVariants(event.currentTarget.value)}
            onBlur={(event) => commitVariants(event.currentTarget.value)}
            className="w-32 rounded-control border border-[var(--glass-border)] bg-[var(--hover-bg)] px-1.5 py-0.5 text-[11.5px]"
          />
        )}
      </div>
    </div>
  );
}

/**
 * Settings categories. Each section registers itself here so the search box
 * can find a setting by name ("hotkey", "thinking", "theme") rather than
 * making you hunt through tabs.
 */
export const SETTINGS_CATEGORIES = [
  {
    id: "general",
    label: "General",
    blurb: "Notifications, the quick-ask hotkey, and how the app reads.",
    keywords: "notification toast hotkey shortcut keyboard density compact scroll follow generated ui html widget sandbox screenshot capture",
  },
  {
    id: "appearance",
    label: "Appearance",
    blurb: "Theme, background art, and how much of it shows through.",
    keywords: "theme dark light mode background wallpaper image video dim blur look",
  },
  {
    id: "chat",
    label: "Chat",
    blurb: "How replies read and how sending works.",
    keywords: "thinking reasoning effort send key enter title auto token models context image embedding",
  },
  {
    id: "tools",
    label: "Tools",
    blurb: "What the model may do on its own, and the tools it gets.",
    keywords: "permission ask auto approve workspace index search jina web duckduckgo fetch reader atelier harness round budget steps agent plan build",
  },
  {
    id: "providers",
    label: "Providers",
    blurb: "Endpoints and API keys. This is where models come from.",
    keywords: "api key base url openai anthropic opencode ollama lm studio groq gemini model provider endpoint key",
  },
  {
    id: "usage",
    label: "Usage",
    blurb: "Subscription limits and what this machine has spent.",
    keywords: "usage limits quota credits balance subscription opencode go zen openrouter deepseek zai glm tokens spend cost budget",
  },
  {
    id: "personas",
    label: "Personas",
    blurb: "System prompts you can switch from the composer.",
    keywords: "system prompt role character personality persona",
  },
  {
    id: "memory",
    label: "Memory",
    blurb: "Durable facts Loom remembers about you and each project.",
    keywords: "long term memory facts remember recall pinned global workspace extract auto",
  },
  {
    id: "voice",
    label: "Voice",
    blurb: "Reading replies aloud, and the voices Loom speaks with.",
    keywords: "voice speech speak read aloud tts kokoro audio sound espeak onnx download component",
  },
  {
    id: "mcp",
    label: "MCP",
    blurb: "External tool servers speaking the Model Context Protocol.",
    keywords: "server tools stdio external mcp command npx environment",
  },
  {
    id: "skills",
    label: "Skills",
    blurb: "Markdown prompts offered from the composer's slash menu.",
    keywords: "markdown slash prompts snippets editor skill atelier harness",
  },
  {
    id: "data",
    label: "Data",
    blurb: "Where everything lives, and how much disk it takes.",
    keywords: "folder database files version storage disk cache cleanup size",
  },
  {
    id: "updates",
    label: "Updates",
    blurb: "Check for, download, and install new versions.",
    keywords: "version release download restart update upgrade",
  },
] as const satisfies readonly {
  id: SettingsCategoryId;
  label: string;
  blurb: string;
  keywords: string;
}[];

/** One glyph per category, shared by the nav rail and the search results. */
const CATEGORY_ICONS: Record<
  SettingsCategoryId,
  ComponentType<{ size?: number; className?: string }>
> = {
  general: SettingsIcon,
  appearance: PaletteIcon,
  chat: MessageIcon,
  tools: WrenchIcon,
  providers: PlugIcon,
  usage: GaugeIcon,
  personas: PersonIcon,
  memory: BrainIcon,
  voice: SoundIcon,
  mcp: ServerIcon,
  skills: SparkIcon,
  data: DatabaseIcon,
  updates: RefreshIcon,
};

/** The nav rail reads as four small families, not one flat list. */
const NAV_GROUPS: { label: string; ids: SettingsCategoryId[] }[] = [
  { label: "App", ids: ["general", "appearance"] },
  { label: "Model", ids: ["chat", "providers", "usage", "personas", "memory"] },
  { label: "Extensions", ids: ["tools", "mcp", "skills", "voice"] },
  { label: "System", ids: ["data", "updates"] },
];

// ------------------------------------------------------------------ sections

/** Reusable prompt snippets, offered from the composer's slash menu. */
function PromptsEditor() {
  const prompts = useSettings((state) => state.config.prompts);
  const applyRemote = useSettings((state) => state.applyRemote);
  const [editing, setEditing] = useState<{ id: string; title: string; body: string } | null>(null);

  const save = async () => {
    if (!editing) return;
    const updated = await ipc.upsertPrompt({
      id: editing.id,
      title: editing.title.trim(),
      body: editing.body,
    });
    if (updated) applyRemote(updated);
    setEditing(null);
  };

  const remove = async (id: string) => {
    const updated = await ipc.deletePrompt(id);
    if (updated) applyRemote(updated);
  };

  return (
    <Section title="Prompts">
      {prompts.length === 0 && !editing && (
        <p className="px-1 py-1 text-[12.5px] leading-5 text-faint">
          Snippets you use often. They appear in the composer's / menu next to
          your skills.
        </p>
      )}

      {prompts.map((prompt) => (
        <div key={prompt.id} className="flex items-center gap-1 px-1 py-1.5">
          <span className="min-w-0 flex-1">
            <span className="block truncate text-[13px]">{prompt.title}</span>
            <span className="block truncate text-[11.5px] text-faint">
              {prompt.body.slice(0, 60)}
            </span>
          </span>
          <IconButton
            label={`Edit ${prompt.title}`}
            onClick={() => setEditing({ ...prompt })}
          >
            <EditIcon size={14} />
          </IconButton>
          <IconButton
            label={`Delete ${prompt.title}`}
            tone="danger"
            onClick={() => void remove(prompt.id)}
          >
            <TrashIcon size={14} />
          </IconButton>
        </div>
      ))}

      {editing ? (
        <div className="my-2 space-y-2 rounded-row bg-[var(--hover-bg)] p-2.5">
          <input
            value={editing.title}
            placeholder="Title"
            onChange={(event) => setEditing({ ...editing, title: event.currentTarget.value })}
            className={inputClass}
          />
          <textarea
            value={editing.body}
            placeholder="Prompt text"
            rows={4}
            onChange={(event) => setEditing({ ...editing, body: event.currentTarget.value })}
            className={cn(inputClass, "resize-none")}
          />
          <div className="flex gap-2">
            <button
              type="button"
              onClick={() => void save()}
              className="btn-primary flex-1 px-3 py-1.5 text-[12.5px]"
            >
              Save
            </button>
            <button
              type="button"
              onClick={() => setEditing(null)}
              className="btn-ghost px-3 py-1.5 text-[12.5px]"
            >
              Cancel
            </button>
          </div>
        </div>
      ) : (
        <div className="px-1 py-2">
          <button
            type="button"
            onClick={() => setEditing({ id: "", title: "", body: "" })}
            className="chip px-2.5 py-1 text-[12px]"
          >
            <PlusIcon size={13} />
            New prompt
          </button>
        </div>
      )}
    </Section>
  );
}
function DataSection({ info }: { info: AppInfo | null }) {
  return (
    <Section title="Data">
      <Row label="App version">
        <span className="text-[13px] text-soft">{info?.version ?? "—"}</span>
      </Row>
      <div className="px-1 py-2.5">
        <p className="text-[13px] text-soft">Data folder</p>
        <p
          className="mt-1 truncate font-mono text-[11.5px] text-faint select-all"
          title={info?.loomHome ?? ""}
        >
          {info?.loomHome ?? "—"}
        </p>
      </div>
    </Section>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return bytes + " B";
  if (bytes < 1024 * 1024) return Math.round(bytes / 1024) + " KB";
  if (bytes < 1024 * 1024 * 1024) return (bytes / (1024 * 1024)).toFixed(1) + " MB";
  return (bytes / (1024 * 1024 * 1024)).toFixed(2) + " GB";
}

/** Disk usage for the Loom home folder, with the two safe cleanups. */
function StorageSection() {
  const [usage, setUsage] = useState<StorageUsage | null>(null);
  const [note, setNote] = useState<string | null>(null);

  const refresh = () => {
    void ipc.storageUsage().then((result) => {
      if (result) setUsage(result);
    });
  };

  useEffect(refresh, []);

  const act = async (what: "cache" | "generated") => {
    const freed = what === "cache" ? await ipc.clearCache() : await ipc.clearGenerated();
    setNote(freed ? "Freed " + formatBytes(freed) + "." : "Nothing to remove.");
    refresh();
  };

  const rows: { label: string; value: number }[] = usage
    ? [
        { label: "Database", value: usage.database },
        { label: "Attachments", value: usage.attachments },
        { label: "Generated images", value: usage.generated },
        { label: "Backgrounds", value: usage.backgrounds },
        { label: "Update cache", value: usage.cache },
      ]
    : [];

  return (
    <Section title="Storage">
      <Row label="Total">
        <span className="text-[13px] text-soft">
          {usage ? formatBytes(usage.total) : "…"}
        </span>
      </Row>
      <div className="px-1 py-2">
        {rows.map((row) => (
          <div key={row.label} className="flex items-center justify-between py-1">
            <span className="text-[12.5px] text-faint">{row.label}</span>
            <span className="text-[12.5px] text-soft">{formatBytes(row.value)}</span>
          </div>
        ))}
      </div>
      <div className="flex flex-wrap gap-1.5 px-1 py-2.5">
        <button
          type="button"
          onClick={() => void act("cache")}
          className="chip px-2.5 py-1 text-[12px]"
        >
          Clear update cache
        </button>
        <button
          type="button"
          onClick={() => void act("generated")}
          className="chip px-2.5 py-1 text-[12px]"
        >
          Clear generated images
        </button>
      </div>
      {note && (
        <p className="px-1 py-2 text-[12px] text-[var(--accent)]">{note}</p>
      )}
    </Section>
  );
}

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
        <p className="px-1 py-1 text-[12.5px] leading-5 text-faint">
          Add a provider to start chatting. Local providers (Ollama, LM Studio)
          need no key.
        </p>
      )}

      <div className="[&>*+*]:border-t [&>*+*]:border-[var(--glass-border)]">
        {providerIds.map((id) => {
          const provider = config.providers[id];
          const hasKey = keyStatus[id] ?? provider.keyRequired === false;
          return (
            <div key={id} className="px-1 py-2.5">
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

                <IconButton
                  label="Fetch models"
                  onClick={() => void fetchModels(id)}
                  disabled={busy === id}
                >
                  <RefreshIcon size={15} className={busy === id ? "animate-spin" : ""} />
                </IconButton>
                <IconButton label="Edit" onClick={() => startEdit(id)}>
                  <ChevronDownIcon size={15} />
                </IconButton>
                <IconButton
                  label="Delete provider"
                  tone="danger"
                  onClick={() => void remove(id)}
                >
                  <TrashIcon size={15} />
                </IconButton>
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
                    className="btn-ghost shrink-0 px-2 py-1 text-[12px]"
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
        <div className="my-2 rounded-row bg-[var(--hover-bg)] p-3">
          <div className="mb-2 flex items-center justify-between">
            <p className="text-[13px] font-medium">
              {form.id && config.providers[form.id] ? "Edit provider" : "New provider"}
            </p>
            <IconButton label="Close" onClick={() => setForm(null)}>
              <CloseIcon size={15} />
            </IconButton>
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
            className="btn-primary mt-3 w-full px-3 py-2 text-[13px]"
          >
            {busy ? "Working…" : "Save & test connection"}
          </button>
        </div>
      ) : (
        <div className="px-1 py-2.5">
          <p className="mb-2 text-[12px] text-faint">Add provider</p>
          <div className="flex flex-wrap gap-1.5">
            {presets.map((preset) => (
              <button
                key={preset.id}
                type="button"
                title={preset.note}
                onClick={() => startAdd(preset)}
                className="chip px-2.5 py-1 text-[12px]"
              >
                {preset.name}
              </button>
            ))}
            <button
              type="button"
              onClick={() => startAdd(null)}
              className="chip px-2.5 py-1 text-[12px]"
            >
              <PlusIcon size={13} />
              Custom
            </button>
          </div>
        </div>
      )}

      {notice && (
        <p className="px-1 py-2 text-[12px] text-[var(--accent)]">{notice}</p>
      )}
      {error && (
        <p className="px-1 py-2 text-[12px] text-[var(--danger)]">{error}</p>
      )}
    </Section>
  );
}

// ---------------------------------------------------------------------------
// Personas
// ---------------------------------------------------------------------------

function emptyPersona(): Persona {
  return {
    id: "",
    name: "",
    systemPrompt: "",
    modelRef: null,
    variant: null,
    description: "",
    tags: [],
    emoji: null,
    color: null,
    favorite: false,
    greeting: "",
    style: "",
    rules: "",
    outputFormat: "",
    examples: [],
    capabilities: {
      temperature: null,
      topP: null,
      maxOutputTokens: null,
      permissionMode: null,
      agentMode: null,
      tools: [],
      mcpServers: [],
    },
    memory: { enabled: false, tokenBudget: 0 },
    revision: 0,
    updatedAt: 0,
  };
}

function commaList(text: string): string[] {
  return text
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
}

function numberOrNull(text: string): number | null {
  if (text.trim() === "") return null;
  const value = Number(text);
  return Number.isFinite(value) ? value : null;
}

/** Add/remove the facts a persona remembers across chats. */
function PersonaMemoryEditor({ personaId }: { personaId: string }) {
  const [entries, setEntries] = useState<MemoryEntry[]>([]);
  const [key, setKey] = useState("");
  const [value, setValue] = useState("");

  useEffect(() => {
    let live = true;
    void ipc.personaMemory(personaId).then((list) => {
      if (live && list) setEntries(list);
    });
    return () => {
      live = false;
    };
  }, [personaId]);

  const add = async () => {
    if (!key.trim() || !value.trim()) return;
    const list = await ipc.setPersonaMemory(
      personaId,
      key.trim(),
      value.trim(),
      "user",
    );
    if (list) setEntries(list);
    setKey("");
    setValue("");
  };

  const forget = async (id: string) => {
    const list = await ipc.deletePersonaMemory(personaId, id);
    if (list) setEntries(list);
  };

  const clear = async () => {
    const list = await ipc.clearPersonaMemory(personaId);
    if (list) setEntries(list);
  };

  return (
    <div className="space-y-2 rounded-row bg-[var(--hover-bg)] p-2.5">
      <p className="text-[12px] text-faint">
        Facts this persona remembers across every chat. The model can add its own
        with the remember tool when memory is enabled.
      </p>
      {entries.map((entry) => (
        <div key={entry.id} className="flex items-start gap-2">
          <div className="min-w-0 flex-1">
            <p className="truncate text-[12.5px]">{entry.key}</p>
            <p className="text-[12px] leading-4 text-faint">{entry.value}</p>
          </div>
          <span className="text-[11px] text-faint">{entry.source}</span>
          <IconButton
            label={`Forget ${entry.key}`}
            tone="danger"
            onClick={() => void forget(entry.id)}
          >
            <TrashIcon size={13} />
          </IconButton>
        </div>
      ))}
      {entries.length === 0 && (
        <p className="text-[12px] text-faint">Nothing remembered yet.</p>
      )}
      <div className="flex gap-2">
        <input
          value={key}
          placeholder="Key"
          onChange={(event) => setKey(event.currentTarget.value)}
          className={cn(inputClass, "flex-1")}
        />
        <input
          value={value}
          placeholder="Value"
          onChange={(event) => setValue(event.currentTarget.value)}
          className={cn(inputClass, "flex-[2]")}
        />
        <button
          type="button"
          onClick={() => void add()}
          className="chip px-2.5 py-1 text-[12px]"
        >
          <PlusIcon size={13} />
        </button>
      </div>
      {entries.length > 0 && (
        <button
          type="button"
          onClick={() => void clear()}
          className="text-[12px] text-faint hover:text-[var(--ink)]"
        >
          Clear all
        </button>
      )}
    </div>
  );
}

function PersonasSection() {
  const config = useSettings((state) => state.config);
  const applyRemote = useSettings((state) => state.applyRemote);
  const [editing, setEditing] = useState<Persona | null>(null);
  const [aux, setAux] = useState({ tags: "", tools: "", mcp: "" });
  const [memoryFor, setMemoryFor] = useState<string | null>(null);
  const [profile, setProfile] = useState(config.userProfile);
  const [groupName, setGroupName] = useState("");
  const [groupMembers, setGroupMembers] = useState("");
  const [groupCast, setGroupCast] = useState(false);

  const startEdit = (persona: Persona | null) => {
    const draft = persona ?? emptyPersona();
    setEditing(draft);
    setAux({
      tags: draft.tags.join(", "),
      tools: draft.capabilities.tools.join(", "),
      mcp: draft.capabilities.mcpServers.join(", "),
    });
  };

  const save = async () => {
    if (!editing) return;
    const name = editing.name.trim();
    if (!name) return;
    // An empty id means "create": the backend allocates one (and rejects an
    // unknown id, so a client-generated uuid would be an error).
    const persona: Persona = {
      ...editing,
      name,
      tags: commaList(aux.tags),
      capabilities: {
        ...editing.capabilities,
        tools: commaList(aux.tools),
        mcpServers: commaList(aux.mcp),
      },
    };
    const updated = await ipc.upsertPersona(persona);
    if (updated) applyRemote(updated);
    setEditing(null);
  };

  const remove = async (id: string) => {
    const updated = await ipc.deletePersona(id);
    if (updated) applyRemote(updated);
  };

  const saveProfile = async () => {
    const updated = await ipc.setUserProfile(profile);
    if (updated) applyRemote(updated);
  };

  const addGroup = async () => {
    const name = groupName.trim();
    if (!name) return;
    const members = commaList(groupMembers)
      .map(
        (token) =>
          config.personas.find(
            (persona) =>
              persona.id === token ||
              persona.name.toLowerCase() === token.toLowerCase(),
          )?.id,
      )
      .filter((id): id is string => Boolean(id));
    const updated = await ipc.upsertPersonaGroup({
      id: "",
      name,
      members,
      cast: groupCast,
    });
    if (updated) applyRemote(updated);
    setGroupName("");
    setGroupMembers("");
    setGroupCast(false);
  };

  const removeGroup = async (id: string) => {
    const updated = await ipc.deletePersonaGroup(id);
    if (updated) applyRemote(updated);
  };

  const label = (text: string) => (
    <p className="text-[11px] uppercase tracking-wide text-faint">{text}</p>
  );

  return (
    <Section title="Personas">
      <div className="[&>*+*]:border-t [&>*+*]:border-[var(--glass-border)]">
        {config.personas.map((persona) => (
          <div key={persona.id}>
            <div className="flex items-center gap-1 px-1 py-1.5">
              <span className="w-5 shrink-0 text-center text-[14px]">
                {persona.emoji ?? "·"}
              </span>
              <div className="min-w-0 flex-1">
                <p className="truncate text-[13px]">
                  {persona.favorite ? "★ " : ""}
                  {persona.name}
                </p>
                <p className="truncate text-[12px] text-faint">
                  {persona.description ||
                    `${persona.tags.join(", ") || "No description"}${
                      persona.memory.enabled ? " · memory on" : ""
                    }`}
                </p>
              </div>
              {persona.memory.enabled && (
                <IconButton
                  label={`Memory for ${persona.name}`}
                  onClick={() =>
                    setMemoryFor(memoryFor === persona.id ? null : persona.id)
                  }
                >
                  <BrainIcon size={14} />
                </IconButton>
              )}
              <IconButton
                label={`Edit ${persona.name}`}
                onClick={() => startEdit(persona)}
              >
                <EditIcon size={14} />
              </IconButton>
              <IconButton
                label={`Delete ${persona.name}`}
                tone="danger"
                onClick={() => void remove(persona.id)}
              >
                <TrashIcon size={14} />
              </IconButton>
            </div>
            {memoryFor === persona.id && (
              <div className="px-1 pb-2">
                <PersonaMemoryEditor personaId={persona.id} />
              </div>
            )}
          </div>
        ))}
        {config.personas.length === 0 && (
          <p className="px-1 py-1 text-[12.5px] leading-5 text-faint">
            Personas are reusable system prompts with optional model, tool and
            memory settings. Add one and pick it from the composer.
          </p>
        )}
      </div>

      {editing ? (
        <div className="my-2 space-y-3 rounded-row bg-[var(--hover-bg)] p-2.5">
          <div className="flex gap-2">
            <input
              value={editing.emoji ?? ""}
              placeholder="🙂"
              maxLength={2}
              onChange={(event) =>
                setEditing({
                  ...editing,
                  emoji: event.currentTarget.value || null,
                })
              }
              className={cn(inputClass, "w-12 text-center")}
            />
            <input
              value={editing.name}
              placeholder="Persona name"
              onChange={(event) =>
                setEditing({ ...editing, name: event.currentTarget.value })
              }
              className={cn(inputClass, "flex-1")}
            />
            <label className="flex items-center gap-1 text-[12px] text-faint">
              <input
                type="checkbox"
                checked={editing.favorite}
                onChange={(event) =>
                  setEditing({
                    ...editing,
                    favorite: event.currentTarget.checked,
                  })
                }
              />
              Pin
            </label>
          </div>

          <input
            value={editing.description}
            placeholder="One-line description"
            onChange={(event) =>
              setEditing({ ...editing, description: event.currentTarget.value })
            }
            className={inputClass}
          />

          <input
            value={aux.tags}
            placeholder="Tags, comma separated"
            onChange={(event) =>
              setAux({ ...aux, tags: event.currentTarget.value })
            }
            className={inputClass}
          />

          <div>
            {label("System prompt")}
            <textarea
              value={editing.systemPrompt}
              placeholder="System prompt. {{user}}, {{date}}, {{workdir}} and {{model}} are filled at send time."
              rows={5}
              onChange={(event) =>
                setEditing({
                  ...editing,
                  systemPrompt: event.currentTarget.value,
                })
              }
              className={cn(inputClass, "resize-none")}
            />
          </div>

          <div className="grid grid-cols-3 gap-2">
            <div>
              {label("Style")}
              <textarea
                value={editing.style}
                rows={3}
                onChange={(event) =>
                  setEditing({ ...editing, style: event.currentTarget.value })
                }
                className={cn(inputClass, "resize-none")}
              />
            </div>
            <div>
              {label("Rules")}
              <textarea
                value={editing.rules}
                rows={3}
                onChange={(event) =>
                  setEditing({ ...editing, rules: event.currentTarget.value })
                }
                className={cn(inputClass, "resize-none")}
              />
            </div>
            <div>
              {label("Output format")}
              <textarea
                value={editing.outputFormat}
                rows={3}
                onChange={(event) =>
                  setEditing({
                    ...editing,
                    outputFormat: event.currentTarget.value,
                  })
                }
                className={cn(inputClass, "resize-none")}
              />
            </div>
          </div>

          <div>
            {label("Opening message")}
            <textarea
              value={editing.greeting}
              placeholder="Posted as the first assistant turn when a chat starts with this persona"
              rows={2}
              onChange={(event) =>
                setEditing({ ...editing, greeting: event.currentTarget.value })
              }
              className={cn(inputClass, "resize-none")}
            />
          </div>

          <div>
            {label("Examples")}
            <div className="space-y-1.5">
              {editing.examples.map((example, index) => (
                <div key={index} className="flex gap-2">
                  <input
                    value={example.user}
                    placeholder="User"
                    onChange={(event) => {
                      const examples = [...editing.examples];
                      examples[index] = {
                        ...example,
                        user: event.currentTarget.value,
                      };
                      setEditing({ ...editing, examples });
                    }}
                    className={cn(inputClass, "flex-1")}
                  />
                  <input
                    value={example.assistant}
                    placeholder="Assistant"
                    onChange={(event) => {
                      const examples = [...editing.examples];
                      examples[index] = {
                        ...example,
                        assistant: event.currentTarget.value,
                      };
                      setEditing({ ...editing, examples });
                    }}
                    className={cn(inputClass, "flex-1")}
                  />
                  <IconButton
                    label="Remove example"
                    tone="danger"
                    onClick={() =>
                      setEditing({
                        ...editing,
                        examples: editing.examples.filter(
                          (_, item) => item !== index,
                        ),
                      })
                    }
                  >
                    <TrashIcon size={13} />
                  </IconButton>
                </div>
              ))}
              <button
                type="button"
                onClick={() =>
                  setEditing({
                    ...editing,
                    examples: [
                      ...editing.examples,
                      { user: "", assistant: "" },
                    ],
                  })
                }
                className="chip px-2.5 py-1 text-[12px]"
              >
                <PlusIcon size={13} />
                Add example
              </button>
            </div>
          </div>

          <div className="grid grid-cols-3 gap-2">
            <div>
              {label("Provider")}
              <input
                value={editing.modelRef?.providerId ?? ""}
                placeholder="inherit"
                onChange={(event) => {
                  const providerId = event.currentTarget.value;
                  setEditing({
                    ...editing,
                    modelRef: providerId
                      ? {
                          providerId,
                          modelId: editing.modelRef?.modelId ?? "",
                        }
                      : null,
                  });
                }}
                className={inputClass}
              />
            </div>
            <div>
              {label("Model")}
              <input
                value={editing.modelRef?.modelId ?? ""}
                placeholder="inherit"
                onChange={(event) => {
                  const modelId = event.currentTarget.value;
                  setEditing({
                    ...editing,
                    modelRef: modelId
                      ? {
                          providerId: editing.modelRef?.providerId ?? "",
                          modelId,
                        }
                      : null,
                  });
                }}
                className={inputClass}
              />
            </div>
            <div>
              {label("Variant")}
              <input
                value={editing.variant ?? ""}
                placeholder="inherit"
                onChange={(event) =>
                  setEditing({
                    ...editing,
                    variant: event.currentTarget.value || null,
                  })
                }
                className={inputClass}
              />
            </div>
          </div>

          <div className="grid grid-cols-3 gap-2">
            <div>
              {label("Temperature")}
              <input
                value={editing.capabilities.temperature ?? ""}
                placeholder="inherit"
                onChange={(event) =>
                  setEditing({
                    ...editing,
                    capabilities: {
                      ...editing.capabilities,
                      temperature: numberOrNull(event.currentTarget.value),
                    },
                  })
                }
                className={inputClass}
              />
            </div>
            <div>
              {label("Top P")}
              <input
                value={editing.capabilities.topP ?? ""}
                placeholder="inherit"
                onChange={(event) =>
                  setEditing({
                    ...editing,
                    capabilities: {
                      ...editing.capabilities,
                      topP: numberOrNull(event.currentTarget.value),
                    },
                  })
                }
                className={inputClass}
              />
            </div>
            <div>
              {label("Max output tokens")}
              <input
                value={editing.capabilities.maxOutputTokens ?? ""}
                placeholder="inherit"
                onChange={(event) =>
                  setEditing({
                    ...editing,
                    capabilities: {
                      ...editing.capabilities,
                      maxOutputTokens: numberOrNull(event.currentTarget.value),
                    },
                  })
                }
                className={inputClass}
              />
            </div>
          </div>

          <div className="grid grid-cols-2 gap-2">
            <div>
              {label("Permission mode")}
              <select
                value={editing.capabilities.permissionMode ?? ""}
                onChange={(event) =>
                  setEditing({
                    ...editing,
                    capabilities: {
                      ...editing.capabilities,
                      permissionMode: (event.currentTarget.value ||
                        null) as PermissionMode | null,
                    },
                  })
                }
                className={inputClass}
              >
                <option value="">Inherit</option>
                <option value="ask">Ask</option>
                <option value="auto-read-only">Auto read-only</option>
                <option value="auto-all">Auto all</option>
              </select>
            </div>
            <div>
              {label("Agent mode")}
              <select
                value={editing.capabilities.agentMode ?? ""}
                onChange={(event) =>
                  setEditing({
                    ...editing,
                    capabilities: {
                      ...editing.capabilities,
                      agentMode: (event.currentTarget.value ||
                        null) as AgentMode | null,
                    },
                  })
                }
                className={inputClass}
              >
                <option value="">Inherit</option>
                <option value="build">Build</option>
                <option value="plan">Plan</option>
                <option value="review">Review</option>
                <option value="chat">Chat</option>
              </select>
            </div>
          </div>

          <div className="grid grid-cols-2 gap-2">
            <div>
              {label("Allowed tools (empty = all)")}
              <input
                value={aux.tools}
                placeholder="read_file, edit_file, run_command"
                onChange={(event) =>
                  setAux({ ...aux, tools: event.currentTarget.value })
                }
                className={inputClass}
              />
            </div>
            <div>
              {label("Allowed MCP servers (empty = all)")}
              <input
                value={aux.mcp}
                placeholder="filesystem, github"
                onChange={(event) =>
                  setAux({ ...aux, mcp: event.currentTarget.value })
                }
                className={inputClass}
              />
            </div>
          </div>

          <div className="flex items-center gap-3">
            <label className="flex items-center gap-1 text-[12.5px]">
              <input
                type="checkbox"
                checked={editing.memory.enabled}
                onChange={(event) =>
                  setEditing({
                    ...editing,
                    memory: {
                      ...editing.memory,
                      enabled: event.currentTarget.checked,
                    },
                  })
                }
              />
              Persistent memory
            </label>
            <label className="flex items-center gap-1 text-[12px] text-faint">
              Token budget
              <input
                value={editing.memory.tokenBudget || ""}
                placeholder="1200"
                onChange={(event) =>
                  setEditing({
                    ...editing,
                    memory: {
                      ...editing.memory,
                      tokenBudget: numberOrNull(event.currentTarget.value) ?? 0,
                    },
                  })
                }
                className={cn(inputClass, "w-24")}
              />
            </label>
          </div>

          <div className="flex gap-2">
            <button
              type="button"
              onClick={() => void save()}
              className="btn-primary flex-1 px-3 py-1.5 text-[12.5px]"
            >
              Save
            </button>
            <button
              type="button"
              onClick={() => setEditing(null)}
              className="btn-ghost px-3 py-1.5 text-[12.5px]"
            >
              Cancel
            </button>
          </div>
        </div>
      ) : (
        <div className="px-1 py-2">
          <button
            type="button"
            onClick={() => startEdit(null)}
            className="chip px-2.5 py-1 text-[12px]"
          >
            <PlusIcon size={13} />
            New persona
          </button>
        </div>
      )}

      <div className="mt-3 space-y-2 [&>*+*]:border-t [&>*+*]:border-[var(--glass-border)]">
        {label("Groups")}
        {config.personaGroups.map((group) => (
          <div key={group.id} className="flex items-center gap-2 px-1 py-1.5">
            <div className="min-w-0 flex-1">
              <p className="truncate text-[13px]">
                {group.name}
                {group.cast ? " · cast" : ""}
              </p>
              <p className="truncate text-[12px] text-faint">
                {group.members
                  .map(
                    (id) =>
                      config.personas.find((persona) => persona.id === id)
                        ?.name ?? id,
                  )
                  .join(", ") || "No members"}
              </p>
            </div>
            <IconButton
              label={`Delete ${group.name}`}
              tone="danger"
              onClick={() => void removeGroup(group.id)}
            >
              <TrashIcon size={14} />
            </IconButton>
          </div>
        ))}
        <div className="flex flex-wrap items-center gap-2 px-1 py-1.5">
          <input
            value={groupName}
            placeholder="Group name"
            onChange={(event) => setGroupName(event.currentTarget.value)}
            className={cn(inputClass, "w-40")}
          />
          <input
            value={groupMembers}
            placeholder="Members: names or ids, comma separated"
            onChange={(event) => setGroupMembers(event.currentTarget.value)}
            className={cn(inputClass, "flex-1")}
          />
          <label className="flex items-center gap-1 text-[12px] text-faint">
            <input
              type="checkbox"
              checked={groupCast}
              onChange={(event) => setGroupCast(event.currentTarget.checked)}
            />
            Cast
          </label>
          <button
            type="button"
            onClick={() => void addGroup()}
            className="chip px-2.5 py-1 text-[12px]"
          >
            <PlusIcon size={13} />
            Add group
          </button>
        </div>
      </div>

      <div className="mt-3 space-y-2">
        {label("About you")}
        <div className="flex flex-wrap gap-2 px-1">
          <input
            value={profile.name}
            placeholder="Name"
            onChange={(event) =>
              setProfile({ ...profile, name: event.currentTarget.value })
            }
            className={cn(inputClass, "w-40")}
          />
          <input
            value={profile.pronouns}
            placeholder="Pronouns"
            onChange={(event) =>
              setProfile({ ...profile, pronouns: event.currentTarget.value })
            }
            className={cn(inputClass, "w-32")}
          />
          <input
            value={profile.about}
            placeholder="A line personas should know about you"
            onChange={(event) =>
              setProfile({ ...profile, about: event.currentTarget.value })
            }
            className={cn(inputClass, "flex-1")}
          />
          <button
            type="button"
            onClick={() => void saveProfile()}
            className="chip px-2.5 py-1 text-[12px]"
          >
            <CheckIcon size={13} />
            Save
          </button>
        </div>
      </div>
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

      <Row label="Tool calls">
        <Segmented
          value={config.interface.showToolCalls}
          options={[
            { id: "collapsed", label: "Collapsed", title: "Compact rows you expand when you want the arguments and output" },
            { id: "hidden", label: "Hidden", title: "Never show searches and tool activity in the transcript" },
            { id: "expanded", label: "Expanded", title: "Arguments and output open from the start" },
          ]}
          onChange={(value) => void saveInterface({ showToolCalls: value })}
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

      <Toggle
        label="Auto-title chats"
        hint="Name new chats with the titles model after the first reply."
        checked={config.chat.autoTitle}
        onChange={(value) =>
          void ipc.setChatSettings({ autoTitle: value }).then((updated) => {
            if (updated) applyRemote(updated);
          })
        }
      />

      <Row label="Titles model">
        <select
          value={liteValue}
          onChange={(event) => void setLite(event.currentTarget.value)}
          className={cn(fieldBase, "w-44 text-[13px]")}
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

      <Row label="Max output" hint="0 uses the model's own output limit">
        <input
          type="number"
          min={0}
          max={200000}
          step={256}
          value={config.chat.maxOutputTokens}
          onChange={(event) =>
            void setChatSettings({
              maxOutputTokens: Number(event.currentTarget.value),
            })
          }
          className={cn(fieldBase, "w-28 text-[13px]")}
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
          className={cn(fieldBase, "w-44 text-[13px]")}
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
          className={cn(fieldBase, "w-44 text-[13px]")}
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
      <div className="[&>*+*]:border-t [&>*+*]:border-[var(--glass-border)]">
        {Object.entries(config.mcpServers ?? {}).map(([id, server]) => (
          <div key={id} className="flex items-center gap-1 px-1 py-1.5">
            <span className="min-w-0 flex-1">
              <span className="block truncate text-[13px]">{server.name || id}</span>
              <span className="block truncate font-mono text-[11px] text-faint">
                {server.command} {server.args.join(" ")}
              </span>
            </span>
            <IconButton
              label={`Delete ${server.name || id}`}
              tone="danger"
              onClick={() =>
                void ipc.deleteMcpServer(id).then((updated) => {
                  if (updated) applyRemote(updated);
                  setTools(null);
                })
              }
            >
              <TrashIcon size={14} />
            </IconButton>
          </div>
        ))}
        {Object.keys(config.mcpServers ?? {}).length === 0 && (
          <p className="px-1 py-1 text-[12.5px] leading-5 text-faint">
            Add a server and its tools join every chat. Discovery happens on
            save, and again whenever you ask.
          </p>
        )}
      </div>

      {form ? (
        <div className="my-2 space-y-2 rounded-row bg-[var(--hover-bg)] p-2.5">
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
            className={cn(fieldBase, "w-full resize-none font-mono text-[11.5px]")}
          />
          <div className="flex gap-2">
            <button
              type="button"
              disabled={busy}
              onClick={() => void save()}
              className="btn-primary flex-1 px-3 py-1.5 text-[12.5px]"
            >
              {busy ? "Connecting…" : "Save & connect"}
            </button>
            <button
              type="button"
              onClick={() => setForm(null)}
              className="btn-ghost px-3 py-1.5 text-[12.5px]"
            >
              Cancel
            </button>
          </div>
        </div>
      ) : (
        <div className="px-1 py-2">
          <button
            type="button"
            onClick={() =>
              setForm({ id: "", name: "", command: "", args: "", env: "" })
            }
            className="chip px-2.5 py-1 text-[12px]"
          >
            <PlusIcon size={13} />
            Add MCP server
          </button>
        </div>
      )}

      <div className="flex items-center gap-2 px-1 py-2.5">
        <button
          type="button"
          onClick={() => void discover()}
          disabled={busy}
          className="chip px-2.5 py-1 text-[12px]"
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
        <div className="flex flex-wrap gap-1 px-1 py-2.5">
          {tools.map((tool) => (
            <span
              key={tool.modelName}
              className="rounded-capsule border border-[var(--glass-border)] px-2 py-0.5 font-mono text-[10.5px] text-faint"
            >
              {tool.server}:{tool.modelName.split("__").pop()}
            </span>
          ))}
        </div>
      )}

      {error && (
        <p className="px-1 py-2 text-[12px] text-[var(--danger)]">{error}</p>
      )}
    </Section>
  );
}

// ---------------------------------------------------------------------------
// Skills
// ---------------------------------------------------------------------------

function SkillsSection() {
  const skills = useSkills((state) => state.skills);
  const loadSkills = useSkills((state) => state.load);
  const [editing, setEditing] = useState<{
    id: string;
    name: string;
    description: string;
    body: string;
    isNew: boolean;
  } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void loadSkills();
  }, [loadSkills]);

  const startNew = () => {
    setError(null);
    setEditing({ id: "", name: "", description: "", body: "", isNew: true });
  };

  const startEdit = async (id: string) => {
    setError(null);
    try {
      const skill = await ipc.readSkill(id);
      if (!skill) return;
      setEditing({
        id: skill.id,
        name: skill.name,
        description: skill.description,
        body: skill.prompt,
        isNew: false,
      });
    } catch (cause) {
      setError(messageOf(cause));
    }
  };

  const save = async () => {
    if (!editing) return;
    setBusy(true);
    setError(null);
    try {
      const id = editing.id.trim();
      await ipc.saveSkill({
        id,
        name: editing.name.trim() || id,
        description: editing.description.trim(),
        body: editing.body,
      });
      await loadSkills();
      setEditing(null);
    } catch (cause) {
      // The backend owns validation (id shape, body size); show its message.
      setError(messageOf(cause));
    } finally {
      setBusy(false);
    }
  };

  const remove = async (id: string) => {
    setError(null);
    try {
      await ipc.deleteSkill(id);
      await loadSkills();
    } catch (cause) {
      setError(messageOf(cause));
    }
  };

  return (
    <Section title="Skills">
      {skills.length === 0 && !editing ? (
        <p className="px-1 py-1 text-[12.5px] leading-5 text-faint">
          Markdown prompts in <span className="font-mono">~/.loom/skills/</span>,
          offered in the composer&apos;s <span className="font-mono">/</span>{" "}
          menu. Write them here, or let the model write them while a chat is in
          Atelier mode.
        </p>
      ) : (
        <div className="[&>*+*]:border-t [&>*+*]:border-[var(--glass-border)]">
          {skills.map((skill) => (
            <div key={skill.id} className="flex items-center gap-1 px-1 py-1.5">
              <span className="min-w-0 flex-1">
                <span className="block truncate text-[13px]">
                  <span className="font-mono text-[12px] text-faint">
                    /{skill.id}
                  </span>{" "}
                  {skill.name}
                </span>
                {skill.description && (
                  <span className="block truncate text-[11.5px] text-faint">
                    {skill.description}
                  </span>
                )}
              </span>
              <IconButton
                label={`Edit ${skill.id}`}
                onClick={() => void startEdit(skill.id)}
              >
                <EditIcon size={14} />
              </IconButton>
              <IconButton
                label={`Delete ${skill.id}`}
                tone="danger"
                onClick={() => void remove(skill.id)}
              >
                <TrashIcon size={14} />
              </IconButton>
            </div>
          ))}
        </div>
      )}

      {editing ? (
        <div className="my-2 space-y-2 rounded-row bg-[var(--hover-bg)] p-2.5">
          <input
            value={editing.id}
            disabled={!editing.isNew}
            placeholder="id (lowercase letters, digits, dashes)"
            onChange={(event) =>
              setEditing({ ...editing, id: event.currentTarget.value })
            }
            className={cn(inputClass, "font-mono disabled:opacity-60")}
          />
          <input
            value={editing.name}
            placeholder="Name"
            onChange={(event) =>
              setEditing({ ...editing, name: event.currentTarget.value })
            }
            className={inputClass}
          />
          <input
            value={editing.description}
            placeholder="One-line description"
            onChange={(event) =>
              setEditing({ ...editing, description: event.currentTarget.value })
            }
            className={inputClass}
          />
          <textarea
            value={editing.body}
            placeholder="Prompt body"
            rows={5}
            onChange={(event) =>
              setEditing({ ...editing, body: event.currentTarget.value })
            }
            className={cn(fieldBase, "w-full resize-none font-mono text-[12px]")}
          />
          <div className="flex gap-2">
            <button
              type="button"
              disabled={busy}
              onClick={() => void save()}
              className="btn-primary flex-1 px-3 py-1.5 text-[12.5px]"
            >
              {busy ? "Saving…" : "Save"}
            </button>
            <button
              type="button"
              onClick={() => {
                setEditing(null);
                setError(null);
              }}
              className="btn-ghost px-3 py-1.5 text-[12.5px]"
            >
              Cancel
            </button>
          </div>
        </div>
      ) : (
        <div className="px-1 py-2">
          <button
            type="button"
            onClick={startNew}
            className="chip px-2.5 py-1 text-[12px]"
          >
            <PlusIcon size={13} />
            New skill
          </button>
        </div>
      )}

      {error && (
        <p className="px-1 py-2 text-[12px] text-[var(--danger)]">{error}</p>
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
      <div className="flex flex-wrap gap-1.5 px-1 py-2.5">
        <button
          type="button"
          onClick={() => void check()}
          disabled={busy}
          className="chip px-2.5 py-1 text-[12px]"
        >
          {busy ? "Working…" : "Check for updates"}
        </button>
        {manifest && !staged && (
          <button
            type="button"
            onClick={() => void download()}
            disabled={busy}
            className="btn-primary px-2.5 py-1 text-[12px]"
          >
            Download {manifest.version}
          </button>
        )}
        {staged && (
          <button
            type="button"
            onClick={() => void ipc.applyUpdate(staged)}
            className="btn-primary px-2.5 py-1 text-[12px]"
          >
            Restart &amp; install
          </button>
        )}
      </div>
      {status && (
        <p className="px-1 py-2 text-[12px] text-[var(--accent)]">{status}</p>
      )}
      {manifest?.notes && (
        <p className="px-1 py-2 text-[12px] leading-5 whitespace-pre-wrap text-soft">
          {manifest.notes}
        </p>
      )}
      {error && (
        <p className="px-1 py-2 text-[12px] text-[var(--danger)]">{error}</p>
      )}
    </Section>
  );
}


/**
 * Long-term memory: durable facts about the user (global) and about each
 * workspace, plus the switch for the automatic extraction pass.
 */
function MemorySection() {
  const config = useSettings((state) => state.config);
  const applyRemote = useSettings((state) => state.applyRemote);
  const [scope, setScope] = useState<string>("global");
  const [memories, setMemories] = useState<StoredMemory[]>([]);
  const [draft, setDraft] = useState("");
  const [pinDraft, setPinDraft] = useState(false);
  const [editing, setEditing] = useState<{ id: string; content: string } | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = async (wanted: string) => {
    try {
      setMemories((await ipc.listMemories(wanted)) ?? []);
      setError(null);
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
    }
  };

  useEffect(() => {
    if (!isTauri) return;
    void refresh(scope);
  }, [scope]);

  const saveInterface = async (patch: Partial<typeof config.interface>) => {
    const updated = await ipc.setInterfaceSettings({ ...config.interface, ...patch });
    if (updated) applyRemote(updated);
  };

  const add = async () => {
    if (!draft.trim()) return;
    try {
      await ipc.upsertMemory(null, scope, draft.trim(), pinDraft);
      setDraft("");
      setPinDraft(false);
      await refresh(scope);
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
    }
  };

  const saveEdit = async () => {
    if (!editing) return;
    try {
      await ipc.upsertMemory(editing.id, scope, editing.content.trim(), false);
      setEditing(null);
      await refresh(scope);
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
    }
  };

  const workspaceOptions = config.workspaces;

  return (
    <>
      <Section
        title="Long-term memory"
        description="Facts Loom keeps across chats. Pinned facts ride in every prompt; the rest are pulled in when they match what you are asking."
      >
        <Toggle
          label="Learn from conversations automatically"
          hint="After each reply a small model proposes durable facts; they appear here immediately and can be edited or deleted."
          checked={config.interface.autoMemory}
          onChange={(value) => void saveInterface({ autoMemory: value })}
        />
      </Section>

      <Section title="Scope">
        <div className="flex flex-wrap gap-1 px-1 py-2.5">
          <button
            type="button"
            onClick={() => setScope("global")}
            className={cn(
              "rounded-capsule px-2.5 py-1 text-[12px]",
              scope === "global"
                ? "bg-[var(--hover-bg)] text-[var(--ink)]"
                : "text-faint hover:text-[var(--ink)]",
            )}
          >
            About you
          </button>
          {workspaceOptions.map((workspace) => (
            <button
              key={workspace.path}
              type="button"
              onClick={() => setScope(workspace.path)}
              className={cn(
                "rounded-capsule px-2.5 py-1 text-[12px]",
                scope === workspace.path
                  ? "bg-[var(--hover-bg)] text-[var(--ink)]"
                  : "text-faint hover:text-[var(--ink)]",
              )}
              title={workspace.path}
            >
              {workspace.name || workspace.path.split(/[\\/]/).pop()}
            </button>
          ))}
        </div>
      </Section>

      <Section title={scope === "global" ? "Facts about you" : "Facts about this project"}>
        {memories.length === 0 && (
          <p className="px-1 py-2 text-[12.5px] leading-5 text-faint">
            Nothing saved here yet. Ask in a chat (“remember that I prefer tabs”),
            or add one below.
          </p>
        )}
        {memories.map((memory) => (
          <div key={memory.id} className="flex items-start gap-1.5 px-1 py-2">
            <button
              type="button"
              title={memory.pinned ? "Unpin" : "Pin into every prompt"}
              onClick={() =>
                void ipc
                  .upsertMemory(memory.id, memory.scope, memory.content, !memory.pinned)
                  .then(() => refresh(scope))
              }
              className={cn(
                "mt-0.5 shrink-0",
                memory.pinned ? "text-[var(--accent)]" : "text-faint",
              )}
            >
              <SparkIcon size={14} />
            </button>
            <span className="min-w-0 flex-1">
              {editing?.id === memory.id ? (
                <textarea
                  value={editing.content}
                  onChange={(event) =>
                    setEditing({ id: memory.id, content: event.currentTarget.value })
                  }
                  rows={2}
                  className={cn(inputClass, "resize-y")}
                />
              ) : (
                <span className="block text-[12.5px] leading-5 text-soft">
                  {memory.content}
                </span>
              )}
              <span className="mt-0.5 block text-[11px] text-faint">
                {memory.source === "auto"
                  ? "learned automatically"
                  : memory.source === "user"
                    ? "added by you"
                    : "saved by the model"}
                {memory.pinned ? " · pinned" : ""}
              </span>
            </span>
            {editing?.id === memory.id ? (
              <button
                type="button"
                onClick={() => void saveEdit()}
                className="hover-surface grid h-7 w-7 shrink-0 place-items-center rounded-control text-soft"
                title="Save"
              >
                <CheckIcon size={14} />
              </button>
            ) : (
              <button
                type="button"
                onClick={() => setEditing({ id: memory.id, content: memory.content })}
                className="hover-surface grid h-7 w-7 shrink-0 place-items-center rounded-control text-soft"
                title="Edit"
              >
                <EditIcon size={14} />
              </button>
            )}
            <button
              type="button"
              onClick={() =>
                void ipc.deleteMemory(memory.id).then(() => refresh(scope))
              }
              className="hover-surface grid h-7 w-7 shrink-0 place-items-center rounded-control text-soft"
              title="Forget"
            >
              <TrashIcon size={14} />
            </button>
          </div>
        ))}
      </Section>

      <Section title="Add a fact">
        <div className="space-y-2 px-1 py-2.5">
          <textarea
            value={draft}
            onChange={(event) => setDraft(event.currentTarget.value)}
            placeholder={
              scope === "global"
                ? "Something durable about you…"
                : "Something durable about this project…"
            }
            rows={2}
            className={cn(inputClass, "resize-y")}
          />
          <label className="flex items-center gap-2 text-[12.5px] text-soft">
            <input
              type="checkbox"
              checked={pinDraft}
              onChange={(event) => setPinDraft(event.currentTarget.checked)}
            />
            Pin into every prompt
          </label>
          {error && <p className="text-[12px] text-[var(--danger)]">{error}</p>}
          <div className="flex items-center justify-between gap-2">
            <button
              type="button"
              onClick={() =>
                void ipc.clearMemories(scope).then(() => refresh(scope))
              }
              className="btn-ghost px-2.5 py-1.5 text-[12px] text-faint"
            >
              Clear this scope
            </button>
            <button
              type="button"
              disabled={!draft.trim()}
              onClick={() => void add()}
              className="btn-ghost px-2.5 py-1.5 text-[12px] disabled:opacity-50"
            >
              Remember
            </button>
          </div>
        </div>
      </Section>
    </>
  );
}

function GeneralSection() {
  const config = useSettings((state) => state.config);
  const applyRemote = useSettings((state) => state.applyRemote);
  const setShortcutsOpen = useUi((state) => state.setShortcutsOpen);
  const [hotkeyDraft, setHotkeyDraft] = useState(config.interface.hotkey);
  const [hotkeyNote, setHotkeyNote] = useState<string | null>(null);
  const [autostart, setAutostart] = useState(false);

  useEffect(() => {
    if (!isTauri) return;
    void ipc.autostartEnabled().then((enabled) => setAutostart(Boolean(enabled)));
  }, []);

  const toggleAutostart = async (value: boolean) => {
    const previous = autostart;
    setAutostart(value);
    const actual = await ipc.setAutostart(value).catch(() => null);
    if (actual === null && isTauri) {
      // The command failed; do not show a state the registry does not have.
      setAutostart(previous);
    } else if (actual !== null) {
      setAutostart(actual);
    }
  };

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
        <div className="flex items-center gap-1.5 px-1 py-2.5">
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
            className="btn-ghost shrink-0 px-2.5 py-1.5 text-[12px]"
          >
            Apply
          </button>
        </div>
      )}
      {hotkeyNote && (
        <p className="px-1 py-1.5 text-[12px] text-faint">{hotkeyNote}</p>
      )}

      <Toggle
        label="Start Loom when you sign in"
        hint="Windows launches Loom automatically at login."
        checked={autostart}
        onChange={(value) => void toggleAutostart(value)}
      />

      <Row label="Keyboard shortcuts">
        <button
          type="button"
          onClick={() => setShortcutsOpen(true)}
          className="chip px-2.5 py-1 text-[12px]"
        >
          Show all
        </button>
      </Row>

      <Toggle
        label="Screenshot quick ask"
        hint="The first quick ask carries the screen as it was when the overlay opened; later ones capture on send. The model must support images."
        checked={config.interface.captureOnSend}
        onChange={(value) => void saveInterface({ captureOnSend: value })}
      />
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
      <Toggle
        label="Live UI blocks"
        hint="Let replies render ```loom-ui HTML as themed widgets. Sanitized and script-free: no app access."
        checked={config.interface.generatedUi}
        onChange={(value) => void saveInterface({ generatedUi: value })}
      />

      <Toggle
        label="Condensed replies"
        hint="Mark a reply that was answered from a summary of the chat's older turns, and let the line expand to show what the model was given. The long conversation still fits the window either way."
        checked={config.interface.showCondensing}
        onChange={(value) => void saveInterface({ showCondensing: value })}
      />
    </Section>
  );
}

// ------------------------------------------------------------------ sections

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

function AppearanceSection() {
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
      <Section title="Theme">
        <Row label="Mode" hint="Light is the default; both palettes work over any artwork.">
          <Segmented
            value={config.theme}
            options={[
              { id: "light", label: "Light", icon: <SunIcon size={14} /> },
              { id: "dark", label: "Dark", icon: <MoonIcon size={14} /> },
            ]}
            onChange={setTheme}
          />
        </Row>
      </Section>

      <Section title="Background">
        <div className="px-1 py-2.5">
          <div className="grid grid-cols-3 gap-2">
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

          <div className="mt-2.5 flex flex-wrap gap-1.5">
            <button
              type="button"
              onClick={() => void pickBackground("image")}
              className="chip px-2.5 py-1 text-[12px]"
            >
              Choose image…
            </button>
            <button
              type="button"
              onClick={() => void pickBackground("video")}
              className="chip px-2.5 py-1 text-[12px]"
            >
              Choose video…
            </button>
            {config.background.kind !== "builtin" && (
              <button
                type="button"
                onClick={() => setBackground({ kind: "builtin", path: null })}
                className="chip-danger px-2.5 py-1 text-[12px]"
              >
                Remove custom
              </button>
            )}
          </div>

          {config.background.kind !== "builtin" && config.background.path && (
            <p
              className="mt-2 truncate font-mono text-[11px] text-faint select-all"
              title={config.background.path}
            >
              {config.background.path}
            </p>
          )}
        </div>

        <Row
          label="Dim"
          hint={
            config.theme === "dark"
              ? "Dark mode keeps a minimum veil so bright art cannot wash out the text."
              : "Veil your own artwork so text stays legible. The built-in presets need none."
          }
        >
          <div className="flex items-center gap-2">
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
            <span className="w-7 text-right text-[12px] text-faint tabular-nums">
              {config.background.dim}
            </span>
          </div>
        </Row>
        <Row label="Blur" hint="Soften the artwork behind the glass.">
          <div className="flex items-center gap-2">
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
            <span className="w-7 text-right text-[12px] text-faint tabular-nums">
              {config.background.blur}
            </span>
          </div>
        </Row>
      </Section>
    </>
  );
}

/** One metric line: a percent window with a bar, or a balance figure. */
function UsageMetricRow({ metric }: { metric: UsageMetric }) {
  const percent = percentOf(metric);
  const reset = formatReset(metric.resetsAt, metric.resetsAtMs);
  const fill =
    metricTone(metric) === "hot" ? "bg-[var(--danger)]" : "bg-[var(--accent)]";
  const captions = [
    metric.detail,
    reset ? `resets ${reset}` : null,
    metric.status && metric.status !== "ok" ? metric.status : null,
  ].filter((line): line is string => Boolean(line));

  return (
    <div>
      <div className="flex items-baseline justify-between gap-3">
        <span className="text-[12.5px] text-faint">{metric.label}</span>
        <span className="text-[12.5px] text-soft">{metricSummary(metric)}</span>
      </div>
      {percent !== null && (
        <div className="mt-1 h-1.5 overflow-hidden rounded-capsule bg-[var(--ink-ghost)]">
          <div
            className={cn("h-full rounded-capsule", fill)}
            style={{ width: `${Math.min(100, Math.max(0, percent))}%` }}
          />
        </div>
      )}
      {captions.length > 0 && (
        <p className="mt-0.5 text-[11.5px] leading-4 text-faint">
          {captions.join(" · ")}
        </p>
      )}
    </div>
  );
}

/** Live vendor limits plus what this machine has actually spent. */
function UsageSection() {
  const capable = useUsage((state) => state.capable);
  const byProvider = useUsage((state) => state.byProvider);
  const errors = useUsage((state) => state.errors);
  const loading = useUsage((state) => state.loading);
  const refresh = useUsage((state) => state.refresh);
  const refreshProvider = useUsage((state) => state.refreshProvider);
  const summary = useUsage((state) => state.summary);
  const summaryLoading = useUsage((state) => state.summaryLoading);
  const summaryError = useUsage((state) => state.summaryError);
  const refreshSummary = useUsage((state) => state.refreshSummary);
  const providers = useSettings((state) => state.config.providers);

  useEffect(() => {
    void refresh();
    void refreshSummary();
  }, [refresh, refreshSummary]);

  const nameOf = (providerId: string) =>
    providers[providerId]?.name ?? providerId;
  const tokens = (input: number, output: number, cost: number, priced: number) =>
    `${compactTokens(input) ?? "0"} in · ${compactTokens(output) ?? "0"} out` +
    (priced > 0 ? ` · ~$${cost.toFixed(2)}` : "");

  return (
    <>
      <Section
        title="Limits"
        description="Live from the vendor, using the same API key as chat."
      >
        {capable.length === 0 ? (
          <p className="px-1 py-2 text-[12.5px] leading-5 text-faint">
            No enabled provider reports limits. Vendors with a usage API are
            OpenCode Go, OpenRouter, DeepSeek, and Z.ai coding plans; add one
            in Providers and its card appears here.
          </p>
        ) : (
          capable.map((entry) => {
            const usage = byProvider[entry.providerId];
            const error = errors[entry.providerId];
            return (
              <div key={entry.providerId} className="px-1 py-2.5">
                <div className="flex items-center gap-2">
                  <span className="text-[13px] text-soft">{entry.name}</span>
                  <span className="text-[11.5px] text-faint">
                    {entry.source}
                  </span>
                  <div className="flex-1" />
                  <button
                    type="button"
                    onClick={() => void refreshProvider(entry.providerId)}
                    className="chip px-2.5 py-1 text-[12px]"
                  >
                    Refresh
                  </button>
                </div>

                {error && (
                  <p className="mt-1.5 text-[12px] leading-4 text-[var(--danger)]">
                    {error}
                  </p>
                )}
                {!error && !usage && (
                  <p className="mt-1.5 text-[12px] text-faint">
                    {loading ? "Reading…" : "No reading yet."}
                  </p>
                )}
                {usage && (
                  <div className="mt-2 space-y-2.5">
                    {usage.metrics.map((metric) => (
                      <UsageMetricRow key={metric.id} metric={metric} />
                    ))}
                  </div>
                )}
              </div>
            );
          })
        )}
        <p className="px-1 py-2 text-[11.5px] leading-4 text-faint">
          Values refresh every few minutes while the app runs. A key without a
          subscription reports that plainly rather than showing zero.
        </p>
      </Section>

      <Section
        title="Local usage"
        description="Tokens recorded in this Loom database; costs are list-price estimates."
      >
        {summaryError && (
          <p className="px-1 py-2 text-[12px] text-[var(--danger)]">
            {summaryError}
          </p>
        )}
        {!summaryError && !summary && (
          <p className="px-1 py-2 text-[12.5px] text-faint">
            {summaryLoading ? "Counting…" : "Nothing recorded yet."}
          </p>
        )}
        {summary && (
          <>
            <Row label="All providers">
              <span className="text-right text-[12.5px] text-soft">
                {tokens(
                  summary.inputTokens,
                  summary.outputTokens,
                  summary.estimatedCostUsd,
                  summary.pricedReplies,
                )}
              </span>
            </Row>
            {summary.providers.map((entry) => (
              <Row
                key={entry.providerId}
                label={nameOf(entry.providerId)}
                hint={`${entry.replies} ${entry.replies === 1 ? "reply" : "replies"}`}
              >
                <span className="text-right text-[12.5px] text-soft">
                  {tokens(
                    entry.inputTokens,
                    entry.outputTokens,
                    entry.estimatedCostUsd,
                    entry.pricedReplies,
                  )}
                </span>
              </Row>
            ))}
          </>
        )}
        <p className="px-1 py-2 text-[11.5px] leading-4 text-faint">
          Only chats on this machine are counted, and only models with known
          prices feed the estimate.
        </p>
      </Section>
    </>
  );
}

export function SettingsPanel() {
  const open = useUi((state) => state.settingsOpen);
  const setOpen = useUi((state) => state.setSettingsOpen);
  const sidebarOpen = useUi((state) => state.sidebarOpen);
  const category = useUi((state) => state.settingsCategory);
  const setCategory = useUi((state) => state.setSettingsCategory);
  const [info, setInfo] = useState<AppInfo | null>(null);
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

  const searchRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) requestAnimationFrame(() => searchRef.current?.focus());
  }, [open]);

  if (!open) return null;

  const needle = query.trim().toLowerCase();
  const matched = SETTINGS_CATEGORIES.filter(
    (entry) =>
      !needle ||
      entry.label.toLowerCase().includes(needle) ||
      entry.blurb.toLowerCase().includes(needle) ||
      entry.keywords.includes(needle),
  );
  const searching = needle.length > 0;
  const current = SETTINGS_CATEGORIES.find((entry) => entry.id === category);
  const openCategory = (id: SettingsCategoryId) => {
    setCategory(id);
    setQuery("");
  };

  return (
    <div
      className={cn(
        "absolute inset-0 z-40 flex justify-end p-3 pt-16",
        // The chats popup sits at the left edge; leave its column alone so the
        // two panes read as side by side rather than stacked.
        sidebarOpen && "pl-[332px]",
      )}
    >
      <button
        type="button"
        aria-label="Close settings"
        onClick={() => setOpen(false)}
        // Not `inset-0`: the scrim must not dim or swallow clicks over the
        // chats popup while that is open.
        className={cn(
          "absolute top-0 right-0 bottom-0 cursor-default bg-black/10",
          sidebarOpen ? "left-[332px]" : "left-0",
        )}
      />

      <div className="animate-fade-up panel-strong relative flex h-full w-[720px] max-w-full flex-col overflow-hidden rounded-sheet">
        <div className="flex items-center gap-3 px-4 pt-3 pb-1">
          <h2 className="text-[14.5px] font-semibold">Settings</h2>
          <SearchField
            inputRef={searchRef}
            value={query}
            onChange={setQuery}
            placeholder="Search settings…"
            className="min-w-0 flex-1"
            onKeyDown={(event) => {
              if (event.key === "Enter" && searching) {
                const first = matched[0];
                if (first) openCategory(first.id);
              }
            }}
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
          <nav className="w-[180px] shrink-0 overflow-y-auto border-r border-[var(--glass-border)] px-2 py-2">
            {NAV_GROUPS.map((group) => (
              <div key={group.label} className="pt-3 first:pt-0">
                <p className="px-2.5 pb-1 text-[10.5px] font-semibold tracking-[0.09em] text-faint uppercase">
                  {group.label}
                </p>
                {group.ids.map((id) => {
                  const entry = SETTINGS_CATEGORIES.find((item) => item.id === id);
                  if (!entry) return null;
                  const Icon = CATEGORY_ICONS[id];
                  const active = entry.id === category && !searching;
                  const hit =
                    searching && matched.some((item) => item.id === id);
                  return (
                    <button
                      key={id}
                      type="button"
                      onClick={() => openCategory(id)}
                      className={cn(
                        "relative flex w-full items-center gap-2 rounded-row px-2.5 py-1.5 text-left text-[13px] transition-colors",
                        active
                          ? "bg-[var(--hover-bg)] text-[var(--ink)]"
                          : "text-soft hover:bg-[var(--hover-bg)] hover:text-[var(--ink)]",
                        hit && !active && "text-[var(--accent)]",
                      )}
                    >
                      {active && (
                        // The same knot the sidebar uses for the open chat.
                        <span
                          aria-hidden="true"
                          className="absolute top-1/2 left-0 h-3.5 w-[2px] -translate-y-1/2 rounded-full bg-[var(--accent)]"
                        />
                      )}
                      <Icon
                        size={15}
                        className={active ? "shrink-0 text-[var(--accent)]" : "shrink-0 text-faint"}
                      />
                      <span className="min-w-0 flex-1 truncate">{entry.label}</span>
                    </button>
                  );
                })}
              </div>
            ))}
          </nav>

          <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4">
            {searching ? (
              matched.length === 0 ? (
                <EmptyState
                  icon={<SearchIcon size={26} />}
                  title={`No settings match “${query.trim()}”`}
                  hint="Try another word — “theme”, “key”, “hotkey”, “update”."
                  action={
                    <button
                      type="button"
                      onClick={() => setQuery("")}
                      className="btn-ghost px-3 py-1.5 text-[12.5px]"
                    >
                      Clear search
                    </button>
                  }
                />
              ) : (
                <>
                  <p className="mb-2 px-1 text-[12px] text-faint">
                    {matched.length} {matched.length === 1 ? "category" : "categories"}{" "}
                    mention “{query.trim()}”
                  </p>
                  <div className="space-y-1.5">
                    {matched.map((entry) => {
                      const Icon = CATEGORY_ICONS[entry.id];
                      return (
                        <button
                          key={entry.id}
                          type="button"
                          onClick={() => openCategory(entry.id)}
                          className="hover-surface flex w-full items-center gap-3 rounded-control border border-[var(--glass-border)] bg-[var(--card-bg)] px-3 py-2.5 text-left"
                        >
                          <Icon size={16} className="shrink-0 text-[var(--accent)]" />
                          <span className="min-w-0 flex-1">
                            <span className="block text-[13px]">{entry.label}</span>
                            <span className="block truncate text-[11.5px] text-faint">
                              {entry.blurb}
                            </span>
                          </span>
                          <ChevronDownIcon
                            size={14}
                            className="shrink-0 -rotate-90 text-faint"
                          />
                        </button>
                      );
                    })}
                  </div>
                  <p className="px-1 pt-3 text-[11.5px] text-faint">
                    Enter opens {matched[0]?.label}.
                  </p>
                </>
              )
            ) : (
              <>
                {current && (
                  <header className="mb-3 px-1">
                    <h3 className="text-[16px] font-semibold tracking-[-0.01em]">
                      {current.label}
                    </h3>
                    <p className="mt-0.5 text-[12px] leading-5 text-faint">
                      {current.blurb}
                    </p>
                  </header>
                )}
                {category === "general" && <GeneralSection />}
                {category === "appearance" && <AppearanceSection />}
                {category === "chat" && <ChatSection />}
                {category === "tools" && <ToolsSection />}
                {category === "providers" && <ProvidersSection />}
                {category === "usage" && <UsageSection />}
                {category === "personas" && <PersonasSection />}
                {category === "memory" && <MemorySection />}
                {category === "voice" && <VoiceSettings />}
                {category === "mcp" && <McpSection />}
                {category === "skills" && (
                  <>
                    <SkillsSection />
                    <PromptsEditor />
                  </>
                )}
                {category === "data" && (
                  <>
                    <DataSection info={info} />
                    <StorageSection />
                  </>
                )}
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
  const models = useProviders((state) => state.models);
  const [tools, setTools] = useState<
    { name: string; description: string; readOnly: boolean; scope?: ToolScope }[]
  >([]);
  const [keyDraft, setKeyDraft] = useState("");
  const [hasKey, setHasKey] = useState(false);

  // Only a vision model can drive a computer turn: screenshots are the eyes.
  const visionModels = models.filter(
    (entry) => entry.enabled && entry.spec.inputModalities.includes("image"),
  );

  useEffect(() => {
    void ipc.listTools().then((result) => setTools(result ?? []));
    void ipc.searchKeyStatus().then((stored) => setHasKey(Boolean(stored)));
  }, []);

  const setMode = async (mode: PermissionMode) => {
    const updated = await ipc.setChatSettings({ permissionMode: mode });
    if (updated) applyRemote(updated);
  };

  const setAgentMode = async (mode: AgentMode) => {
    const updated = await ipc.setChatSettings({ agentMode: mode });
    if (updated) applyRemote(updated);
  };

  const setProvider = async (provider: SearchProvider) => {
    const updated = await ipc.setSearchProvider(provider);
    if (updated) applyRemote(updated);
  };

  const saveKey = async () => {
    const key = keyDraft.trim();
    if (!key) return;
    await ipc.setSearchKey(key);
    setKeyDraft("");
    setHasKey(true);
  };

  const clearKey = async () => {
    await ipc.setSearchKey("");
    setKeyDraft("");
    setHasKey(false);
  };

  return (
    <>
      <Section title="Permissions">
        <Row label="Default mode">
          <Segmented
            value={config.chat.permissionMode}
            options={GLOBAL_PERMISSION_MODES.map((mode) => ({
              id: mode.id,
              label: mode.label,
              title: mode.help,
            }))}
            onChange={(value) => void setMode(value)}
          />
        </Row>
        <Row label="Tool steps per turn">
          <input
            type="number"
            min={1}
            max={200}
            value={config.chat.maxToolRounds}
            title="How many tool round-trips one reply may take (1-200); at the limit the model is told to summarise"
            onChange={(event) =>
              void ipc
                .setChatSettings({
                  maxToolRounds: Number(event.currentTarget.value),
                })
                .then((updated) => {
                  if (updated) applyRemote(updated);
                })
            }
            className={cn(fieldBase, "w-28 text-[13px]")}
          />
        </Row>
        <p className="px-1 py-2 text-[12px] leading-5 text-faint">
          Each chat can override this from the composer. Write-file and command
          tools always ask unless the mode is “Auto all”. A fourth mode,{" "}
          <span className="text-soft">Atelier</span>, runs everything and lets
          the model edit Loom&apos;s harness — personas, MCP servers, skills,
          prompts, providers and settings. It is deliberately per chat and
          cannot be set as the default: switch it from the composer&apos;s
          permission chip.
        </p>
      </Section>

      <Section title="Computer use">
        <p className="px-1 py-2 text-[12px] leading-5 text-faint">
          Per chat, from the composer&apos;s{" "}
          <span className="text-soft">Computer</span> chip. When it is on, Loom
          can take screenshots and drive the mouse, keyboard, windows,
          processes, and the clipboard without asking for each action — the
          chip is the standing consent, and switching it off stops the turn at
          once. Clicking, scrolling or typing anywhere but Loom&apos;s own
          windows pauses the turn (moving the mouse does not); it resumes after
          30 seconds of quiet or when you press Resume, and{" "}
          <span className="font-mono">Ctrl+Alt+Esc</span> stops it from anywhere.
          Stop ends only the chat that is driving — other chats and background
          runs are untouched. Plan and Review modes make the chip
          screenshots-only, since they refuse anything that changes the machine.
          UAC prompts, the lock screen, and elevated windows cannot be seen or
          controlled, and DRM or anti-cheat windows may capture black.
          Screenshots are sent to your model provider and kept in the chat (the
          newest 200 per chat) so you can see what Loom saw. Typing long text
          goes through the clipboard and puts it back afterwards, images and
          copied files included.
        </p>
        <Row label="Thinking">
          <Segmented
            value={config.chat.computerVariant ?? "inherit"}
            options={[
              { id: "off", label: "Off", title: "No reasoning tokens on computer turns — fastest" },
              { id: "low", label: "Low", title: "Cheapest effort the model offers (default)" },
              { id: "inherit", label: "Inherit", title: "Use the chat's usual thinking level" },
            ]}
            onChange={(value) =>
              void ipc
                .setChatSettings({
                  computerVariant: value === "inherit" ? null : value,
                })
                .then((updated) => {
                  if (updated) applyRemote(updated);
                })
            }
          />
        </Row>
        <Row label="Screenshot size">
          <Segmented
            value={String(config.chat.computerScreenshotEdge)}
            options={[
              { id: "0", label: "Native", title: "Raw monitor pixels; most detail, most bytes (default)" },
              { id: "1568", label: "1568", title: "The longest edge most providers actually use" },
              { id: "1280", label: "1280", title: "Smaller frames: fewer tokens per round, small text gets blurrier" },
            ]}
            onChange={(value) =>
              void ipc
                .setChatSettings({ computerScreenshotEdge: Number(value) })
                .then((updated) => {
                  if (updated) applyRemote(updated);
                })
            }
          />
        </Row>
        <Row label="Computer model">
          <select
            value={
              config.chat.computerModel
                ? `${config.chat.computerModel.providerId}::${config.chat.computerModel.modelId}`
                : ""
            }
            title="Vision model used only while the Computer chip is armed; slower main models keep their quality for everything else"
            onChange={(event) => {
              const raw = event.currentTarget.value;
              if (!raw) {
                void ipc.setChatSettings({ computerModel: null }).then((updated) => {
                  if (updated) applyRemote(updated);
                });
                return;
              }
              const [providerId, modelId] = raw.split("::");
              void ipc
                .setChatSettings({ computerModel: { providerId, modelId } })
                .then((updated) => {
                  if (updated) applyRemote(updated);
                });
            }}
            className={cn(inputClass, "max-w-[320px]")}
          >
            <option value="">Same as the chat</option>
            {visionModels.map((entry) => (
              <option
                key={`${entry.providerId}::${entry.modelId}`}
                value={`${entry.providerId}::${entry.modelId}`}
              >
                {entry.providerName} · {entry.spec.name ?? entry.modelId}
              </option>
            ))}
          </select>
        </Row>
        <p className="px-1 py-2 text-[12px] leading-5 text-faint">
          Computer use is latency-bound, so armed chats default to cheap
          thinking and can hand the wheel to a fast vision model. The
          screenshot size affects every round: native keeps every pixel, while
          smaller frames cut prefill tokens — use `region` screenshots for
          small text at any size.
        </p>
      </Section>

      <Section title="Agent mode">
        <Row label="Default mode">
          <Segmented
            value={config.chat.agentMode}
            options={[
              { id: "plan", label: "Plan", title: "Research and propose without changing anything" },
              { id: "build", label: "Build", title: "Change the workspace and run commands" },
              { id: "review", label: "Review", title: "Read, then report issues ranked by severity without changing anything" },
              { id: "chat", label: "Chat", title: "Answer from the model and the web only — the fastest mode" },
            ]}
            onChange={(value) => void setAgentMode(value)}
          />
        </Row>
        <p className="px-1 py-2 text-[12px] leading-5 text-faint">
          In Plan mode the model researches and proposes instead:{" "}
          <span className="font-mono">write_file</span>,{" "}
          <span className="font-mono">edit_file</span> and{" "}
          <span className="font-mono">run_command</span> are refused, and it is
          told to ask more questions before writing the plan. Review mode is
          read-only too: it reports issues ranked by severity instead of
          proposing a plan. Chat mode is not read-only but <em>narrow</em>: it
          is offered only <span className="font-mono">web_search</span>,{" "}
          <span className="font-mono">fetch_url</span>,{" "}
          <span className="font-mono">datetime</span> and{" "}
          <span className="font-mono">ask_user</span>, refuses every other tool,
          skips MCP tool discovery, and stops after three tool rounds — so it
          answers from what the model knows and the web, and says so when a task
          needs Build. Each chat can override this from the composer.
        </p>
      </Section>

      <Section title="Web search">
        <Row label="Engine">
          <Segmented
            value={config.searchProvider}
            options={[
              { id: "auto", label: "Auto", title: "Jina when a key is stored, DuckDuckGo otherwise" },
              { id: "jina", label: "Jina", title: "Jina AI search and readable page fetching" },
              { id: "duckduckgo", label: "DuckDuckGo", title: "Keyless DuckDuckGo results, local page reader" },
            ]}
            onChange={(value) => void setProvider(value)}
          />
        </Row>
        <Row label="Jina API key">
          <div className="flex w-full max-w-[320px] gap-1.5">
            <input
              type="password"
              value={keyDraft}
              placeholder={hasKey ? "Stored — type to replace" : "jina_…"}
              onChange={(event) => setKeyDraft(event.currentTarget.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") void saveKey();
              }}
              className={inputClass}
            />
            <button
              type="button"
              disabled={!keyDraft.trim()}
              onClick={() => void saveKey()}
              className="btn-ghost shrink-0 px-2 py-1 text-[12px]"
            >
              Save
            </button>
            {hasKey && (
              <button
                type="button"
                onClick={() => void clearKey()}
                className="btn-danger shrink-0 px-2 py-1 text-[12px]"
              >
                Clear
              </button>
            )}
          </div>
        </Row>
        <p className="px-1 py-2 text-[12px] leading-5 text-faint">
          Jina backs <span className="font-mono">web_search</span> and page
          fetching. Without a key, search falls back to DuckDuckGo; fetching
          falls back to a local reader. The key goes to the credential vault,
          not the config file.
        </p>
      </Section>

      <Section title="Tools">
        <div className="[&>*+*]:border-t [&>*+*]:border-[var(--glass-border)]">
          {tools.map((tool) => (
            <div key={tool.name} className="px-1 py-2">
              <p className="font-mono text-[12px] text-soft">
                {tool.name}
                {tool.scope === "harness" && (
                  <span className="ml-2 rounded-capsule border border-[var(--accent)]/50 px-1.5 py-0.5 text-[10px] text-[var(--accent)]">
                    Atelier only
                  </span>
                )}
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
            <p className="px-1 py-1 text-[12.5px] text-faint">No tools reported.</p>
          )}
        </div>
      </Section>
    </>
  );
}
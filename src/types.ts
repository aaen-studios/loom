export type Theme = "light" | "dark";

export type BackgroundKind = "builtin" | "image" | "video";

/**
 * A background Loom has stored in `~/.loom/backgrounds`.
 *
 * Retention keeps the one in use plus the two before it, and these are what the
 * picker lists so the kept files are reachable rather than invisible disk usage.
 */
export interface StoredBackground {
  path: string;
  /** The name it was picked under, without the uuid prefix Loom stores it with. */
  name: string;
  kind: "image" | "video";
  bytes: number;
  inUse: boolean;
}

export interface BackgroundConfig {
  kind: BackgroundKind;
  preset: string;
  path: string | null;
  /** 0..=100 black overlay strength */
  dim: number;
  /** 0..=64 px blur on the background layer */
  blur: number;
}

/**
 * How the app's colours are chosen.
 *
 * `default` uses the stylesheet's own tokens. `custom` uses three picked
 * colours and derives everything else from them. `adaptive` samples the
 * background image for surfaces and the accent, and takes its text colour from
 * the active theme rather than from the picture — a photograph cannot be
 * trusted with contrast.
 */
export type PaletteMode = "default" | "custom" | "adaptive";

/** The three colours a palette is built from, as hex. */
export interface PaletteConfig {
  mode: PaletteMode;
  accent: string;
  ink: string;
  surface: string;
}

export type ProviderKind = "openai-compatible" | "anthropic";
export type ModelsSource = "manual" | "fetched";
export type Modality = "text" | "image" | "audio" | "video" | "pdf";
export type PermissionMode = "ask" | "auto-read-only" | "auto-all" | "atelier";
/**
 * Plan and Review refuse the write and command tools; Chat is narrower still —
 * it is offered only web search, page fetch, the clock and ask_user, and
 * refuses everything else. Build lets the permission gate decide.
 */
export type AgentMode = "plan" | "build" | "review" | "chat";
/** What a tool can reach; shown as a badge on the permission card. */
export type ToolScope =
  | "workspace"
  | "harness"
  | "web"
  | "mcp"
  | "computer"
  | "browser";

/**
 * Where a model's metadata came from. A refresh may correct anything below
 * `user`; a hand edit is never overwritten.
 */
export type MetadataSource = "user" | "api" | "unknown" | `catalog@${number}`;

export interface ReasoningSpec {
  enabled: boolean;
  variants: string[];
  defaultVariant: string | null;
}

export interface ModelSpec {
  name: string | null;
  context: number | null;
  output: number | null;
  inputModalities: Modality[];
  reasoning: ReasoningSpec | null;
  favorite: boolean;
  source: MetadataSource;
}

export interface ProviderConfig {
  name: string;
  kind: ProviderKind;
  baseUrl: string;
  headers: Record<string, string>;
  enabled: boolean;
  models: Record<string, ModelSpec>;
  modelsSource: ModelsSource;
  lastFetchedAt: number | null;
  keyRequired: boolean;
  /** Header the gateway wants filled with a stable per-chat id. */
  sessionHeader: string | null;
  /**
   * Which built-in preset this instance came from. Set for a *duplicated*
   * provider, whose id no longer matches a preset, so the gateway behaviour
   * still resolves.
   */
  presetId: string | null;
  /**
   * Models the user switched off. A denylist, so anything not listed is
   * selected — which is what makes an existing config (and a newly discovered
   * model) selected without a write.
   */
  disabledModels: string[];
  /** Whether a refresh's newly discovered models start selected. */
  autoSelectModels: boolean;
}

export interface ProviderPreset {
  id: string;
  name: string;
  kind: ProviderKind;
  baseUrl: string;
  keyRequired: boolean;
  note: string;
  sessionHeader: string | null;
}

export interface ModelEntry {
  providerId: string;
  providerName: string;
  kind: ProviderKind;
  /**
   * Ready to use: the provider is on *and* the model is selected. Every picker
   * filters on this one field, so unselecting a model hides it everywhere.
   */
  enabled: boolean;
  /** The provider's own switch, independent of selection. */
  providerEnabled: boolean;
  selected: boolean;
  keyReady: boolean;
  keyRequired: boolean;
  modelId: string;
  spec: ModelSpec;
}

export interface ModelRef {
  providerId: string;
  modelId: string;
}

/**
 * One of the auxiliary models (image generation, embeddings).
 *
 * An empty `providerId` means "whichever provider serves the id", which is what
 * a legacy bare string parses to. Qualified when the same model id is
 * configured on more than one provider.
 */
export interface AuxModelRef {
  providerId: string;
  modelId: string;
}

export interface ChatDefaults {
  providerId: string | null;
  modelId: string | null;
  variant: string | null;
  lite: ModelRef | null;
  imageModel: AuxModelRef | null;
  embeddingModel: AuxModelRef | null;
  autoTitle: boolean;
  recentModels: ModelRef[];
  permissionMode: PermissionMode;
  agentMode: AgentMode;
  maxOutputTokens: number;
  /** Tool round-trips one user turn may take (1-200). */
  maxToolRounds: number;
  /** Thinking effort on computer turns: "off", "low", or null to inherit. */
  computerVariant: string | null;
  /** Fast vision model used only while the Computer chip is armed. */
  computerModel: ModelRef | null;
  /** Longest screenshot edge; 0 is native resolution. */
  computerScreenshotEdge: number;
}

/** One `user`/`assistant` example turn attached to a persona. */
export interface PersonaExample {
  user: string;
  assistant: string;
}

/** Runtime defaults a persona applies when the session has not overridden them. */
export interface PersonaCapabilities {
  temperature: number | null;
  topP: number | null;
  maxOutputTokens: number | null;
  permissionMode: PermissionMode | null;
  agentMode: AgentMode | null;
  /** Tool-name allowlist; empty means every tool the mode offers. */
  tools: string[];
  /** MCP server-id allowlist; empty means every connected server. */
  mcpServers: string[];
}

/** Persistent memory across every chat that uses the persona. */
export interface PersonaMemory {
  enabled: boolean;
  tokenBudget: number;
}

export interface Persona {
  id: string;
  name: string;
  systemPrompt: string;
  modelRef: ModelRef | null;
  variant: string | null;
  /** One-line summary for the picker. */
  description: string;
  tags: string[];
  /** Avatar emoji, e.g. "🛠". */
  emoji: string | null;
  /** Avatar colour token, e.g. "violet". */
  color: string | null;
  favorite: boolean;
  /** Opening assistant message for chats started with this persona. */
  greeting: string;
  /** Optional prompt sections appended after the main prompt. */
  style: string;
  rules: string;
  outputFormat: string;
  examples: PersonaExample[];
  capabilities: PersonaCapabilities;
  memory: PersonaMemory;
  /** Bumped on every write; chats compare their snapshot to flag staleness. */
  revision: number;
  updatedAt: number;
}

/** Who the user is, for `{{user}}` and friends in persona prompts. */
export interface UserProfile {
  name: string;
  pronouns: string;
  about: string;
}

/** A named set of personas; `cast` groups can start multi-persona chats. */
export interface PersonaGroup {
  id: string;
  name: string;
  members: string[];
  cast: boolean;
}

/** One persistent persona memory entry. */
export interface MemoryEntry {
  id: string;
  personaId: string;
  key: string;
  value: string;
  /** `user` or `model`. */
  source: string;
  createdAt: number;
  updatedAt: number;
}

/**
 * A long-term memory: a durable fact about the user (`scope: "global"`) or
 * about one workspace (its folder path).
 */
export interface StoredMemory {
  id: string;
  scope: string;
  content: string;
  pinned: boolean;
  sourceSession: string | null;
  sourceMessage: string | null;
  /** `user`, `model`, or `auto` for the extraction pass. */
  source: string;
  createdAt: number;
  updatedAt: number;
}

export type TaskStatus =
  | "queued"
  | "running"
  | "done"
  | "failed"
  | "interrupted"
  | "cancelled"
  | "skipped";

/** A detached run: a background subagent, or one firing of a job. */
export interface Task {
  id: string;
  /** The hidden session the run's transcript lives in. */
  sessionId: string;
  originSession: string | null;
  jobId: string | null;
  title: string;
  prompt: string;
  providerId: string | null;
  modelId: string | null;
  status: TaskStatus;
  detail: string | null;
  result: string | null;
  notify: boolean;
  createdAt: number;
  startedAt: number | null;
  finishedAt: number | null;
}

export type CommandStatus = "running" | "done" | "failed" | "stopped" | "orphaned";

/**
 * A shell command Loom started. Rows outlive their process: the log stays on
 * disk, so a command that finished (or was orphaned by a restart) is still
 * readable here.
 */
export interface CommandRun {
  id: string;
  /** The chat that started it, when a chat did. */
  sessionId: string | null;
  /** Short label, e.g. "unit tests". Falls back to the command's first line. */
  label: string;
  command: string;
  cwd: string;
  /** Process id of the shell Loom spawned; 0 when it never started. */
  pid: number;
  status: CommandStatus;
  exitCode: number | null;
  logPath: string;
  /** True when the model asked for a background run rather than a timeout. */
  background: boolean;
  createdAt: number;
  finishedAt: number | null;
}

/** A scheduled job: a prompt plus a five-field cron expression. */
export interface Job {
  id: string;
  name: string;
  cron: string;
  enabled: boolean;
  prompt: string;
  providerId: string | null;
  modelId: string | null;
  personaId: string | null;
  workdir: string | null;
  permissionMode: string | null;
  notifyOnSuccess: boolean;
  catchUpMinutes: number;
  lastRunAt: number | null;
  lastStatus: string | null;
  nextRunAt: number | null;
  createdAt: number;
  updatedAt: number;
}

export interface McpServerConfig {
  name: string;
  command: string;
  args: string[];
  env: Record<string, string>;
  enabled: boolean;
}

/* ---------------------------------------------------------------------------
   Glass

   Two palettes' worth of surfaces are tuned by `--pill-bg` and friends, and
   this is the one place the user gets to move them. Everything here is typed in
   Rust as well, and that is not ceremony: `InterfaceConfig` is a struct with a
   container-level `#[serde(default)]` and **no** `extra` catch-all, and
   `set_interface_settings` replaces the whole thing. So an untyped nested key
   would be dropped on every save, the sliders would appear to work, and the
   values would be gone on the next launch.
--------------------------------------------------------------------------- */

/**
 * Refraction mode for a liquid surface.
 *
 * `shader` is deliberately absent from the type rather than merely unoffered in
 * the settings screen: it rasterises its displacement map pixel by pixel in a
 * nested loop on mount and again on every resize, which is the wrong trade for
 * a 40px pill. Leaving it out of the union is what stops a later caller passing
 * it by accident.
 */
export type GlassMode = "standard" | "polar" | "prominent";

/**
 * A refracting surface's parameters — the six numbers, without the switches.
 *
 * The render-side shape. `LiquidGlassConfig` is this plus the on/off flags, and
 * defining one as a subset of the other is what makes it impossible for the two
 * to drift apart.
 */
export type LiquidParams = Omit<
  LiquidGlassConfig,
  "enabled" | SurfaceFlags
>;

/**
 * The per-surface switches, as a unit.
 *
 * Named so the group can be omitted from `LiquidParams` in one place: the six
 * numbers and the six switches are different kinds of thing, and the render-side
 * type wants only the numbers.
 */
type SurfaceFlags =
  | "pills"
  | "composer"
  | "panels"
  | "popovers"
  | "cards"
  | "overlays";

/**
 * A refracting surface's parameters, as stored.
 *
 * Field for field the same as `LiquidParams` in `lib/glass.ts`, which is the
 * same shape minus the flags. The clamps live in `lib/glass.ts` and are applied
 * on the way *out* of here, because Rust stores whatever it is given.
 *
 * `elasticity` is a float, which is safe here in a way an out-of-range integer
 * is not: JSON has no NaN literal and an `f64` accepts any number it can carry,
 * so this field cannot be the reason a config fails to parse. `refraction`,
 * `frost`, `saturation` and `chromatics` stay integers both because they are
 * counts of pixels or percent and because a value outside `u8` would fail the
 * load rather than degrade one field.
 */
export interface LiquidGlassConfig {
  /** Whether any surface refracts at all. The master switch. */
  enabled: boolean;
  /**
   * Which surfaces refract, per group rather than all-or-nothing.
   *
   * Grouped by *where they sit* rather than by component name, because that is
   * what decides how the effect reads and what it costs:
   *
   *  - `pills`    — the title bar. Over the bare background, which is the one
   *                 backdrop a displacement map has nothing to act on; the
   *                 measured worst case, and still worth having because the
   *                 rims catch the drifting artwork.
   *  - `composer` — the largest single area, and the only one that resizes as
   *                 you type. The most expensive, so it gets its own switch.
   *  - `panels`   — torn-off windows in the dock.
   *  - `popovers` — menus and pickers: the model picker, the persona menu, the
   *                 workspace chip, the slash menu. Over the transcript, so
   *                 there is text behind them to bend.
   *  - `cards`    — surfaces inside the transcript: tool calls, the goal panel,
   *                 the question card, queued messages.
   *  - `overlays` — the full-surface ones: the settings drawer, the shortcut
   *                 sheet, voice mode, the update toast.
   */
  pills: boolean;
  composer: boolean;
  panels: boolean;
  popovers: boolean;
  cards: boolean;
  overlays: boolean;
  /** `displacementScale`: how far edge samples are pulled. 0–120. */
  refraction: number;
  /** Backdrop blur in px — *not* the library's own `blurAmount` units. */
  frost: number;
  /** Percent. 100 is neutral. */
  saturation: number;
  /** Chromatic fringing on the rim. 0–5. */
  chromatics: number;
  /** How far the surface follows the pointer. 0 is rigid. */
  elasticity: number;
  mode: GlassMode;
}

/**
 * The app-wide glass knobs.
 *
 * Both multipliers default to 100, so the app looks exactly as it does today
 * until a slider moves. That is deliberate: light theme over bright artwork is
 * already named in `docs/spec.md` as the weakest point in the design, and
 * shipping a glass feature should not trade it away silently.
 */
export interface GlassConfig {
  /**
   * Percent of each surface token's own alpha, 50–100.
   *
   * A multiplier rather than a replacement colour, so `--pill-bg` and
   * `--panel-bg-strong` keep their own values and only their opacity moves.
   * Lower is more see-through, which is what "glassier" means here.
   */
  tint: number;
  /** Percent backdrop blur, 0–200. Higher is frostier. */
  blur: number;
  liquid: LiquidGlassConfig;
}

export type ThinkingDisplay = "collapsed" | "hidden" | "expanded";
/** How much tool-call detail the transcript shows. */
export type ToolCallDisplay = "collapsed" | "expanded" | "hidden";
export type SendKey = "enter" | "ctrl-enter";
/** Whether the chat list is split into workspace groups or shown flat. */
export type SidebarGrouping = "workspace" | "none";
/** Order of chats in the sidebar list. */
export type SidebarSort = "recent" | "oldest" | "title";

export interface InterfaceConfig {
  showThinking: ThinkingDisplay;
  showToolCalls: ToolCallDisplay;
  sendKey: SendKey;
  notifyOnCompletion: boolean;
  hotkeyEnabled: boolean;
  hotkey: string;
  alwaysFollow: boolean;
  sidebarPinned: boolean;
  sidebarWidth: number;
  sidebarGrouping: SidebarGrouping;
  /**
   * Workspace groups the user has collapsed, by folder path; `""` is the
   * "No workspace" group. Persisted, so the fold survives a restart — which is
   * the point of folding four folders you are not working in.
   */
  sidebarCollapsedGroups: string[];
  sidebarSort: SidebarSort;
  /**
   * Hand-placed order of the workspace groups, by folder path. A group listed
   * here keeps that place; one that is not — including a folder just added —
   * sorts above them by recency. In grouped mode this plus each chat's
   * `position` are the whole order; the Sort menu governs the flat list only.
   */
  sidebarWorkspaceOrder: string[];
  compact: boolean;
  /** Attach a screenshot of the current monitor to every quick-ask send. */
  captureOnSend: boolean;
  /** Render ```loom-ui blocks as live, themed widgets. */
  generatedUi: boolean;
  /** Show the faint line under a reply answered from a condensed view. */
  showCondensing: boolean;
  /** Let the lite model propose durable facts after each reply. */
  autoMemory: boolean;
  /** App-wide glass tuning, and the refracting-surface parameters. */
  glass: GlassConfig;
  /**
   * IDE mode: the editor shell has replaced the chat surface.
   *
   * Persisted rather than held for the session, which is a deliberate departure
   * from how the dock's zones behave. A panel is something you *ask* to see, so
   * it opens closed every launch — but IDE mode is a way of working, and someone
   * who has switched to it has said so. Persisting it is the difference between
   * a mode and a dismissal.
   */
  ideMode: boolean;
  /**
   * Width in pixels of the chat column while IDE mode is on.
   *
   * Its own setting rather than `sidebarWidth`, because the two answer different
   * questions — one is "how wide is the chats list", the other "how much room
   * does the model get while I am editing" — and sharing one number would make
   * widening either silently narrow the other.
   */
  ideChatWidth: number;
  ideSidebarOpen: boolean;
  ideSidebarWidth: number;
}

export interface StorageUsage {
  dataDir: string;
  database: number;
  attachments: number;
  generated: number;
  backgrounds: number;
  cache: number;
  total: number;
}

/**
 * Older turns of a chat folded into one block so the request fits the model's
 * window. `digest` is the in-process fallback; `summary` is written by the lite
 * model in the background.
 */
export interface Condensed {
  /** How many messages the block stands in for. */
  covered: number;
  source: "digest" | "summary";
  /** Estimated tokens the block cost. */
  tokens: number;
}

/** The stored condensed block itself, fetched by the expander. */
export interface SessionSummary {
  sessionId: string;
  coversThroughId: string;
  coversThroughAt: number;
  coveredCount: number;
  text: string;
  tokens: number;
  model: string | null;
  updatedAt: number;
}

/** One line in a vendor usage card: a percent window or an amount. */
export interface UsageMetric {
  id: string;
  label: string;
  /** 0-100 when the vendor reports a window percentage. */
  percent: number | null;
  used: number | null;
  limit: number | null;
  remaining: number | null;
  /** `percent`, `usd`, `cny`, `tokens`, `calls`. */
  unit: string;
  resetsAt: string | null;
  resetsAtMs: number | null;
  status: string | null;
  /** Auxiliary text, e.g. DeepSeek's granted/topped-up split. */
  detail: string | null;
}

/** Live usage/quota for one provider, straight from the vendor. */
export interface ProviderUsage {
  providerId: string;
  source: string;
  fetchedAt: number;
  metrics: UsageMetric[];
}

export interface UsageCapableProvider {
  providerId: string;
  name: string;
  source: string;
}

export interface ProviderTotals {
  providerId: string;
  replies: number;
  pricedReplies: number;
  inputTokens: number;
  outputTokens: number;
  estimatedCostUsd: number;
}

/** Tokens and estimated spend recorded in the local database. */
export interface UsageSummary {
  replies: number;
  pricedReplies: number;
  inputTokens: number;
  outputTokens: number;
  estimatedCostUsd: number;
  providers: ProviderTotals[];
}

export interface Prompt {
  id: string;
  title: string;
  body: string;
}

/** A saved folder; chats point at one by path. */
export interface Workspace {
  path: string;
  name: string;
  addedAt: number;
}

/** Which service backs the web tools. */
export type SearchProvider = "auto" | "jina" | "duckduckgo";

/**
 * A band of browser tools. Not a permission — the composer's Browser chip is
 * the consent. Tiers exist because tool schemas are a *fixed* cost against every
 * request's token budget, so a chat that only needs to read a page should not
 * pay for the arbitrary-JavaScript escape hatch on every turn.
 */
export type BrowserTier = "see" | "act" | "dev";

/** How the built-in browser behaves. Mirrors `BrowserConfig` in config.rs. */
export interface BrowserConfig {
  tiers: BrowserTier[];
  /** Longest screenshot edge in pixels; 0 is native resolution. */
  screenshotEdge: number;
  /** Thinking effort for browser turns, when the chat has no explicit variant. */
  variant: string | null;
  /** A fast model used only while the chip is armed; `null` keeps the chat's. */
  model: ModelRef | null;
  /** Tell the model to reach for the browser before `fetch_url` when armed. */
  preferOverFetch: boolean;
  /** `downloads` for the OS folder, `loom` for ~/.loom/browser/downloads. */
  downloadDestination: string;
  /** Open a link in a reply in Loom's browser; Ctrl+click does the opposite. */
  openLinksInBrowser: boolean;
  /**
   * Origins the browser refuses to load, whatever the model asks for. Enforced
   * by the shell rather than by a tool, so no call can route around it. Empty by
   * default: this is a setting, not a permission card.
   */
  blockedOrigins: string[];
  /** Step budget for a browser turn, which is many small round trips. */
  maxSteps: number;
  /** Content blocking: the network half of an ad blocker. */
  blocking: BlockingConfig;
}

/**
 * Content blocking.
 *
 * Not uBlock Origin — WebView2 has no extension API, so that is impossible
 * rather than unimplemented. It is the same technique one layer down: every
 * request is offered to the host before it goes out, so one matching a filter
 * rule is never made. Same lists, network layer only.
 */
export interface BlockingConfig {
  enabled: boolean;
  /** Preset ids that are on, e.g. `easylist`. */
  lists: string[];
  /** Extra list URLs the user added. */
  customLists: string[];
  /** Hosts and domains never blocked, for a site a list breaks. */
  allow: string[];
}

/** One offered filter list, as the settings page lists it. */
export interface FilterListPreset {
  id: string;
  name: string;
  url: string;
}

/**
 * What the blocker is actually doing.
 *
 * `installed` is separate from `enabled` on purpose, and the difference is the
 * point: "switched off" and "switched on but every list failed to download" are
 * very different states, and a page full of ads should not look the same in both.
 */
export interface BlockingStatus {
  enabled: boolean;
  installed: boolean;
  /** Rules loaded across every list. */
  rules: number;
  /** Requests cancelled since launch. */
  blocked: number;
  /** Per-list state, so a list that failed says so rather than being absent. */
  sources: { id: string; name: string; rules: number; error: string | null }[];
}

export interface AppConfig {
  schemaVersion: number;
  theme: Theme;
  background: BackgroundConfig;
  palette: PaletteConfig;
  sidebarCollapsed: boolean;
  providers: Record<string, ProviderConfig>;
  personas: Persona[];
  personaGroups: PersonaGroup[];
  userProfile: UserProfile;
  mcpServers: Record<string, McpServerConfig>;
  chat: ChatDefaults;
  interface: InterfaceConfig;
  prompts: Prompt[];
  workspaces: Workspace[];
  /** Where the docked panels go, keyed by workspace folder path. */
  dock: Record<string, DockLayout>;
  /** The arrangement a folder follows until it has one of its own. */
  dockDefault: DockLayout;
  terminal: TerminalConfig;
  /** The file editor: how it renders, and whether it saves on its own. */
  editor: EditorConfig;
  /** How the AI commit-message button writes its message. */
  commit: CommitConfig;
  browser: BrowserConfig;
  searchProvider: SearchProvider;
}

/* ---------------------------------------------------------------------------
   The dock

   Rust owns this and the UI projects it, rather than the other way round,
   because a panel can be torn off into its own window — and two webviews do not
   share a JavaScript heap, so no `zustand` store can be the source of truth for
   both. Reads come from `dock_layout`, writes go through `set_dock_layout`, and
   the authoritative result arrives on `loom://dock`.
--------------------------------------------------------------------------- */

/** Which window edge a zone is anchored to. */
export type DockEdge = "left" | "right" | "bottom";

/** One docked area: an edge, a size, and the panels stacked in it as tabs. */
export interface DockZone {
  id: string;
  edge: DockEdge;
  /** Width for a side zone, height for the bottom one, in logical pixels. */
  size: number;
  /** Whether it is showing. A closed zone keeps its panels and its size. */
  open: boolean;
  /** Panel ids in tab order. */
  panels: string[];
  /** Which tab is showing. */
  active: number;
}

export interface DockLayout {
  zones: DockZone[];
  /** Shell profile id for this folder's terminal, once one has been chosen. */
  shell: string | null;
}

/** Terminal appearance. A shell is read for hours, so this is the user's call. */
export interface TerminalConfig {
  fontFamily: string;
  fontSize: number;
  /** Percentage, as CSS line-height. */
  lineHeight: number;
  /** WebGL rendering, only when the WebView supports it and the user opts in. */
  webgl: boolean;
}

/** A shell Loom found on this machine. */
export interface PtyProfile {
  id: string;
  name: string;
  /** True for the profile Loom starts when nothing has been chosen. */
  default: boolean;
}

/** A live shell, as the backend sees it. */
export interface PtyInfo {
  id: string;
  profile: string | null;
  workdir: string;
  alive: boolean;
  rows: number;
  cols: number;
}

export interface AppInfo {
  name: string;
  version: string;
  loomHome: string;
}

export interface Session {
  id: string;
  title: string;
  providerId: string | null;
  modelId: string | null;
  variant: string | null;
  personaId: string | null;
  systemPrompt: string | null;
  workdir: string | null;
  permissionMode: PermissionMode | null;
  agentMode: AgentMode | null;
  /** Computer use is armed for this chat (the composer's Computer chip). */
  computerAccess: boolean;
  /** The built-in browser is armed for this chat (the Browser chip). */
  browserAccess: boolean;
  /**
   * Hand-placed row in the chats popup. `null` means this chat has never been
   * dragged, and those keep sorting newest-first above the placed ones — so a
   * chat started after you arranged a group still lands on top of it.
   */
  position: number | null;
  createdAt: number;
  updatedAt: number;
}

export type ToolCallStatus = "running" | "ok" | "error" | "denied";

/** A message waiting its turn while the assistant is busy. */
export interface QueuedMessage {
  id: string;
  text: string;
  attachments: Attachment[];
}

export type TodoStatus = "pending" | "in_progress" | "completed";

/** One item on a chat's live task list, maintained by `todo_write`. */
export interface Todo {
  id: string;
  content: string;
  status: TodoStatus;
  position: number;
}

/** An image a computer tool produced (a screenshot), stored on disk. */
export interface ToolImage {
  name: string;
  mime: string;
  path: string;
}

export interface ToolCallRecord {
  id: string;
  name: string;
  arguments: string;
  status: ToolCallStatus;
  output: string;
  /** Code points of reply text that came before this call, in stream order. */
  after?: number;
  /** Stream order within the turn, breaking ties at the same offset. */
  seq?: number;
  /** Screenshots and other images the call produced, kept on disk. */
  images?: ToolImage[];
}

export interface WorkspaceInfo {
  workdir: string | null;
  branch: string | null;
  isRepo: boolean;
}

/* ---------------------------------------------------------------------------
   Git and the editor

   Mirrors of the shapes in `crates/loom-core/src/git.rs` and `edit.rs`. The
   `#[serde(rename_all = "camelCase")]` on those structs is what makes these the
   same shape rather than a translation — a field renamed on one side and not
   the other is a silent `undefined`, which is the failure `docs/spec.md` records
   under "Streaming fix (the important one)".
--------------------------------------------------------------------------- */

/** One changed path, with both sides of the index. */
export interface GitFile {
  /** Relative to the repository root, always `/`-separated. */
  path: string;
  /** Where a rename came from; null otherwise. */
  from: string | null;
  /** The index differs from HEAD. */
  staged: boolean;
  /** The work tree differs from the index. */
  unstaged: boolean;
  /** git has never seen this path. */
  untracked: boolean;
  /** An unmerged entry — a conflict to resolve before committing. */
  conflicted: boolean;
}

export interface GitStatus {
  isRepo: boolean;
  root: string | null;
  branch: string | null;
  detached: boolean;
  upstream: string | null;
  ahead: number;
  behind: number;
  /** `rebase` | `merge` | `cherry-pick` | `revert` while one is half-finished. */
  operation: string | null;
  files: GitFile[];
}

export interface Branch {
  name: string;
  current: boolean;
  upstream: string | null;
}

export interface GitCommit {
  /** Abbreviated. */
  id: string;
  subject: string;
  author: string;
  /** Unix seconds. */
  at: number;
}

export interface CommitResult {
  id: string;
  subject: string;
  files: number;
}

/** How a file's line endings are written back. */
export type LineEnding = "lf" | "crlf";

/** A file, as the editor loads it. */
export interface TextFile {
  path: string;
  absolute: string;
  /** Line endings normalised to `\n`. */
  text: string;
  /** SHA-256 of the raw bytes on disk, which is what a save checks against. */
  hash: string;
  bytes: number;
  lines: number;
  eol: LineEnding;
  bom: boolean;
  /** Not valid UTF-8; readable, and flagged rather than hidden. */
  lossy: boolean;
  readOnly: boolean;
  /** Freshness hint from size and mtime, so a later stat can compare like
   *  for like without hashing the file. */
  hashHint: string;
}

/**
 * What a save did.
 *
 * A conflict is a **value**, not an error: it is an expected outcome of
 * autosave, and the UI branches on it. An error string would have to be parsed
 * to recover that distinction.
 */
export type SaveOutcome =

  | { kind: "written"; hash: string; bytes: number; hashHint: string }
  | { kind: "conflict"; currentHash: string; modified: number };

/** One entry in a directory listing. */
export interface TreeEntry {
  name: string;
  /** Relative to the workspace root, `/`-separated. */
  path: string;
  isDir: boolean;
}

/** A file's state on disk. */
export interface FileStat {
  path: string;
  exists: boolean;
  bytes: number;
  /** Unix milliseconds, or 0 when the file is gone. */
  modified: number;
  /**
   * A cheap freshness hint: size and mtime together.
   *
   * The exact check is a hash of the whole file, which an editor cannot afford
   * on every tool call. This is what makes the common case — nothing changed —
   * one integer comparison, and only a file that looks different pays for the
   * read.
   */
  hashHint: string;
}

/* ---------------------------------------------------------------------------
   The editor and git, in config
--------------------------------------------------------------------------- */

export type WordWrap = "off" | "on";
export type CommitStyle = "conventional" | "plain";

export interface EditorConfig {
  fontSize: number;
  tabSize: number;
  wordWrap: WordWrap;
  minimap: boolean;
  lineNumbers: boolean;
  autosave: boolean;
  autosaveDelayMs: number;
  renderWhitespace: boolean;
  trimOnSave: boolean;
  insertFinalNewline: boolean;
}

export interface CommitConfig {
  style: CommitStyle;
  includeBody: boolean;
  /** Characters of diff the model is shown. */
  diffBudget: number;
}


export interface PendingPermission {
  sessionId: string;
  messageId: string;
  callId: string;
  name: string;
  arguments: string;
  readOnly: boolean;
  /**
   * Why this call is asking, when the permission mode alone would have let it
   * run. Only `delete_path` produces one: it is the single tool judged by what
   * the call would destroy rather than by its name, so the card has to say
   * what that is.
   */
  reason?: string | null;
}

export interface QuestionOption {
  label: string;
  description?: string | null;
}

/** An `ask_user` call, normalized by the engine. */
export interface AskQuestion {
  question: string;
  header?: string | null;
  options: QuestionOption[];
  allowMultiple: boolean;
  allowFreeText: boolean;
}

export interface PendingQuestion {
  sessionId: string;
  messageId: string;
  callId: string;
  question: AskQuestion;
}

/** What the user picked or typed. Mirrors `tools::Answer` in the engine. */
export interface QuestionAnswer {
  selected: string[];
  text: string | null;
  cancelled: boolean;
}

export interface Message {
  id: string;
  sessionId: string;
  role: "user" | "assistant";
  content: string;
  reasoning: string | null;
  extra: string | null;
  /** Which persona produced an assistant reply, in multi-persona chats. */
  personaId: string | null;
  createdAt: number;
}

export interface Usage {
  inputTokens: number | null;
  outputTokens: number | null;
}

export interface UpdateManifest {
  version: string;
  notes: string;
  payload: string;
  sha256: string;
  signature?: string | null;
}

export type AttachmentKind = "image" | "text" | "pdf";

export interface Attachment {
  id: string;
  kind: AttachmentKind;
  name: string;
  mime: string;
  size: number;
  path: string;
  text?: string | null;
  /** Automatic screenshots: for the model, not rendered as content. */
  hidden?: boolean;
}

export type EngineEvent =
  | { type: "started"; sessionId: string; messageId: string }
  | { type: "delta"; sessionId: string; messageId: string; text: string }
  | {
      type: "reasoning";
      sessionId: string;
      messageId: string;
      text: string;
      /** Reply-text offset this thinking spell preceded; keeps it in place. */
      after: number;
      /** Stream order within the turn, for ties at the same offset. */
      seq: number;
    }
  | {
      type: "toolCallStarted";
      sessionId: string;
      messageId: string;
      callId: string;
      name: string;
      arguments: string;
      /** Stream order within the turn, for ties at the same text offset. */
      seq: number;
    }
  | {
      type: "toolCallFinished";
      sessionId: string;
      messageId: string;
      callId: string;
      ok: boolean;
      output: string;
      images?: ToolImage[];
    }
  | {
      type: "toolPermissionRequest";
      sessionId: string;
      messageId: string;
      callId: string;
      name: string;
      arguments: string;
      readOnly: boolean;
      /** Present only when the mode would otherwise have allowed the call. */
      reason?: string | null;
    }
  | {
      type: "questionRequest";
      sessionId: string;
      messageId: string;
      callId: string;
      question: AskQuestion;
    }
  | {
      type: "done";
      sessionId: string;
      messageId: string;
      content: string;
      reasoning: string | null;
      usage: Usage;
      /**
       * Set when this reply was answered from a condensed view of the chat's
       * older turns. Not a failure: the turn succeeded, and this only says
       * what the model could see.
       */
      condensed?: Condensed | null;
    }
  | {
      /**
       * The turn ended early — a limit, a provider refusal, a loop. Not an
       * error: the partial reply above it stays where it is, and the chat must
       * still be handed back.
       */
      type: "notice";
      sessionId: string;
      /** The turn that stopped; null when no message owns the note. */
      messageId: string | null;
      text: string;
      detail: string | null;
    }
  | { type: "title"; sessionId: string; title: string }
  | { type: "computerPaused"; sessionId: string }
  | { type: "computerResumed"; sessionId: string }
  | {
      /** The model rewrote this chat's task list; the panel follows live. */
      type: "todosChanged";
      sessionId: string;
      todos: Todo[];
    }
  | {
      /** A harness tool changed the config; the UI reloads to stay in step. */
      type: "harnessChanged";
      sessionId: string;
      section: string;
      summary: string;
    }
  | {
      /**
       * A tool finished and may have touched the filesystem.
       *
       * Sent after every call rather than only the writers: deciding whether a
       * given call changed anything cannot cover an MCP server's tools, and what
       * this actually asks — "restat what you have open" — is cheap and always
       * correct. The editor and the git panel are what listen.
       */
      type: "filesChanged";
      sessionId: string;
      tool: string;
    }
  | { type: "taskChanged"; task: Task }
  | {
      /** A shell command Loom started changed status. */
      type: "commandChanged";
      command: CommandRun;
    }
  | { type: "jobChanged"; job: Job }
  | {
      /** The memory pass (or a tool) saved facts. */
      type: "memoryChanged";
      scope: string;
      added: number;
      sessionId: string;
      messageId: string;
    };

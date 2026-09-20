import { create } from "zustand";
import type {
  AppConfig,
  BackgroundConfig,
  GlassConfig,
  PaletteConfig,
  Theme,
} from "../types";
import { backdropFor } from "../lib/background";
import { DEFAULT_GLASS } from "../lib/glass";
import { ipc } from "../lib/ipc";

/**
 * The part of the config that decides what the window *looks* like, mirrored
 * into `localStorage`.
 *
 * The real config lives behind an async IPC call, so on every cold start the
 * first paint used the hardcoded defaults: light theme, no blur, the wrong
 * preset. A dark user watched a light window flip to dark; anyone with a
 * blurred or custom background watched it arrive a frame or two late. Those
 * were not slow loads — the app was painting something it already knew was
 * wrong, because it had not asked yet.
 *
 * `localStorage` is synchronous, so this is readable before the first render.
 * Only what changes pixels is mirrored; everything else still comes from the
 * config, which remains the source of truth and overwrites this a moment later.
 */
const APPEARANCE_KEY = "loomAppearance";

interface CachedAppearance {
  theme?: Theme;
  background?: BackgroundConfig;
  palette?: PaletteConfig;
}

/** Matches `PaletteConfig::default()` in `crates/loom-core/src/config.rs`. */
const DEFAULT_PALETTE: PaletteConfig = {
  mode: "default",
  accent: "#8ea2ff",
  ink: "#f2f5fa",
  surface: "#12151f",
};

/** Matches `BackgroundConfig::default()` in `crates/loom-core/src/config.rs`. */
const DEFAULT_BACKGROUND: BackgroundConfig = {
  kind: "builtin",
  // Theme-following. Pinning `porcelain` as the default is what made the
  // built-in background look absent from the picker: a light preset under
  // dark mode's veil is grey and matches no swatch on offer.
  preset: "auto",
  path: null,
  dim: 0,
  blur: 0,
};

/**
 * The cached appearance, or nothing usable.
 *
 * Never throws and never returns a partial value: a corrupt entry, a field of
 * the wrong type, or storage being unavailable all fall back to the defaults,
 * because the config still arrives a moment later and corrects anything wrong.
 * A cache that could stop the app booting would be a worse bug than the flash
 * it exists to prevent.
 */
function readAppearance(): CachedAppearance {
  try {
    const raw = localStorage.getItem(APPEARANCE_KEY);
    if (!raw) return {};
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== "object" || parsed === null) return {};
    const { theme, background, palette } = parsed as CachedAppearance;
    return {
      theme: theme === "dark" || theme === "light" ? theme : undefined,
      // Spread over the defaults so a cache written by an older build cannot
      // introduce a missing key that the rest of the app assumes exists.
      background:
        background && typeof background === "object"
          ? { ...DEFAULT_BACKGROUND, ...background }
          : undefined,
      palette:
        palette && typeof palette === "object"
          ? { ...DEFAULT_PALETTE, ...palette }
          : undefined,
    };
  } catch {
    return {};
  }
}

/** Mirrors the visible part of a config so the next launch can start correctly. */
function cacheAppearance(config: AppConfig): void {
  try {
    localStorage.setItem(
      APPEARANCE_KEY,
      JSON.stringify({
        theme: config.theme,
        background: config.background,
        palette: config.palette,
      } satisfies CachedAppearance),
    );
  } catch {
    // Private mode, or a full quota. The app works; it just flashes on launch.
  }
}

const CACHED = readAppearance();

/**
 * Applies the cached theme to the document, before React mounts.
 *
 * Without this the class arrived with React's first commit, so a dark-mode user
 * saw one painted frame of the light theme. Called from `main.tsx` as early as
 * the bundle allows.
 */
export function applyCachedAppearance(): void {
  if (typeof document === "undefined") return;
  // Re-read rather than using `CACHED`. The function's job is "apply what is
  // cached *now*", and reading at call time is what makes that true — the
  // module-level read only exists to seed `DEFAULT_CONFIG` at boot.
  const dark = readAppearance().theme === "dark";
  document.documentElement.classList.toggle("dark", dark);
  document.documentElement.style.background = backdropFor(dark);
}

export const DEFAULT_CONFIG: AppConfig = {
  schemaVersion: 1,
  // Hydrated from the cache, so `App`'s first render already has the right
  // theme and the right background instead of flipping once the config lands.
  theme: CACHED.theme ?? "light",
  background: CACHED.background ?? DEFAULT_BACKGROUND,
  // Seeded from the launch cache for the same reason as the theme: a custom
  // palette that arrived with the config would flash the default first.
  palette: CACHED.palette ?? DEFAULT_PALETTE,
  sidebarCollapsed: false,
  providers: {},
  personas: [],
  personaGroups: [],
  userProfile: { name: "", pronouns: "", about: "" },
  mcpServers: {},
  chat: {
    providerId: null,
    modelId: null,
    variant: null,
    lite: null,
    imageModel: null,
    embeddingModel: null,
    autoTitle: true,
    recentModels: [],
    permissionMode: "ask",
    agentMode: "build",
    maxOutputTokens: 0,
    maxToolRounds: 40,
    computerVariant: "low",
    computerModel: null,
    computerScreenshotEdge: 0,
  },
  interface: {
    showThinking: "collapsed",
    showToolCalls: "collapsed",
    sendKey: "enter",
    notifyOnCompletion: true,
    hotkeyEnabled: true,
    hotkey: "Ctrl+Shift+Space",
    alwaysFollow: false,
    sidebarPinned: false,
    sidebarWidth: 264,
    sidebarGrouping: "workspace",
    sidebarCollapsedGroups: [],
    sidebarSort: "recent",
    sidebarWorkspaceOrder: [],
    compact: false,
    generatedUi: true,
    showCondensing: true,
    captureOnSend: true,
    autoMemory: true,
    // Present here as well as in Rust, and that is the whole point: `saveConfig`
    // replaces the entire struct, so a default without `glass` would silently
    // reset the user's sliders on the next unrelated save.
    glass: DEFAULT_GLASS,
  },
  prompts: [],
  workspaces: [],
  // Mirrors `DockLayout::default()` in `dock.rs`.
  //
  // **Every zone closed.** Loom opens on the conversation, and every panel is a
  // click or a keystroke away. This shipped with the chats list open, which
  // meant the app decided on every launch that a panel was wanted — and a panel
  // is something you choose to look at. It also covered the first 300px of the
  // reply you had just launched the app to read.
  dock: {},
  dockDefault: {
    zones: [
      { id: "left", edge: "left", size: 300, open: false, panels: ["sessions"], active: 0 },
      { id: "right", edge: "right", size: 460, open: false, panels: ["terminal"], active: 0 },
      { id: "bottom", edge: "bottom", size: 260, open: false, panels: ["runs"], active: 0 },
    ],
    shell: null,
  },
  terminal: {
    fontFamily: "JetBrains Mono",
    fontSize: 13,
    lineHeight: 130,
    webgl: false,
  },
  // Mirrors `BrowserConfig::default()` in `config.rs`.
  browser: {
    // See and Act are on; Dev — `browser_evaluate`'s arbitrary JavaScript, and
    // the cookie and storage writers — ships off, because an escape hatch should
    // be a decision the user makes rather than one they discover.
    tiers: ["see", "act"],
    screenshotEdge: 0,
    variant: "low",
    model: null,
    preferOverFetch: true,
    downloadDestination: "downloads",
    openLinksInBrowser: true,
    blockedOrigins: [],
    maxSteps: 80,
    blocking: {
      // Mirrors `BlockingConfig::default()`: on, with the two lists that
      // between them catch most of what "an ad blocker" means.
      enabled: true,
      lists: ["peter-lowe", "easylist"],
      customLists: [],
      allow: [],
    },
  },
  searchProvider: "auto",
};

interface SettingsState {
  config: AppConfig;
  loaded: boolean;
  load: () => Promise<void>;
  applyRemote: (config: AppConfig) => void;
  setTheme: (theme: Theme) => void;
  setBackground: (patch: Partial<BackgroundConfig>) => void;
  setPalette: (patch: Partial<PaletteConfig>) => void;
  setGlass: (patch: Partial<GlassConfig>) => void;
  /** The refracting surfaces, one level deeper than `setGlass`. */
  setLiquid: (patch: Partial<GlassConfig["liquid"]>) => void;
}

let saveTimer: number | undefined;

/** Debounced persist so slider drags don't hammer the config file. */
function scheduleSave(get: () => SettingsState): void {
  if (saveTimer !== undefined) window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => {
    saveTimer = undefined;
    void ipc.saveConfig(get().config);
  }, 250);
}

export const useSettings = create<SettingsState>((set, get) => ({
  config: DEFAULT_CONFIG,
  loaded: false,

  load: async () => {
    const remote = await ipc.getConfig();
    const config = remote ?? DEFAULT_CONFIG;
    cacheAppearance(config);
    set({ config, loaded: true });
  },

  // Every path that can change the appearance writes the cache, not just the
  // two setters: a config edited on another machine, or on a first run, arrives
  // through here and would otherwise leave the next launch painting the old
  // theme.
  applyRemote: (config) => {
    cacheAppearance(config);
    set({ config });
  },

  setTheme: (theme) => {
    const config = { ...get().config, theme };
    cacheAppearance(config);
    set({ config });
    scheduleSave(get);
  },

  setBackground: (patch) => {
    const config = {
      ...get().config,
      background: { ...get().config.background, ...patch },
    };
    cacheAppearance(config);
    set({ config });
    scheduleSave(get);
  },

  setPalette: (patch) => {
    const config = {
      ...get().config,
      palette: { ...get().config.palette, ...patch },
    };
    // The palette is part of the appearance the launch cache carries, because a
    // custom theme that appeared a frame late would be the same flash the cache
    // exists to prevent.
    cacheAppearance(config);
    set({ config });
    scheduleSave(get);
  },

  // Glass deliberately does **not** go into the launch cache. It is not part of
  // what makes the first frame wrong — the tint and blur multipliers only move
  // when the user drags a slider, and the surfaces are already painted by then —
  // so caching it would add a second place for the value to be stale without
  // preventing a flash.
  //
  // Both persist through `saveConfig`, which writes the whole config: the same
  // route `setBackground` and `setPalette` take. No new Rust command, and
  // nothing to keep in step when a field is added.
  setGlass: (patch) => {
    const config = {
      ...get().config,
      interface: {
        ...get().config.interface,
        glass: { ...get().config.interface.glass, ...patch },
      },
    };
    set({ config });
    scheduleSave(get);
  },

  setLiquid: (patch) => {
    const config = {
      ...get().config,
      interface: {
        ...get().config.interface,
        glass: {
          ...get().config.interface.glass,
          liquid: { ...get().config.interface.glass.liquid, ...patch },
        },
      },
    };
    set({ config });
    scheduleSave(get);
  },
}));

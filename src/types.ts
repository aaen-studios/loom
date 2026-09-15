export type Theme = "light" | "dark";

export type BackgroundKind = "builtin" | "image" | "video";

export interface BackgroundConfig {
  kind: BackgroundKind;
  preset: string;
  path: string | null;
  /** 0..=100 black overlay strength */
  dim: number;
  /** 0..=64 px blur on the background layer */
  blur: number;
}

export interface AppConfig {
  schemaVersion: number;
  theme: Theme;
  background: BackgroundConfig;
  sidebarCollapsed: boolean;
}

export interface AppInfo {
  name: string;
  version: string;
  loomHome: string;
}

export interface ChatMessage {
  id: string;
  role: "user" | "assistant";
  content: string;
  pending?: boolean;
}

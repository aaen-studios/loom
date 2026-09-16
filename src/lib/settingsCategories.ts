/**
 * The settings categories, as a type the UI store can hold so the composer's
 * usage badge can deep-link straight to "Usage". The panel owns the labels,
 * blurbs, keywords, and icons; `satisfies` keeps this list honest.
 */
export type SettingsCategoryId =
  | "general"
  | "appearance"
  | "chat"
  | "tools"
  | "providers"
  | "usage"
  | "personas"
  | "memory"
  | "voice"
  | "mcp"
  | "skills"
  | "data"
  | "updates";

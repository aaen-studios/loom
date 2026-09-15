import { useEffect, useState, type ReactNode } from "react";
import { cn } from "../lib/cn";
import { BACKGROUND_PRESETS } from "../lib/background";
import { call } from "../lib/tauri";
import type { AppInfo, Theme } from "../types";
import { useSettings } from "../stores/settings";
import { useUi } from "../stores/ui";
import { CloseIcon, MoonIcon, SunIcon } from "./icons";

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
      <span className="text-[13px] text-soft">{label}</span>
      {children}
    </div>
  );
}

const THEME_OPTIONS: { id: Theme; label: string; icon: ReactNode }[] = [
  { id: "light", label: "Light", icon: <SunIcon size={15} /> },
  { id: "dark", label: "Dark", icon: <MoonIcon size={15} /> },
];

/**
 * Settings drawer. M0 covers appearance (theme, background, dim, blur) and
 * the data directory. Providers, personas, permissions, and tools join here
 * in M1/M3.
 */
export function SettingsPanel() {
  const open = useUi((state) => state.settingsOpen);
  const setOpen = useUi((state) => state.setSettingsOpen);
  const config = useSettings((state) => state.config);
  const setTheme = useSettings((state) => state.setTheme);
  const setBackground = useSettings((state) => state.setBackground);
  const [info, setInfo] = useState<AppInfo | null>(null);

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

  return (
    <div className="absolute inset-0 z-40 flex justify-end p-3 pt-16">
      <button
        type="button"
        aria-label="Close settings"
        onClick={() => setOpen(false)}
        className="absolute inset-0 cursor-default bg-black/10"
      />

      <div className="animate-fade-up glass relative flex h-full w-[380px] flex-col overflow-hidden rounded-2xl">
        <div className="flex items-center justify-between px-4 py-3">
          <h2 className="text-[14.5px] font-semibold">Settings</h2>
          <button
            type="button"
            aria-label="Close settings"
            onClick={() => setOpen(false)}
            className="glass-hover grid h-8 w-8 place-items-center rounded-lg text-soft"
          >
            <CloseIcon size={16} />
          </button>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto">
          <Section title="Appearance">
            <Row label="Theme">
              <div className="flex rounded-full border border-[var(--glass-border)] p-0.5">
                {THEME_OPTIONS.map((option) => (
                  <button
                    key={option.id}
                    type="button"
                    onClick={() => setTheme(option.id)}
                    className={cn(
                      "flex items-center gap-1.5 rounded-full px-3 py-1 text-[12.5px] transition",
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
          </Section>

          <Section title="Background">
            <div className="grid grid-cols-3 gap-2">
              {BACKGROUND_PRESETS.map((preset) => (
                <button
                  key={preset.id}
                  type="button"
                  title={preset.name}
                  onClick={() =>
                    setBackground({
                      kind: "builtin",
                      preset: preset.id,
                      path: null,
                    })
                  }
                  className={cn(
                    "group relative h-16 overflow-hidden rounded-xl border transition",
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

            <p className="mt-3 text-[12px] leading-5 text-faint">
              Your own images and video backgrounds arrive in M2.
            </p>

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
        </div>
      </div>
    </div>
  );
}

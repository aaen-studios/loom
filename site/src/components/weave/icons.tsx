import type { ReactNode } from "react";

/**
 * The icon set for the warp.
 *
 * Drawn inline at 18px on a 24-unit grid with the app's 1.7 stroke. An icon
 * package would be one more thing to hold in step with the product for the sake of
 * nine paths.
 *
 * Each one is the *smallest honest picture* of the thread it heads — a brain for
 * reasoning, a shield for permission modes, a terminal for commands that outlive
 * the turn. Where no honest glyph existed, the thread gets none rather than a
 * decorative shape standing in for an idea.
 */

export function Icon({ children, size = 18 }: { children: ReactNode; size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.7}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

export const Icons = {
  /** Two lobes, for a reasoning panel. */
  Brain: () => (
    <Icon>
      <path d="M12 5.5a3 3 0 0 0-5.7 1.3A3 3 0 0 0 5 12a3 3 0 0 0 1.6 2.6A3 3 0 0 0 12 18.5z" />
      <path d="M12 5.5a3 3 0 0 1 5.7 1.3A3 3 0 0 1 19 12a3 3 0 0 1-1.6 2.6A3 3 0 0 1 12 18.5z" />
    </Icon>
  ),
  Shield: () => (
    <Icon>
      <path d="M12 3l7.5 3v5.5c0 4.6-3.1 8.4-7.5 9.5-4.4-1.1-7.5-4.9-7.5-9.5V6z" />
      <path d="M9 12l2.2 2.2L15.5 10" />
    </Icon>
  ),
  Terminal: () => (
    <Icon>
      <rect x="3" y="4.5" width="18" height="15" rx="3" />
      <path d="M7.5 10l2.5 2.5-2.5 2.5M12.5 15h4" />
    </Icon>
  ),
  Layers: () => (
    <Icon>
      <path d="M12 3.5l8 4.2-8 4.2-8-4.2z" />
      <path d="M4 12.5l8 4.2 8-4.2M4 16.5l8 4.2 8-4.2" />
    </Icon>
  ),
  Folder: () => (
    <Icon>
      <path d="M3 7.5A2.5 2.5 0 0 1 5.5 5h3.2l2 2.2h7.8A2.5 2.5 0 0 1 21 9.7v7.3a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 17z" />
    </Icon>
  ),
  Plug: () => (
    <Icon>
      <path d="M9 3.5v5M15 3.5v5" />
      <path d="M6.5 8.5h11v3a5.5 5.5 0 0 1-5.5 5.5A5.5 5.5 0 0 1 6.5 11.5z" />
      <path d="M12 17v3.5" />
    </Icon>
  ),
  Chart: () => (
    <Icon>
      <path d="M4 20V10M10 20V4M16 20v-7M22 20H2" />
    </Icon>
  ),
  Mouse: () => (
    <Icon>
      <rect x="7" y="3" width="10" height="18" rx="5" />
      <path d="M12 7v3" />
    </Icon>
  ),
  /** A panel with a chat column in it: a reply that renders live UI. */
  Widget: () => (
    <Icon>
      <rect x="3.5" y="4.5" width="17" height="15" rx="3" />
      <path d="M3.5 9.5h17M9 9.5v10" />
    </Icon>
  ),
} as const;

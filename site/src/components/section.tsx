import type { ReactNode } from "react";

/**
 * Shared furniture for the landing sections.
 *
 * The section rhythm lives here so every band on the page has the same
 * measure and the same vertical spacing. Without it, six sections written at
 * different times drift apart by a few pixels each, which reads as sloppy
 * without anyone being able to point at what is wrong.
 */
export function Section({
  id,
  eyebrow,
  title,
  lead,
  children,
  className,
}: {
  id?: string;
  eyebrow?: string;
  title: string;
  lead?: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section id={id} className={`scroll-mt-24 px-4 py-16 sm:px-6 sm:py-20 ${className ?? ""}`}>
      <div className="mx-auto max-w-5xl">
        <header className="max-w-2xl">
          {eyebrow && (
            <p className="text-faint text-[12px] font-medium tracking-[0.14em] uppercase">
              {eyebrow}
            </p>
          )}
          <h2 className="mt-3 text-[26px] leading-tight font-medium tracking-tight text-balance sm:text-[32px]">
            {title}
          </h2>
          {lead && (
            <p className="text-soft mt-4 text-[15px] leading-6">{lead}</p>
          )}
        </header>
        <div className="mt-10">{children}</div>
      </div>
    </section>
  );
}

/**
 * A feature card.
 *
 * `panel rounded-control` rather than a bespoke card style: the app's own glass
 * surface, so a feature card and a settings section in the product are the same
 * object at different scales.
 */
export function Feature({
  icon,
  title,
  children,
}: {
  icon: ReactNode;
  title: string;
  children: ReactNode;
}) {
  return (
    <article className="panel rounded-control p-4">
      <span className="text-[var(--accent)] mb-3 block">{icon}</span>
      <h3 className="text-[14.5px] font-medium">{title}</h3>
      <p className="text-soft mt-2 text-[13.5px] leading-[1.6]">{children}</p>
    </article>
  );
}

/** A grid of features. Auto-fitting, so it reflows without breakpoint math. */
export function FeatureGrid({ children }: { children: ReactNode }) {
  return (
    <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">{children}</div>
  );
}

/* ---------------------------------------------------------------------------
   Icons

   Drawn inline at 18px on a 24-unit grid, matching the app's icon weight
   (1.7 stroke). A shared icon dependency would be one more thing to keep in
   step with the app for no benefit.
--------------------------------------------------------------------------- */

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
  Widget: () => (
    <Icon>
      <rect x="3.5" y="4.5" width="17" height="15" rx="3" />
      <path d="M3.5 9.5h17M9 9.5v10" />
    </Icon>
  ),
  Command: () => (
    <Icon>
      <path d="M8 4a3 3 0 1 0 0 6h8a3 3 0 1 0 0-6v6a3 3 0 1 0 3 3H5a3 3 0 1 0 3-3z" />
    </Icon>
  ),
} as const;

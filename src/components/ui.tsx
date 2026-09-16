import type {
  ButtonHTMLAttributes,
  KeyboardEvent,
  ReactNode,
  RefObject,
} from "react";
import { cn } from "../lib/cn";
import { CloseIcon, SearchIcon } from "./icons";

/* ---------------------------------------------------------------------------
   Shared surface primitives

   The sidebar and the settings drawer are the same object at different
   scales: a list of rows on a glass card. These are the pieces both use, so
   the two screens cannot drift apart.
--------------------------------------------------------------------------- */

/** A grouped card of rows. Rows inside share one hairline between them. */
export function Section({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <section className="mb-3 rounded-control border border-[var(--glass-border)] bg-[var(--card-bg)] px-3 py-3 last:mb-0">
      <h3 className="px-1 text-[11px] font-semibold tracking-[0.09em] text-faint uppercase">
        {title}
      </h3>
      {description && (
        <p className="mt-1 px-1 text-[11.5px] leading-4 text-faint">{description}</p>
      )}
      <div className="mt-1.5 [&>*+*]:border-t [&>*+*]:border-[var(--glass-border)]">
        {children}
      </div>
    </section>
  );
}

/** Label on the left, control on the right.
 *
 * `children` is optional because a display-only row is a real thing — a
 * component that is simply installed or not, a path, a version. */
export function Row({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children?: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-4 px-1 py-2.5">
      <span className="min-w-0 flex-1">
        <span className="block text-[13px] text-soft">{label}</span>
        {hint && (
          <span className="block text-[11.5px] leading-4 text-faint">{hint}</span>
        )}
      </span>
      {children}
    </div>
  );
}

/**
 * The shared field surface: inputs, selects and textareas inside a card.
 * Width and font size are deliberately absent here — a caller that sets
 * `w-44` or a smaller mono size must not fight a `w-full`/`text-[13px]`
 * baked into the base (Tailwind resolves that clash by stylesheet order,
 * not by class order).
 */
export const fieldBase =
  "rounded-row border border-[var(--glass-border)] bg-[var(--hover-bg)] px-2.5 py-1.5 text-[var(--ink)] placeholder:text-[var(--ink-faint)] transition-colors focus:border-[var(--accent)] focus:shadow-[0_0_0_3px_var(--accent-soft)]";

/** A field that fills its container at the standard reading size. */
export const inputClass = cn("w-full text-[13px]", fieldBase);

/** Small switch used across the sections. */
export function Toggle({
  checked,
  onChange,
  label,
  hint,
  disabled,
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
  label: string;
  hint?: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className="flex w-full items-start justify-between gap-4 rounded-row px-1 py-2.5 text-left focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-[var(--accent)] disabled:opacity-50"
    >
      <span className="min-w-0">
        <span className="block text-[13px] text-soft">{label}</span>
        {hint && (
          <span className="block text-[11.5px] leading-4 text-faint">{hint}</span>
        )}
      </span>
      <span
        aria-hidden="true"
        className={cn(
          "mt-0.5 flex h-5 w-9 shrink-0 items-center rounded-capsule border p-0.5 transition-colors",
          checked
            ? "border-transparent bg-[var(--accent)]"
            : "border-[var(--glass-border)] bg-[var(--ink-ghost)]",
        )}
      >
        <span
          className={cn(
            "h-3.5 w-3.5 rounded-capsule transition-transform",
            checked
              ? "translate-x-4 bg-white"
              : "translate-x-0 bg-[var(--ink-faint)]",
          )}
        />
      </span>
    </button>
  );
}

/** Two or three mutually exclusive options, e.g. thinking display. */
export function Segmented<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { id: T; label: string; title?: string; icon?: ReactNode }[];
  onChange: (value: T) => void;
}) {
  return (
    <div className="flex rounded-capsule border border-[var(--glass-border)] bg-[var(--ink-ghost)] p-0.5">
      {options.map((option) => (
        <button
          key={option.id}
          type="button"
          title={option.title}
          aria-pressed={value === option.id}
          onClick={() => onChange(option.id)}
          className={cn(
            "flex items-center gap-1.5 rounded-capsule px-2.5 py-1 text-[12px] transition-colors",
            "focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-[var(--accent)]",
            value === option.id
              ? "bg-[var(--control-bg)] text-[var(--control-ink)]"
              : "text-soft hover:text-[var(--ink)]",
          )}
        >
          {option.icon}
          {option.label}
        </button>
      ))}
    </div>
  );
}

/** A search input with its own icon and a clear affordance. */
export function SearchField({
  value,
  onChange,
  placeholder,
  id,
  inputRef,
  onKeyDown,
  className,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder: string;
  id?: string;
  inputRef?: RefObject<HTMLInputElement | null>;
  onKeyDown?: (event: KeyboardEvent<HTMLInputElement>) => void;
  className?: string;
}) {
  return (
    <div className={cn("relative", className)}>
      <SearchIcon
        size={14}
        className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-faint"
      />
      <input
        id={id}
        ref={inputRef}
        type="text"
        value={value}
        placeholder={placeholder}
        onChange={(event) => onChange(event.currentTarget.value)}
        onKeyDown={onKeyDown}
        className="w-full rounded-row border border-[var(--glass-border)] bg-[var(--hover-bg)] py-1.5 pr-7 pl-8 text-[12.5px] text-[var(--ink)] transition-colors placeholder:text-[var(--ink-faint)] focus:border-[var(--accent)] focus:shadow-[0_0_0_3px_var(--accent-soft)]"
      />
      {value && (
        <button
          type="button"
          aria-label="Clear search"
          onClick={() => onChange("")}
          className="absolute top-1/2 right-1.5 grid h-5 w-5 -translate-y-1/2 place-items-center rounded-capsule text-faint hover:bg-[var(--hover-bg)] hover:text-[var(--ink)]"
        >
          <CloseIcon size={12} />
        </button>
      )}
    </div>
  );
}

/** A quiet square icon button, with an active tint and a danger tone. */
export function IconButton({
  label,
  active,
  tone = "quiet",
  className,
  children,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  label: string;
  active?: boolean;
  tone?: "quiet" | "danger";
}) {
  return (
    <button
      type="button"
      {...rest}
      title={label}
      aria-label={label}
      className={cn(
        "grid h-7 w-7 shrink-0 place-items-center rounded-control transition-colors",
        "focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]",
        tone === "danger"
          ? "text-faint hover:bg-[var(--hover-bg)] hover:text-[var(--danger)]"
          : active
            ? "text-[var(--accent)] hover:bg-[var(--hover-bg)]"
            : "text-faint hover:bg-[var(--hover-bg)] hover:text-[var(--ink)]",
        className,
      )}
    >
      {children}
    </button>
  );
}

/** A keycap: the physical shape of the shortcut it names. */
export function Kbd({ children }: { children: ReactNode }) {
  return <kbd className="kbd">{children}</kbd>;
}

/** The empty state of any list: a mark, a reason, and a way forward. */
export function EmptyState({
  icon,
  title,
  hint,
  action,
}: {
  icon?: ReactNode;
  title: string;
  hint?: string;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-col items-center gap-1.5 px-6 py-10 text-center">
      {icon && <span className="mb-1 text-faint opacity-70">{icon}</span>}
      <p className="text-[13px] text-soft">{title}</p>
      {hint && (
        <p className="max-w-[230px] text-[12px] leading-5 text-faint">{hint}</p>
      )}
      {action && <div className="mt-2">{action}</div>}
    </div>
  );
}

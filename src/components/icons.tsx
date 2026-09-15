import type { SVGProps } from "react";

type IconProps = SVGProps<SVGSVGElement> & { size?: number };

function Icon({ size = 18, children, ...rest }: IconProps) {
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
      {...rest}
    >
      {children}
    </svg>
  );
}

export function PanelLeftIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <rect x="3" y="4" width="18" height="16" rx="3" />
      <path d="M9.5 4v16" />
    </Icon>
  );
}

export function MinimizeIcon(props: IconProps) {
  return (
    <Icon {...props} strokeWidth={1.9}>
      <path d="M5.5 12h13" />
    </Icon>
  );
}

export function MaximizeIcon(props: IconProps) {
  return (
    <Icon {...props} strokeWidth={1.9}>
      <rect x="5.5" y="5.5" width="13" height="13" rx="1.6" />
    </Icon>
  );
}

export function RestoreIcon(props: IconProps) {
  return (
    <Icon {...props} strokeWidth={1.9}>
      <rect x="4.5" y="7.5" width="11" height="11" rx="1.6" />
      <path d="M9 7.5V6.4A1.9 1.9 0 0 1 10.9 4.5h6.7A1.9 1.9 0 0 1 19.5 6.4v6.7a1.9 1.9 0 0 1-1.9 1.9H16" />
    </Icon>
  );
}

export function CloseIcon(props: IconProps) {
  return (
    <Icon {...props} strokeWidth={1.9}>
      <path d="M6.5 6.5l11 11M17.5 6.5l-11 11" />
    </Icon>
  );
}

export function PlusIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M12 5.5v13M5.5 12h13" />
    </Icon>
  );
}

export function ChevronDownIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M6.5 9.5l5.5 5.5 5.5-5.5" />
    </Icon>
  );
}

export function SettingsIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="12" cy="12" r="3.1" />
      <path d="M19.2 13.6a7.6 7.6 0 0 0 0-3.2l1.9-1.4-1.9-3.3-2.2.9a7.6 7.6 0 0 0-2.8-1.6L13.7 2h-3.4l-.5 2.9a7.6 7.6 0 0 0-2.8 1.6l-2.2-.9-1.9 3.3 1.9 1.4a7.6 7.6 0 0 0 0 3.2l-1.9 1.4 1.9 3.3 2.2-.9a7.6 7.6 0 0 0 2.8 1.6l.5 2.9h3.4l.5-2.9a7.6 7.6 0 0 0 2.8-1.6l2.2.9 1.9-3.3z" />
    </Icon>
  );
}

export function PaperclipIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M20 11.5l-8.1 8.1a5.2 5.2 0 0 1-7.4-7.4l8.6-8.6a3.5 3.5 0 0 1 4.9 4.9l-8.5 8.5a1.8 1.8 0 0 1-2.5-2.5l7.8-7.8" />
    </Icon>
  );
}

export function ArrowUpIcon(props: IconProps) {
  return (
    <Icon {...props} strokeWidth={2}>
      <path d="M12 19V5.5M6.5 11L12 5.5 17.5 11" />
    </Icon>
  );
}

export function SparkIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M12 3.5l1.9 5.1 5.1 1.9-5.1 1.9L12 17.5l-1.9-5.1L5 10.5l5.1-1.9z" />
      <path d="M18.5 16.5l.8 2 2 .8-2 .8-.8 2-.8-2-2-.8 2-.8z" />
    </Icon>
  );
}

export function SunIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2.8v2.1M12 19.1v2.1M4.9 4.9l1.5 1.5M17.6 17.6l1.5 1.5M2.8 12h2.1M19.1 12h2.1M4.9 19.1l1.5-1.5M17.6 6.4l1.5-1.5" />
    </Icon>
  );
}

export function MoonIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M20 13.6A8.2 8.2 0 0 1 10.4 4a8.3 8.3 0 1 0 9.6 9.6z" />
    </Icon>
  );
}

export function FolderIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M4 7.5A2.5 2.5 0 0 1 6.5 5h3.2l1.8 2h6A2.5 2.5 0 0 1 20 9.5v7A2.5 2.5 0 0 1 17.5 19h-11A2.5 2.5 0 0 1 4 16.5z" />
    </Icon>
  );
}

export function GitBranchIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="6.5" cy="6" r="2.2" />
      <circle cx="6.5" cy="18" r="2.2" />
      <circle cx="17.5" cy="9" r="2.2" />
      <path d="M6.5 8.2v7.6M8.7 6h4.3A4.5 4.5 0 0 1 17.5 10.5V10" />
    </Icon>
  );
}

/** Brand glyph: three woven threads. */
export function LoomMark(props: IconProps) {
  return (
    <svg
      width={props.size ?? 18}
      height={props.size ?? 18}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.9}
      strokeLinecap="round"
      aria-hidden="true"
    >
      <path d="M6.5 5.5c0 6.5 5.5 6.5 5.5 13" />
      <path d="M12 5.5c0 6.5 5.5 6.5 5.5 13" />
      <path d="M6.5 18.5h11" opacity="0.55" />
    </svg>
  );
}

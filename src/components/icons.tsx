import type { SVGProps } from "react";
import { cn } from "../lib/cn";

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

/** A terminal: the docked shell. */
export function TerminalIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <rect x="3" y="4.5" width="18" height="15" rx="3" />
      <path d="m7.5 9.5 3 2.5-3 2.5" />
      <path d="M13 15h3.5" />
    </Icon>
  );
}

/** Files and changes: the diff and tree panel. */
export function FilesIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M6 3.5h7.5L19 9v11.5H6z" />
      <path d="M13.5 3.5V9H19" />
    </Icon>
  );
}

/** A panel being torn off into its own window. */
export function PopOutIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M14 4.5h5.5V10" />
      <path d="M19.5 4.5 12 12" />
      <path d="M18 14.5v4a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 4 18.5v-11A1.5 1.5 0 0 1 5.5 6h4" />
    </Icon>
  );
}

/** The world: the browser panel, once it lands. */
export function GlobeIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="12" cy="12" r="8.5" />
      <path d="M3.5 12h17" />
      <path d="M12 3.5c2.2 2.4 3.3 5.3 3.3 8.5S14.2 18.1 12 20.5c-2.2-2.4-3.3-5.3-3.3-8.5S9.8 5.9 12 3.5Z" />
    </Icon>
  );
}

/** A speaker cone with waves: voice mode, and reading a reply aloud. */
export function SoundIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M11 4.8 6.9 8.4H3.6v7.2h3.3L11 19.2z" />
      <path d="M14.6 9a4.4 4.4 0 0 1 0 6" />
      <path d="M17.4 6.4a8 8 0 0 1 0 11.2" />
    </Icon>
  );
}

/**
 * A microphone: dictation, and the voice-mode dial.
 *
 * The capsule and the cradle are separate strokes rather than one outline, so
 * the shape still reads at 16 px next to a paperclip.
 */
export function MicIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <rect x="9" y="2.5" width="6" height="12" rx="3" />
      <path d="M5.5 11.5a6.5 6.5 0 0 0 13 0" />
      <path d="M12 18v3.5" />
    </Icon>
  );
}

/** A microphone with a slash: the microphone is open, so this stops it. */
export function MicOffIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <rect x="9" y="2.5" width="6" height="12" rx="3" />
      <path d="M5.5 11.5a6.5 6.5 0 0 0 13 0" />
      <path d="M12 18v3.5" />
      <path d="M4 3.5 20 20.5" />
    </Icon>
  );
}

export function MinimizeIcon(props: IconProps) {  return (
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

export function PlayIcon(props: IconProps) {
  return (
    <Icon {...props} strokeWidth={1.6}>
      <path d="M8.5 5.8l9.2 6.2-9.2 6.2V5.8z" />
    </Icon>
  );
}

export function RunsIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M4.5 6.5h15M4.5 12h15M4.5 17.5h9" />
      <circle cx="17.5" cy="17.5" r="2.4" />
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

export function SortIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M4.5 7h15" />
      <path d="M7.5 12h9" />
      <path d="M10.5 17h3" />
    </Icon>
  );
}

export function PinIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M9 4.5h6" />
      <path d="M10 4.5v5.6a2 2 0 0 1-.9 1.7l-1.6 1V14h9v-1.2l-1.6-1a2 2 0 0 1-.9-1.7V4.5" />
      <path d="M12 14v5.5" />
    </Icon>
  );
}

/** A droplet: the Glass settings, and the refracting surfaces generally. */
export function DropletIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M12 3.5c3 3.4 5.5 6.2 5.5 9.2a5.5 5.5 0 0 1-11 0c0-3 2.5-5.8 5.5-9.2z" />
      <path d="M9.5 13.2a2.5 2.5 0 0 0 2.5 2.5" />
    </Icon>
  );
}

/** A framed picture: the background artwork. */
export function ImageIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <rect x="3" y="4.5" width="18" height="15" rx="3" />
      <circle cx="8.5" cy="9.5" r="1.4" />
      <path d="m4 17 4.6-4.6a2 2 0 0 1 2.8 0L16 17" />
      <path d="m14 15.2 1.6-1.6a2 2 0 0 1 2.8 0L21 16.2" />
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

export function CheckIcon(props: IconProps) {
  return (
    <Icon {...props} strokeWidth={2}>
      <path d="M5 12.5l4.5 4.5L19 7.5" />
    </Icon>
  );
}

export function PersonIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="12" cy="8.5" r="3.6" />
      <path d="M5 19.5a7 7 0 0 1 14 0" />
    </Icon>
  );
}

export function StopIcon(props: IconProps) {
  return (
    <Icon {...props} strokeWidth={1.6}>
      <rect x="7" y="7" width="10" height="10" rx="2.4" fill="currentColor" stroke="none" />
    </Icon>
  );
}

/**
 * The drag handle: six dots on a 24 px grid, filled rather than stroked so it
 * reads as a texture rather than as another line icon.
 *
 * It lives here rather than beside the queue that first needed it, because a
 * list you reorder by hand is a recurring shape and a second hand-drawn grip
 * would eventually disagree with this one about its dot spacing.
 */
export function GripIcon({ size = 12, className, ...rest }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="currentColor"
      aria-hidden="true"
      className={cn("shrink-0", className)}
      {...rest}
    >
      <circle cx="9" cy="6.5" r="1.4" />
      <circle cx="15" cy="6.5" r="1.4" />
      <circle cx="9" cy="12" r="1.4" />
      <circle cx="15" cy="12" r="1.4" />
      <circle cx="9" cy="17.5" r="1.4" />
      <circle cx="15" cy="17.5" r="1.4" />
    </svg>
  );
}

export function TrashIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M5.5 7.5h13M10 7.5V5.8A1.3 1.3 0 0 1 11.3 4.5h1.4A1.3 1.3 0 0 1 14 5.8v1.7M8 7.5l.7 10a1.4 1.4 0 0 0 1.4 1.3h3.8a1.4 1.4 0 0 0 1.4-1.3l.7-10" />
    </Icon>
  );
}

export function RefreshIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M20 11.5a8 8 0 1 0-2.4 5.7" />
      <path d="M20 5.5v6h-6" />
    </Icon>
  );
}

export function KeyIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="8" cy="14.5" r="3.5" />
      <path d="M10.6 11.9l7.4-7.4M15 7.5l2.2 2.2M17.2 5.3l2.2 2.2" />
    </Icon>
  );
}

export function BrainIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M9.5 4.8a3 3 0 0 0-3 3 3 3 0 0 0-1.4 5.4A3 3 0 0 0 7.6 18a2.8 2.8 0 0 0 4.9-1.6V7.8a3 3 0 0 0-3-3z" />
      <path d="M14.5 4.8a3 3 0 0 1 3 3 3 3 0 0 1 1.4 5.4A3 3 0 0 1 16.4 18a2.8 2.8 0 0 1-4.9-1.6" />
    </Icon>
  );
}

export function FileIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M13.5 3.5H7.5A2 2 0 0 0 5.5 5.5v13a2 2 0 0 0 2 2h9a2 2 0 0 0 2-2V8.5z" />
      <path d="M13.5 3.5v5h5" />
    </Icon>
  );
}

export function WrenchIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M14.7 6.3a4.5 4.5 0 0 1 5.9 5.9l-8.4 8.4a2.2 2.2 0 0 1-3.1 0l-2.8-2.8a2.2 2.2 0 0 1 0-3.1z" />
      <path d="M6.2 5.2l3.1 3.1" />
    </Icon>
  );
}

export function PaletteIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M12 3.5c-4.7 0-8.5 3.6-8.5 8s3.8 8 8.5 8c1 0 1.8-.8 1.8-1.8 0-.5-.2-.9-.5-1.3-.2-.3-.4-.7-.4-1 0-1 .9-1.9 2-1.9h1.6c2 0 3.5-1.6 3.5-3.5 0-3.6-3.6-6.5-8-6.5z" />
      <circle cx="7.8" cy="12.2" r="0.9" />
      <circle cx="10.3" cy="8.3" r="0.9" />
      <circle cx="14.8" cy="8.2" r="0.9" />
    </Icon>
  );
}

export function MessageIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M4.5 6.8a2.3 2.3 0 0 1 2.3-2.3h10.4a2.3 2.3 0 0 1 2.3 2.3v7.4a2.3 2.3 0 0 1-2.3 2.3H9l-4.5 3.5z" />
    </Icon>
  );
}

export function GaugeIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M4.5 16.5a8 8 0 1 1 15 0" />
      <path d="M12 16.5l3.5-6" />
      <circle cx="12" cy="16.5" r="1.2" />
    </Icon>
  );
}

export function PlugIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M9 3.5v5M15 3.5v5" />
      <path d="M6.5 8.5h11v3a5.5 5.5 0 0 1-5.5 5.5 5.5 5.5 0 0 1-5.5-5.5z" />
      <path d="M12 17v3.5" />
    </Icon>
  );
}

export function ServerIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <rect x="4" y="4.5" width="16" height="6" rx="2" />
      <rect x="4" y="13.5" width="16" height="6" rx="2" />
      <path d="M7.5 7.5h.01M7.5 16.5h.01" />
    </Icon>
  );
}

export function DatabaseIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <ellipse cx="12" cy="6" rx="7" ry="3" />
      <path d="M5 6v12c0 1.7 3.1 3 7 3s7-1.3 7-3V6" />
      <path d="M5 12c0 1.7 3.1 3 7 3s7-1.3 7-3" />
    </Icon>
  );
}

export function DownloadIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M12 4.5v10M7.5 10.5L12 15l4.5-4.5" />
      <path d="M5 19.5h14" />
    </Icon>
  );
}

export function CopyIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <rect x="9" y="9" width="10.5" height="10.5" rx="2.4" />
      <path d="M15 6.5V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v7a2 2 0 0 0 2 2h.5" />
    </Icon>
  );
}

export function EditIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M16.5 4.5l3 3L8 19l-4 1 1-4z" />
      <path d="M13.5 7.5l3 3" />
    </Icon>
  );
}

export function SearchIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="11" cy="11" r="6" />
      <path d="M15.5 15.5L20 20" />
    </Icon>
  );
}

export function ExternalLinkIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M14 4.5h5.5V10" />
      <path d="M19.2 4.8L12.5 11.5" />
      <path d="M19 14.5v3.6a2.4 2.4 0 0 1-2.4 2.4H6.9a2.4 2.4 0 0 1-2.4-2.4V7.4A2.4 2.4 0 0 1 6.9 5h3.6" />
    </Icon>
  );
}

export function CameraIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M6 6.5h1.2l1.1-1.7a1 1 0 0 1 .85-.45h5.7a1 1 0 0 1 .85.45l1.1 1.7H18a3 3 0 0 1 3 3v7a3 3 0 0 1-3 3H6a3 3 0 0 1-3-3v-7a3 3 0 0 1 3-3z" />
      <circle cx="12" cy="13" r="3.4" />
    </Icon>
  );
}

export function TargetIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="12" cy="12" r="7.2" />
      <circle cx="12" cy="12" r="2.6" />
      <path d="M12 4.8V2.6M12 21.4v-2.2M4.8 12H2.6M21.4 12h-2.2" />
    </Icon>
  );
}

export function CircleIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="12" cy="12" r="7.2" />
    </Icon>
  );
}

export function HalfCircleIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="12" cy="12" r="7.2" />
      <path d="M12 4.8a7.2 7.2 0 0 1 0 14.4z" fill="currentColor" stroke="none" />
    </Icon>
  );
}

/**
 * Brand glyph: three woven threads. `weaving` draws them on as it mounts
 * (the warp first, then the weft), for the opening screen.
 */
export function LoomMark({
  size = 18,
  weaving = false,
  className,
  ...rest
}: IconProps & { weaving?: boolean }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.9}
      strokeLinecap="round"
      className={cn(weaving && "loom-mark-weaving", className)}
      aria-hidden="true"
      {...rest}
    >
      <path
        d="M6.5 5.5c0 6.5 5.5 6.5 5.5 13"
        pathLength={weaving ? 1 : undefined}
      />
      <path
        d="M12 5.5c0 6.5 5.5 6.5 5.5 13"
        pathLength={weaving ? 1 : undefined}
      />
      <path d="M6.5 18.5h11" opacity="0.55" pathLength={weaving ? 1 : undefined} />
    </svg>
  );
}

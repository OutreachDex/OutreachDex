import { useEffect, useId, useRef, useState } from "react";
import type { ReactNode } from "react";
import { confirm } from "@tauri-apps/plugin-dialog";
import { t } from "../app/i18n";
import { copyText } from "../app/clipboard";

const paths: Record<string, string> = {
  edit: "M12 20h9 M16 3l5 5-12 12H4v-5z",
  terminal: "M3 4h18v16H3z M7 8l4 4-4 4 M13 16h4",
  expand: "M8 3H3v5 M16 3h5v5 M21 16v5h-5 M8 21H3v-5",
  today: "M12 3v2 M12 19v2 M5 12H3 M21 12h-2 M6.3 6.3L4.9 4.9 M19.1 19.1l-1.4-1.4 M6.3 17.7l-1.4 1.4 M19.1 4.9l-1.4 1.4 M16 12a4 4 0 1 1-8 0 4 4 0 0 1 8 0",
  people:
    "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2 M16 3a4 4 0 0 1 0 8 M22 21v-2a4 4 0 0 0-3-3.87 M13 7a4 4 0 1 1-8 0 4 4 0 0 1 8 0",
  contacts:
    "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2 M16 3a4 4 0 0 1 0 8 M22 21v-2a4 4 0 0 0-3-3.87 M13 7a4 4 0 1 1-8 0 4 4 0 0 1 8 0",
  followups: "M12 8v4l3 2 M22 12a10 10 0 1 1-3-7 M22 2v6h-6",
  table: "M3 3h18v18H3z M3 9h18 M9 9v12",
  list: "M8 6h13 M8 12h13 M8 18h13 M3 6h.01 M3 12h.01 M3 18h.01",
  mail: "M3 5h18v14H3z M3 5l9 7 9-7",
  refresh:
    "M20 7a9 9 0 0 0-15-2L2 8 M2 3v5h5 M4 17a9 9 0 0 0 15 2l3-3 M22 21v-5h-5",
  history: "M3 12a9 9 0 1 0 3-6.7L3 8 M3 3v5h5 M12 7v5l4 2",
  settings:
    "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z",
  search: "M21 21l-5-5 M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0",
  plus: "M12 5v14 M5 12h14",
  arrow: "M5 12h14 M13 6l6 6-6 6",
  back: "M19 12H5 M11 6l-6 6 6 6",
  up: "M18 15l-6-6-6 6",
  down: "M6 9l6 6 6-6",
  chevron: "M9 6l6 6-6 6",
  spark: "M12 3l2.5 6.5L21 12l-6.5 2.5L12 21l-2.5-6.5L3 12l6.5-2.5z",
  note: "M14 2H4v20h16V8z M14 2v6h6 M8 12h8 M8 16h6",
  check: "M5 12l4 4L19 6",
  folder: "M3 5h6l2 2h10v13H3z",
  close: "M6 6l12 12 M6 18L18 6",
  sun: "M12 3v2 M12 19v2 M5 12H3 M21 12h-2 M6.3 6.3L4.9 4.9 M19.1 19.1l-1.4-1.4 M6.3 17.7l-1.4 1.4 M19.1 4.9l-1.4 1.4 M16 12a4 4 0 1 1-8 0 4 4 0 0 1 8 0",
  moon: "M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z",
  send: "M22 2L11 13 M22 2l-7 20-4-9-9-4z",
  inbox: "M22 12h-6l-2 3h-4l-2-3H2 M5.5 5h13L22 12v7H2v-7z",
  link: "M10 13a5 5 0 0 0 7.5.5l3-3a5 5 0 0 0-7-7l-1.7 1.7 M14 11a5 5 0 0 0-7.5-.5l-3 3a5 5 0 0 0 7 7l1.7-1.7",
  more: "M5 12h.01 M12 12h.01 M19 12h.01",
  help: "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20z M9.1 9a3 3 0 0 1 5.8 1c0 2-3 3-3 3 M12 17h.01",
};
export function Icon({ name, size = 16 }: { name: string; size?: number }) {
  return (
    <svg
      aria-hidden="true"
      className="icon"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d={paths[name] || paths.note} />
    </svg>
  );
}
// The OutreachDex mark (same drawing as docs/images/logo.svg): a speech bubble with an outgoing arrow.
export function Logo({ size = 22 }: { size?: number }) {
  return (
    <svg aria-hidden="true" className="logo" width={size} height={size} viewBox="0 0 64 64">
      <rect width="64" height="64" rx="16" fill="var(--brand)" />
      <path d="M22 13h20a10 10 0 0 1 10 10v10a10 10 0 0 1-10 10H29l-9 8v-8.4A10 10 0 0 1 12 33V23a10 10 0 0 1 10-10z" fill="#fff" />
      <path d="M23 28h16m-6-6 6 6-6 6" fill="none" stroke="var(--brand)" strokeWidth="4.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}
export const statuses: Record<string, string> = {
  draft: t("Pending review"),
  approved: t("Approved"),
  sent: t("Sent"),
  discarded: t("Discarded"),
  sending: t("Sending"),
  uncertain: t("Check the send result"),
};
export const statusTone: Record<string, string> = {
  draft: "amber",
  approved: "violet",
  sent: "green",
  discarded: "gray",
  sending: "blue",
  uncertain: "red",
};
export const channelNames: Record<string, string> = {
  email: t("Mail"),
  x: "X",
  instagram: "Instagram",
  wechat: t("WeChat"),
  whatsapp: "WhatsApp",
  tiktok: "TikTok",
  facebook: "Facebook / Messenger",
  linkedin: "LinkedIn",
  homepage: t("Website"),
  scholar: "Scholar",
  github: "GitHub",
  other: t("Other"),
};
// Mirrors gc_core::channels. Only email is sent by the app; the rest are "copy + mark as sent".
/** Channels a draft, list or interaction can use. */
export const messageChannels = ["email", "x", "instagram", "wechat", "whatsapp", "tiktok", "facebook", "linkedin", "other"];
/** Every kind a contact channel can have: message channels plus profile links. */
export const channelKinds = [
  "email",
  "x",
  "instagram",
  "wechat",
  "whatsapp",
  "tiktok",
  "facebook",
  "linkedin",
  "homepage",
  "scholar",
  "github",
  "other",
];
export const authors: Record<string, string> = { me: t("Manual"), claude: "Claude", codex: "Codex" };
export function initials(name: string) {
  const text = name.trim();
  if (/^[\u4e00-\u9fff]/.test(text)) return text[0];
  return (
    text
      .replace(/\([^)]*\)/g, " ")
      .split(/\s+/)
      .filter((s) => /^[\p{L}\p{N}]/u.test(s))
      .map((s) => s[0])
      .slice(0, 2)
      .join("")
      .toUpperCase() || "?"
  );
}
const tones = ["#e07a5f", "#3d9970", "#7b61ff", "#2b7bb9", "#c0841a", "#b5651d", "#d1495b", "#5b8c5a"];
export function Avatar({ name, size }: { name: string; size?: "lg" }) {
  let hash = 0;
  for (const ch of name) hash = (hash * 31 + ch.charCodeAt(0)) >>> 0;
  return (
    <span className={`avatar ${size || ""}`} style={{ background: tones[hash % tones.length] }} aria-hidden="true">
      {initials(name || "?")}
    </span>
  );
}
export function Tag({ tone = "gray", children, title }: { tone?: string; children: ReactNode; title?: string }) {
  return (
    <span className={`tag ${tone}`} title={title}>
      {children}
    </span>
  );
}
export function Kbd({ children }: { children: ReactNode }) {
  return <kbd className="kbd">{children}</kbd>;
}
export function Empty({
  title,
  icon = "people",
  children,
}: {
  title: string;
  icon?: string;
  children?: ReactNode;
}) {
  return (
    <div className="empty">
      <span className="empty-icon">
        <Icon name={icon} size={22} />
      </span>
      <h3>{title}</h3>
      {children && <div className="empty-body">{children}</div>}
    </div>
  );
}
export function Modal({
  title,
  close,
  children,
  wide = false,
  footer,
  protectChanges = false,
}: {
  title: string;
  close?: () => void;
  children: ReactNode;
  wide?: boolean;
  footer?: ReactNode;
  protectChanges?: boolean;
}) {
  const ref = useRef<HTMLElement>(null);
  const changed = useRef(false);
  const dismiss = async () => {
    if (
      protectChanges &&
      changed.current &&
      !(await confirm(t("Discard unsaved changes?"), {
        title: t("Close editor"),
        kind: "warning",
      }))
    )
      return;
    close?.();
  };
  const heading = useId();
  const closeRef = useRef<(() => void) | undefined>(dismiss);
  closeRef.current = close ? dismiss : undefined;
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    const panel = ref.current!;
    const focusable = () =>
      [
        ...panel.querySelectorAll<HTMLElement>(
          'button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex="0"]',
        ),
      ].filter((el) => el.getClientRects().length);
    const initial =
      panel.querySelector<HTMLElement>(
        "input:not(:disabled), textarea:not(:disabled), select:not(:disabled)",
      ) ||
      focusable()[0] ||
      panel;
    // A dialog opened while the user types in the Agent (e.g. a draft it opened via ui.open)
    // must not take the keyboard from the terminal; clicking into the dialog still works.
    if (!(document.activeElement instanceof HTMLElement && document.activeElement.closest(".agent-dock"))) initial.focus();
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        closeRef.current?.();
      }
      if (e.key === "Tab") {
        const items = focusable();
        e.preventDefault();
        if (!items.length) panel.focus();
        else {
          const current = items.indexOf(document.activeElement as HTMLElement);
          const next =
            (current + (e.shiftKey ? -1 : 1) + items.length) % items.length;
          items[next].focus();
        }
      }
    };
    panel.addEventListener("keydown", key);
    return () => {
      panel.removeEventListener("keydown", key);
      previous?.focus();
    };
  }, []);
  return (
    <div className="overlay">
      <section
        ref={ref}
        onChangeCapture={() => {
          changed.current = true;
        }}
        tabIndex={-1}
        role="dialog"
        aria-modal="true"
        aria-labelledby={heading}
        className={`modal ${wide ? "wide" : ""}`}
      >
        <div className="modal-head">
          <h2 id={heading}>{title}</h2>
          {close && (
            <button
              className="icon-btn"
              aria-label={t("Close dialog")}
              onClick={() => void dismiss()}
            >
              <Icon name="close" />
            </button>
          )}
        </div>
        <div className="modal-body">{children}</div>
        {footer && <footer className="modal-foot">{footer}</footer>}
      </section>
    </div>
  );
}

/** The assistants the app works with; neither is bundled. Commands from each vendor's install docs. */
const ASSISTANTS: Record<string, { name: string; install: string; signIn: string }> = {
  claude: { name: "Claude Code", install: "curl -fsSL https://claude.ai/install.sh | bash", signIn: "claude" },
  codex: { name: "Codex", install: "curl -fsSL https://chatgpt.com/codex/install.sh | sh", signIn: "codex" },
};

/** How to install a missing assistant: the official command to run in Terminal, then sign in. */
export function InstallHint({ agent }: { agent: string }) {
  const [copied, setCopied] = useState(false);
  const code = useRef<HTMLElement>(null);
  const info = ASSISTANTS[agent];
  if (!info) return null;
  // If the clipboard is unavailable, select the command so ⌘C copies it.
  const selectCommand = () => {
    const range = document.createRange();
    if (!code.current) return;
    range.selectNodeContents(code.current);
    window.getSelection()?.removeAllRanges();
    window.getSelection()?.addRange(range);
  };
  return (
    <div className="install-hint">
      <span>{t("{name} isn't installed. Run this in Terminal, then run “{command}” once to sign in:", { name: info.name, command: info.signIn })}</span>
      <span className="install-command">
        <code ref={code}>{info.install}</code>
        <button
          className="btn ghost"
          onClick={() =>
            void copyText(info.install).then(
              () => {
                setCopied(true);
                setTimeout(() => setCopied(false), 2000);
              },
              selectCommand,
            )
          }
        >
          {copied ? t("Copied") : t("Copy")}
        </button>
      </span>
    </div>
  );
}

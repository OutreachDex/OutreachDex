import { useEffect, useRef } from "react";
import { Icon, Logo } from "../components/ui";
import { setPrefs, useScheme } from "./prefs";
import type { View } from "./App";
import { t } from "./i18n";

const items: [View, string, string][] = [
  ["today", t("Today"), "today"],
  ["people", t("People"), "people"],
  ["lists", t("Lists"), "list"],
  ["mail", t("Mail"), "mail"],
  ["sources", t("Sources"), "link"],
];

export function Sidebar({
  root,
  view,
  busy,
  counts,
  agentOpen,
  agentSessions,
  navigate,
  toggleAgent,
  choose,
  openSearch,
}: {
  root: string;
  view: View;
  busy: boolean;
  counts: { today: number; people: number; sources: number };
  agentOpen: boolean;
  agentSessions: number;
  navigate: (view: View) => void;
  toggleAgent: () => void;
  choose: (create: boolean) => void;
  openSearch: () => void;
}) {
  const menu = useRef<HTMLDetailsElement>(null);
  const scheme = useScheme();
  useEffect(() => {
    const outside = (e: PointerEvent) => {
      if (menu.current && !menu.current.contains(e.target as Node)) menu.current.open = false;
    };
    const escape = (e: KeyboardEvent) => {
      if (e.key === "Escape" && menu.current) menu.current.open = false;
    };
    window.addEventListener("pointerdown", outside);
    window.addEventListener("keydown", escape);
    return () => {
      window.removeEventListener("pointerdown", outside);
      window.removeEventListener("keydown", escape);
    };
  }, []);
  const active = (key: View) => view === key || (key === "people" && (view === "person" || view === "table"));
  const name = root ? root.split("/").pop() : t("Choose workspace");
  return (
    <aside className="side">
      <details ref={menu} className="ws">
        <summary title={root || t("Choose workspace")}>
          <Logo />
          <span className="ws-name">{name}</span>
          <Icon name="down" size={12} />
        </summary>
        <div className="menu">
          {root && <div className="menu-note" title={root}>{root}</div>}
          <button
            onClick={() => {
              if (menu.current) menu.current.open = false;
              choose(false);
            }}
          >
            <Icon name="folder" />
            {t("Open workspace…")}
          </button>
          <button
            onClick={() => {
              if (menu.current) menu.current.open = false;
              choose(true);
            }}
          >
            <Icon name="plus" />
            {t("New workspace…")}
          </button>
        </div>
      </details>
      {root && (
        <button className="side-search" onClick={openSearch} aria-label={t("Search")} title={t("Search · ⌘ / Ctrl+K")}>
          <Icon name="search" size={14} />
          <span>{t("Search")}</span>
          <kbd className="kbd">⌘K</kbd>
        </button>
      )}
      <nav aria-label={t("Main navigation")}>
        {items.map(([key, label, icon]) => {
          const count = key === "today" ? counts.today : key === "people" ? counts.people : key === "sources" ? counts.sources : 0;
          return (
            <button
              key={key}
              className={`nav ${active(key) ? "on" : ""}`}
              // Narrow windows show icons only; the name keeps the button identifiable.
              aria-label={count > 0 ? `${label} ${count}` : label}
              title={label}
              aria-current={active(key) ? "page" : undefined}
              disabled={busy || !root}
              onClick={() => navigate(key)}
            >
              <Icon name={icon} />
              <span className="nav-label">{label}</span>
              {count > 0 && <span className={`count ${key === "today" || key === "sources" ? "hot" : ""}`}>{count}</span>}
            </button>
          );
        })}
      </nav>
      <div className="side-foot">
        {root && (
          <button className={`nav ${agentOpen ? "on" : ""}`} onClick={toggleAgent} title="Agent · ⌘ / Ctrl+J">
            <Icon name="terminal" />
            <span className="nav-label">Agent</span>
            {agentSessions > 0 ? (
              <span className="count" title={t("{n} sessions still connected", { n: agentSessions })}>
                <span className="dot ok" /> {agentSessions}
              </span>
            ) : (
              <span className="count">⌘J</span>
            )}
          </button>
        )}
        <button
          className={`nav ${view === "help" ? "on" : ""}`}
          aria-label={t("Help")}
          title={t("Help")}
          aria-current={view === "help" ? "page" : undefined}
          disabled={busy}
          onClick={() => navigate("help")}
        >
          <Icon name="help" />
          <span className="nav-label">{t("Help")}</span>
        </button>
        <div className="side-row">
          <button
            className={`nav grow ${view === "settings" ? "on" : ""}`}
            aria-label={t("Settings")}
            title={t("Settings")}
            aria-current={view === "settings" ? "page" : undefined}
            disabled={busy}
            onClick={() => navigate("settings")}
          >
            <Icon name="settings" />
            <span className="nav-label">{t("Settings")}</span>
          </button>
          <button
            className="icon-btn"
            title={scheme === "dark" ? t("Switch to light") : t("Switch to dark")}
            aria-label={scheme === "dark" ? t("Switch to light") : t("Switch to dark")}
            onClick={() => setPrefs({ theme: scheme === "dark" ? "light" : "dark" })}
          >
            <Icon name={scheme === "dark" ? "sun" : "moon"} />
          </button>
        </div>
      </div>
    </aside>
  );
}

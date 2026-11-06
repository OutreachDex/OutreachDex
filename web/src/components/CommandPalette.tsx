import { useEffect, useMemo, useRef, useState } from "react";
import { Avatar, Icon, statuses } from "./ui";
import { matches, type Row, type V } from "../app/domain";
import { t } from "../app/i18n";

export type Command = { id: string; group: string; title: string; hint?: string; icon?: string; avatar?: string; run: () => void };

/** ⌘K: jump to a page, person, list or draft, or run a common action. */
export function CommandPalette({
  open,
  close,
  rows,
  call,
  pages,
  actions,
  openPerson,
  openList,
  openDraft,
}: {
  open: boolean;
  close: () => void;
  rows: Row[];
  call: <T>(action: string, args?: V) => Promise<T>;
  pages: Command[];
  actions: Command[];
  openPerson: (slug: string) => void;
  openList: (id: string) => void;
  openDraft: (draft: V) => void;
}) {
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const [lists, setLists] = useState<V[]>([]);
  const [drafts, setDrafts] = useState<V[]>([]);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (!open) return;
    setQuery("");
    setIndex(0);
    requestAnimationFrame(() => input.current?.focus());
    void call<V[]>("list.all").then(setLists).catch(() => setLists([]));
    void call<V[]>("draft.list").then(setDrafts).catch(() => setDrafts([]));
  }, [open, call]);
  const items = useMemo(() => {
    const q = query.trim().toLowerCase();
    const hit = (...parts: unknown[]) => !q || parts.join(" ").toLowerCase().includes(q);
    const name = (slug: string) => rows.find((r) => r.values.Slug === slug)?.values.Name || slug;
    const people: Command[] = rows
      .filter((r) => matches(r, q))
      .slice(0, q ? 8 : 5)
      .map((r) => ({
        id: `p:${r.values.Slug}`,
        group: t("People"),
        title: r.values.Name || r.values.Slug,
        hint: [r.values.Affiliation, r.values.Email].filter(Boolean).join(" · "),
        avatar: r.values.Name || "?",
        run: () => openPerson(r.values.Slug),
      }));
    const listItems: Command[] = lists
      .filter((l) => hit(l.name, l.description))
      .slice(0, 5)
      .map((l) => ({ id: `l:${l.id}`, group: t("Lists"), title: l.name, hint: t("{n} people", { n: l.counts?.matched ?? 0 }), icon: "list", run: () => openList(l.id) }));
    const draftItems: Command[] = q
      ? drafts
          .filter((d) => hit(d.metadata.subject, d.body, name(d.metadata.contact_id)))
          .slice(0, 5)
          .map((d) => ({
            id: `d:${d.path}`,
            group: t("Draft"),
            title: d.metadata.subject || t("Message draft"),
            hint: `${name(d.metadata.contact_id)} · ${statuses[d.metadata.status] || d.metadata.status}`,
            icon: "edit",
            run: () => openDraft(d),
          }))
      : [];
    return [
      ...pages.filter((p) => hit(p.title, p.hint)),
      ...people,
      ...listItems,
      ...draftItems,
      ...actions.filter((a) => hit(a.title, a.hint)),
    ];
  }, [query, rows, lists, drafts, pages, actions]);
  if (!open) return null;
  const choose = (item?: Command) => {
    if (!item) return;
    close();
    item.run();
  };
  let group = "";
  return (
    <div className="overlay palette-overlay" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div className="palette" role="dialog" aria-modal="true" aria-label={t("Search")}>
        <div className="palette-input">
          <Icon name="search" />
          <input
            ref={input}
            aria-label={t("Search people, lists, drafts or pages")}
            placeholder={t("Search people, lists, drafts, or jump to a page…")}
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setIndex(0);
            }}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                e.preventDefault();
                close();
              } else if (e.key === "ArrowDown") {
                e.preventDefault();
                setIndex((i) => Math.min(items.length - 1, i + 1));
              } else if (e.key === "ArrowUp") {
                e.preventDefault();
                setIndex((i) => Math.max(0, i - 1));
              } else if (e.key === "Enter" && !e.nativeEvent.isComposing) {
                e.preventDefault();
                choose(items[index]);
              }
            }}
          />
          <kbd className="kbd">Esc</kbd>
        </div>
        <div className="palette-list" role="listbox">
          {items.map((item, i) => {
            const header = item.group !== group ? (group = item.group) : null;
            return (
              <div key={item.id}>
                {header && <div className="palette-group">{header}</div>}
                <button
                  role="option"
                  aria-selected={i === index}
                  className={`palette-item ${i === index ? "on" : ""}`}
                  onMouseEnter={() => setIndex(i)}
                  onClick={() => choose(item)}
                >
                  {item.avatar ? <Avatar name={item.avatar} /> : <Icon name={item.icon || "arrow"} />}
                  <span className="grow clip">{item.title}</span>
                  {item.hint && <span className="faint small clip palette-hint">{item.hint}</span>}
                </button>
              </div>
            );
          })}
          {!items.length && <p className="faint pad">{t("Nothing found for “{query}”.", { query })}</p>}
        </div>
      </div>
    </div>
  );
}

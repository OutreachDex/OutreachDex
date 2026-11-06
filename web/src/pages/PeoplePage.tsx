import { useEffect, useRef, useState } from "react";
import { Avatar, Empty, Icon, Tag } from "../components/ui";
import { awaitsReply, lastTouch, matches, relative, stages, stageTone, type Row } from "../app/domain";
import { lang, t } from "../app/i18n";

export type PeopleView = "all" | "reply" | "followup";

export function PeoplePage({
  rows,
  view,
  setView,
  query,
  setQuery,
  stage,
  setStage,
  selected,
  stale,
  busy,
  open,
  create,
  importFile,
  openTable,
  bulkStage,
  bulkFollowUp,
  exportCsv,
  onSelection,
}: {
  onSelection?: (slugs: string[]) => void;
  rows: Row[];
  view: PeopleView;
  setView: (v: PeopleView) => void;
  query: string;
  setQuery: (v: string) => void;
  stage: string;
  setStage: (v: string) => void;
  selected: string;
  stale: (r: Row) => boolean;
  busy: boolean;
  open: (slug: string) => void;
  create: () => void;
  importFile: () => void;
  openTable: () => void;
  bulkStage: (slugs: string[], stage: string) => Promise<unknown>;
  bulkFollowUp: (slugs: string[], days: number | null) => Promise<unknown>;
  exportCsv: (slugs: string[] | null) => void;
}) {
  const visible = rows
    .filter(
      (r) =>
        matches(r, query) &&
        (!stage || r.values.Stage === stage) &&
        (view === "all" || (view === "reply" ? awaitsReply(r) : stale(r))),
    )
    .sort((a, b) => (view === "all" ? 0 : lastTouch(b) - lastTouch(a)));
  const counts = { all: rows.length, reply: rows.filter(awaitsReply).length, followup: rows.filter(stale).length };
  const body = useRef<HTMLTableSectionElement>(null);
  const [checked, setChecked] = useState<Set<string>>(new Set());
  const picked = [...checked].filter((slug) => rows.some((r) => r.values.Slug === slug));
  const pickedKey = picked.join("\n");
  // The Agent can resolve "these people" to the rows picked here (MCP context.get).
  useEffect(() => {
    onSelection?.(pickedKey ? pickedKey.split("\n") : []);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pickedKey]);
  useEffect(() => () => onSelection?.([]), []); // eslint-disable-line react-hooks/exhaustive-deps
  const allChecked = visible.length > 0 && visible.every((r) => checked.has(r.values.Slug));
  useEffect(() => {
    body.current?.querySelector<HTMLElement>("tr.sel")?.scrollIntoView({ block: "nearest" });
  }, [selected]);
  const move = (delta: number) => {
    const index = visible.findIndex((r) => r.values.Slug === selected);
    const next = visible[Math.max(0, Math.min(visible.length - 1, index + delta))];
    if (next) body.current?.querySelector<HTMLElement>(`tr[data-slug="${CSS.escape(next.values.Slug)}"]`)?.focus();
  };

  return (
    <div className="page-fill">
      {picked.length > 0 && (
        <div className="views bulk" role="toolbar" aria-label={t("Bulk actions")}>
          <b>{t("{n} selected", { n: picked.length })}</b>
          <span className="divider" />
          <select className="chip set" aria-label={t("Change stage for selected")} value="" disabled={busy} onChange={(e) => e.target.value && void bulkStage(picked, e.target.value).then(() => setChecked(new Set()))}>
            <option value="">{t("Set stage to…")}</option>
            {stages().map((s) => (
              <option key={s} value={s}>
                {s}
              </option>
            ))}
          </select>
          <select
            className="chip set"
            aria-label={t("Schedule follow-up for selected")}
            value=""
            disabled={busy}
            onChange={(e) => {
              const v = e.target.value;
              if (v) void bulkFollowUp(picked, v === "clear" ? null : Number(v)).then(() => setChecked(new Set()));
            }}
          >
            <option value="">{t("Scheduling follow-up…")}</option>
            <option value="1">{t("Tomorrow")}</option>
            <option value="3">{t("In 3 days")}</option>
            <option value="7">{t("In 1 week")}</option>
            <option value="30">{t("In 1 month")}</option>
            <option value="clear">{t("Clear follow-up")}</option>
          </select>
          <button className="btn ghost" disabled={busy} onClick={() => exportCsv(picked)}>
            {t("Export CSV")}
          </button>
          <span className="sp" />
          <button className="btn ghost" onClick={() => setChecked(new Set())}>
            {t("Clear selection")}
          </button>
        </div>
      )}
      <div className={`views ${picked.length ? "hidden" : ""}`}>
        {(
          [
            ["all", t("All")],
            ["reply", t("Awaiting reply")],
            ["followup", t("Follow up due")],
          ] as const
        ).map(([key, label]) => (
          <button key={key} className={`view ${view === key ? "on" : ""}`} aria-pressed={view === key} onClick={() => setView(key)}>
            {label}
            <span className="count">{counts[key]}</span>
          </button>
        ))}
        <span className="divider" />
        <div className="search-inline">
          <Icon name="search" size={14} />
          <input
            aria-label={t("Search contacts")}
            placeholder={t("Search name, organization, email…")}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          {query && (
            <button className="icon-btn sm" aria-label={t("Clear search")} onClick={() => setQuery("")}>
              <Icon name="close" size={12} />
            </button>
          )}
        </div>
        <select className={`chip ${stage ? "set" : ""}`} aria-label={t("Filter by stage")} value={stage} onChange={(e) => setStage(e.target.value)}>
          <option value="">{t("Stage: All")}</option>
          {stages().map((s) => (
            <option key={s} value={s}>
              {t("Stage: {stage}", { stage: s })}
            </option>
          ))}
        </select>
        <span className="sp" />
        <button className="btn ghost" disabled={busy} onClick={importFile} title={t("Import contacts from CSV / TSV")} aria-label={t("Import")}>
          <Icon name="inbox" />
          <span className="label-wide">{t("Import")}</span>
        </button>
        <button className="btn ghost" disabled={busy || !visible.length} onClick={() => exportCsv(visible.length === rows.length ? null : visible.map((r) => r.values.Slug))} title={t("Export the current list as CSV")} aria-label={t("Export")}>
          <Icon name="send" />
          <span className="label-wide">{t("Export")}</span>
        </button>
        <button className="btn ghost" onClick={openTable} title={t("Edit contacts, channels and activity directly in kit.table")} aria-label={t("Table editor")}>
          <Icon name="table" />
          <span className="label-wide">{t("Table editor")}</span>
        </button>
        <button className="btn primary" disabled={busy} onClick={create} aria-label={t("New contact")}>
          <Icon name="plus" />
          {t("New")}
          <span className="label-wide">
            {lang === "en" || lang === "ko" ? " " : ""}
            {t("Contacts")}
          </span>
        </button>
      </div>
      {visible.length === 0 ? (
        <Empty
          title={
            rows.length
              ? view === "reply"
                ? t("No one is waiting for your reply")
                : view === "followup"
                  ? t("No one needs a follow-up")
                  : t("No matching contacts")
              : t("Build your network")
          }
        >
          {rows.length ? t("Try other keywords or clear the filters.") : t("Create a contact, or import a list from CSV / TSV.")}
        </Empty>
      ) : (
        <div className="grid-wrap">
          <table className="grid people">
            <thead>
              <tr>
                <th className="check-col">
                  <input
                    type="checkbox"
                    aria-label={t("Select all in current list")}
                    checked={allChecked}
                    onChange={(e) => setChecked(new Set(e.target.checked ? visible.map((r) => r.values.Slug) : []))}
                  />
                </th>
                <th>{t("Name")}</th>
                <th>{t("Stage")}</th>
                <th>{t("Affiliation")}</th>
                <th>{t("Email")}</th>
                <th>{t("Last activity")}</th>
                <th>{t("Next step")}</th>
              </tr>
            </thead>
            <tbody ref={body}>
              {visible.map((r) => {
                const v = r.values;
                return (
                  <tr
                    key={r.id}
                    data-slug={v.Slug}
                    tabIndex={0}
                    className={selected === v.Slug ? "sel" : ""}
                    aria-selected={selected === v.Slug}
                    aria-disabled={busy || undefined}
                    onClick={() => !busy && open(v.Slug)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") open(v.Slug);
                      if (e.key === "ArrowDown" || e.key === "j") {
                        e.preventDefault();
                        (e.currentTarget.nextElementSibling as HTMLElement | null)?.focus();
                      }
                      if (e.key === "ArrowUp" || e.key === "k") {
                        e.preventDefault();
                        (e.currentTarget.previousElementSibling as HTMLElement | null)?.focus();
                      }
                      if (e.key === "Home") move(-Infinity);
                      if (e.key === "End") move(Infinity);
                    }}
                  >
                    <td className="check-col" onClick={(e) => e.stopPropagation()}>
                      <input
                        type="checkbox"
                        aria-label={t("Select {name}", { name: v.Name })}
                        checked={checked.has(v.Slug)}
                        onChange={(e) => {
                          const next = new Set(checked);
                          if (e.target.checked) next.add(v.Slug);
                          else next.delete(v.Slug);
                          setChecked(next);
                        }}
                      />
                    </td>
                    <td>
                      <div className="name">
                        <Avatar name={String(v.Name || "?")} />
                        <span>{v.Name || t("Untitled")}</span>
                        {v["Native name"] && !String(v.Name || "").includes(v["Native name"]) && <small className="faint">{v["Native name"]}</small>}
                      </div>
                    </td>
                    <td>
                      <Tag tone={stageTone(v.Stage)}>{v.Stage || "—"}</Tag>
                      {stale(r) && (
                        <Tag tone="amber" title={t("No reply past the follow-up period after sending")}>
                          {t("Follow up due")}
                        </Tag>
                      )}
                    </td>
                    <td className="muted clip">{v.Affiliation}</td>
                    <td className="muted clip">{v.Email}</td>
                    <td className="muted">{relative(lastTouch(r)) || <span className="faint">{t("Never")}</span>}</td>
                    <td className="clip">{v["Next action"] || <span className="faint">—</span>}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

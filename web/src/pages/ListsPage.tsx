import { useEffect, useMemo, useState } from "react";
import { confirm } from "@tauri-apps/plugin-dialog";
import { Avatar, Empty, Icon, Modal, Tag, channelNames, messageChannels, statuses, statusTone } from "../components/ui";
import { contexts, purposes, stageList, type Row, type V } from "../app/domain";
import { Remember } from "../components/Remember";
import { DraftHints } from "../components/DraftHints";
import { RecipientCheck } from "../components/RecipientCheck";
import { t, locale } from "../app/i18n";

type Call = <T>(action: string, args?: V) => Promise<T>;
type Work = (label: string, fn: () => Promise<unknown>) => Promise<void>;

const memberStatus: Record<string, [string, string]> = {
  new: [t("Not prepared"), "gray"],
  queued: [t("Queued"), "blue"],
  preparing: [t("Preparing"), "violet"],
  ready: [t("Pending review"), "amber"],
  no_draft: [t("No draft"), "gray"],
  failed: [t("Failed"), "red"],
  skipped: [t("Skipped"), "gray"],
  suppressed: [t("Do not contact"), "red"],
};
const roleLabels: Record<string, string> = { open: t("Not contacted"), waiting: t("Awaiting reply"), active: t("Replied / active"), closed: t("Paused"), suppressed: t("Do not contact") };

export function ListsPage({
  rows,
  sources,
  busy,
  call,
  work,
  version,
  refresh,
  notify,
  openPerson,
  openDraft,
  askAgent,
  focus,
  onCurrent,
}: {
  onCurrent?: (id: string) => void;
  focus?: string;
  askAgent: (list: string) => void;
  rows: Row[];
  sources: V[];
  busy: string;
  call: Call;
  work: Work;
  version: number;
  refresh: () => void;
  notify: (message: string) => void;
  openPerson: (slug: string) => void;
  openDraft: (draft: V) => void;
}) {
  const [lists, setLists] = useState<V[]>([]);
  const [selected, setSelected] = useState("");
  const [detail, setDetail] = useState<V | null>(null);
  const [editing, setEditing] = useState<V | null>(null);
  const [checked, setChecked] = useState<Set<string>>(new Set());
  const [reviewing, setReviewing] = useState(false);
  useEffect(() => {
    if (focus) setSelected(focus);
  }, [focus]);
  const current = lists.find((l) => l.id === selected) || lists[0];
  useEffect(() => {
    onCurrent?.(current?.id || "");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [current?.id]);
  useEffect(() => () => onCurrent?.(""), []); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    let alive = true;
    void call<V[]>("list.all").then((items) => alive && setLists(items)).catch(() => {});
    return () => {
      alive = false;
    };
  }, [version, call]);
  useEffect(() => {
    if (!current) return setDetail(null);
    let alive = true;
    void call<V>("list.get", { id: current.id })
      .then((d) => alive && setDetail(d))
      .catch((e) => notify(String(e)));
    return () => {
      alive = false;
    };
  }, [current?.id, version, call]);
  const members: V[] = detail?.members || [];
  const queued = members.filter((m) => m.status === "queued" || m.status === "preparing").length;
  useEffect(() => {
    if (!queued || !current) return;
    const timer = setInterval(() => void call<V>("list.get", { id: current.id }).then(setDetail).catch(() => {}), 2500);
    return () => clearInterval(timer);
  }, [queued, current?.id, call]);
  const ready = members.filter((m) => m.status === "ready" && m.draft_status !== "sent" && m.draft_status !== "discarded");
  const fresh = members.filter((m) => ["new", "failed", "no_draft"].includes(m.status) && !m.left);
  const sourceName = (key: string) => sources.find((s) => s.key === key)?.name || key;

  const prepare = (keys?: string[]) =>
    work(t("Handing to the Agent…"), async () => {
      const r = await call<V>("list.prepare", { id: current.id, ...(keys ? { members: keys } : {}) });
      setChecked(new Set());
      refresh();
      if (!r.queued) return notify(t("Nobody needs preparing"));
      // The Agent researches each queued person in the right pane and saves drafts for review.
      askAgent(current.id);
      notify(t("Handed {n} people to the Agent on the right; you can follow every step in the terminal. Nothing will be sent.", { n: r.queued }));
    });

  if (reviewing && detail)
    return (
      <Review
        list={detail.list}
        members={ready}
        rows={rows}
        busy={busy}
        call={call}
        work={work}
        limit={detail.daily_limit}
        sentToday={detail.sent_today}
        close={() => {
          setReviewing(false);
          refresh();
        }}
        notify={notify}
        openPerson={openPerson}
        openDraft={openDraft}
      />
    );

  return (
    <div className="lists-layout">
      <aside className="lists-side">
        <div className="lists-head">
          <b>{t("Lists")}</b>
          <button className="icon-btn sm" aria-label={t("New list")} title={t("New list")} onClick={() => setEditing({ name: "", description: "", source: sources[0]?.key || "", channel: "email", playbook: "", context: "", filters: [] })}>
            <Icon name="plus" size={14} />
          </button>
        </div>
        {lists.map((l) => (
          <button key={l.id} className={`nav ${current?.id === l.id ? "on" : ""}`} onClick={() => (setSelected(l.id), setChecked(new Set()))}>
            <span className="nav-label">{l.name}</span>
            {l.counts.ready > 0 ? <span className="count hot">{l.counts.ready}</span> : <span className="count">{l.counts.matched}</span>}
          </button>
        ))}
        {!lists.length && <p className="faint small pad">{t("No lists yet.")}</p>}
      </aside>
      <section className="lists-main">
        {!current || !detail ? (
          <Empty title={t("Use lists to find people worth contacting")} icon="list">
            {t("Filter by source attributes, events and contact status, e.g. “used up free credits but never paid”, “cancelled subscription”, “feedback without a reply”. The Agent researches and drafts one by one, you review one by one, and you always decide what gets sent.")}
            <div className="row-gap">
              <button className="btn primary" onClick={() => setEditing({ name: "", description: "", source: sources[0]?.key || "", channel: "email", playbook: "", context: "", filters: [] })}>
                <Icon name="plus" />
                {t("New list")}
              </button>
            </div>
          </Empty>
        ) : (
          <>
            <header className="list-header">
              <div className="grow">
                <h1>{detail.list.name}</h1>
                {detail.list.description && <p className="list-purpose">{detail.list.description}</p>}
                <div className="list-meta">
                  <Tag>{detail.list.source ? t("Source: {name}", { name: sourceName(detail.list.source) }) : t("Contacts")}</Tag>
                  <Tag>{channelNames[detail.list.channel] || detail.list.channel}</Tag>
                  <Tag tone={detail.sent_today >= detail.daily_limit ? "red" : "gray"} title={t("Daily limit for list messages; adjust it under [outreach] in globalcontact.toml")}>
                    {t("Today {sent} / {limit}", { sent: detail.sent_today, limit: detail.daily_limit })}
                  </Tag>
                  {detail.list.filters.map((f: V, i: number) => (
                    <span key={i} className="chip set">
                      {describe(f, sources.find((s) => s.key === detail.list.source))}
                    </span>
                  ))}
                  {!detail.list.filters.length && <span className="faint small">{t("No filters: includes every candidate")}</span>}
                </div>
              </div>
              <div className="row-gap list-actions">
                <button className="btn ghost" onClick={() => setEditing({ ...detail.list })}>
                  <Icon name="edit" />
                  {t("Edit filters")}
                </button>
                <button className="btn" disabled={!!busy || !ready.length} onClick={() => setReviewing(true)}>
                  {t("Review one by one")} {ready.length > 0 && <span className="count hot">{ready.length}</span>}
                </button>
                <button className="btn primary" disabled={!!busy || !(checked.size || fresh.length)} onClick={() => void prepare(checked.size ? [...checked] : undefined)}>
                  <Icon name="spark" />
                  {checked.size ? t("Prepare {n} selected", { n: checked.size }) : t("Prepare {n} people", { n: fresh.length })}
                </button>
              </div>
            </header>
            {queued > 0 && (
              <div className="banner notice slim">
                <span className="pulse" />
                <span className="grow">
                  {t("{n} people are waiting for the Agent to research and draft. Progress is shown in the terminal on the right. Drafts are only saved for review, never sent.", { n: queued })}
                </span>
                <button className="btn ghost" disabled={!!busy} onClick={() => askAgent(current.id)}>
                  {t("Let the Agent continue")}
                </button>
                <button className="btn ghost" onClick={() => void work(t("Clearing queue…"), async () => (await call("list.cancel", { id: current.id }), refresh()))}>
                  {t("Clear queue")}
                </button>
              </div>
            )}
            {!members.length ? (
              <Empty title={t("Nobody matches")} icon="people">
                {t("Adjust the filters, or sync data under “Sources” first.")}
              </Empty>
            ) : (
              <div className="grid-wrap">
                <table className="grid members">
                  <thead>
                    <tr>
                      <th className="check-col">
                        <input
                          type="checkbox"
                          aria-label={t("Select everyone who can be prepared")}
                          checked={!!fresh.length && fresh.every((m) => checked.has(m.key))}
                          onChange={(e) => setChecked(new Set(e.target.checked ? fresh.map((m) => m.key) : []))}
                        />
                      </th>
                      <th className="person-col">{t("People")}</th>
                      <th>{t("Why selected")}</th>
                      <th className="status-col">{t("Status")}</th>
                      <th />
                    </tr>
                  </thead>
                  <tbody>
                    {members.map((m) => {
                      // One status: the draft's once it moved past review (approved, sent…), else the member's.
                      const [label, tone] =
                        m.draft_status && m.draft_status !== "draft"
                          ? [statuses[m.draft_status] || m.draft_status, statusTone[m.draft_status] || "gray"]
                          : memberStatus[m.status] || [m.status, "gray"];
                      const reasons: string[] = m.evidence || [];
                      const selectable = ["new", "failed", "no_draft"].includes(m.status) && !m.left;
                      return (
                        <tr key={m.key} className={m.left ? "left" : ""}>
                          <td className="check-col">
                            <input
                              type="checkbox"
                              aria-label={t("Select {name}", { name: m.name })}
                              disabled={!selectable}
                              checked={checked.has(m.key)}
                              onChange={(e) => {
                                const next = new Set(checked);
                                if (e.target.checked) next.add(m.key);
                                else next.delete(m.key);
                                setChecked(next);
                              }}
                            />
                          </td>
                          <td className="person-col">
                            <div className="member">
                              <Avatar name={m.name || "?"} />
                              <span className="member-text">
                                {m.contact ? (
                                  <button className="link plain" onClick={() => openPerson(m.contact)}>
                                    {m.name || m.contact}
                                  </button>
                                ) : (
                                  <b>{m.name}</b>
                                )}
                                {m.email && <small className="faint">{m.email}</small>}
                              </span>
                            </div>
                          </td>
                          <td className="reasons" title={reasons.join(t("; "))}>
                            {m.left ? (
                              <span className="faint">{t("No longer matches")}</span>
                            ) : (
                              <>
                                {reasons.slice(0, 3).map((r, i) => (
                                  <span key={i} className="reason">
                                    {r}
                                  </span>
                                ))}
                                {reasons.length > 3 && <span className="faint small">+{reasons.length - 3}</span>}
                              </>
                            )}
                            {m.error && m.status === "no_draft" && <div className="faint small">{m.error}</div>}
                          </td>
                          <td className="status-col">
                            <Tag tone={tone} title={m.error || undefined}>
                              {label}
                            </Tag>
                          </td>
                          <td className="row-actions">
                            {selectable && (
                              <button className="btn ghost" disabled={!!busy} onClick={() => void prepare([m.key])}>
                                {t("Prepare")}
                              </button>
                            )}
                            {!["skipped", "suppressed"].includes(m.status) && !m.left && (
                              <button className="btn ghost" disabled={!!busy} onClick={() => void work(t("Skipping…"), async () => (await call("list.skip", { id: current.id, member: m.key }), refresh()))}>
                                {t("Skip")}
                              </button>
                            )}
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}
          </>
        )}
      </section>
      {editing && (
        <ListEditor
          value={editing}
          sources={sources}
          call={call}
          busy={busy}
          close={() => setEditing(null)}
          save={(list) =>
            void work(t("Saving list…"), async () => {
              const r = await call<V>("list.save", { id: list.id || "", list });
              setEditing(null);
              setSelected(r.id);
              notify(t("List saved to the workspace lists/ folder"));
              refresh();
            })
          }
          remove={
            editing.id
              ? () =>
                  void work(t("Deleting list…"), async () => {
                    if (!(await confirm(t("Delete the list “{name}”? Drafts already created are kept.", { name: editing.name })))) return;
                    await call("list.delete", { id: editing.id });
                    setEditing(null);
                    setSelected("");
                    refresh();
                  })
              : undefined
          }
        />
      )}
    </div>
  );
}

function describe(f: V, source?: V) {
  const attr = source?.manifest?.attributes?.find((a: V) => `attr.${a.key}` === f.field);
  const event = source?.manifest?.event_types?.find((et: V) => f.field.endsWith(`.${et.type}`));
  const name = attr?.label || (event ? event.label || event.type : f.field.replace(/^(contact|attr|event|no_event)\./, ""));
  const value = Array.isArray(f.value) ? f.value.map((v: string) => roleLabels[v] || v).join(t(", ")) : f.value;
  if (f.field === "role") return t("Stage: {value}", { value });
  if (f.field.startsWith("no_event.")) return t("No “{name}”", { name });
  if (f.field.startsWith("event_count.")) {
    const ops: Record<string, string> = { gte: "≥", gt: ">", lte: "≤", lt: "<", eq: "=" };
    return (f.days ? t("Within {n} days, ", { n: f.days }) : "") + t("“{name}” {op} {n} times", { name, op: ops[f.op] || "≥", n: f.value });
  }
  if (f.field.startsWith("event.")) return f.op === "older_than_days" ? t("“{name}” more than {n} days ago", { name, n: f.value }) : t("“{name}” within {n} days", { name, n: f.value });
  if (f.field === "last_contacted") return f.op === "never" ? t("Never contacted") : t("Last contacted more than {n} days ago", { n: f.value });
  if (f.field === "last_reply") return f.op === "never" ? t("Never replied") : t("Last reply more than {n} days ago", { n: f.value });
  const ops: Record<string, string> = { eq: "=", neq: "≠", gt: ">", gte: "≥", lt: "<", lte: "≤", contains: t("contains"), empty: t("is empty"), not_empty: t("is not empty") };
  return `${name} ${ops[f.op] || f.op} ${["empty", "not_empty"].includes(f.op) ? "" : value}`;
}

type FieldOption = { value: string; label: string; kind: "attr" | "contact" | "event" | "count" | "role" | "date" };

function ListEditor({
  value,
  sources,
  call,
  busy,
  close,
  save,
  remove,
}: {
  value: V;
  sources: V[];
  call: Call;
  busy: string;
  close: () => void;
  save: (list: V) => void;
  remove?: () => void;
}) {
  const [list, setList] = useState<V>(value);
  const [preview, setPreview] = useState<V | null>(null);
  const source = sources.find((s) => s.key === list.source);
  const fields: FieldOption[] = useMemo(
    () => [
      ...(source?.manifest?.attributes || []).map((a: V) => ({ value: `attr.${a.key}`, label: t("Source attribute · {name}", { name: a.label || a.key }), kind: "attr" as const })),
      ...(source?.manifest?.event_types || []).flatMap((et: V) => [
        { value: `event.${et.type}`, label: t("Event · has “{name}”", { name: et.label || et.type }), kind: "event" as const },
        { value: `no_event.${et.type}`, label: t("Event · no “{name}”", { name: et.label || et.type }), kind: "event" as const },
        { value: `event_count.${et.type}`, label: t("Event count · “{name}”", { name: et.label || et.type }), kind: "count" as const },
      ]),
      { value: "role", label: t("Contact · stage"), kind: "role" as const },
      { value: "last_contacted", label: t("Contact · last contacted"), kind: "date" as const },
      { value: "last_reply", label: t("Contact · last reply"), kind: "date" as const },
      { value: "contact.Campaign", label: t("Contact · campaign"), kind: "contact" as const },
      { value: "contact.Affiliation", label: t("Contact · organization"), kind: "contact" as const },
      { value: "contact.Language", label: t("Contact · language"), kind: "contact" as const },
    ],
    [source],
  );
  useEffect(() => {
    const timer = setTimeout(
      () =>
        void call<V>("list.preview", { list: { ...list, name: list.name || "preview" } })
          .then(setPreview)
          .catch((e) => setPreview({ error: String(e) })),
      350,
    );
    return () => clearTimeout(timer);
  }, [JSON.stringify(list), call]);
  const setFilter = (i: number, next: V) => setList({ ...list, filters: list.filters.map((f: V, j: number) => (j === i ? next : f)) });
  const kindOf = (field: string) =>
    fields.find((f) => f.value === field)?.kind ||
    (field.startsWith("event_count.") ? "count" : field.startsWith("no_event.") || field.startsWith("event.") ? "event" : "contact");
  return (
    <Modal
      title={value.id ? t("Edit list") : t("New list")}
      wide
      close={busy ? undefined : close}
      protectChanges
      footer={
        <div className="row-gap end grow">
          {remove && (
            <button className="btn ghost danger" onClick={remove}>
              {t("Delete list")}
            </button>
          )}
          <span className="grow faint small">
            {preview?.error ? <span className="danger-text">{preview.error}</span> : preview ? t("Currently matching: {n} people (do-not-contact excluded)", { n: preview.count }) : t("Calculating…")}
          </span>
          <button className="btn" onClick={close}>
            {t("Cancel")}
          </button>
          <button className="btn primary" disabled={!!busy || !list.name.trim() || !!preview?.error} onClick={() => save(list)}>
            {t("Save list")}
          </button>
        </div>
      }
    >
      <div className="form-grid">
        <label className="span">
          {t("List name")}
          <input value={list.name} placeholder={t("e.g. Used up free credits but never paid")} onChange={(e) => setList({ ...list, name: e.target.value })} />
        </label>
        <label className="span">
          {t("Purpose (tells the Agent why to contact these people)")}
          <input value={list.description} placeholder={t("e.g. find out where they got stuck and whether they need help")} onChange={(e) => setList({ ...list, description: e.target.value })} />
        </label>
        <label>
          {t("Candidates from")}
          <select value={list.source} onChange={(e) => setList({ ...list, source: e.target.value, filters: [] })}>
            <option value="">{t("Existing contacts")}</option>
            {sources.map((s) => (
              <option key={s.key} value={s.key}>
                {t("Source: {name}", { name: s.name })}
              </option>
            ))}
          </select>
        </label>
        <label>
          {t("Channel")}
          <select value={list.channel} onChange={(e) => setList({ ...list, channel: e.target.value })}>
            {messageChannels.map((c) => (
              <option key={c} value={c}>
                {channelNames[c]}
              </option>
            ))}
          </select>
        </label>
        <label>
          {t("Purpose")}
          <select value={list.playbook} onChange={(e) => setList({ ...list, playbook: e.target.value })}>
            <option value="">{t("Prepare outreach (default)")}</option>
            {purposes().map((p) => (
              <option key={p.id} value={p.id}>
                {p.label}
              </option>
            ))}
          </select>
        </label>
        <label>
          {t("Business context")}
          <select value={list.context} onChange={(e) => setList({ ...list, context: e.target.value })}>
            <option value="">{t("Contact’s existing information")}</option>
            {contexts().map((p) => (
              <option key={p.id} value={p.id}>
                {p.label}
              </option>
            ))}
          </select>
        </label>
      </div>
      <div className="section-h">
        {t("Filters")} <span className="c">{t("all must match")}</span>
        <span className="sp" />
        <button className="btn ghost" onClick={() => setList({ ...list, filters: [...list.filters, { field: fields[0].value, op: "eq", value: "" }] })}>
          <Icon name="plus" />
          {t("Add condition")}
        </button>
      </div>
      <div className="filters-edit">
        {list.filters.map((f: V, i: number) => {
          const kind = kindOf(f.field);
          return (
            <div className="filter-row" key={i}>
              <select
                value={f.field}
                onChange={(e) => {
                  const k = kindOf(e.target.value);
                  setFilter(i, {
                    field: e.target.value,
                    op: k === "count" ? "gte" : k === "event" ? (e.target.value.startsWith("no_event.") ? "is" : "within_days") : k === "role" ? "in" : k === "date" ? "never" : "eq",
                    value: k === "count" ? 2 : k === "event" ? (e.target.value.startsWith("no_event.") ? null : 30) : k === "role" ? ["open"] : "",
                    ...(k === "count" ? { days: 14 } : {}),
                  });
                }}
              >
                {fields.map((o) => (
                  <option key={o.value} value={o.value}>
                    {o.label}
                  </option>
                ))}
                {!fields.some((o) => o.value === f.field) && <option value={f.field}>{f.field}</option>}
              </select>
              {kind === "role" && (
                <div className="seg">
                  {["open", "waiting", "active", "closed"].map((r) => {
                    const on = (f.value || []).includes(r);
                    return (
                      <button key={r} className={on ? "on" : ""} onClick={() => setFilter(i, { ...f, value: on ? f.value.filter((x: string) => x !== r) : [...(f.value || []), r] })}>
                        {roleLabels[r]}
                      </button>
                    );
                  })}
                </div>
              )}
              {kind === "event" && !f.field.startsWith("no_event.") && (
                <>
                  <select value={f.op} onChange={(e) => setFilter(i, { ...f, op: e.target.value })}>
                    <option value="within_days">{t("within the last")}</option>
                    <option value="older_than_days">{t("more than")}</option>
                  </select>
                  <input className="num" type="number" min="1" value={f.value ?? 30} onChange={(e) => setFilter(i, { ...f, value: Number(e.target.value) })} />
                  <span className="muted">{t("days")}</span>
                </>
              )}
              {kind === "count" && (
                <>
                  <input
                    className="num"
                    type="number"
                    min="1"
                    aria-label={t("Days")}
                    placeholder={t("All")}
                    value={f.days ?? ""}
                    onChange={(e) => setFilter(i, { ...f, days: e.target.value ? Number(e.target.value) : undefined })}
                  />
                  <span className="muted">{t("days, ")}</span>
                  <select value={f.op} onChange={(e) => setFilter(i, { ...f, op: e.target.value })}>
                    <option value="gte">{t("at least")}</option>
                    <option value="lte">{t("at most")}</option>
                    <option value="eq">{t("exactly")}</option>
                  </select>
                  <input className="num" type="number" min="0" aria-label={t("Count")} value={f.value ?? 1} onChange={(e) => setFilter(i, { ...f, value: Number(e.target.value) })} />
                  <span className="muted">{t("times")}</span>
                </>
              )}
              {kind === "date" && (
                <>
                  <select value={f.op} onChange={(e) => setFilter(i, { ...f, op: e.target.value, value: e.target.value === "never" ? null : 30 })}>
                    <option value="never">{t("Never")}</option>
                    <option value="older_than_days">{t("More than N days ago")}</option>
                    <option value="within_days">{t("Within the last N days")}</option>
                  </select>
                  {f.op !== "never" && <input className="num" type="number" min="1" value={f.value ?? 30} onChange={(e) => setFilter(i, { ...f, value: Number(e.target.value) })} />}
                </>
              )}
              {(kind === "attr" || kind === "contact") && (
                <>
                  <select value={f.op} onChange={(e) => setFilter(i, { ...f, op: e.target.value })}>
                    {[
                      ["eq", t("is")],
                      ["neq", t("is not")],
                      ["contains", t("contains")],
                      ["gt", t("greater than")],
                      ["gte", "≥"],
                      ["lt", t("less than")],
                      ["lte", "≤"],
                      ["empty", t("is empty")],
                      ["not_empty", t("is not empty")],
                    ].map(([op, label]) => (
                      <option key={op} value={op}>
                        {label}
                      </option>
                    ))}
                  </select>
                  {!["empty", "not_empty"].includes(f.op) && (
                    <input value={f.value ?? ""} onChange={(e) => setFilter(i, { ...f, value: /^-?\d+(\.\d+)?$/.test(e.target.value) ? Number(e.target.value) : e.target.value })} />
                  )}
                </>
              )}
              <span className="sp" />
              <button className="icon-btn sm" aria-label={t("Remove condition")} onClick={() => setList({ ...list, filters: list.filters.filter((_: V, j: number) => j !== i) })}>
                <Icon name="close" size={12} />
              </button>
            </div>
          );
        })}
        {!list.filters.length && <p className="faint small">{t("With no conditions, every candidate is included.")}{list.source ? t("The source’s attributes and event types come from its manifest.") : ""}</p>}
      </div>
      {preview && preview.sample?.length > 0 && (
        <>
          <div className="section-h">
            {t("Preview")} <span className="c">{t("first {n}", { n: preview.sample.length })}</span>
          </div>
          <div className="list">
            {preview.sample.map((m: V, i: number) => (
              <div className="identity" key={i}>
                <Avatar name={m.name || "?"} />
                <span className="identity-main">
                  <b>{m.name}</b>
                  <small>{m.evidence.join(" · ")}</small>
                </span>
              </div>
            ))}
          </div>
        </>
      )}
      {!stageList().length && <p className="faint small">{t("Loading workspace configuration…")}</p>}
    </Modal>
  );
}

/** One person at a time: evidence and "why now" beside the draft. Approving never sends. */
function Review({
  list,
  members,
  rows,
  busy,
  call,
  work,
  limit,
  sentToday,
  close,
  notify,
  openPerson,
  openDraft,
}: {
  list: V;
  members: V[];
  rows: Row[];
  busy: string;
  call: Call;
  work: Work;
  limit: number;
  sentToday: number;
  close: () => void;
  notify: (message: string) => void;
  openPerson: (slug: string) => void;
  openDraft: (draft: V) => void;
}) {
  const [index, setIndex] = useState(0);
  const [draft, setDraft] = useState<V | null>(null);
  const [facts, setFacts] = useState<V[]>([]);
  const [addresses, setAddresses] = useState<string[] | null>(null);
  const [done, setDone] = useState<Set<string>>(new Set());
  const queue = members.filter((m) => !done.has(m.key));
  const member = queue[Math.min(index, queue.length - 1)];
  useEffect(() => {
    if (!member?.contact) return;
    let alive = true;
    setDraft(null);
    void call<V[]>("draft.list", { slug: member.contact }).then((items) => {
      if (alive) setDraft(structuredClone(items.find((d) => d.path === member.draft) || null));
    });
    void call<V[]>("source.person", { slug: member.contact }).then((f) => alive && setFacts(f)).catch(() => setFacts([]));
    setAddresses(null);
    void call<string[]>("contact.addresses", { slug: member.contact }).then((a) => alive && setAddresses(a)).catch(() => {});
    return () => {
      alive = false;
    };
  }, [member?.key, call]);
  const next = () => {
    setDone(new Set([...done, member.key]));
    setIndex(Math.max(0, Math.min(index, queue.length - 2)));
  };
  const saveDraft = () => call<V>("draft.save", { ...draft!.metadata, path: draft!.path, body: draft!.body });
  const approve = () =>
    work(t("Approving draft…"), async () => {
      await saveDraft();
      await call("draft.status", { path: draft!.path, status: "approved" });
      notify(t("Approved the draft for {name}; you still confirm it in the draft before sending", { name: member.name }));
      next();
    });
  const row = rows.find((r) => r.values.Slug === member?.contact);
  const name = row?.values.Name || member?.name;
  if (!member)
    return (
      <div className="review-done">
        <Empty title={t("This batch is reviewed")} icon="check">
          {t("Approved drafts are under each contact’s “Drafts”; you still confirm each one before sending. Sent today: {sent} / {limit}.", { sent: sentToday, limit })}
          <div className="row-gap">
            <button className="btn primary" onClick={close}>
              {t("Back to list")}
            </button>
          </div>
        </Empty>
      </div>
    );
  const locked = draft && ["sent", "sending", "uncertain"].includes(draft.metadata.status);
  return (
    <div className="review">
      <aside className="queue">
        <div className="queue-head">
          <button className="link plain" onClick={close}>
            <Icon name="back" size={12} /> {list.name}
          </button>
          <div className="faint small">
            {t("{n} pending review · sent today {sent}/{limit}", { n: queue.length, sent: sentToday, limit })}
          </div>
        </div>
        {queue.map((m, i) => (
          <button key={m.key} className={`q ${m.key === member.key ? "on" : ""}`} onClick={() => setIndex(i)}>
            <Avatar name={m.name || "?"} />
            <span className="grow clip">{m.name}</span>
          </button>
        ))}
      </aside>
      <section className="review-main">
        <header className="rec-head">
          <Avatar name={name || "?"} size="lg" />
          <div className="rec-title">
            <h1>{name}</h1>
            <div className="rec-sub">
              {member.email && <span>{member.email}</span>}
              {member.contact && (
                <button className="link" onClick={() => openPerson(member.contact)}>
                  {t("Open person page")}
                </button>
              )}
            </div>
          </div>
        </header>
        <div className="review-split">
          <div className="evidence">
            <div className="why-now">
              <b>{t("Why reach out now")}</b>
              <ul>
                {(member.evidence || []).map((e: string, i: number) => (
                  <li key={i}>{e}</li>
                ))}
              </ul>
              {list.description && <p className="small">{t("List purpose: {text}", { text: list.description })}</p>}
            </div>
            {(row?.values["AI summary"] || row?.values["Next action"]) && (
              <section className="card">
                <h3>
                  {t("What the Agent knows")} <span className="k ai">{t("AI inferred")}</span>
                </h3>
                {row?.values["AI summary"] && <p className="prewrap small">{row.values["AI summary"]}</p>}
                {row?.values["Next action"] && (
                  <p className="prewrap small">
                    <b>{t("Next step")}{t(": ")}</b>
                    {row.values["Next action"]}
                  </p>
                )}
                <button className="link subtle" onClick={() => openPerson(member.contact)}>
                  {t("See the full analysis and Agent memory")}
                </button>
              </section>
            )}
            {facts
              .filter((f) => f.match !== "pending")
              .map((f) => (
                <section className="card" key={f.external_id}>
                  <h3>
                    {t("Source facts")} <span className="k fact">{f.source_name || f.source}</span>
                  </h3>
                  <dl className="facts">
                    {(f.facts || []).map((x: V) => (
                      <div key={x.key}>
                        <dt>{x.label}</dt>
                        <dd>{x.type === "datetime" ? new Date(String(x.value)).toLocaleDateString(locale) : String(x.value)}</dd>
                      </div>
                    ))}
                  </dl>
                </section>
              ))}
          </div>
          <div className="draft-pane">
            {!draft ? (
              <p className="faint pad">{t("Loading draft…")}</p>
            ) : (
              <>
                <div className="draft-fields">
                  <label>
                    {t("To")}
                    <input value={draft.metadata.to || ""} disabled={!!locked} onChange={(e) => setDraft({ ...draft, metadata: { ...draft.metadata, to: e.target.value, status: "draft" } })} />
                  </label>
                  {draft.metadata.channel === "email" && (
                    <RecipientCheck
                      to={draft.metadata.to || ""}
                      known={addresses}
                      disabled={!!locked}
                      pick={(to) => setDraft({ ...draft, metadata: { ...draft.metadata, to, status: "draft" } })}
                    />
                  )}
                  {draft.metadata.channel === "email" && (
                    <label>
                      {t("Subject")}
                      <input value={draft.metadata.subject || ""} disabled={!!locked} onChange={(e) => setDraft({ ...draft, metadata: { ...draft.metadata, subject: e.target.value, status: "draft" } })} />
                    </label>
                  )}
                </div>
                <textarea className="draft-text" value={draft.body} readOnly={!!locked} onChange={(e) => setDraft({ ...draft, body: e.target.value, metadata: { ...draft.metadata, status: "draft" } })} />
                <DraftHints body={draft.body} channel={draft.metadata.channel} language={row?.values.Language} />
                <div className="draft-actions">
                  <button className="btn ghost" disabled={!!busy} onClick={() => void work(t("Skipping…"), async () => (await call("list.skip", { id: list.id, member: member.key }), next()))}>
                    {t("Skip")}
                  </button>
                  <button
                    className="btn ghost danger"
                    disabled={!!busy}
                    onClick={() =>
                      void work(t("Recording do-not-contact…"), async () => {
                        if (!(await confirm(t("Stop contacting {name}? Their address will be added to suppression.txt, excluded from lists and sending, and this draft will be discarded.", { name: member.name })))) return;
                        await call("list.suppress", { id: list.id, member: member.key });
                        next();
                      })
                    }
                  >
                    {t("Do not contact")}
                  </button>
                  <span className="sp" />
                  <button className="btn" disabled={!!busy || !!locked} onClick={() => void work(t("Saving…"), async () => (await saveDraft(), notify(t("Draft saved"))))}>
                    {t("Save")}
                  </button>
                  {draft.metadata.status === "approved" ? (
                    <button className="btn" onClick={() => openDraft(draft)}>
                      {t("Go to send…")}
                    </button>
                  ) : (
                    <button className="btn primary" disabled={!!busy || !!locked || !draft.body.trim()} onClick={() => void approve()}>
                      <Icon name="check" />
                      {t("Approve and next")}
                    </button>
                  )}
                </div>
                <Remember
                  remember={(note) =>
                    work(t("Remembering preference…"), async () => {
                      await call("preferences.add", { note });
                      notify(t("Remembered. Future analysis and drafts will follow it (edit in Settings → Writing preferences)"));
                    })
                  }
                />
                <p className="faint small">{t("Approving never sends. To send, choose a sending account in the draft and confirm again; do-not-contact addresses and the daily limit are always enforced.")}</p>
              </>
            )}
          </div>
        </div>
      </section>
    </div>
  );
}

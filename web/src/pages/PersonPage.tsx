import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Icon, Empty, Avatar, Tag, authors, statuses, statusTone, channelNames, messageChannels } from "../components/ui";
import { TableEditor } from "../kits/TableEditor";
import { ProfileFields, type FieldDef } from "../components/ProfileFields";
import { draftStale, stages, stageTone, relative, type Row, type V } from "../app/domain";
import { locale, t } from "../app/i18n";

type Props = {
  workspaceDocument?: ReactNode;
  selected: string;
  detail: V | null;
  stale: boolean;
  facts: V[];
  fields: FieldDef[];
  saveField: (title: string, value: unknown) => Promise<unknown>;
  addField: (title: string, type: string, choices: string[]) => Promise<unknown>;
  addChoice: (title: string, choice: string) => Promise<unknown>;
  decide: (identity: V, action: "confirm" | "reject") => void;
  followUp: (value: { days?: number | null; date?: string }) => void;
  reply: () => void;
  loadMemory: () => Promise<V>;
  agentWorking: boolean;
  loadAnalysis: () => Promise<V>;
  openSource: (href: string) => void;
  showSession: (session: string) => void;
  showAgentWork: () => void;
  loadHistory: () => Promise<V[]>;
  tab: string;
  root: string;
  notePath: string;
  busy: string;
  draftChannel: string;
  setTab: (v: string) => void;
  setDraftChannel: (v: string) => void;
  update: (v: V) => unknown;
  edit: () => void;
  merge: () => void;
  addChannel: () => void;
  intake: () => void;
  newDraft: () => void;
  openDraft: (d: V) => void;
  showNote: (path: string) => Promise<unknown>;
  closeNote: () => void;
  fail: (message: string) => void;
  analyze: () => void;
  resumeDraft: boolean;
  hasDraftEdit: (draft: V) => boolean;
};

const tabs = [
  ["overview", t("Overview")],
  ["drafts", t("Drafts")],
  ["timeline", t("Activity")],
  ["dossier", t("Dossier")],
] as const;

export function PersonPage(p: Props) {
  const content = useRef<HTMLDivElement>(null);
  const scrollPositions = useRef(new Map<string, number>());
  useLayoutEffect(() => {
    const element = content.current;
    if (!element) return;
    const key = `${p.selected}:${p.tab}`;
    element.scrollTop = scrollPositions.current.get(key) || 0;
    return () => {
      scrollPositions.current.set(key, element.scrollTop);
    };
  }, [p.selected, p.tab, !!p.detail]);

  if (p.workspaceDocument) return <div className="record-doc">{p.workspaceDocument}</div>;
  const d = p.detail;
  if (!d)
    return (
      <Empty title={p.selected ? t("Opening contact…") : t("Select a contact")}>
        {t("Review background and activity to prepare for your next conversation.")}
      </Empty>
    );
  const v = d.contact.values;
  const pending = d.drafts.filter((draft: V) => ["draft", "approved"].includes(draft.metadata.status));
  const counts: Record<string, number | null> = {
    overview: null,
    drafts: pending.length || null,
    timeline: d.timeline.length || null,
    dossier: null,
  };
  const draftList = d.drafts.filter(
    (draft: V) => p.draftChannel === "all" || draft.metadata.channel === p.draftChannel,
  );
  // Filter by the channels this person's drafts actually use, so new kinds don't crowd the bar.
  const draftChannels = [
    "all",
    ...messageChannels.filter(
      (c) => c === p.draftChannel || d.drafts.some((draft: V) => draft.metadata.channel === c),
    ),
  ];

  return (
    <div className="record">
      <div ref={content} className={`rec-main ${p.tab === "dossier" ? "is-doc" : ""}`}>
        <header className="rec-head">
          <Avatar name={String(v.Name || "?")} size="lg" />
          <div className="rec-title">
            <h1>
              {v.Name || t("Untitled")}
              {v["Native name"] && !String(v.Name || "").includes(v["Native name"]) && <small>{v["Native name"]}</small>}
            </h1>
            <div className="rec-sub">
              <span>{v.Affiliation || t("No organization yet")}</span>
              <Tag tone={stageTone(v.Stage)}>{v.Stage || "—"}</Tag>
              {p.stale && <Tag tone="amber">{t("Follow up due")}</Tag>}
              {v.Campaign && <Tag>{v.Campaign}</Tag>}
              {p.agentWorking && (
                <button className="pill agent-working" onClick={p.showAgentWork} title={t("See what the Agent is doing in the right pane")}>
                  <span className="pulse" />
                  {t("Agent working")}
                </button>
              )}
            </div>
          </div>
        </header>
        <div className="rec-actions">
          <button className="btn primary" disabled={!!p.busy} onClick={p.analyze} title={t("Ask the Agent on the right about this person's background, recent activity and next step")}>
            <Icon name="spark" />
            {t("Ask Agent")}
          </button>
          <button className="btn" disabled={!!p.busy} onClick={p.reply} title={t("Have the Agent on the right draft a reply to the latest message for your review")}>
            <Icon name="mail" />
            {t("Agent drafts reply")}
          </button>
          <button className="btn" onClick={p.newDraft}>
            <Icon name="edit" />
            {p.resumeDraft ? t("Continue draft") : t("Write draft")}
          </button>
          <button className="btn" onClick={p.intake}>
            <Icon name="plus" />
            {t("Log info / feedback")}
          </button>
          <button className="btn ghost" onClick={p.edit}>
            {t("Edit profile")}
          </button>
          <button className="btn ghost" onClick={p.merge} title={t("If the same person has two records, merge the other one into this")}>
            {t("Merge…")}
          </button>
        </div>
        <div
          className="tabs"
          role="tablist"
          aria-label={t("Contact sections")}
          onKeyDown={(e) => {
            if (p.busy || !["ArrowLeft", "ArrowRight", "Home", "End"].includes(e.key)) return;
            const buttons = [...e.currentTarget.querySelectorAll<HTMLButtonElement>("[role=tab]")];
            const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
            const next =
              e.key === "Home"
                ? 0
                : e.key === "End"
                  ? buttons.length - 1
                  : (index + (e.key === "ArrowRight" ? 1 : -1) + buttons.length) % buttons.length;
            e.preventDefault();
            buttons[next].focus();
            buttons[next].click();
          }}
        >
          {tabs.map(([key, title]) => (
            <button
              key={key}
              id={`tab-${key}`}
              role="tab"
              className="tab"
              disabled={!!p.busy}
              tabIndex={p.tab === key ? 0 : -1}
              aria-selected={p.tab === key}
              aria-controls={`panel-${key}`}
              onClick={() => {
                p.setTab(key);
                if (key === "dossier" && !p.notePath) void p.showNote(d.dossierPath);
              }}
            >
              {title}
              {counts[key] !== null && <span className="count">{counts[key]}</span>}
            </button>
          ))}
        </div>

        {p.tab === "overview" && (
          <div role="tabpanel" id="panel-overview" aria-labelledby="tab-overview" className="panel">
            <section className="next-step">
              <div>
                <span className="label">{t("Next step")}</span>
                <p>
                  {v["Next action"] ||
                    (pending.length
                      ? t("Review drafts to prepare for the next conversation.")
                      : t("Sort out needs and recent feedback, and decide the purpose of the next contact."))}
                </p>
              </div>
              <button className="btn" onClick={() => (pending.length ? p.setTab("drafts") : p.analyze())}>
                {pending.length ? t("Review {n} drafts", { n: pending.length }) : t("Ask Agent")}
                <Icon name="arrow" />
              </button>
            </section>
            <FollowUp
              value={String(v["Follow up"] || "").slice(0, 10)}
              cadence={Number(v["Keep in touch"]) || 0}
              busy={!!p.busy}
              change={p.followUp}
              setCadence={(days) => void p.saveField("Keep in touch", days || null)}
            />
            {p.facts.filter((f) => f.match === "pending").map((f) => (
              <div className="suggestion" key={`${f.source}:${f.external_id}`}>
                <Icon name="link" />
                <span className="grow">
                  <b>{f.source_name || f.source}</b>
                  {t(" user “{name}” may be this person", { name: f.name || f.external_id })}
                  <small className="faint"> · {(f.emails || []).map((e: [string, boolean]) => e[0]).join(t(", "))}
                    {t(" (unverified)")}
                  </small>
                </span>
                <button className="btn" disabled={!!p.busy} onClick={() => p.decide(f, "reject")}>
                  {t("Not them")}
                </button>
                <button className="btn primary" disabled={!!p.busy} onClick={() => p.decide(f, "confirm")}>
                  {t("Same person")}
                </button>
              </div>
            ))}
            <div className="cards">
              {p.facts
                .filter((f) => f.match !== "pending")
                .map((f) => (
                  <section className="card" key={`${f.source}:${f.external_id}`}>
                    <h3>
                      {t("Source facts")} <span className="k fact">{f.source_name || f.source}</span>
                    </h3>
                    {(f.facts || []).length ? (
                      <dl className="facts">
                        {f.facts.map((x: V) => (
                          <div key={x.key}>
                            <dt>
                              {x.label}
                              {x.pii && <span className="faint"> · {t("Personal data")}</span>}
                            </dt>
                            <dd>{x.type === "datetime" ? relative(String(x.value)) || String(x.value) : String(x.value)}</dd>
                          </div>
                        ))}
                      </dl>
                    ) : (
                      <p className="faint">{t("The source provided no attributes.")}</p>
                    )}
                    {(f.history || []).filter((h: V) => h.key).length > 0 && (
                      <details className="fact-history">
                        <summary>{t("Changes {n}", { n: (f.history || []).filter((h: V) => h.key).length })}</summary>
                        <ul>
                          {(f.history || [])
                            .filter((h: V) => h.key)
                            .slice(-8)
                            .reverse()
                            .map((h: V, i: number) => (
                              <li key={i}>
                                <span className="faint">{new Date(h.at).toLocaleDateString(locale)}</span> {h.label || h.key}{t(": ")}{h.from} → <b>{h.to}</b>
                              </li>
                            ))}
                        </ul>
                      </details>
                    )}
                    <p className="faint small">
                      {f.external_id} · {t("synced {when}", { when: relative(f.synced_at) })}
                    </p>
                  </section>
                ))}
              <section className="card">
                <h3>
                  {t("Hook")} <span className="k fact">{t("Profile")}</span>
                </h3>
                <p className="prewrap">{v.Hook || <span className="faint">{t("No reason to contact this person recorded yet.")}</span>}</p>
              </section>
              <section className="card">
                <h3>
                  {t("Background summary")} <span className="k ai">{t("AI inferred")}</span>
                </h3>
                {v["AI summary"] ? (
                  <p className="prewrap">{v["AI summary"]}</p>
                ) : (
                  <p className="faint">
                    {t("No summary yet. ")}
                    <button className="link" onClick={p.analyze}>
                      {t("Ask the Agent")}
                    </button>
                    {t(" to generate one.")}
                  </p>
                )}
              </section>
              <AgentAnalysis load={p.loadAnalysis} version={d} analyze={p.analyze} openSource={p.openSource} showSession={p.showSession} />
              <AgentMemory load={p.loadMemory} version={d} open={p.showNote} analyze={p.analyze} />
              {v.Notes && (
                <section className="card wide">
                  <h3>
                    {t("Before reaching out")} <span className="k ask">{t("Heads-up")}</span>
                  </h3>
                  <p className="prewrap">{v.Notes}</p>
                </section>
              )}
            </div>
            <div className="section-h">
              {t("Pending drafts")} <span className="c">{pending.length}</span>
              <span className="sp" />
              <button className="link" onClick={() => p.setTab("drafts")}>
                {t("All drafts")}
              </button>
            </div>
            {pending.length ? (
              <div className="rows">
                {pending.slice(0, 3).map((draft: V) => (
                  <button key={draft.path} className="row-item" onClick={() => p.openDraft(draft)}>
                    <Icon name={draft.metadata.channel === "email" ? "mail" : "send"} />
                    <span className="grow">
                      <b>{draft.metadata.subject || t("Message draft")}</b>
                      <small>{channelNames[draft.metadata.channel] || draft.metadata.channel}</small>
                    </span>
                    <Tag tone={p.hasDraftEdit(draft) ? "blue" : statusTone[draft.metadata.status]}>
                      {p.hasDraftEdit(draft) ? t("Unsaved edits") : t(statuses[draft.metadata.status])}
                    </Tag>
                  </button>
                ))}
              </div>
            ) : (
              <p className="faint pad">{t("No pending drafts.")}</p>
            )}
            <div className="section-h">
              {t("Recent activity")} <span className="c">{d.timeline.length}</span>
              <span className="sp" />
              <button className="link" onClick={() => p.setTab("timeline")}>
                {t("All activity")}
              </button>
            </div>
            <Timeline rows={d.timeline.slice(0, 4)} open={p.showNote} />
          </div>
        )}

        {p.tab === "drafts" && (
          <section role="tabpanel" id="panel-drafts" aria-labelledby="tab-drafts" className="panel">
            <div className="panel-bar">
              <div className="seg" aria-label={t("Filter drafts by channel")}>
                {draftChannels.map((c) => (
                  <button
                    key={c}
                    aria-pressed={p.draftChannel === c}
                    className={p.draftChannel === c ? "on" : ""}
                    onClick={() => p.setDraftChannel(c)}
                  >
                    {c === "all" ? t("All") : channelNames[c]}
                  </button>
                ))}
              </div>
              <span className="sp" />
              <button className="btn primary" onClick={p.newDraft}>
                <Icon name="plus" />
                {p.resumeDraft ? t("Continue draft") : t("New draft")}
              </button>
            </div>
            {draftList.length ? (
              <div className="draft-grid">
                {draftList.map((draft: V) => (
                  <button key={draft.path} className="draft-card" onClick={() => p.openDraft(draft)}>
                    <span className="draft-meta">
                      <span>{channelNames[draft.metadata.channel] || draft.metadata.channel}</span>
                      <Tag tone={p.hasDraftEdit(draft) ? "blue" : statusTone[draft.metadata.status]}>
                        {p.hasDraftEdit(draft) ? t("Unsaved edits") : statuses[draft.metadata.status] ? t(statuses[draft.metadata.status]) : draft.metadata.status}
                      </Tag>
                    </span>
                    <b>{draft.metadata.subject || t("Message draft")}</b>
                    {draftStale(draft, v["Last reply"]) && <Tag tone="red" title={t("They wrote again after this draft was created")}>{t("May be stale")}</Tag>}
                    <p>{draft.body.slice(0, 220)}</p>
                    <small>{authors[draft.metadata.created_by] ? t(authors[draft.metadata.created_by]) : draft.metadata.created_by || t("Manual")}</small>
                  </button>
                ))}
              </div>
            ) : (
              <Empty title={t("No drafts yet")} icon="edit">
                {t("Write one yourself or let the Agent on the right draft it. Everything needs your approval before sending.")}
              </Empty>
            )}
          </section>
        )}

        {p.tab === "timeline" && (
          <section role="tabpanel" id="panel-timeline" aria-labelledby="tab-timeline" className="panel">
            <div className="panel-bar">
              <span className="muted">{t("Email, user feedback, follow-up notes and background")}</span>
              <span className="sp" />
              <button className="btn" onClick={p.intake}>
                <Icon name="plus" />
                {t("Log info / feedback")}
              </button>
            </div>
            {d.timeline.length ? (
              <Timeline rows={d.timeline} open={p.showNote} reply={p.busy ? undefined : p.reply} />
            ) : (
              <Empty title={t("Start with the first record")} icon="history">
                {t("Paste a web page, social post or user feedback, or sync your mailbox.")}
              </Empty>
            )}
          </section>
        )}

        <section
          role="tabpanel"
          id="panel-dossier"
          aria-labelledby="tab-dossier"
          className={p.tab === "dossier" ? "panel doc-panel" : "hidden"}
        >
          {p.notePath ? (
            <div className="doc-frame">
              <div className="doc-bar">
                <Icon name="note" />
                <b>{p.notePath === d.dossierPath ? t("Contact dossier") : p.notePath.split("/").pop()}</b>
                <small className="faint">{p.notePath}</small>
                <span className="sp" />
                <button className="btn" onClick={p.closeNote}>
                  {t("Save and close")}
                </button>
              </div>
              <TableEditor key={p.notePath} root={p.root} path={p.notePath} onError={p.fail} onOpen={p.showNote} />
            </div>
          ) : (
            <Empty title={t("Dossier saved")} icon="note">
              <button className="btn" onClick={() => void p.showNote(d.dossierPath)}>
                {t("Open dossier")}
              </button>
            </Empty>
          )}
        </section>
      </div>

      <aside className="attrs" aria-label={t("Contact properties")}>
        <h4>{t("Properties")}</h4>
        <label className="attr">
          <span className="lab">{t("Stage")}</span>
          <select
            className="bare"
            disabled={!!p.busy}
            aria-label={t("Contact stage")}
            value={v.Stage || ""}
            onChange={(e) => p.update({ Stage: e.target.value })}
          >
            {!v.Stage && <option value="">{t("Not set")}</option>}
            {stages().map((s) => (
              <option key={s}>{s}</option>
            ))}
          </select>
        </label>
        <Attr label={t("Last sent")} value={relative(v["Last contacted"]) || t("Never")} />
        <Attr label={t("Last reply")} value={relative(v["Last reply"]) || "—"} />
        <hr />
        <h4>{t("Profile")}</h4>
        <ProfileFields fields={p.fields} values={v} busy={!!p.busy} save={p.saveField} addField={p.addField} addChoice={p.addChoice} />
        <hr />
        <h4>
          {t("Channels")} <span className="c">{d.channels.length}</span>
          <button className="icon-btn sm" aria-label={t("Add channel")} onClick={p.addChannel}>
            <Icon name="plus" size={14} />
          </button>
        </h4>
        {d.channels.length === 0 && <p className="faint small">{t("Add an email, social account or website.")}</p>}
        {d.channels.map((c: Row) => (
          <div className="channel" key={c.id} title={c.values.Source}>
            <span className="channel-kind">{channelNames[c.values.Kind] || c.values.Kind}</span>
            <span className="channel-value">{c.values.Value}</span>
            <span className={`channel-ok ${c.values.Verified ? "yes" : ""}`}>
              {c.values.Verified ? t("Verified") : t("Unverified")}
            </span>
          </div>
        ))}
        <hr />
        <ChangeHistory key={p.selected} load={p.loadHistory} />
      </aside>
    </div>
  );
}

/** The next follow-up date, with quick snoozes. Sending schedules it; a reply clears it. */
const cadences: [number, string][] = [
  [0, t("None")],
  [7, t("Weekly")],
  [14, t("Every 2 weeks")],
  [30, t("Monthly")],
  [90, t("Quarterly")],
  [180, t("Every 6 months")],
];

function FollowUp({
  value,
  cadence,
  busy,
  change,
  setCadence,
}: {
  value: string;
  cadence: number;
  busy: boolean;
  change: (v: { days?: number | null; date?: string }) => void;
  setCadence: (days: number) => void;
}) {
  const today = new Date().toISOString().slice(0, 10);
  const due = !!value && value <= today;
  return (
    <div className={`follow-up ${due ? "due" : ""}`}>
      <Icon name="followups" size={14} />
      <span className="grow">
        {value ? (
          <>
            {t("Next follow-up: ")}
            <b>{new Date(value + "T00:00:00").toLocaleDateString(locale, { month: "long", day: "numeric", weekday: "short" })}</b>
            {due && <span className="tag red">{t("Due")}</span>}
          </>
        ) : (
          <span className="faint">{t("No follow-up scheduled")}</span>
        )}
      </span>
      {[
        [1, t("Tomorrow")],
        [3, t("In 3 days")],
        [7, t("In 1 week")],
      ].map(([days, label]) => (
        <button key={days} className="btn ghost" disabled={busy} onClick={() => change({ days: days as number })}>
          {label}
        </button>
      ))}
      <input type="date" aria-label={t("Choose follow-up date")} className="date-pick" disabled={busy} value={value} min={today} onChange={(e) => e.target.value && change({ date: e.target.value })} />
      {value && (
        <button className="btn ghost" disabled={busy} onClick={() => change({ days: null })}>
          {t("Clear")}
        </button>
      )}
      <label className="cadence" title={t("When set, sending a message schedules the next follow-up at this interval; you'll also be reminded if it's been too long")}>
        {t("Keep in touch")}
        <select className="bare" aria-label={t("Keep-in-touch interval")} disabled={busy} value={cadences.some(([d]) => d === cadence) ? cadence : cadence || 0} onChange={(e) => setCadence(Number(e.target.value))}>
          {cadences.map(([d, l]) => (
            <option key={d} value={d}>
              {l}
            </option>
          ))}
          {cadence > 0 && !cadences.some(([d]) => d === cadence) && <option value={cadence}>{t("Every {n} days", { n: cadence })}</option>}
        </select>
      </label>
    </div>
  );
}

const actorNames: Record<string, string> = {
  "system:agent": "Agent",
  "system:contacts": t("You"),
  "system:timeline": t("Activity"),
  "system:emailsync": t("Mail sync"),
  "system:sources": t("Source sync"),
  "system:followup": t("Follow-up"),
  "system:merge": t("Merge"),
};

/** Field changes of this contact, loaded when opened. "Who" shows once the table kit records it. */
function ChangeHistory({ load }: { load: () => Promise<V[]> }) {
  const [items, setItems] = useState<V[] | null>(null);
  const show = (v: unknown) => (v === null || v === undefined || v === "" ? t("(empty)") : Array.isArray(v) ? v.join(", ") : String(v));
  return (
    <details
      className="change-history"
      onToggle={(e) => {
        if ((e.target as HTMLDetailsElement).open && items === null) void load().then(setItems).catch(() => setItems([]));
      }}
    >
      <summary>{t("Change history")}</summary>
      {items === null ? (
        <p className="faint small">{t("Loading…")}</p>
      ) : items.length ? (
        <ul>
          {items.slice(0, 40).map((h) => (
            <li key={h.id}>
              <span className="faint">{new Date(h.created_at).toLocaleString(locale, { month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" })}</span>{" "}
              {h.actor && <span className="tag gray">{actorNames[h.actor] || h.actor}</span>} <b>{h.field_title || t("Field")}</b>{t(": ")}{show(h.before)} → {show(h.after)}
            </li>
          ))}
        </ul>
      ) : (
        <p className="faint small">{t("No changes yet.")}</p>
      )}
    </details>
  );
}

function Attr({ label, value }: { label: string; value?: string }) {
  return (
    <div className="attr">
      <span className="lab">{label}</span>
      <span className="val" title={value || ""}>
        {value || <span className="faint">—</span>}
      </span>
    </div>
  );
}

const directionIcon: Record<string, string> = { in: "inbox", out: "send", note: "note" };
const directionName: Record<string, string> = { in: t("Received"), out: t("Sent"), note: t("Note") };

function Timeline({ rows, open, reply }: { rows: Row[]; open: (path: string) => Promise<unknown>; reply?: () => void }) {
  if (!rows.length) return <p className="faint pad">{t("No activity yet.")}</p>;
  return (
    <div className="timeline">
      {rows.map((r) => (
        <div className="tl-item" key={r.id}>
          <button className="tl-open" disabled={!r.values.File} onClick={() => void open(r.values.File)}>
            <span className="tl-ic">
              <Icon name={r.values.Channel === "email" ? "mail" : directionIcon[r.values.Direction] || "note"} size={14} />
            </span>
            <span className="grow">
              <b>{r.values.Title || t("Untitled record")}</b>
              <small>
                {directionName[r.values.Direction] || r.values.Direction} ·{" "}
                {channelNames[r.values.Channel] || r.values.Channel}
              </small>
            </span>
            <span className="faint small">{relative(r.values.At)}</span>
          </button>
          {reply && r.values.Direction === "in" && r.values.Channel === "email" && r.values.File && (
            <button className="link subtle" title={t("Have the Agent on the right read the intent of this message and draft a reply for review")} onClick={reply}>
              {t("Agent reply")}
            </button>
          )}
        </div>
      ))}
    </div>
  );
}

// Entry kinds of memory/<slug>.md (gc-service memory.rs): the label the backend writes, the
// English UI label and the tag tone. Workspaces written by older builds have Chinese labels, so
// those (and the raw kind names) are recognized too.
const memoryKinds: Record<string, { label: string; tone: string; aliases: string[] }> = {
  fact: { label: t("Fact"), tone: "fact", aliases: ["fact", "事实"] },
  insight: { label: t("Insight"), tone: "ai", aliases: ["insight", "判断"] },
  preference: { label: t("Preference"), tone: "fact", aliases: ["preference", "偏好"] },
  commitment: { label: t("Commitment"), tone: "ask", aliases: ["commitment", "约定"] },
  question: { label: t("To confirm"), tone: "ask", aliases: ["question", "to confirm", "open question", "待确认"] },
  change: { label: t("Actions"), tone: "", aliases: ["change", "actions", "action", "操作"] },
};
function memoryKind(label = "") {
  const key = label.trim().toLowerCase();
  return Object.keys(memoryKinds).find((k) => memoryKinds[k].aliases.includes(key));
}

/** What the Agent has learned and changed for this person, newest first (memory/<slug>.md). */
function AgentMemory({ load, version, open, analyze }: { load: () => Promise<V>; version: unknown; open: (path: string) => Promise<unknown>; analyze: () => void }) {
  const [memory, setMemory] = useState<V | null>(null);
  const [all, setAll] = useState(false);
  useEffect(() => {
    let alive = true;
    load()
      .then((m) => alive && setMemory(m))
      .catch(() => alive && setMemory(null));
    return () => {
      alive = false;
    };
    // Reload whenever the contact detail refreshes (workspace changes, including Agent writes).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [version]);
  const entries = String(memory?.text || "")
    .split(/\n(?=- \d{4}-)/)
    .filter((e) => e.startsWith("- "))
    .map((e) => {
      const [head, ...rest] = e.slice(2).split("\n");
      const [at, kind, origin] = head.split(" · ");
      return { at, kind: memoryKind(kind), label: kind, origin, text: rest.map((l) => l.replace(/^ {2}/, "")).join("\n").trim() };
    })
    .reverse();
  const shown = all ? entries : entries.filter((e) => e.kind !== "change").slice(0, 6);
  // Not loaded yet: show nothing rather than a misleading empty state.
  if (!memory) return null;
  return (
    <section className="card wide agent-memory" aria-label={t("Agent memory")}>
      <h3>
        {t("Agent memory")} <span className="k ai">{entries.length}</span>
        <span className="sp" />
        {entries.length > 0 && (
          <button className="link subtle" onClick={() => setAll(!all)}>
            {all ? t("Key points only") : t("Include change log")}
          </button>
        )}
        {memory?.path && entries.length > 0 && (
          <button className="link subtle" onClick={() => void open(memory.path)}>
            {t("Open full memory")}
          </button>
        )}
      </h3>
      {!entries.length ? (
        <p className="faint">
          {t("Facts, insights, commitments and changes the Agent learns are logged here over time, and the next analysis builds on them.")}
          <button className="link" onClick={analyze}>
            {t("Ask the Agent")}
          </button>
        </p>
      ) : !shown.length ? (
        <p className="faint">{t("Only the change log so far. Choose “Include change log” to see it.")}</p>
      ) : (
        <ul className="memory-list">
          {shown.map((e, i) => (
            <li key={i}>
              <span className={`k ${e.kind ? memoryKinds[e.kind].tone : ""}`}>{e.kind ? memoryKinds[e.kind].label : e.label}</span>
              <span className="prewrap grow">{e.text}</span>
              <span className="faint small">{e.at?.slice(5)}</span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

// Closing sections the analysis prompt asks for (gc-service terminal.rs). Analyses saved by
// older builds use the Chinese headings.
const evidenceHeadings = ["Evidence", "Sources", "Basis", "依据"];
const openHeadings = ["To confirm", "Open questions", "Unconfirmed", "待确认"];

/** Bullet count under the first "## <title>" heading of the analysis matching one of `titles`. */
function bullets(text: string, titles: string[]) {
  const heading = text.match(new RegExp(`^## (${titles.join("|")})[^\\S\\n]*$`, "im"));
  if (!heading || heading.index === undefined) return 0;
  const rest = text.slice(heading.index + heading[0].length);
  const end = rest.search(/\n## /);
  return (end < 0 ? rest : rest.slice(0, end)).split("\n").filter((l) => /^\s*[-*] /.test(l)).length;
}

/** The Agent's latest saved analysis of this person (dossier AI analysis section), rendered. */
function AgentAnalysis({
  load,
  version,
  analyze,
  openSource,
  showSession,
}: {
  load: () => Promise<V>;
  version: unknown;
  analyze: () => void;
  openSource: (href: string) => void;
  showSession: (session: string) => void;
}) {
  const [analysis, setAnalysis] = useState<V | null>(null);
  const [expanded, setExpanded] = useState(false);
  useEffect(() => {
    let alive = true;
    load()
      .then((a) => alive && setAnalysis(a))
      .catch(() => alive && setAnalysis(null));
    return () => {
      alive = false;
    };
    // Reload whenever the contact detail refreshes (workspace changes, including Agent writes).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [version]);
  const text = String(analysis?.text || "");
  if (!text) return null;
  const long = text.split("\n").length > 14;
  return (
    <section className="card wide agent-analysis" aria-label={t("Agent analysis")}>
      <h3>
        {t("Agent analysis")} <span className="k ai">{t("AI inferred")}</span>
        <span className="faint small">
          {analysis?.at ? relative(new Date(analysis.at).toISOString()) : ""}
          {" · "}
          {t("Evidence: {n} · Open questions: {m}", { n: bullets(text, evidenceHeadings), m: bullets(text, openHeadings) })}
        </span>
        <span className="sp" />
        {analysis?.session && (
          <button className="link subtle" onClick={() => showSession(analysis.session)}>
            {t("View session")}
          </button>
        )}
        <button className="link subtle" onClick={analyze}>
          {t("Ask again")}
        </button>
      </h3>
      <div className={`markdown analysis-body ${long && !expanded ? "clamped" : ""}`}>
        <Markdown
          remarkPlugins={[remarkGfm]}
          components={{
            a: ({ href, children }) => (
              <a
                href={href}
                onClick={(e) => {
                  e.preventDefault();
                  if (href) openSource(href);
                }}
              >
                {children}
              </a>
            ),
          }}
        >
          {text}
        </Markdown>
      </div>
      {long && (
        <button className="link subtle" onClick={() => setExpanded(!expanded)}>
          {expanded ? t("Collapse") : t("Show all")}
        </button>
      )}
    </section>
  );
}

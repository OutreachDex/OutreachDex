import { useState, type ReactNode } from "react";
import { Avatar, Empty, Icon, Tag, authors, channelNames, statuses, statusTone } from "../components/ui";
import { awaitsReply, cadenceDue, draftStale, isDue, relative, type Row, type V } from "../app/domain";
import { t } from "../app/i18n";

const outcomeTags: Record<string, [string, string]> = {
  done: [t("Done"), "green"],
  needs_input: [t("Awaiting your reply"), "amber"],
  blocked: [t("Blocked"), "red"],
};

export function TodayPage({
  agentRecent,
  openAgentSession,
  rows,
  drafts,
  stale,
  followupDays,
  mailboxes,
  sources,
  openSources,
  agents,
  root,
  openSettings,
  importContacts,
  open,
  openDraft,
  openPeople,
  askAgent,
  connectMail,
}: {
  rows: Row[];
  drafts: V[];
  stale: (r: Row) => boolean;
  followupDays: number;
  mailboxes: number;
  sources: V[];
  openSources: () => void;
  agents: V;
  root: string;
  openSettings: () => void;
  importContacts: () => void;
  open: (slug: string, tab?: string) => void;
  openDraft: (draft: V) => void;
  openPeople: (view: "reply" | "followup") => void;
  askAgent: (task: "sync") => void;
  agentRecent: V[];
  openAgentSession: (session: string, contact?: string) => void;
  connectMail: () => void;
}) {
  const bySlug = new Map(rows.map((r) => [r.values.Slug, r]));
  const name = (slug: string) => bySlug.get(slug)?.values.Name || slug || t("Workspace");
  const reply = rows.filter(awaitsReply);
  const followups = rows.filter(stale);
  const review = drafts.filter((d) => ["draft", "approved", "uncertain"].includes(d.metadata.status));
  const total = reply.length + review.length + followups.length;
  const hour = new Date().getHours();
  const greeting = hour < 6 ? t("Working late") : hour < 12 ? t("Good morning") : hour < 18 ? t("Good afternoon") : t("Good evening");

  const onboarding = (
    <Onboarding
      root={root}
      steps={[
        { done: rows.length > 0, label: t("Add contacts"), hint: t("Create them or import a list from CSV / a table"), action: t("Import"), run: importContacts },
        { done: mailboxes > 0, label: t("Connect mailbox"), hint: t("Incoming mail is matched to contacts, so “Awaiting your reply” stays accurate"), action: t("Open settings"), run: openSettings },
        { done: sources.length > 0, label: t("Connect data source"), hint: t("Bring in users and events from your backend (GCSP)"), action: t("Connect"), run: openSources },
        {
          done: !!(agents.claude?.available || agents.codex?.available),
          label: t("Set up an AI assistant"),
          hint: t("Install and sign in to Claude Code or Codex in a terminal"),
          action: t("View"),
          run: openSettings,
        },
      ]}
    />
  );
  if (!rows.length)
    return (
      <div className="today">
        {onboarding}
        <Empty title={t("Start with your first contact")} icon="people">
          {t("Create contacts or organize a list in the table. This page gathers what to reply to, review and follow up on each day.")}
        </Empty>
      </div>
    );

  return (
    <div className="today">
      <h1>
        {total ? t("{greeting} — {n} things need your attention", { greeting, n: total }) : t("{greeting} — nothing needs your attention right now", { greeting })}
      </h1>
      <p className="muted">{t("Sorted by priority. Every send needs your approval and confirmation.")}</p>
      {onboarding}
      <div className="stats">
        <Stat label={t("Awaiting reply")} value={reply.length} onClick={() => openPeople("reply")} />
        <Stat label={t("Drafts to review")} value={review.length} onClick={() => document.getElementById("today-review")?.scrollIntoView({ behavior: "smooth", block: "start" })} />
        <Stat label={t("Follow up due")} value={followups.length} hint={t("Scheduled follow-up date reached, or no reply {n} days after sending", { n: followupDays })} onClick={() => openPeople("followup")} />
      </div>
      <div className="hint-card">
        <Icon name="terminal" />
        <span className="grow">
          <b>{t("Let the Agent check what's new")}</b>
          <small>{t("Syncs connected mailboxes and data sources, then summarizes who has new messages or activity by urgency. Summary only — no drafting, no sending.")}</small>
        </span>
        <button className="btn" onClick={() => askAgent("sync")}>
          {t("Sync and summarize")}
        </button>
      </div>

      {agentRecent.length > 0 && (
        <Section title={t("Recent Agent results")} count={agentRecent.length}>
          {agentRecent.map((a) => (
            <Item
              key={a.id}
              avatar={a.contact ? name(a.contact) : "Agent"}
              who={a.contact ? name(a.contact) : t("Workspace")}
              why={a.summary}
              tag={<Tag tone={outcomeTags[a.status]?.[1] || "gray"}>{outcomeTags[a.status]?.[0] || a.status}</Tag>}
              when={relative(new Date(a.at).toISOString())}
              onClick={() => openAgentSession(a.session, a.contact || undefined)}
            />
          ))}
        </Section>
      )}

      <Section title={t("Awaiting your reply")} count={reply.length} empty={t("No one is waiting for your reply.")}>
        {reply.slice(0, 6).map((r) => (
          <Item
            key={r.id}
            avatar={r.values.Name}
            who={r.values.Name}
            sub={r.values.Affiliation}
            why={r.values["Next action"] || t("They replied — review the activity and prepare a response.")}
            tag={<Tag tone="green">{t("Replied")}</Tag>}
            when={relative(r.values["Last reply"])}
            onClick={() => open(r.values.Slug, "timeline")}
          />
        ))}
      </Section>

      <Section id="today-review" title={t("Drafts to review")} count={review.length} empty={t("No drafts to review.")}>
        {review.slice(0, 8).map((d) => (
          <Item
            key={d.path}
            avatar={name(d.metadata.contact_id)}
            who={name(d.metadata.contact_id)}
            why={`${d.metadata.subject || t("Message draft")} · ${channelNames[d.metadata.channel] || d.metadata.channel}`}
            tag={
              draftStale(d, bySlug.get(d.metadata.contact_id)?.values["Last reply"]) ? (
                <Tag tone="red" title={t("They wrote again after this draft was created")}>{t("May be stale")}</Tag>
              ) : (
                <Tag tone={statusTone[d.metadata.status]}>{statuses[d.metadata.status] || d.metadata.status}</Tag>
              )
            }
            when={authors[d.metadata.created_by] || d.metadata.created_by || ""}
            onClick={() => openDraft(d)}
          />
        ))}
      </Section>

      <Section title={t("Follow-ups due")} count={followups.length} empty={t("No follow-ups are due.")}>
        {followups.slice(0, 6).map((r) => (
          <Item
            key={r.id}
            avatar={r.values.Name}
            who={r.values.Name}
            sub={r.values.Affiliation}
            why={
              r.values["Next action"] ||
              (isDue(r) ? t("The scheduled follow-up date has arrived.") : cadenceDue(r) ? t("You planned to get in touch every {n} days — it's time.", { n: r.values["Keep in touch"] }) : t("No reply since you sent — consider following up from a different angle."))
            }
            tag={
              <Tag tone="amber">
                {isDue(r) ? t("Follow up {date}", { date: String(r.values["Follow up"]).slice(5, 10) }) : cadenceDue(r) ? t("Keep in touch") : t("Follow up due")}
              </Tag>
            }
            when={relative(r.values["Last contacted"])}
            onClick={() => open(r.values.Slug, "overview")}
          />
        ))}
      </Section>

      {sources.some((s) => s.counts?.pending || s.last_run?.status === "failed") && (
        <Section title={t("Needs your decision")} count={sources.reduce((n, s) => n + (s.counts?.pending || 0) + (s.last_run?.status === "failed" ? 1 : 0), 0)}>
          {sources
            .filter((s) => s.counts?.pending)
            .map((s) => (
              <Item
                key={`p-${s.id}`}
                avatar={s.name}
                who={s.name}
                why={t("{n} users share an email with a contact but are unverified — confirm whether they're the same person", { n: s.counts.pending })}
                tag={<Tag tone="amber">{t("Match to confirm")}</Tag>}
                when=""
                onClick={openSources}
              />
            ))}
          {sources
            .filter((s) => s.last_run?.status === "failed")
            .map((s) => (
              <Item
                key={`f-${s.id}`}
                avatar={s.name}
                who={s.name}
                why={s.last_run.error}
                tag={<Tag tone="red">{t("Sync failed")}</Tag>}
                when={relative(s.last_run.started_at)}
                onClick={openSources}
              />
            ))}
        </Section>
      )}

      {!mailboxes && (
        <div className="hint-card">
          <Icon name="mail" />
          <span className="grow">
            <b>{t("Connect mailbox")}</b>
            <small>{t("Incoming mail is matched to contacts automatically, so “Awaiting your reply” stays accurate.")}</small>
          </span>
          <button className="btn" onClick={connectMail}>
            {t("Open settings")}
          </button>
        </div>
      )}

    </div>
  );
}

type Step = { done: boolean; label: string; hint: string; action: string; run: () => void };

/** First-run checklist; hides once everything is done or the person dismisses it. */
function Onboarding({ root, steps }: { root: string; steps: Step[] }) {
  const key = `gc.onboarding:${root}`;
  const [hidden, setHidden] = useState(() => localStorage.getItem(key) === "hidden");
  const done = steps.filter((s) => s.done).length;
  if (hidden || done === steps.length) return null;
  return (
    <section className="onboarding" aria-label={t("Get started")}>
      <div className="onboarding-head">
        <b>{t("Get started")}</b>
        <span className="faint small">
          {done} / {steps.length}
        </span>
        <span className="sp" />
        <button
          className="link subtle"
          onClick={() => {
            localStorage.setItem(key, "hidden");
            setHidden(true);
          }}
        >
          {t("Don't show again")}
        </button>
      </div>
      {steps.map((s) => (
        <div key={s.label} className={`step ${s.done ? "done" : ""}`}>
          <span className="step-mark">{s.done && <Icon name="check" size={11} />}</span>
          <span className="grow">
            {s.label}
            <small className="faint"> · {s.hint}</small>
          </span>
          {!s.done && (
            <button className="btn ghost" onClick={s.run}>
              {s.action}
            </button>
          )}
        </div>
      ))}
    </section>
  );
}

function Stat({ label, value, hint, onClick }: { label: string; value: number; hint?: string; onClick?: () => void }) {
  return (
    <button className="stat" disabled={!onClick} onClick={onClick} title={hint}>
      <span className="l">{label}</span>
      <span className="n">{value}</span>
    </button>
  );
}

function Section({
  id,
  title,
  count,
  empty,
  action,
  children,
}: {
  id?: string;
  title: string;
  count: number;
  empty?: string;
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="today-section" id={id}>
      <div className="section-h">
        {title} <span className="c">{count}</span>
        <span className="sp" />
        {action}
      </div>
      {count ? <div className="list">{children}</div> : <p className="faint pad">{empty}</p>}
    </section>
  );
}

function Item({
  avatar,
  who,
  sub,
  why,
  tag,
  when,
  onClick,
}: {
  avatar: string;
  who: string;
  sub?: string;
  why: string;
  tag: ReactNode;
  when: string;
  onClick: () => void;
}) {
  return (
    <button className="item" onClick={onClick}>
      <Avatar name={avatar || "?"} />
      <span className="who">
        {who}
        {sub && <small>{sub}</small>}
      </span>
      <span className="why">{why}</span>
      {tag}
      <span className="when">{when}</span>
    </button>
  );
}

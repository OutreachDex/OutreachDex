import { useEffect, useState } from "react";
import { confirm } from "@tauri-apps/plugin-dialog";
import { Avatar, Empty, Icon, Modal, Tag } from "../components/ui";
import { matches as rowMatches, relative, type Row, type V } from "../app/domain";
import { t } from "../app/i18n";

type Call = <T>(action: string, args?: V) => Promise<T>;
type Work = (label: string, fn: () => Promise<unknown>) => Promise<void>;

const levelTone: Record<string, string> = { ok: "green", warn: "amber", fail: "red" };
const levelMark: Record<string, string> = { ok: t("Pass"), warn: t("Warning"), fail: t("Failed") };

export function SourcesPage({
  sources,
  rows,
  busy,
  call,
  work,
  refresh,
  openPerson,
  openUrl,
  notify,
  askAgent,
  openDocument,
}: {
  askAgent: (source: string, externalId: string, name: string) => void;
  openDocument: (path: string) => void;
  sources: V[];
  rows: Row[];
  busy: string;
  call: Call;
  work: Work;
  refresh: () => void;
  openPerson: (slug: string) => void;
  openUrl: (url: string) => void;
  notify: (message: string) => void;
}) {
  const [connecting, setConnecting] = useState(false);
  const [selected, setSelected] = useState("");
  const current = sources.find((s) => s.id === selected) || sources[0];
  const [tab, setTab] = useState<"pending" | "unlinked" | "linked">("pending");
  const [people, setPeople] = useState<V[]>([]);
  const [query, setQuery] = useState("");
  const [version, setVersion] = useState(0);
  const [viewing, setViewing] = useState<V | null>(null);
  useEffect(() => {
    if (!current) return;
    let alive = true;
    void call<V[]>("source.people", { source: current.key })
      .then((items) => alive && setPeople(items))
      .catch(() => alive && setPeople([]));
    return () => {
      alive = false;
    };
  }, [current?.key, current?.last_run?.finished_at, version, call]);
  const name = (slug: string) => rows.find((r) => r.values.Slug === slug)?.values.Name || slug;
  const byTab = {
    pending: people.filter((p) => p.match === "pending"),
    unlinked: people.filter((p) => p.match === "unlinked"),
    linked: people.filter((p) => p.match === "auto" || p.match === "confirmed"),
  };
  const shown = byTab[tab].filter(
    (p) => !query || [p.name, p.external_id, ...(p.emails || []).map((e: [string, boolean]) => e[0])].join(" ").toLowerCase().includes(query.toLowerCase()),
  );
  const labels: Record<string, string> = Object.fromEntries(
    (current?.manifest?.attributes || []).map((a: V) => [a.key, a.label || a.key]),
  );
  const decide = (person: V, action: string, slug?: string) =>
    work(t("Working…"), async () => {
      const result = await call<V>("identity.decide", { source: person.source, external_id: person.external_id, action, slug });
      if (action === "reject") notify(t("Marked as a different person; future syncs won’t suggest this again"));
      else notify(t("Linked to {name}", { name: name(result.contact) || t("new contact") }) + (result.events ? t(", added {n} activity records", { n: result.events }) : ""));
      setVersion((v) => v + 1);
      refresh();
    });
  const sync = (source: V) =>
    work(t("Syncing {name}…", { name: source.name }), async () => {
      const r = await call<V>("source.sync", { id: source.id });
      notify(t("Sync complete: {people} people, {events} events; {linked} linked, {pending} to confirm", { people: r.people, events: r.events, linked: r.linked, pending: r.pending }) + (r.skipped ? t(", {n} invalid records skipped", { n: r.skipped }) : ""));
      refresh();
    });

  if (!sources.length)
    return (
      <div className="sources">
        <Empty title={t("Connect your first data source")} icon="link">
          {t("Any backend that implements the three read-only GCSP/1 endpoints can hand its users and their feedback, purchase and usage events to OutreachDex for analysis and follow-up.")}
          <div className="row-gap">
            <button className="btn primary" onClick={() => setConnecting(true)}>
              <Icon name="plus" />
              {t("Connect source")}
            </button>
          </div>
          <small className="faint">{t("See docs/gcsp.md for the spec; to try it locally, run node scripts/gcsp-mock.mjs.")}</small>
        </Empty>
        {connecting && (
          <ConnectDialog
            call={call}
            work={work}
            busy={busy}
            close={() => setConnecting(false)}
            done={(message) => {
              setConnecting(false);
              notify(t("Connected {name}", { name: message }));
              refresh();
            }}
          />
        )}
      </div>
    );

  return (
    <div className="sources">
      <div className="source-cards">
        {sources.map((s) => {
          const run = s.last_run;
          return (
            <button key={s.id} className={`source-card ${current?.id === s.id ? "on" : ""}`} onClick={() => setSelected(s.id)}>
              <span className="source-top">
                <span className={`dot ${!run ? "" : run.status === "failed" ? "err" : run.status === "running" ? "warn" : "ok"}`} />
                <b>{s.name}</b>
                <span className="faint small">{s.key}</span>
              </span>
              <span className="source-stats">
                <span>
                  <b>{s.counts.linked}</b> {t("Linked")}
                </span>
                <span className={s.counts.pending ? "hot" : ""}>
                  <b>{s.counts.pending}</b> {t("To confirm")}
                </span>
                <span>
                  <b>{s.counts.unlinked}</b> {t("Unlinked")}
                </span>
              </span>
              <span className="faint small">
                {!run
                  ? t("Not synced yet")
                  : run.status === "failed"
                    ? t("Sync failed: {error}", { error: run.error })
                    : run.status === "running"
                      ? t("Syncing…")
                      : t("Synced {when} · {people} people · {events} events", { when: relative(run.finished_at || run.started_at), people: run.people, events: run.events })}
              </span>
            </button>
          );
        })}
        <button className="source-card add" onClick={() => setConnecting(true)}>
          <Icon name="plus" />
          {t("Connect source")}
        </button>
      </div>

      {current && (
        <section className="source-detail">
          <div className="panel-bar">
            <div className="seg">
              {(
                [
                  ["pending", t("To confirm"), byTab.pending.length],
                  ["unlinked", t("Unlinked"), byTab.unlinked.length],
                  ["linked", t("Linked"), byTab.linked.length],
                ] as const
              ).map(([key, label, n]) => (
                <button key={key} className={tab === key ? "on" : ""} onClick={() => setTab(key)}>
                  {label} <span className="count">{n}</span>
                </button>
              ))}
            </div>
            <div className="search-inline">
              <Icon name="search" size={14} />
              <input aria-label={t("Search people in this source")} placeholder={t("Search name, email…")} value={query} onChange={(e) => setQuery(e.target.value)} />
            </div>
            <span className="sp" />
            <button className="btn" disabled={!!busy} onClick={() => void sync(current)}>
              <Icon name="refresh" />
              {t("Sync now")}
            </button>
            <details className="more">
              <summary className="btn ghost" aria-label={t("More actions")}>
                <Icon name="more" />
              </summary>
              <div className="menu right">
                <div className="menu-note">{current.url} · {t("syncs every {n} minutes", { n: current.interval })}</div>
                <label className="menu-field" title={t("Language of contacts created from this source; the Agent drafts in the contact's language")}>
                  {t("Language for new contacts")}
                  <select
                    aria-label={t("Language for new contacts")}
                    // Older workspaces store Chinese as "中文"; the backend accepts both.
                    value={current.language_setting === "中文" ? "Chinese" : current.language_setting || "auto"}
                    disabled={!!busy}
                    onChange={(e) =>
                      void work(t("Saving…"), async () => {
                        await call("source.language", { source: current.key, language: e.target.value });
                        refresh();
                      })
                    }
                  >
                    <option value="auto">
                      {current.manifest?.source?.default_locale
                        ? t("Automatic (source says: {lang})", { lang: current.language || "" })
                        : t("Automatic (workspace default)")}
                    </option>
                    <option value="English">English</option>
                    <option value="Chinese">{t("Chinese")}</option>
                  </select>
                </label>
                <button
                  onClick={() =>
                    void work(t("Resetting…"), async () => {
                      await call("source.reset", { id: current.id });
                      notify(t("The next sync will start from the beginning; existing link decisions are kept"));
                    })
                  }
                >
                  <Icon name="refresh" />
                  {t("Resync from the beginning")}
                </button>
                <button
                  className="danger"
                  onClick={() =>
                    void work(t("Deleting…"), async () => {
                      if (!(await confirm(t("Disconnect {name}? Synced identities and evidence files in the workspace are kept.", { name: current.name })))) return;
                      await call("source.delete", { id: current.id });
                      setSelected("");
                      refresh();
                    })
                  }
                >
                  <Icon name="close" />
                  {t("Disconnect source")}
                </button>
              </div>
            </details>
          </div>
          {tab === "pending" && byTab.pending.length > 0 && (
            <p className="muted small pad">{t("These people share an email with a contact, but the source hasn’t verified it. Confirming merges their activity; choosing “Not them” stops future suggestions.")}</p>
          )}
          {shown.length === 0 ? (
            <Empty title={tab === "pending" ? t("No matches to confirm") : tab === "unlinked" ? t("No unlinked people") : t("Nobody linked yet")} icon="people">
              {tab === "linked" ? t("People are linked automatically when a verified email matches a contact.") : null}
            </Empty>
          ) : (
            <div className="list">
              {shown.slice(0, 200).map((p) => (
                <div className="identity" key={`${p.source}:${p.external_id}`}>
                  <Avatar name={p.name || p.external_id} />
                  <button
                    className="identity-main identity-open"
                    title={t("See this person's data and events from the source")}
                    onClick={() => setViewing(p)}
                  >
                    <b>{p.name || p.external_id}</b>
                    <small>
                      {(p.emails || []).map((e: [string, boolean]) => `${e[0]}${e[1] ? " ✓" : ""}`).join(t(", ")) || p.external_id}
                    </small>
                    <span className="identity-facts">
                      {Object.entries(p.attributes || {})
                        .filter(([, v]) => v !== null && v !== "")
                        .slice(0, 4)
                        .map(([k, v]) => (
                          <span key={k}>
                            {labels[k] || k}{t(": ")}{String(v)}
                          </span>
                        ))}
                    </span>
                  </button>
                  {tab === "pending" && (
                    <>
                      <span className="identity-candidate">
                        {t("Might be")} <button className="link" onClick={() => openPerson(p.contact)}>{name(p.contact)}</button>
                      </span>
                      <button className="btn" disabled={!!busy} onClick={() => void decide(p, "reject")}>
                        {t("Not them")}
                      </button>
                      <button className="btn primary" disabled={!!busy} onClick={() => void decide(p, "confirm")}>
                        {t("Same person")}
                      </button>
                    </>
                  )}
                  {tab === "unlinked" && (
                    <>
                      <LinkPicker rows={rows} disabled={!!busy} pick={(slug) => void decide(p, "link", slug)} />
                      <button className="btn" disabled={!!busy} onClick={() => void decide(p, "create")}>
                        {t("Create contact")}
                      </button>
                    </>
                  )}
                  {tab === "linked" && (
                    <>
                      <Tag tone={p.match === "auto" ? "green" : "violet"}>{p.match === "auto" ? t("Auto-linked") : t("Confirmed")}</Tag>
                      <button className="link" onClick={() => openPerson(p.contact)}>
                        {name(p.contact)}
                      </button>
                    </>
                  )}
                  {p.url && (
                    <button className="icon-btn sm" title={t("View in backend: {url}", { url: p.url })} aria-label={t("View in backend")} onClick={() => openUrl(p.url)}>
                      <Icon name="link" size={13} />
                    </button>
                  )}
                </div>
              ))}
              {shown.length > 200 && <p className="faint pad">{t("Showing the first 200 only; use search to narrow it down.")}</p>}
            </div>
          )}
        </section>
      )}
      {viewing && (
        <SourcePerson
          person={viewing}
          call={call}
          busy={busy}
          close={() => setViewing(null)}
          openDocument={(path) => {
            setViewing(null);
            openDocument(path);
          }}
          openPerson={(slug) => {
            setViewing(null);
            openPerson(slug);
          }}
          create={() => {
            setViewing(null);
            void decide(viewing, "create");
          }}
          askAgent={() => {
            setViewing(null);
            askAgent(viewing.source, viewing.external_id, viewing.name || viewing.external_id);
          }}
        />
      )}
      {connecting && (
        <ConnectDialog
          call={call}
          work={work}
          busy={busy}
          close={() => setConnecting(false)}
          done={(message) => {
            setConnecting(false);
            notify(t("Connected {name}", { name: message }));
            refresh();
          }}
        />
      )}
    </div>
  );
}

function LinkPicker({ rows, disabled, pick }: { rows: Row[]; disabled: boolean; pick: (slug: string) => void }) {
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState(false);
  const found = rows.filter((r) => rowMatches(r, query)).slice(0, 8);
  return (
    <span className="link-picker">
      <button className="btn ghost" disabled={disabled} onClick={() => setOpen(!open)}>
        {t("Link to…")}
      </button>
      {open && (
        <span className="menu right">
          <input autoFocus placeholder={t("Search contacts")} value={query} onChange={(e) => setQuery(e.target.value)} onKeyDown={(e) => e.key === "Escape" && setOpen(false)} />
          {found.map((r) => (
            <button
              key={r.id}
              onClick={() => {
                setOpen(false);
                pick(r.values.Slug);
              }}
            >
              <Avatar name={r.values.Name || "?"} />
              <span className="grow">
                {r.values.Name}
                <small className="faint"> {r.values.Email}</small>
              </span>
            </button>
          ))}
          {!found.length && <span className="faint small pad">{t("No matching contacts")}</span>}
        </span>
      )}
    </span>
  );
}

function ConnectDialog({
  call,
  work,
  busy,
  close,
  done,
}: {
  call: Call;
  work: Work;
  busy: string;
  close: () => void;
  done: (message: string) => void;
}) {
  const [url, setUrl] = useState("");
  const [token, setToken] = useState("");
  const [header, setHeader] = useState("Authorization");
  const [report, setReport] = useState<V | null>(null);
  const check = () =>
    work(t("Checking source…"), async () => {
      setReport(null);
      setReport(await call<V>("source.check", { url, token, header }));
    });
  // Connect and run the first sync as one operation, so the source never sits unsynced.
  const save = () =>
    work(t("Connecting and running the first sync…"), async () => {
      const added = await call<V>("source.add", { url, token, header });
      try {
        const r = await call<V>("source.sync", { id: added.id });
        done(t("{name}: {people} people, {events} events, {linked} linked, {pending} to confirm", { name: added.name, people: r.people, events: r.events, linked: r.linked, pending: r.pending }));
      } catch (e) {
        done(t("{name} (first sync failed: {error}. You can retry later with “Sync now”.)", { name: added.name, error: String(e) }));
      }
    });
  const failed = report && !report.passed;
  return (
    <Modal
      title={t("Connect data source")}
      wide
      close={busy ? undefined : close}
      protectChanges
      footer={
        <div className="row-gap end grow">
          <span className="faint small grow">{t("The token is stored encrypted on this device; OutreachDex only calls read-only endpoints.")}</span>
          <button className="btn" disabled={!!busy || !url.trim()} onClick={() => void check()}>
            {report ? t("Check again") : t("Check")}
          </button>
          <button className="btn primary" disabled={!!busy || !report || !!failed} onClick={() => void save()}>
            {t("Connect and sync")}
          </button>
        </div>
      }
    >
      <div className="form-grid">
        <label className="span">
          {t("Source URL")}
          <input value={url} placeholder="https://admin.example.com/gcsp" onChange={(e) => (setUrl(e.target.value), setReport(null))} />
        </label>
        <label>
          Token
          <input type="password" value={token} placeholder={t("Read-only token issued by your backend")} onChange={(e) => (setToken(e.target.value), setReport(null))} />
        </label>
        <label>
          {t("Header")}
          <input value={header} onChange={(e) => (setHeader(e.target.value), setReport(null))} />
        </label>
      </div>
      {report && (
        <div className="check-report">
          <div className={`check-summary ${report.passed ? "ok" : "fail"}`}>
            <Icon name={report.passed ? "check" : "close"} />
            {report.passed
              ? t("{name} conforms to GCSP/1 and can be connected", { name: report.manifest?.source?.name }) + (report.findings.some((f: V) => f.level === "warn") ? t(" (with warnings; fixing the backend is recommended)") : "")
              : t("Check failed; fix the backend API first")}
          </div>
          {report.findings.map((f: V, i: number) => (
            <div className="check-row" key={i}>
              <Tag tone={levelTone[f.level]}>{levelMark[f.level]}</Tag>
              <b>{f.name}</b>
              <span className="muted">{f.detail}</span>
            </div>
          ))}
          {report.sample?.length > 0 && (
            <>
              <div className="section-h">{t("Data preview")}</div>
              <div className="list">
                {report.sample.map((p: V) => (
                  <div className="identity" key={p.external_id}>
                    <Avatar name={p.display_name || p.external_id} />
                    <span className="identity-main">
                      <b>{p.display_name || p.external_id}</b>
                      <small>{(p.identities || []).map((i: V) => i.value).join(t(", "))}</small>
                    </span>
                  </div>
                ))}
              </div>
            </>
          )}
        </div>
      )}
      {!report && (
        <p className="muted small">
          {t("First check that the API conforms to GCSP/1: manifest, authentication, pagination and field formats. See docs/gcsp.md for the spec.")}
        </p>
      )}
    </Modal>
  );
}

/** One person from a data source, linked or not: facts, how they changed, and their events. */
function SourcePerson({
  person,
  call,
  busy,
  close,
  openDocument,
  openPerson,
  create,
  askAgent,
}: {
  person: V;
  call: Call;
  busy: string;
  close: () => void;
  openDocument: (path: string) => void;
  openPerson: (slug: string) => void;
  create: () => void;
  askAgent: () => void;
}) {
  const [detail, setDetail] = useState<V | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    let alive = true;
    call<V>("source.identity", { source: person.source, external_id: person.external_id })
      .then((d) => alive && setDetail(d))
      .catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
  }, [person.source, person.external_id, call]);
  const linked = person.match === "auto" || person.match === "confirmed";
  const history = ((detail?.history || []) as V[]).filter((h) => h.key).slice(-10).reverse();
  return (
    <Modal
      title={person.name || person.external_id}
      close={close}
      wide
      footer={
        <>
          <span className="faint small grow">
            {detail?.source_name || person.source} · {person.external_id}
          </span>
          {linked ? (
            <button className="btn" onClick={() => openPerson(person.contact)}>
              {t("Open contact")}
            </button>
          ) : (
            <button className="btn" disabled={!!busy} onClick={create}>
              {t("Create contact")}
            </button>
          )}
          <button className="btn primary" disabled={!!busy} onClick={askAgent} title={t("Ask the Agent on the right about this user; no need to make them a contact first")}>
            <Icon name="spark" />
            {t("Ask Agent")}
          </button>
        </>
      }
    >
      {error ? (
        <p className="inline-error">{t(error)}</p>
      ) : !detail ? (
        <p className="faint">{t("Loading…")}</p>
      ) : (
        <div className="source-person">
          <p className="muted small">{(detail.emails || []).map((e: [string, boolean]) => `${e[0]}${e[1] ? " ✓" : ""}`).join(t(", "))}</p>
          <section>
            <h3>{t("Source facts")}</h3>
            {(detail.facts || []).length ? (
              <dl className="facts">
                {detail.facts.map((x: V) => (
                  <div key={x.key}>
                    <dt>{x.label}</dt>
                    <dd>{x.type === "datetime" ? relative(String(x.value)) || String(x.value) : String(x.value)}</dd>
                  </div>
                ))}
              </dl>
            ) : (
              <p className="faint">{t("The source provided no attributes.")}</p>
            )}
          </section>
          {history.length > 0 && (
            <section>
              <h3>{t("Changes")}</h3>
              <ul className="plain-list small">
                {history.map((h: V, i: number) => (
                  <li key={i}>
                    <span className="faint">{relative(h.at) || h.at}</span> {h.label || h.key}{t(": ")}{h.from} → <b>{h.to}</b>
                  </li>
                ))}
              </ul>
            </section>
          )}
          <section>
            <h3>
              {t("Events")} <span className="faint">{(detail.events || []).length}</span>
            </h3>
            {(detail.events || []).length ? (
              <ul className="source-events">
                {(detail.events as V[]).slice(0, 30).map((e) => (
                  <li key={e.path}>
                    <button className="link plain" onClick={() => openDocument(e.path)}>
                      <span className="k fact">{e.label || e.type}</span>
                      <span className="grow clip">{e.title}</span>
                      <span className="faint small">{relative(e.occurred_at) || e.occurred_at}</span>
                    </button>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="faint">{t("No events yet.")}</p>
            )}
          </section>
        </div>
      )}
    </Modal>
  );
}

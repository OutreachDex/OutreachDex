import { useEffect, useState } from "react";
import { confirm } from "@tauri-apps/plugin-dialog";
import { Icon, InstallHint, Tag } from "../components/ui";
import { setPrefs, usePrefs } from "../app/prefs";
import type { V } from "../app/domain";
import { t, lang, languages, setLang, type Lang } from "../app/i18n";
import { saveEditors } from "../kits/TableEditor";

type Call = <T>(action: string, args?: V) => Promise<T>;
type Work = (label: string, fn: () => Promise<unknown>) => Promise<void>;

export function SettingsPage({
  root,
  config,
  setConfig,
  agents,
  accounts,
  call,
  work,
  refresh,
  save,
  fail,
}: {
  root: string;
  config: V;
  setConfig: (v: V) => void;
  agents: V;
  accounts: V;
  call: Call;
  work: Work;
  refresh: () => void;
  save: () => void;
  fail: (message: string) => void;
}) {
  const prefs = usePrefs();
  return (
    <div className="settings">
      <section className="settings-section">
        <header>
          <h2>{t("Appearance")}</h2>
          <p>{t("Light follows Attio, dark follows Linear. Mix any density with any theme.")}</p>
        </header>
        <div className="setting-row">
          <span className="lab">{t("Theme")}</span>
          <div className="seg">
            {(
              [
                ["system", t("System")],
                ["light", t("Light")],
                ["dark", t("Dark")],
              ] as const
            ).map(([key, label]) => (
              <button key={key} className={prefs.theme === key ? "on" : ""} aria-pressed={prefs.theme === key} onClick={() => setPrefs({ theme: key })}>
                <Icon name={key === "dark" ? "moon" : key === "light" ? "sun" : "settings"} size={14} />
                {label}
              </button>
            ))}
          </div>
        </div>
        <div className="setting-row">
          <span className="lab">{t("Density")}</span>
          <div className="seg">
            {(
              [
                ["comfortable", t("Comfortable")],
                ["compact", t("Compact")],
              ] as const
            ).map(([key, label]) => (
              <button key={key} className={prefs.density === key ? "on" : ""} aria-pressed={prefs.density === key} onClick={() => setPrefs({ density: key })}>
                {label}
              </button>
            ))}
          </div>
        </div>
        <div className="setting-row">
          <span className="lab">{t("Language")}</span>
          <select
            aria-label={t("Interface language")}
            value={lang}
            onChange={(e) => {
              const next = e.target.value as Lang;
              void saveEditors()
                .catch(() => {})
                .then(() => setLang(next));
            }}
          >
            {languages.map(([code, name]) => (
              <option key={code} value={code}>
                {name}
              </option>
            ))}
          </select>
          <span className="muted">{t("The window reloads after switching; open documents are saved first.")}</span>
        </div>
      </section>

      <section className="settings-section">
        <header>
          <h2>{t("Assistant")}</h2>
          <p>{t("Uses the Claude Code or Codex you're signed in to. On first use, sign in from the terminal.")}</p>
        </header>
        <div className="setting-row">
          <span className="lab">{t("Default assistant")}</span>
          <select aria-label={t("Default assistant")} value={config.defaultAgent || "claude"} onChange={(e) => setConfig({ ...config, defaultAgent: e.target.value })}>
            <option value="claude">Claude Code</option>
            <option value="codex">Codex</option>
          </select>
          {["claude", "codex"].map((kind) => (
            <Tag key={kind} tone={agents[kind]?.available ? "green" : "gray"}>
              {kind === "claude" ? "Claude Code" : "Codex"} {agents[kind]?.available ? t("Detected") : t("Not found")}
            </Tag>
          ))}
        </div>
        {["claude", "codex"]
          .filter((kind) => agents[kind] && !agents[kind].available)
          .map((kind) => (
            <div className="setting-row" key={kind}>
              <InstallHint agent={kind} />
            </div>
          ))}
        <div className="setting-row">
          <span className="lab">{t("Follow-up reminder")}</span>
          <input
            className="num"
            type="number"
            min="1"
            value={config.followupDays || 7}
            onChange={(e) => setConfig({ ...config, followupDays: Math.max(1, Number(e.target.value)) })}
          />
          <span className="muted">{t("days without a reply → mark for follow-up")}</span>
        </div>
        <label className="setting-row check">
          <input type="checkbox" aria-label={t("Pause all sending")} checked={!!config.sendingPaused} onChange={(e) => setConfig({ ...config, sendingPaused: e.target.checked })} />
          {t("Pause all sending (email sends and “Mark as sent” are refused; drafts can still be edited and approved)")}
        </label>
        <details className="advanced">
          <summary>{t("Advanced · custom assistant programs")}</summary>
          {["claude", "codex"].map((kind) => (
            <div className="form-grid" key={kind}>
              <label>
                {t("{kind} interactive CLI path", { kind })}
                <input
                  value={config.terminal?.[kind] || ""}
                  placeholder={t("Auto-detect local install")}
                  onChange={(e) => setConfig({ ...config, terminal: { ...config.terminal, [kind]: e.target.value } })}
                />
              </label>
            </div>
          ))}
        </details>
        <div className="setting-actions">
          <button className="btn primary" onClick={save}>
            {t("Save settings")}
          </button>
          <span className="faint small">{t("Table AI fields use your Claude Code in a one-off session without tools.")}</span>
        </div>
      </section>

      {root && <Preferences key={`p-${root}`} call={call} work={work} />}

      {root && <MailSettings key={root} accounts={accounts} call={call} work={work} refresh={refresh} />}
    </div>
  );
}

/** preferences.md: how the owner wants messages written; every analysis and draft reads it. */
function Preferences({ call, work }: { call: Call; work: Work }) {
  const [text, setText] = useState<string | null>(null);
  const [saved, setSaved] = useState("");
  useEffect(() => {
    void call<V>("preferences.get").then((r) => {
      setText(r.text || "");
      setSaved(r.text || "");
    });
  }, [call]);
  return (
    <section className="settings-section">
      <header>
        <h2>{t("Writing preferences")}</h2>
        <p>{t("Your tone, length, forms of address, things to avoid, and so on. The assistant follows them when analyzing and drafting. Saved in the workspace's preferences.md; you can also “Remember a writing preference” while reviewing a draft.")}</p>
      </header>
      <textarea
        aria-label={t("Writing preferences content")}
        rows={8}
        value={text ?? ""}
        disabled={text === null}
        placeholder={t("- Skip the small talk; state the purpose up front\n- Keep messages under 100 words\n- No exclamation marks or buzzwords like “synergy”")}
        onChange={(e) => setText(e.target.value)}
      />
      <div className="setting-actions">
        <button
          className="btn primary"
          disabled={text === null || text === saved}
          onClick={() =>
            void work(t("Saving writing preferences…"), async () => {
              await call("preferences.save", { text });
              setSaved(text || "");
            })
          }
        >
          {t("Save preferences")}
        </button>
        {text !== null && text !== saved && <span className="faint small">{t("Unsaved changes")}</span>}
      </div>
    </section>
  );
}

const blank = (kind: string) => ({
  port: kind === "imap" ? 993 : 587,
  tls: kind === "imap" ? "tls" : "starttls",
  interval: 15,
  mailbox: "INBOX",
});

function MailSettings({ accounts, call, work, refresh }: { accounts: V; call: Call; work: Work; refresh: () => void }) {
  const [kind, setKind] = useState("imap"),
    [editing, setEditing] = useState(""),
    [values, setValues] = useState<V>(blank("imap")),
    [result, setResult] = useState("");
  const field = (key: string, label: string, type = "text") => (
    <label key={key}>
      {label}
      <input type={type} value={values[key] || ""} onChange={(e) => setValues({ ...values, [key]: e.target.value })} />
    </label>
  );
  return (
    <section className="settings-section">
      <header>
        <h2>{t("Mail accounts")}</h2>
        <p>{t("Passwords are stored encrypted in the local app data folder; each workspace is configured separately.")}</p>
      </header>
      <div className="accounts">
        {["imap", "smtp"].flatMap((k) =>
          accounts[k].map((a: V) => (
            <div className="account" key={a.id}>
              <Tag tone={k === "imap" ? "blue" : "violet"}>{k === "imap" ? t("Incoming") : t("Outgoing")}</Tag>
              <b className="grow">{a.name}</b>
              <button
                className="btn ghost"
                onClick={() => {
                  setKind(k);
                  setEditing(a.id);
                  const v = k === "imap" ? a.connection : a.settings;
                  setValues({
                    ...v,
                    tls: v.tls_mode || v.tlsMode,
                    interval: a.schedule?.interval_minutes || 15,
                    enabled: a.schedule?.enabled,
                    attachments: a.include_attachments,
                    name: a.name,
                  });
                }}
              >
                {t("Edit")}
              </button>
              <button
                className="btn ghost"
                onClick={() =>
                  void work(t("Testing connection…"), async () => {
                    const r = await call<V | null>("mail.test", { kind: k, id: a.id });
                    setResult(
                      r?.ok
                        ? `${a.name}${t(": ")}${t("connected")}${r.latencyMs ? ` (${r.latencyMs} ms)` : ""}`
                        : `${a.name}${t(": ")}${r?.message || t("connection failed; check the settings and try again")}`,
                    );
                  })
                }
              >
                {t("Test")}
              </button>
              <button
                className="btn ghost danger"
                onClick={() =>
                  void work(t("Deleting connection…"), async () => {
                    if (await confirm(t("Delete the connection settings for {name}?", { name: a.name }))) {
                      await call("mail.delete", { kind: k, id: a.id });
                      refresh();
                    }
                  })
                }
              >
                {t("Delete")}
              </button>
            </div>
          )),
        )}
        {!accounts.imap.length && !accounts.smtp.length && <p className="faint">{t("No mailbox connected yet.")}</p>}
      </div>
      <div className="setting-row">
        <span className="lab">{editing ? t("Edit connection") : t("New connection")}</span>
        <div className="seg">
          {[
            ["imap", t("IMAP · incoming")],
            ["smtp", t("SMTP · outgoing")],
          ].map(([k, label]) => (
            <button
              key={k}
              className={kind === k ? "on" : ""}
              onClick={() => {
                setKind(k);
                setEditing("");
                setValues(blank(k));
              }}
            >
              {label}
            </button>
          ))}
        </div>
        {editing && (
          <button
            className="link"
            onClick={() => {
              setEditing("");
              setValues(blank(kind));
            }}
          >
            {t("Create new instead")}
          </button>
        )}
      </div>
      <div className="form-grid">
        {field("name", t("Connection name"))}
        {field("host", t("Server"))}
        {field("port", t("Port"), "number")}
        {field("username", t("Username"))}
        {field("password", editing ? t("Password (leave blank to keep)") : t("Password / app password"), "password")}
        <label>
          {t("Encryption")}
          <select value={values.tls || "tls"} onChange={(e) => setValues({ ...values, tls: e.target.value })}>
            <option value="tls">TLS</option>
            <option value="starttls">STARTTLS</option>
            <option value="plain">{t("Plain (local testing)")}</option>
          </select>
        </label>
        {field("proxy", t("Proxy URL (optional)"))}
        {kind === "imap" ? (
          <>
            {field("mailbox", t("Folder"))}
            {field("interval", t("Sync interval (minutes)"), "number")}
            <label className="check">
              <input type="checkbox" checked={!!values.enabled} onChange={(e) => setValues({ ...values, enabled: e.target.checked })} />
              {t("Sync automatically")}
            </label>
            <label className="check">
              <input type="checkbox" checked={!!values.attachments} onChange={(e) => setValues({ ...values, attachments: e.target.checked })} />
              {t("Download attachments")}
            </label>
          </>
        ) : (
          <>
            {field("from", t("From address"))}
            {field("fromName", t("Sender name"))}
          </>
        )}
      </div>
      <div className="setting-actions">
        <button
          className="btn primary"
          onClick={() =>
            void work(t("Saving and verifying mailbox…"), async () => {
              const input =
                kind === "imap"
                  ? {
                      name: values.name,
                      connection: {
                        host: values.host,
                        port: Number(values.port),
                        tls_mode: values.tls,
                        username: values.username,
                        mailbox: values.mailbox || "INBOX",
                        proxy: values.proxy || "",
                      },
                      ...(values.password ? { password: values.password } : {}),
                      schedule: { enabled: !!values.enabled, interval_minutes: Number(values.interval || 15) },
                      include_attachments: !!values.attachments,
                    }
                  : {
                      name: values.name,
                      password: values.password,
                      settings: {
                        host: values.host,
                        port: Number(values.port),
                        username: values.username,
                        tlsMode: values.tls,
                        from: values.from,
                        fromName: values.fromName || "",
                        proxy: values.proxy || "",
                      },
                    };
              await call("mail.save", { kind, ...(editing ? { id: editing } : {}), input });
              setValues({ ...values, password: "" });
              setResult(t("Connection saved"));
              refresh();
            })
          }
        >
          {editing ? t("Update connection") : t("Save connection")}
        </button>
        {result && <span className="muted prewrap">{result}</span>}
      </div>
    </section>
  );
}

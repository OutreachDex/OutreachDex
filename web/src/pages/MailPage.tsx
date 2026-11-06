import { Empty, Icon, Tag } from "../components/ui";
import { TableEditor } from "../kits/TableEditor";
import { relative, type Row, type V } from "../app/domain";
import { t } from "../app/i18n";

export function MailPage({
  root,
  accounts,
  mail,
  rows,
  notePath,
  busy,
  sync,
  read,
  assign,
  closeNote,
  showNote,
  connect,
  fail,
}: {
  root: string;
  accounts: V;
  mail: V[];
  rows: Row[];
  notePath: string;
  busy: boolean;
  sync: (id: string) => void;
  read: (path: string) => void;
  assign: (slug: string, email: string) => void;
  closeNote: () => void;
  showNote: (path: string) => Promise<unknown>;
  connect: () => void;
  fail: (message: string) => void;
}) {
  return (
    <div className={`mail-layout ${notePath ? "reading" : ""}`}>
      <div className="mail-list">
        <div className="mailboxes">
          {accounts.imap.length === 0 ? (
            <Empty title={t("Connect your mailbox")} icon="mail">
              {t("Synced mail is matched to contacts by address, so every conversation is in one place.")}
              <div>
                <button className="btn primary" onClick={connect}>
                  {t("Set up mail connection")}
                </button>
              </div>
            </Empty>
          ) : (
            accounts.imap.map((a: V) => (
              <div className="mailbox" key={a.id}>
                <span className={`dot ${a.last_error ? "err" : "ok"}`} />
                <span className="grow">
                  <b>{a.name}</b>
                  <small>
                    {a.connection.host} · {t("{n} messages", { n: a.synced_messages })}{a.last_error ? ` · ${a.last_error}` : ""}
                  </small>
                </span>
                <button className="btn" disabled={busy} onClick={() => sync(a.id)}>
                  <Icon name="refresh" />
                  {t("Sync now")}
                </button>
              </div>
            ))
          )}
        </div>
        {mail.length > 0 && (
          <div className="section-h">
            {t("Synced mail")} <span className="c">{mail.length}</span>
            <span className="sp" />
            <span className="faint small">{t("Once a sender is assigned to a contact, their messages appear in the activity automatically")}</span>
          </div>
        )}
        <div className="list">
          {[...mail].sort((a, b) => String(b.received_at || "").localeCompare(String(a.received_at || ""))).map((m, i) => {
            const email = String(m.from).match(/[\w.+-]+@[\w.-]+\.[a-z]+/i)?.[0];
            return (
              <div className={`mail-item ${notePath === m.document ? "on" : ""}`} key={i}>
                <button className="mail-open" onClick={() => read(m.document)}>
                  <span className="mail-from">{m.from}</span>
                  <b>{m.subject || t("(no subject)")}</b>
                  <small>{m.preview}</small>
                </button>
                <div className="mail-side">
                  {m.received_at && <span className="faint small">{relative(m.received_at)}</span>}
                  {email ? (
                    <select
                      className="chip"
                      aria-label={t("Assign to contact")}
                      defaultValue=""
                      disabled={busy}
                      onChange={(e) => e.target.value && assign(e.target.value, email)}
                    >
                      <option value="">{t("Assign to contact…")}</option>
                      {rows.map((r) => (
                        <option key={r.id} value={r.values.Slug}>
                          {r.values.Name}
                        </option>
                      ))}
                    </select>
                  ) : (
                    <Tag>{t("No sender address")}</Tag>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      </div>
      {notePath && (
        <div className="doc-frame mail-reader">
          <div className="doc-bar">
            <Icon name="mail" />
            <b>{notePath.split("/").pop()}</b>
            <span className="sp" />
            <button className="btn" onClick={closeNote}>
              {t("Save and close")}
            </button>
          </div>
          <TableEditor key={notePath} root={root} path={notePath} onError={fail} onOpen={showNote} />
        </div>
      )}
    </div>
  );
}

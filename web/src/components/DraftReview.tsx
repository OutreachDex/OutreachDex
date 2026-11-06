import { useState } from "react";
import { Modal, Icon, statuses, channelNames, messageChannels } from "./ui";
import { Remember } from "./Remember";
import { draftStale } from "../app/domain";
import { DraftHints } from "./DraftHints";
import { RecipientCheck } from "./RecipientCheck";
import { t } from "../app/i18n";
type V = Record<string, any>;
type Props = {
  draft: V;
  drafts: V[];
  name: string;
  busy: string;
  smtp: string;
  accounts: V;
  error: string;
  change: (v: V) => void;
  close: () => void;
  setSmtp: (v: string) => void;
  save: () => void;
  approve: () => void;
  discard: () => void;
  send: () => void;
  copy: () => void;
  markSent: () => void;
  remember: (note: string) => Promise<unknown>;
  lastReply?: string;
  /** Known email addresses of the person (null while loading). */
  addresses?: string[] | null;
  /** The contact's Language field, for the language-mismatch hint. */
  language?: string;
};
export function DraftReview(p: Props) {
  const [compare, setCompare] = useState("");
  const [acknowledged, setAcknowledged] = useState(false);
  const d = p.draft,
    status = d.metadata.status,
    locked = ["sent", "sending", "uncertain"].includes(status);
  const stale = draftStale(d, p.lastReply) && !acknowledged;
  // The agent's original wording is offered as a reference once a person has edited it.
  const original =
    typeof d.metadata.ai_original === "string" && d.metadata.ai_original.trim() !== d.body.trim()
      ? { path: "__original", body: d.metadata.ai_original, metadata: { ...d.metadata, subject: t("AI original"), status: d.metadata.status } }
      : null;
  const references = [...(original ? [original] : []), ...p.drafts.filter((item) => item.path !== d.path)];
  const reference = references.find((item) => item.path === compare);
  const metadata = (values: V) =>
    p.change({ ...d, metadata: { ...d.metadata, ...values, status: "draft" } });
  return (
    <Modal
      title={`${d.metadata.channel} · ${statuses[status] || t("Draft")}`}
      wide
      close={p.busy ? undefined : p.close}
      footer={
        <>
          <div className="draft-save-state">
            <span className={d._edited ? "unsaved-dot" : "saved-dot"} />
            {d._edited
              ? t("Changes held for this session · you can keep editing after closing")
              : d.path
                ? t("Saved to workspace")
                : t("New draft · not saved yet")}
          </div>
          <div className="actions wrap">
            <button disabled={!!p.busy || locked} onClick={p.save}>
              {t("Save draft")}
            </button>
            {status === "draft" && (
              <button
                className="primary"
                disabled={!!p.busy || !d.body.trim()}
                onClick={p.approve}
              >
                <Icon name="check" size={16} />
                {t("Save and approve")}
              </button>
            )}
            {status === "approved" &&
              (d.metadata.channel === "email" ? (
                <>
                  <select
                    aria-label={t("SMTP account")}
                    disabled={!!p.busy}
                    value={p.smtp}
                    onChange={(e) => p.setSmtp(e.target.value)}
                  >
                    <option value="">{t("Choose sending account")}</option>
                    {p.accounts.smtp.map((a: V) => (
                      <option key={a.id} value={a.id}>
                        {a.name}
                      </option>
                    ))}
                  </select>
                  <button
                    className="primary"
                    disabled={!!p.busy || !p.smtp || stale}
                    title={stale ? t("First confirm you've read the latest message") : undefined}
                    onClick={p.send}
                  >
                    {t("Send email")}
                  </button>
                </>
              ) : (
                <>
                  <button onClick={p.copy}>{t("Copy body")}</button>
                  <button
                    className="primary"
                    disabled={!!p.busy || stale}
                    title={stale ? t("First confirm you've read the latest message") : undefined}
                    onClick={p.markSent}
                  >
                    {t("Mark as sent")}
                  </button>
                </>
              ))}
          </div>
        </>
      }
    >
      <div className="review-context">
        <div>
          <span className="eyebrow">{t("To")}</span>
          <b>{p.name}</b>
        </div>
        <ol className="review-steps">
          <li className="done">{t("1 Edit")}</li>
          <li className={status !== "draft" ? "done" : ""}>{t("2 Review")}</li>
          <li className={status === "sent" ? "done" : ""}>{t("3 Send")}</li>
        </ol>
      </div>
      {draftStale(d, p.lastReply) && (
        <label className="caution stale-note">
          <input type="checkbox" checked={acknowledged} onChange={(e) => setAcknowledged(e.target.checked)} />
          {t("They wrote again after this draft was created, so it may no longer fit. I've read the latest message and still want to use this draft.")}
        </label>
      )}
      {p.error && (
        <p className="inline-error" role="alert">
          {p.error}
        </p>
      )}
      <div className="review-toolbar">
        <span>
          {locked
            ? t("This draft can't be edited")
            : status === "approved"
              ? t("Approved · editing it requires review again")
              : t("Check the recipient and content, then approve to send.")}
        </span>
        {references.length > 0 && (
          <span className="reference-picker">
            <select
              aria-label={t("Compare draft")}
              value={compare}
              onChange={(e) => setCompare(e.target.value)}
            >
              <option value="">{t("Choose a reference…")}</option>
              {references.map((item) => (
                <option key={item.path} value={item.path}>
                  {item.path === "__original" ? t("AI original (before edits)") : `${item.metadata.subject || t("Message draft")} · ${channelNames[item.metadata.channel] || item.metadata.channel}`}
                </option>
              ))}
            </select>
            <span aria-hidden="true">⌄</span>
          </span>
        )}
      </div>
      <div className={`review-columns ${reference ? "comparing" : ""}`}>
        <section className="draft-compose">
          <div className="recipient-fields">
            <label>
              {t("Channel")}
              <select
                disabled={locked || !!p.busy}
                value={d.metadata.channel}
                onChange={(e) => metadata({ channel: e.target.value })}
              >
                {messageChannels.map((c) => (
                  <option key={c} value={c}>
                    {channelNames[c]}
                  </option>
                ))}
              </select>
            </label>
            <label>
              {t("To")}
              <input
                value={d.metadata.to || ""}
                onChange={(e) => metadata({ to: e.target.value })}
                disabled={locked || !!p.busy}
              />
            </label>
          </div>
          {d.metadata.channel === "email" && (
            <RecipientCheck to={d.metadata.to || ""} known={p.addresses ?? null} disabled={locked || !!p.busy} pick={(to) => metadata({ to })} />
          )}
          <label>
            {t("Subject")}
            <input
              value={d.metadata.subject || ""}
              placeholder={t("Write a clear, specific subject")}
              onChange={(e) => metadata({ subject: e.target.value })}
              disabled={locked || !!p.busy}
            />
          </label>
          <label className="body-label">
            {t("Body")}
            <textarea
              className="draft-body"
              value={d.body}
              placeholder={t("Start with what you want to talk to them about…")}
              readOnly={locked || !!p.busy}
              onChange={(e) =>
                p.change({
                  ...d,
                  body: e.target.value,
                  metadata: { ...d.metadata, status: "draft" },
                })
              }
            />
          </label>
          <DraftHints body={d.body} channel={d.metadata.channel} language={p.language} />
          <div className="compose-bottom">
            <small>{t("{n} characters", { n: d.body.length })}</small>
            <Remember remember={p.remember} />
            <button
              className="text-button danger"
              disabled={!!p.busy || !d.path || locked || status === "discarded"}
              onClick={p.discard}
            >
              {t("Discard")}
            </button>
          </div>
        </section>
        {reference && (
          <aside className="reference-draft">
            <span className="eyebrow">{t("Reference · read-only")}</span>
            <h3>{reference.metadata.subject || t("Message draft")}</h3>
            <small>
              {channelNames[reference.metadata.channel]} ·{" "}
              {statuses[reference.metadata.status]}
            </small>
            <p className="prewrap">{reference.body}</p>
          </aside>
        )}
      </div>
      {status === "approved" &&
        d.metadata.channel === "email" &&
        !p.accounts.smtp.length && (
          <p className="caution">
            {t("Draft approved. Add a sending mailbox in Settings before sending.")}
          </p>
        )}
      {p.busy && (
        <p role="status" className="muted">
          {p.busy}
        </p>
      )}
    </Modal>
  );
}

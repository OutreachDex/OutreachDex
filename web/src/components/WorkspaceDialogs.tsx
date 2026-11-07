import { useState } from "react";
import { Icon, Modal } from "./ui";
import type { V } from "../app/domain";
import { t } from "../app/i18n";

export type Template = { id: string; label: string; description: string };

/** Choose the template for a new workspace; the folder is picked afterwards. */
export function TemplatePicker({
  templates,
  busy,
  close,
  pick,
}: {
  templates: Template[];
  busy: boolean;
  close: () => void;
  pick: (id: string) => void;
}) {
  const [chosen, setChosen] = useState(templates[0]?.id || "blank");
  return (
    <Modal
      title={t("New workspace")}
      close={busy ? undefined : close}
      footer={
        <div className="row-gap end grow">
          <span className="faint small grow">{t("Stages, fields and analysis purposes live in the workspace's globalcontact.toml and playbooks/; you can change them anytime.")}</span>
          <button className="btn" onClick={close} disabled={busy}>
            {t("Cancel")}
          </button>
          <button className="btn primary" disabled={busy} onClick={() => pick(chosen)}>
            {t("Choose folder…")}
          </button>
        </div>
      }
    >
      <div className="choices" role="radiogroup" aria-label={t("Workspace template")}>
        {templates.map((tpl) => (
          <button
            key={tpl.id}
            role="radio"
            aria-checked={chosen === tpl.id}
            className={`choice ${chosen === tpl.id ? "on" : ""}`}
            onClick={() => setChosen(tpl.id)}
            onDoubleClick={() => pick(tpl.id)}
          >
            <span className="choice-mark">{chosen === tpl.id && <Icon name="check" size={12} />}</span>
            <span className="grow">
              <b>{tpl.label}</b>
              <small>{tpl.description}</small>
            </span>
          </button>
        ))}
      </div>
    </Modal>
  );
}

/** Map CSV / TSV columns to contact fields, preview, then import. */
export function ImportDialog({
  preview,
  busy,
  close,
  run,
}: {
  preview: V;
  busy: string;
  close: () => void;
  run: (mapping: (string | null)[]) => void;
}) {
  const [mapping, setMapping] = useState<(string | null)[]>(preview.mapping);
  const named = mapping.filter((m) => m === "Name").length === 1;
  const used = new Set(mapping.filter(Boolean));
  return (
    <Modal
      title={t("Import contacts · {file}", { file: preview.file || "" })}
      wide
      close={busy ? undefined : close}
      footer={
        <div className="row-gap end grow">
          <span className="faint small grow">
            {t("{total} rows. Existing contacts are matched by email, then by name, and updated; the rest are created. Email, website, X, Instagram, WhatsApp, TikTok, Facebook (Messenger) and LinkedIn are added as channels.", { total: preview.total })}
          </span>
          <button className="btn" onClick={close} disabled={!!busy}>
            {t("Cancel")}
          </button>
          <button className="btn primary" disabled={!!busy || !named} onClick={() => run(mapping)}>
            {busy || t("Import {total} rows", { total: preview.total })}
          </button>
        </div>
      }
    >
      {!named && <p className="caution">{t("Map one column to “Name”.")}</p>}
      <div className="import-grid">
        <table className="grid">
          <thead>
            <tr>
              {preview.headers.map((h: string, i: number) => (
                <th key={i}>
                  <span className="import-head">{h || t("Column {n}", { n: i + 1 })}</span>
                  <select
                    aria-label={t("Field for {column}", { column: h })}
                    className={mapping[i] ? "chip set" : "chip"}
                    value={mapping[i] || ""}
                    onChange={(e) => setMapping(mapping.map((m, j) => (j === i ? e.target.value || null : m)))}
                  >
                    <option value="">{t("Don't import")}</option>
                    {preview.fields.map((f: string) => (
                      <option key={f} value={f} disabled={used.has(f) && mapping[i] !== f}>
                        {f}
                      </option>
                    ))}
                  </select>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {preview.sample.map((row: string[], r: number) => (
              <tr key={r}>
                {preview.headers.map((_: string, i: number) => (
                  <td key={i} className={`clip ${mapping[i] ? "" : "faint"}`}>
                    {row[i]}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </Modal>
  );
}

export type ImportKind = "contacts" | "whatsapp" | "linkedin";

/** What to import: a contact list, a WhatsApp chat export or a LinkedIn data archive. */
export function ImportChooser({ close, pick }: { close: () => void; pick: (kind: ImportKind) => void }) {
  const options: [ImportKind, string, string][] = [
    ["contacts", t("Contacts (CSV / TSV)"), t("A spreadsheet of people. Columns are mapped to contact fields before importing.")],
    [
      "whatsapp",
      t("WhatsApp chat"),
      t("In WhatsApp, open a one-to-one chat → contact name → Export chat (Without media), then choose the .zip or .txt."),
    ],
    [
      "linkedin",
      t("LinkedIn messages"),
      t("On LinkedIn: Settings → Data privacy → Get a copy of your data, select Messages and Connections, then choose the .zip you receive."),
    ],
  ];
  return (
    <Modal
      title={t("Import")}
      close={close}
      footer={
        <div className="row-gap end grow">
          <span className="faint small grow">{t("Chats are read from files you export yourself. OutreachDex never signs in to WhatsApp or LinkedIn.")}</span>
          <button className="btn" onClick={close}>
            {t("Cancel")}
          </button>
        </div>
      }
    >
      <div className="choices">
        {options.map(([kind, label, description]) => (
          <button key={kind} className="choice" onClick={() => pick(kind)}>
            <span className="choice-mark">
              <Icon name={kind === "contacts" ? "inbox" : "mail"} size={12} />
            </span>
            <span className="grow">
              <b>{label}</b>
              <small>{description}</small>
            </span>
          </button>
        ))}
      </div>
    </Modal>
  );
}

const day = (iso?: string) => (iso ? new Date(iso).toLocaleDateString() : "");

/** Review a chat export: who is you, which contact each conversation belongs to, then import. */
export function ChatImportDialog({
  preview,
  contacts,
  busy,
  close,
  reload,
  run,
}: {
  preview: V;
  contacts: { slug: string; name: string }[];
  busy: string;
  close: () => void;
  reload: (options: { me?: string; date_order?: string }) => void;
  run: (assign: Record<string, string>) => void;
}) {
  const conversations: V[] = preview.conversations || [];
  const linkedin = preview.kind === "linkedin";
  // Matched conversations go to their contact; an unmatched WhatsApp chat (chosen on purpose)
  // creates a contact, unmatched LinkedIn conversations are skipped until chosen.
  const initial = () =>
    Object.fromEntries(conversations.map((c) => [c.key, c.match?.slug || (linkedin ? "" : "new")]));
  const [assign, setAssign] = useState<Record<string, string>>(initial);
  const [seen, setSeen] = useState(preview);
  if (seen !== preview) {
    setSeen(preview);
    setAssign(initial());
  }
  const chosen = Object.values(assign).filter(Boolean).length;
  const unmatched = conversations.filter((c) => !c.match);
  const by: Record<string, string> = {
    name: t("same name"),
    email: t("same email"),
    whatsapp: t("WhatsApp number"),
    linkedin: t("LinkedIn profile"),
  };
  const needsMe = preview.me === null || preview.me === undefined;
  return (
    <Modal
      title={linkedin ? t("Import LinkedIn messages · {file}", { file: preview.file || "" }) : t("Import WhatsApp chat · {file}", { file: preview.file || "" })}
      wide
      close={busy ? undefined : close}
      footer={
        <div className="row-gap end grow">
          <span className="faint small grow">
            {t("Messages are saved by day under chats/ in the workspace, and the timeline gets one entry per day. Importing a newer export later only adds new messages.")}
          </span>
          <button className="btn" onClick={close} disabled={!!busy}>
            {t("Cancel")}
          </button>
          <button className="btn primary" disabled={!!busy || needsMe || !chosen} onClick={() => run(assign)}>
            {busy || t("Import conversations ({n})", { n: chosen })}
          </button>
        </div>
      }
    >
      {preview.groups > 0 && !linkedin && !conversations.length && (
        <p className="caution">{t("This is a group chat. Only one-to-one chats can be imported; export the chat with each person instead.")}</p>
      )}
      {needsMe && !(preview.groups > 0 && !linkedin) && (
        <p className="row-gap">
          <span>{t("Which one is you?")}</span>
          <select aria-label={t("Which one is you?")} className="chip" value="" onChange={(e) => e.target.value && reload({ me: e.target.value })}>
            <option value="">{t("Choose…")}</option>
            {(preview.meChoices || []).map((c: V) => (
              <option key={c.id} value={c.id}>
                {c.name || c.id}
              </option>
            ))}
          </select>
        </p>
      )}
      {!linkedin && preview.dateOrder && !preview.dateOrderCertain && (
        <p className="row-gap">
          <span>{t("Dates like 01/02 in this export mean")}</span>
          <select aria-label={t("Date order")} className="chip" value={preview.dateOrder} onChange={(e) => reload({ me: preview.me ?? undefined, date_order: e.target.value })}>
            <option value="dmy">{t("day/month")}</option>
            <option value="mdy">{t("month/day")}</option>
          </select>
        </p>
      )}
      {(preview.groups > 0 && linkedin) || preview.spam > 0 ? (
        <p className="faint small">
          {[
            preview.groups > 0 && linkedin ? t("{n} group conversations are not imported.", { n: preview.groups }) : "",
            preview.spam > 0 ? t("{n} spam messages were left out.", { n: preview.spam }) : "",
          ]
            .filter(Boolean)
            .join(" ")}
        </p>
      ) : null}
      {linkedin && unmatched.length > 1 && (
        <p className="row-gap">
          <span className="faint small grow">{t("{n} conversations don't match a contact yet.", { n: unmatched.length })}</span>
          <button className="btn ghost" disabled={!!busy} onClick={() => setAssign({ ...assign, ...Object.fromEntries(unmatched.map((c) => [c.key, "new"])) })}>
            {t("Create contacts for all")}
          </button>
          <button className="btn ghost" disabled={!!busy} onClick={() => setAssign({ ...assign, ...Object.fromEntries(unmatched.map((c) => [c.key, ""])) })}>
            {t("Skip all")}
          </button>
        </p>
      )}
      {conversations.length > 0 && (
        <div className="import-grid">
          <table className="grid">
            <thead>
              <tr>
                <th>{t("Conversation")}</th>
                <th>{t("Messages")}</th>
                <th>{t("Period")}</th>
                <th>{t("Contact")}</th>
              </tr>
            </thead>
            <tbody>
              {conversations.map((c) => (
                <tr key={c.key}>
                  <td className="clip">
                    <b>{c.partner?.name || c.title}</b>
                    {c.partner?.affiliation && <div className="faint small">{c.partner.affiliation}</div>}
                  </td>
                  <td className="faint">{t("{incoming} from them · {outgoing} from you", { incoming: c.incoming, outgoing: c.outgoing })}</td>
                  <td className="faint">
                    {day(c.first)}
                    {c.first !== c.last ? ` – ${day(c.last)}` : ""}
                  </td>
                  <td>
                    <select
                      aria-label={t("Contact for {name}", { name: c.partner?.name || c.title })}
                      className={assign[c.key] ? "chip set" : "chip"}
                      value={assign[c.key] || ""}
                      onChange={(e) => setAssign({ ...assign, [c.key]: e.target.value })}
                    >
                      <option value="">{t("Don't import")}</option>
                      <option value="new">{t("New contact “{name}”", { name: c.partner?.name || c.title })}</option>
                      {contacts.map((p) => (
                        <option key={p.slug} value={p.slug}>
                          {p.name}
                          {c.match?.slug === p.slug ? ` (${by[c.match.by] || c.match.by})` : ""}
                        </option>
                      ))}
                    </select>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Modal>
  );
}

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

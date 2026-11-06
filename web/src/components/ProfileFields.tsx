import { useEffect, useState } from "react";
import { Icon } from "./ui";
import type { V } from "../app/domain";
import { t } from "../app/i18n";

export type FieldDef = { title: string; type: string; choices: string[]; system: boolean };

// Display names for the built-in columns; custom fields show their own title.
const labels: Record<string, string> = {
  Name: t("Name"),
  "Native name": t("Native name"),
  Affiliation: t("Affiliation"),
  Campaign: t("Campaign"),
  Language: t("Language"),
  "Primary channel": t("Preferred channel"),
  Email: t("Email"),
  Homepage: t("Homepage"),
  X: "X",
  Instagram: "Instagram",
  Verified: t("Verified"),
  Hook: t("Hook"),
  "Next action": t("Next step"),
  "Follow up": t("Next follow-up"),
  Notes: t("Notes"),
};
const long = new Set(["long_text"]);
const types: [string, string][] = [
  ["text", t("Text")],
  ["long_text", t("Long text")],
  ["number", t("Number")],
  ["date", t("Date")],
  ["url", t("Link")],
  ["checkbox", t("Yes / No")],
  ["single_select", t("Single select")],
  ["multi_select", t("Multi-select (tags)")],
];

/** Every contact field, edited in place: a value saves when the field loses focus. */
export function ProfileFields({
  fields,
  values,
  busy,
  save,
  addField,
  addChoice,
}: {
  fields: FieldDef[];
  values: V;
  busy: boolean;
  save: (title: string, value: unknown) => Promise<unknown>;
  addField: (title: string, type: string, choices: string[]) => Promise<unknown>;
  addChoice: (title: string, choice: string) => Promise<unknown>;
}) {
  const [adding, setAdding] = useState(false);
  const [title, setTitle] = useState("");
  const [type, setType] = useState("text");
  const [choices, setChoices] = useState("");
  const editable = fields.filter((f) => !f.system);
  return (
    <div className="profile">
      {editable.map((f) => (
        <FieldRow key={f.title} field={f} value={values[f.title]} busy={busy} save={save} addChoice={addChoice} />
      ))}
      {adding ? (
        <form
          className="field-add"
          onSubmit={(e) => {
            e.preventDefault();
            void addField(
              title,
              type,
              choices.split(/[,，、\n]/).map((c) => c.trim()).filter(Boolean),
            ).then(() => {
              setAdding(false);
              setTitle("");
              setChoices("");
            });
          }}
        >
          <input autoFocus aria-label={t("New field name")} placeholder={t("Field name, e.g. Phone, Title, Birthday")} value={title} onChange={(e) => setTitle(e.target.value)} />
          <select aria-label={t("Field type")} value={type} onChange={(e) => setType(e.target.value)}>
            {types.map(([k, l]) => (
              <option key={k} value={k}>
                {l}
              </option>
            ))}
          </select>
          {type.endsWith("select") && (
            <input aria-label={t("Options")} placeholder={t("Options, comma-separated (you can add more later)")} value={choices} onChange={(e) => setChoices(e.target.value)} />
          )}
          <div className="row-gap end">
            <button type="button" className="btn ghost" onClick={() => setAdding(false)}>
              {t("Cancel")}
            </button>
            <button className="btn primary" disabled={busy || !title.trim()}>
              {t("Add field")}
            </button>
          </div>
          <p className="faint small">{t("The field is added to every contact and stored in the workspace's contacts table.")}</p>
        </form>
      ) : (
        <button className="link subtle" onClick={() => setAdding(true)}>
          <Icon name="plus" size={12} /> {t("Add field")}
        </button>
      )}
    </div>
  );
}

function FieldRow({
  field,
  value,
  busy,
  save,
  addChoice,
}: {
  field: FieldDef;
  value: unknown;
  busy: boolean;
  save: (title: string, value: unknown) => Promise<unknown>;
  addChoice: (title: string, choice: string) => Promise<unknown>;
}) {
  const text = Array.isArray(value) ? value.join(", ") : value === null || value === undefined ? "" : String(value);
  const [draft, setDraft] = useState(text);
  useEffect(() => setDraft(text), [text]);
  const label = labels[field.title] || field.title;
  const commit = (next: unknown) => {
    const normalized = typeof next === "string" ? next.trim() : next;
    if (normalized === (typeof value === "string" ? value.trim() : value) || (normalized === "" && !value)) return;
    void save(field.title, normalized === "" ? null : normalized);
  };
  let input;
  if (field.type === "checkbox") {
    input = <input type="checkbox" aria-label={label} disabled={busy} checked={!!value} onChange={(e) => void save(field.title, e.target.checked)} />;
  } else if (field.type === "single_select") {
    input = (
      <select
        className="bare"
        aria-label={label}
        disabled={busy}
        value={text}
        onChange={(e) => {
          const next = e.target.value;
          if (next === "__new") {
            const choice = window.prompt(t("Add an option to “{field}”", { field: label }))?.trim();
            if (choice) void addChoice(field.title, choice).then(() => save(field.title, choice));
            return;
          }
          void save(field.title, next || null);
        }}
      >
        <option value="">—</option>
        {field.choices.map((c) => (
          <option key={c} value={c}>
            {c}
          </option>
        ))}
        {text && !field.choices.includes(text) && <option value={text}>{text}</option>}
        <option value="__new">{t("+ New option…")}</option>
      </select>
    );
  } else if (field.type === "multi_select") {
    input = (
      <input
        className="bare-input"
        aria-label={label}
        disabled={busy}
        value={draft}
        placeholder="—"
        onChange={(e) => setDraft(e.target.value)}
        onBlur={() =>
          void (async () => {
            const items = draft.split(/[,，、]/).map((s) => s.trim()).filter(Boolean);
            for (const item of items) if (!field.choices.includes(item)) await addChoice(field.title, item);
            if (items.join(", ") !== text) await save(field.title, items);
          })()
        }
      />
    );
  } else if (long.has(field.type)) {
    input = (
      <textarea className="bare-input" aria-label={label} disabled={busy} rows={2} value={draft} onChange={(e) => setDraft(e.target.value)} onBlur={() => commit(draft)} />
    );
  } else {
    input = (
      <input
        className="bare-input"
        aria-label={label}
        disabled={busy}
        type={field.type === "number" ? "number" : field.type === "date" ? "date" : "text"}
        placeholder="—"
        value={field.type === "date" ? draft.slice(0, 10) : draft}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={() => commit(field.type === "number" ? (draft === "" ? "" : Number(draft)) : draft)}
        onKeyDown={(e) => e.key === "Enter" && (e.currentTarget as HTMLInputElement).blur()}
      />
    );
  }
  return (
    <label className={`attr ${long.has(field.type) ? "tall" : ""}`}>
      <span className="lab" title={field.title}>
        {label}
      </span>
      {input}
    </label>
  );
}

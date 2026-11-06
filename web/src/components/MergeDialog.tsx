import { useEffect, useState } from "react";
import { Avatar, Modal } from "./ui";
import { matches, type Row, type V } from "../app/domain";
import { t } from "../app/i18n";

/** Merge another record of the same person into this contact, after a preview of what moves. */
export function MergeDialog({
  winner,
  rows,
  busy,
  call,
  close,
  merge,
}: {
  winner: Row;
  rows: Row[];
  busy: string;
  call: <T>(action: string, args?: V) => Promise<T>;
  close: () => void;
  merge: (loser: string) => void;
}) {
  const [query, setQuery] = useState("");
  const [loser, setLoser] = useState("");
  const [preview, setPreview] = useState<V | null>(null);
  const candidates = rows.filter((r) => r.values.Slug !== winner.values.Slug && matches(r, query)).slice(0, 8);
  useEffect(() => {
    if (!loser) return setPreview(null);
    let alive = true;
    void call<V>("contact.merge_preview", { winner: winner.values.Slug, loser })
      .then((p) => alive && setPreview(p))
      .catch((e) => alive && setPreview({ error: String(e) }));
    return () => {
      alive = false;
    };
  }, [loser, call, winner.values.Slug]);
  return (
    <Modal
      title={t("Merge into “{name}”", { name: winner.values.Name })}
      close={busy ? undefined : close}
      footer={
        <div className="row-gap end grow">
          <span className="faint small grow">{t("The merged record is deleted and this can't be undone; the workspace folder can be restored with git or a backup.")}</span>
          <button className="btn" onClick={close}>
            {t("Cancel")}
          </button>
          <button className="btn primary danger-fill" disabled={!!busy || !preview || !!preview.error} onClick={() => merge(loser)}>
            {t("Merge")}
          </button>
        </div>
      }
    >
      <p className="muted">{t("Choose another record for the same person; its data and activity will be merged into “{name}”.", { name: winner.values.Name })}</p>
      <input autoFocus aria-label={t("Search contacts to merge")} placeholder={t("Search name, email…")} value={query} onChange={(e) => setQuery(e.target.value)} />
      <div className="list">
        {candidates.map((r) => (
          <button key={r.id} className={`identity merge-pick ${loser === r.values.Slug ? "on" : ""}`} onClick={() => setLoser(r.values.Slug)}>
            <Avatar name={r.values.Name || "?"} />
            <span className="identity-main">
              <b>{r.values.Name}</b>
              <small>{[r.values.Email, r.values.Affiliation].filter(Boolean).join(" · ")}</small>
            </span>
          </button>
        ))}
      </div>
      {preview?.error && <p className="inline-error">{preview.error}</p>}
      {preview && !preview.error && (
        <div className="caution">
          {t("“{loser}” will be merged into “{winner}”: {channels} channels, {interactions} activity items, {identities} source identities, {drafts} drafts and {intake} recorded notes; the dossier text is appended at the end.", {
            loser: preview.loser,
            winner: preview.winner,
            channels: preview.channels,
            interactions: preview.interactions,
            identities: preview.identities,
            drafts: preview.drafts,
            intake: preview.intake,
          })}
          {preview.fill.length > 0 && <> {t("Empty fields filled: {fields}.", { fields: preview.fill.join(t(", ")) })}</>}
          {preview.drafts > 0 && <> {t("Approved drafts need to be approved again.")}</>}
        </div>
      )}
    </Modal>
  );
}

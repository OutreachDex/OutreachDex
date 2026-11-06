import { useState } from "react";
import { Icon } from "./ui";
import { t } from "../app/i18n";

/** Saves a one-line writing preference to preferences.md; later drafts follow it. */
export function Remember({ remember }: { remember: (note: string) => Promise<unknown> }) {
  const [open, setOpen] = useState(false);
  const [note, setNote] = useState("");
  if (!open)
    return (
      <button type="button" className="link subtle" onClick={() => setOpen(true)} title={t("All future drafts will follow it")}>
        <Icon name="spark" size={12} /> {t("Remember a writing preference")}
      </button>
    );
  return (
    <form
      className="remember"
      onSubmit={(e) => {
        e.preventDefault();
        void remember(note).then(() => {
          setNote("");
          setOpen(false);
        });
      }}
    >
      <input
        autoFocus
        aria-label={t("Writing preferences")}
        placeholder={t("e.g. Skip the small talk and state the purpose; under 80 words")}
        value={note}
        onChange={(e) => setNote(e.target.value)}
        onKeyDown={(e) => e.key === "Escape" && (e.stopPropagation(), setOpen(false))}
      />
      <button className="btn" disabled={!note.trim()}>
        {t("Remember")}
      </button>
    </form>
  );
}

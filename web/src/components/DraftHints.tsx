import { lintDraft } from "../app/lint";
import { t } from "../app/i18n";

/** Copy hints for a draft body; "block" hints mirror the server-side send check. */
export function DraftHints({ body, channel, language }: { body: string; channel: string; language?: string }) {
  const hints = lintDraft(body, channel, language);
  if (!hints.length) return null;
  return (
    <ul className="draft-hints" aria-label={t("Copy check")}>
      {hints.map((h, i) => (
        <li key={i} className={h.level}>
          {h.text}
        </li>
      ))}
    </ul>
  );
}

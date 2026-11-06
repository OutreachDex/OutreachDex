import { Icon } from "./ui";
import { t } from "../app/i18n";

/** The bare address of "Name <a@b.co>" or "a@b.co", lower-cased. */
export function addressOf(to: string) {
  const m = /<([^>]+)>/.exec(to);
  return (m ? m[1] : to).trim().toLowerCase();
}

/**
 * Warns when an email draft is addressed to something that is not one of the person's known
 * addresses (Email field, email channels, linked sources), and offers the known ones. Sending
 * to an unknown address is also refused by the backend unless confirmed.
 */
export function RecipientCheck({
  to,
  known,
  disabled,
  pick,
}: {
  to: string;
  known: string[] | null;
  disabled?: boolean;
  pick: (address: string) => void;
}) {
  if (!known || !to.trim()) return null;
  const address = addressOf(to);
  if (known.includes(address)) return null;
  return (
    <div className="recipient-check" role="alert">
      <Icon name="mail" size={14} />
      <span className="grow">
        {known.length
          ? t("{address} is not a known address for this person. Check it is correct.", { address })
          : t("This person has no known address yet, so {address} cannot be confirmed as theirs.", { address })}
      </span>
      {known.slice(0, 3).map((k) => (
        <button key={k} type="button" className="btn ghost sm" disabled={disabled} onClick={() => pick(k)}>
          {t("Use {address}", { address: k })}
        </button>
      ))}
    </div>
  );
}

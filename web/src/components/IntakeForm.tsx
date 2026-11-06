import { useState } from "react";
import { t } from "../app/i18n";
import { channelNames, messageChannels } from "./ui";
export function IntakeForm({
  busy,
  submit,
}: {
  busy: string;
  submit: (
    data: { channel: string; title: string; body: string },
    analyze: boolean,
    category: string,
  ) => void;
}) {
  const [category, setCategory] = useState("research"),
    [channel, setChannel] = useState("other"),
    [title, setTitle] = useState(""),
    [source, setSource] = useState(""),
    [body, setBody] = useState("");
  const categories: Record<string, string> = {
    research: t("Background"),
    feedback: t("User feedback"),
    followup: t("Follow-up notes"),
  };
  const send = (analyze: boolean) => {
    if (!title.trim() || !body.trim()) return;
    submit(
      {
        channel,
        title: title.trim(),
        body: `${t("Record type: {kind}", { kind: categories[category] })}\n${source.trim() ? `${t("Source: {source}", { source: source.trim() })}\n` : ""}\n${body}`,
      },
      analyze,
      category,
    );
  };
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        send(false);
      }}
    >
      <fieldset disabled={!!busy} aria-busy={!!busy}>
        <p className="muted">{t("Keep the original words, context and source together so later analysis has solid evidence.")}</p>
        <div className="form-grid">
          <label>
            {t("Record type")}
            <select
              value={category}
              onChange={(e) => setCategory(e.target.value)}
            >
              {Object.entries(categories).map(([id, label]) => (
                <option key={id} value={id}>
                  {label}
                </option>
              ))}
            </select>
          </label>
          <label>
            {t("Channel")}
            <select
              aria-label={t("Channel")}
              value={channel}
              onChange={(e) => setChannel(e.target.value)}
            >
              {[
                ["other", t("Web / other")],
                ...messageChannels.filter((c) => c !== "other").map((c) => [c, channelNames[c]]),
              ].map(([id, label]) => (
                <option key={id} value={id}>
                  {label}
                </option>
              ))}
            </select>
          </label>
        </div>
        <label>
          {t("Title")}
          <input
            required
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder={t("e.g. Problems with the first spreadsheet import")}
          />
        </label>
        <label>
          {t("Source link or description")}
          <input
            value={source}
            onChange={(e) => setSource(e.target.value)}
            placeholder={t("Web link, email subject, interview time…")}
          />
        </label>
        <label>
          {t("Link or original text")}
          <textarea
            required
            value={body}
            onChange={(e) => setBody(e.target.value)}
            placeholder={
              category === "feedback"
                ? t("What were they trying to do? What happened? Paste their words and mark what's unconfirmed.")
                : category === "followup"
                  ? t("What did you agree on last time? What feedback did you get? What's solved and what's still open?")
                  : t("Paste the relevant text; saving only a link won't fetch the page.")
            }
          />
        </label>
        <div className="actions wrap">
          <button disabled={!!busy || !title.trim() || !body.trim()}>
            {busy || t("Save")}
          </button>
          <button
            type="button"
            className="primary"
            disabled={!!busy || !title.trim() || !body.trim()}
            onClick={() => send(true)}
          >
            {t("Save and ask the Agent")}
          </button>
        </div>
      </fieldset>
    </form>
  );
}

import { useMemo, useState } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Icon } from "../components/ui";
import { lang, t } from "../app/i18n";

// The help center is the same Markdown as docs/help/ (English) and docs/help/<lang>/ (zh, ja, ko),
// bundled at build time. Articles missing in the current language fall back to English.
const source = import.meta.glob("../../../docs/help/*.md", { query: "?raw", import: "default", eager: true }) as Record<string, string>;
const translated = import.meta.glob("../../../docs/help/{zh,ja,ko}/*.md", { query: "?raw", import: "default", eager: true }) as Record<string, string>;
const topics = Object.entries(source)
  .map(([path, english]) => {
    const file = path.split("/").pop()!;
    const text = (lang !== "en" && translated[`../../../docs/help/${lang}/${file}`]) || english;
    return { file, title: text.match(/^#\s+(.+)$/m)?.[1] || file, text };
  })
  .sort((a, b) => a.file.localeCompare(b.file));

export function HelpPage({ openUrl }: { openUrl: (url: string) => void }) {
  const [current, setCurrent] = useState(topics[0]?.file || "");
  const [query, setQuery] = useState("");
  const shown = useMemo(
    () => topics.filter((t) => !query.trim() || t.text.toLowerCase().includes(query.trim().toLowerCase())),
    [query],
  );
  const topic = topics.find((t) => t.file === current) || topics[0];
  return (
    <div className="help">
      <aside className="help-side">
        <div className="search-inline">
          <Icon name="search" size={14} />
          <input aria-label={t("Search help")} placeholder={t("Search help…")} value={query} onChange={(e) => setQuery(e.target.value)} />
        </div>
        <nav aria-label={t("Help contents")}>
          {shown.map((t) => (
            <button key={t.file} className={`nav ${t.file === topic?.file ? "on" : ""}`} onClick={() => setCurrent(t.file)}>
              <span className="nav-label">{t.title}</span>
            </button>
          ))}
          {!shown.length && <p className="faint small pad">{t("Nothing found.")}</p>}
        </nav>
      </aside>
      <article className="help-body markdown">
        {topic && (
          <Markdown
            remarkPlugins={[remarkGfm]}
            components={{
              a: ({ href = "", children }) => {
                const target = topics.find((t) => href.split("#")[0].endsWith(t.file));
                if (target) return <button onClick={() => setCurrent(target.file)}>{children}</button>;
                if (/^https?:\/\//.test(href)) return <button onClick={() => openUrl(href)}>{children}</button>;
                return <span>{children}</span>;
              },
              img: () => null,
            }}
          >
            {topic.text}
          </Markdown>
        )}
      </article>
    </div>
  );
}

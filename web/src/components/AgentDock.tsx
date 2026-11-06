import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { confirm } from "@tauri-apps/plugin-dialog";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import "@xterm/xterm/css/xterm.css";
import { TableEditor, saveEditors } from "../kits/TableEditor";
import { Icon } from "./ui";
import { useAgentPaneWidth } from "./useAgentPaneWidth";
import { currentScheme, useScheme } from "../app/prefs";
import { copyText } from "../app/clipboard";
import { t, lang, locale } from "../app/i18n";

// Terminal colors follow the app theme; ANSI colors stay readable on both backgrounds.
const terminalTheme = (scheme: "light" | "dark") =>
  scheme === "dark"
    ? {
        background: "#0a0a0b",
        foreground: "#c9cad0",
        cursor: "#a5a7ff",
        selectionBackground: "#2c2d52",
        black: "#1d1e21",
        brightBlack: "#62636b",
        blue: "#7fb0ff",
        brightBlue: "#a9c8ff",
        green: "#5fd38d",
        red: "#f27a7a",
        yellow: "#f0b450",
        magenta: "#b3a4ff",
        cyan: "#6fd0dc",
      }
    : {
        background: "#fafafa",
        foreground: "#2b2d33",
        cursor: "#5b5bd6",
        selectionBackground: "#dcdcfb",
        black: "#2b2d33",
        brightBlack: "#9a9da8",
        blue: "#1f5fc4",
        brightBlue: "#2f6bff",
        green: "#1b7a3d",
        red: "#b42323",
        yellow: "#9a5b00",
        magenta: "#5b3fc4",
        cyan: "#0f7b8a",
        white: "#e1e1e6",
        brightWhite: "#ffffff",
      };

type Session = {
  id: string;
  agent: string;
  status: string;
  created_at: number;
  cols: number;
  rows: number;
  error: string;
  /** The Agent's own latest task.report. */
  task?: { status: string; summary: string; at: number };
};
type Activity = {
  id: number;
  session: string;
  tool: string;
  contact: string;
  target: string;
  detail: string;
  status: string;
  ok: boolean;
  at: number;
};
// What each MCP tool means to the user; unknown tools show their name.
const toolLabels: Record<string, string> = {
  "context.get": t("Check what you're viewing"),
  "workspace.guide": t("Read workspace guide"),
  "memory.read": t("Read Agent memory"),
  "memory.append": t("Remember"),
  "contact.get": t("Read profile"),
  "contact.search": t("Search contacts"),
  "timeline.get": t("Read activity"),
  "source.facts": t("Read source facts"),
  "source.list": t("View data sources"),
  "source.sync": t("Sync data source"),
  "mail.accounts": t("View mailboxes"),
  "mail.sync": t("Sync mailbox"),
  "campaign.get": t("Read campaign brief"),
  "document.list": t("List documents"),
  "document.read": t("Read document"),
  "document.create": t("Create document"),
  "document.replace": t("Edit document"),
  "dossier.write_section": t("Update dossier"),
  "contact.update": t("Update profile"),
  "contact.follow_up": t("Set follow-up"),
  "channel.add": t("Add channel"),
  "draft.save": t("Save draft"),
  "preferences.add": t("Remember writing preference"),
  "list.all": t("View lists"),
  "list.get": t("Read list"),
  "list.draft_save": t("Save list draft"),
  "list.decline": t("Skip list member"),
  "ui.open": t("Show to you"),
  "task.report": t("Report progress"),
};
const taskStatuses: Record<string, string> = {
  working: t("In progress"),
  done: t("Done"),
  needs_input: t("Awaiting your reply"),
  blocked: t("Blocked"),
};
// The backend's running-terminal limit message (gc-service terminal.rs); older builds wrote it in Chinese.
const sessionLimitError = /terminals are already running|个运行中的终端/;
const documentRoots = ["drafts/", "notes/", "people/", "memory/", "campaigns/"];
type Preferences = {
  width?: number;
  selected?: string;
  agent?: string;
  labels?: Record<string, string>;
};
const statuses: Record<string, string> = {
  running: t("Session connected"),
  completed: t("Ended"),
  stopped: t("Stopped"),
  interrupted: t("Interrupted when the app last quit"),
  failed: t("Failed to start or record"),
};
const names: Record<string, string> = { codex: "Codex", claude: "Claude Code" };
function preferences(root: string): Preferences {
  try {
    return JSON.parse(localStorage.getItem(`gc.agent:${root}`) || "{}") || {};
  } catch {
    return {};
  }
}

export type AgentTask = {
  id: number;
  task: "" | "overview" | "reply" | "list" | "sync" | "source_person";
  contact?: string;
  list?: string;
  note?: string;
  /** source_person: the data source and the person's id there, and a name for the session. */
  source?: string;
  external_id?: string;
  label?: string;
};
const taskNames: Record<string, string> = {
  overview: t("Understand"),
  reply: t("Draft reply"),
  list: t("Prepare list"),
  sync: t("Sync and summarize"),
  source_person: t("Understand user"),
};

export function AgentDock({
  root,
  visible,
  close,
  contact,
  contactName,
  document,
  documentPath,
  documentBusy,
  openDocument,
  version,
  defaultAgent,
  request,
  view = "",
  list = "",
  draft = "",
  selection = [],
  names = {},
  focusSession = null,
  show,
  onSessionCount,
}: {
  root: string;
  visible: boolean;
  close: () => void;
  contact: string;
  contactName: string;
  document: string;
  documentPath: string;
  documentBusy: boolean;
  openDocument: (path: string) => Promise<unknown>;
  version: number;
  defaultAgent?: string;
  request: AgentTask | null;
  /** More of what is on screen, for MCP context.get. */
  view?: string;
  list?: string;
  draft?: string;
  selection?: string[];
  /** Contact names by slug, for readable activity. */
  names?: Record<string, string>;
  focusSession?: { session: string; id: number } | null;
  show?: (target: { kind: string; target: string }) => Promise<unknown>;
  onSessionCount: (count: number) => void;
}) {
  const [saved] = useState(() => preferences(root));
  const selectionKey = selection.join("\n");
  const sizing = useAgentPaneWidth(saved.width, visible);
  // Dialogs leave the Agent pane uncovered (styles: .with-agent .overlay), so the terminal stays
  // usable while, for example, a draft the Agent opened is being reviewed.
  useEffect(() => {
    const style = window.document.documentElement.style;
    style.setProperty("--agent-w", visible ? `${sizing.width + 1}px` : "0px");
    return () => style.setProperty("--agent-w", "0px");
  }, [visible, sizing.width]);
  const [sessions, setSessions] = useState<Session[]>([]),
    [selected, setSelected] = useState(saved.selected || "");
  const [agent, setAgent] = useState(saved.agent || defaultAgent || "codex"),
    [labels, setLabels] = useState<Record<string, string>>(saved.labels || {});
  const [available, setAvailable] = useState<
    Record<string, { available: boolean; program: string }>
  >({});
  const [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const [files, setFiles] = useState<string[]>([]),
    [filesLoaded, setFilesLoaded] = useState(false);
  const [panel, setPanel] = useState<"" | "sessions" | "files">(""),
    [fileQuery, setFileQuery] = useState("");
  const [copied, setCopied] = useState("");
  const contextQueue = useRef<Promise<unknown>>(Promise.resolve());
  const [rename, setRename] = useState(false),
    [newName, setNewName] = useState("");
  const terminal = useRef<Terminal | null>(null),
    launchLock = useRef(false),
    handled = useRef(0),
    copyTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const call = useCallback(
    <T,>(action: string, args: Record<string, unknown> = {}) =>
      invoke<T>("app_call", { root, action, args }),
    [root],
  );
  const applySessions = useCallback(
    (next: Session[]) => {
      setSessions(next);
      onSessionCount(next.filter((s) => s.status === "running").length);
      setSelected((current) =>
        next.some((s) => s.id === current)
          ? current
          : next.find((s) => s.status === "running")?.id || next[0]?.id || "",
      );
    },
    [onSessionCount],
  );
  const refresh = useCallback(
    async () => applySessions(await call<Session[]>("terminal.list")),
    [call, applySessions],
  );
  useEffect(() => {
    let disposed = false,
      timer: ReturnType<typeof setTimeout>;
    const load = async () => {
      try {
        const next = await call<Session[]>("terminal.list");
        if (!disposed) applySessions(next);
      } catch (e) {
        if (!disposed) setError(String(e));
      }
      if (!disposed) timer = setTimeout(() => void load(), 3000);
    };
    void load();
    return () => {
      disposed = true;
      clearTimeout(timer);
    };
  }, [call, applySessions]);
  useEffect(() => {
    if (!visible) return;
    let disposed = false;
    void call<typeof available>("terminal.detect")
      .then((d) => {
        if (!disposed) setAvailable(d);
      })
      .catch((e) => {
        if (!disposed) setError(String(e));
      });
    return () => {
      disposed = true;
    };
  }, [visible, call, version]);
  useEffect(() => {
    try {
      localStorage.setItem(
        `gc.agent:${root}`,
        JSON.stringify({ selected, agent, labels, width: sizing.preferred }),
      );
    } catch {
      /* Preferences are optional; terminal history is in SQLite. */
    }
  }, [root, selected, agent, labels, sizing.preferred]);
  useEffect(() => () => clearTimeout(copyTimer.current), []);
  useEffect(() => {
    // Serialize selection updates, including quick A → B → A navigation, so a new question
    // about "this person" resolves through MCP context.get to what is on screen.
    let disposed = false;
    contextQueue.current = contextQueue.current
      .catch(() => {})
      .then(() =>
        call("context.set", {
          contact: contact || null,
          document: document || null,
          view: view || null,
          list: list || null,
          draft: draft || null,
          selection: selectionKey ? selectionKey.split("\n") : [],
        }),
      );
    void contextQueue.current.catch((e) => {
      if (!disposed) setError(String(e));
    });
    return () => {
      disposed = true;
    };
  }, [call, contact, document, view, list, draft, selectionKey]);
  // Readable progress of the selected session, next to its raw terminal output.
  const [activity, setActivity] = useState<Activity[]>([]),
    [showActivity, setShowActivity] = useState(false);
  const selectedRef = useRef(selected);
  selectedRef.current = selected;
  useEffect(() => {
    setActivity([]);
    if (!selected) return;
    let disposed = false;
    void call<Activity[]>("agent.activity", { session: selected, limit: 60 })
      .then((rows) => {
        if (!disposed) setActivity(rows);
      })
      .catch(() => {});
    return () => {
      disposed = true;
    };
  }, [call, selected]);
  useEffect(() => {
    let disposed = false,
      stop: (() => void) | undefined;
    void listen<Activity & { root: string }>("agent-activity", ({ payload }) => {
      if (payload.root !== root) return;
      if (payload.session === selectedRef.current)
        setActivity((rows) => (rows.some((r) => r.id === payload.id) ? rows : [...rows, payload].slice(-60)));
      if (payload.tool === "task.report") void refresh().catch(() => {});
    }).then((unlisten) => (disposed ? unlisten() : (stop = unlisten)));
    return () => {
      disposed = true;
      stop?.();
    };
  }, [root, refresh]);
  // "Agent working" on a person page jumps to the session doing it.
  useEffect(() => {
    if (focusSession) {
      setSelected(focusSession.session);
      setShowActivity(true);
      setPanel("");
    }
  }, [focusSession]);
  useEffect(() => {
    if (!visible || panel !== "files") return;
    let disposed = false;
    setFilesLoaded(false);
    void call<string[]>("documents.list")
      .then((paths) => {
        if (!disposed) {
          setFiles(paths);
          setFilesLoaded(true);
        }
      })
      .catch((e) => {
        if (!disposed) {
          setError(String(e));
          setFilesLoaded(true);
        }
      });
    return () => {
      disposed = true;
    };
  }, [visible, panel, version, call]);
  const current = sessions.find((s) => s.id === selected);
  const running = sessions.filter((s) => s.status === "running").length;
  const latest = activity.at(-1);
  const latestProgress = [...activity].reverse().find((a) => a.tool === "task.report")?.target || "";
  const who = (slug: string) => names[slug] || slug;
  const describe = (a: Activity) => {
    const action = toolLabels[a.tool] || a.tool;
    const parts = [action];
    if (a.contact && a.tool !== "task.report") parts.push(who(a.contact));
    if (a.tool === "task.report") parts.push(a.detail);
    else if (a.detail) parts.push(`“${a.detail}”`);
    else if (a.target && !a.contact) parts.push(a.target.split("/").pop() || a.target);
    const text = parts.join(" · ");
    return a.ok ? text : t("{action} failed: {reason}", { action, reason: a.detail });
  };
  const openable = (a: Activity) =>
    !a.ok
      ? null
      : documentRoots.some((r) => a.target.startsWith(r))
        ? { kind: a.target.startsWith("drafts/") ? "draft" : "document", target: a.target }
        : a.contact
          ? { kind: "contact", target: a.contact }
          : null;
  const label = (s: Session) =>
    labels[s.id] ||
    `${names[s.agent] || s.agent} · ${new Date(s.created_at).toLocaleString(locale, { month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" })}`;
  const missing = available[agent]?.available === false;
  const copy = async (text: string, kind: string) => {
    try {
      await copyText(text);
      setCopied(kind);
      clearTimeout(copyTimer.current);
      copyTimer.current = setTimeout(() => setCopied(""), 2000);
    } catch (e) {
      setError(t("Couldn't copy: {error}", { error: String(e) }));
    }
  };
  const start = async (
    task: AgentTask["task"] = "",
    provider = agent,
    extra: Partial<AgentTask> = {},
  ) => {
    if (launchLock.current) return;
    launchLock.current = true;
    setBusy(true);
    setError("");
    try {
      await saveEditors();
      await contextQueue.current;
      const target = extra.contact ?? contact;
      const launch = () =>
        call<{ id: string }>("terminal.start", {
          agent: provider,
          task,
          contact: target || null,
          list: extra.list || null,
          note: extra.note || null,
          source: extra.source || null,
          external_id: extra.external_id || null,
          document: document || null,
          lang,
          cols: 100,
          rows: 28,
        });
      let result: { id: string };
      try {
        result = await launch();
      } catch (e) {
        // At the running-session limit, offer to end the oldest session that is finished or
        // waiting (its record stays readable), instead of failing.
        const running = (await call<Session[]>("terminal.list")).filter((s) => s.status === "running");
        if (!sessionLimitError.test(String(e)) || !running.length) throw e;
        const settled = running.filter((s) => s.task && s.task.status !== "working");
        const oldest = (settled.length ? settled : running).reduce((a, b) => (a.created_at <= b.created_at ? a : b));
        if (!(await confirm(t("{n} sessions are running. End the oldest, “{name}”, and start the new task? Its record is kept.", { n: running.length, name: label(oldest) }), { title: t("Sessions full") })))
          return;
        await call("terminal.stop", { id: oldest.id });
        result = await launch();
      }
      const who =
        task === "list"
          ? t("Outreach list")
          : task === "sync"
            ? t("Whole workspace")
            : task === "source_person"
              ? extra.label || extra.external_id || ""
            : target === contact && contactName
              ? contactName
              : target || t("Workspace");
      setLabels((all) => ({
        ...all,
        [result.id]: task ? `${taskNames[task]} · ${who}` : `${t("Chat")} · ${names[provider]}`,
      }));
      await refresh();
      setSelected(result.id);
      setPanel("");
    } catch (e) {
      setError(String(e));
    } finally {
      launchLock.current = false;
      setBusy(false);
    }
  };
  // Tasks asked for elsewhere in the app (person page, lists, today) start here.
  useEffect(() => {
    if (!request || request.id === handled.current) return;
    handled.current = request.id;
    void start(request.task, agent, request);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [request]);
  const chooseDocument = async (path: string) => {
    await openDocument(path);
    setPanel("");
  };
  const matchingFiles = files.filter((path) =>
    path.toLowerCase().includes(fileQuery.trim().toLowerCase()),
  );
  const toggle = (next: "sessions" | "files") => {
    setPanel(panel === next ? "" : next);
    setRename(false);
  };
  const blocked = busy || documentBusy || missing;
  return (
    <>
      {visible && (
        <div
          {...sizing.handle}
          className={`agent-resizer ${sizing.dragging ? "dragging" : ""}`}
        />
      )}
      {visible && sizing.dragging && (
        <div className="agent-resize-shield" aria-hidden="true" />
      )}
      <aside
        ref={sizing.dock}
        style={{ flexBasis: sizing.width }}
        aria-label={t("Agent workbench")}
        className="agent-dock"
        data-session={selected}
        hidden={!visible}
      >
        <header className="agent-dock-header">
          <div>
            <i className={`agent-dot ${running ? "on" : ""}`} aria-hidden="true" />
            <strong>Agent</strong>
            <span className="agent-workspace-name" title={current ? label(current) : root}>
              {current ? label(current) : t("No session yet")}
            </span>
          </div>
          <div>
            <button
              className="icon-button"
              aria-label={t("Sessions")}
              title={t("Sessions")}
              aria-expanded={panel === "sessions"}
              onClick={() => toggle("sessions")}
            >
              <Icon name="history" size={16} />
              {sessions.length > 1 && <span className="agent-badge">{sessions.length}</span>}
            </button>
            <button
              className="icon-button"
              aria-label={t("Workspace documents")}
              title={t("Workspace documents")}
              aria-expanded={panel === "files"}
              onClick={() => toggle("files")}
            >
              <Icon name="folder" size={16} />
            </button>
            <button
              className="icon-button"
              aria-label={t("New session")}
              title={t("New {agent} session", { agent: names[agent] })}
              disabled={blocked}
              onClick={() => void start()}
            >
              <Icon name="plus" size={16} />
            </button>
            <button
              className="icon-button"
              aria-label={t("Hide Agent workbench")}
              title={t("Hide · ⌘ / Ctrl+J; tasks keep running")}
              onClick={close}
            >
              <Icon name="close" size={16} />
            </button>
          </div>
        </header>
        {panel === "sessions" && (
          <section className="agent-menu" aria-label={t("Sessions")}>
            <div className="agent-menu-row">
              <label htmlFor="terminal-agent">{t("New sessions use")}</label>
              <select
                id="terminal-agent"
                value={agent}
                disabled={busy}
                onChange={(e) => setAgent(e.target.value)}
              >
                <option value="codex">Codex</option>
                <option value="claude">Claude Code</option>
              </select>
            </div>
            {sessions.length ? (
              <div className="agent-session-list">
                {sessions.map((s) => (
                  <button
                    key={s.id}
                    data-id={s.id}
                    aria-current={s.id === selected}
                    onClick={() => {
                      setSelected(s.id);
                      setRename(false);
                      setError("");
                      setPanel("");
                    }}
                  >
                    <i className={`agent-dot ${s.status === "running" ? "on" : ""}`} />
                    <span>
                      <b>{label(s)}</b>
                      <small>
                        {statuses[s.status] || s.status}
                        {s.task && (
                          <span className={`agent-task ${s.task.status}`}>
                            {taskStatuses[s.task.status] || s.task.status}
                          </span>
                        )}
                      </small>
                    </span>
                  </button>
                ))}
              </div>
            ) : (
              <p className="faint">{t("No sessions yet.")}</p>
            )}
            {current &&
              (rename ? (
                <form
                  className="agent-rename"
                  onSubmit={(e) => {
                    e.preventDefault();
                    const title = newName.trim();
                    if (title) {
                      setLabels((all) => ({ ...all, [current.id]: title }));
                      setRename(false);
                    }
                  }}
                >
                  <input
                    autoFocus
                    aria-label={t("Session name")}
                    maxLength={80}
                    value={newName}
                    onChange={(e) => setNewName(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Escape") {
                        e.stopPropagation();
                        setRename(false);
                      }
                    }}
                  />
                  <button type="submit" disabled={!newName.trim()}>
                    {t("Save name")}
                  </button>
                </form>
              ) : (
                <button
                  className="text-button"
                  aria-label={t("Rename current session")}
                  onClick={() => {
                    setNewName(label(current));
                    setRename(true);
                  }}
                >
                  {t("Rename current session")}
                </button>
              ))}
          </section>
        )}
        {panel === "files" && (
          <section className="agent-files" aria-label={t("Workspace document list")}>
            <input
              autoFocus
              aria-label={t("Search workspace documents")}
              placeholder={t("Search file name or path…")}
              value={fileQuery}
              onChange={(e) => setFileQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") {
                  e.stopPropagation();
                  setPanel("");
                }
              }}
            />
            <div className="agent-files-results">
              {!filesLoaded ? (
                <p>{t("Loading documents…")}</p>
              ) : matchingFiles.length ? (
                matchingFiles.map((path) => (
                  <button
                    key={path}
                    aria-label={path}
                    title={path}
                    disabled={documentBusy}
                    onClick={() => void chooseDocument(path)}
                  >
                    <Icon name="note" size={16} />
                    <span>
                      <b>{path.split("/").pop()}</b>
                      <small>{path}</small>
                    </span>
                    {documentPath === path && <Icon name="check" size={14} />}
                  </button>
                ))
              ) : (
                <p>
                  {fileQuery
                    ? t("No matching documents. Try other keywords.")
                    : t("You can ask the Agent to write a summary report in notes/.")}
                </p>
              )}
            </div>
          </section>
        )}
        <section className="agent-context" aria-label={t("Contact quick tasks")}>
          <span title={document || contactName}>
            {contactName ? (
              <>
                {t("Viewing")} <b>{contactName}</b>
              </>
            ) : (
              t("No contact selected · ask about the whole workspace")
            )}
          </span>
          <div className="agent-quick-tasks">
            <button
              disabled={!contact || blocked}
              title={t("New session: read this person's memory, dossier, activity and source data, then summarize the situation and next step")}
              onClick={() => void start("overview")}
            >
              <Icon name="spark" size={14} />
              {t("Understand")}
            </button>
            <button
              disabled={!contact || blocked}
              title={t("New session: read the intent of their latest message and draft a reply for review")}
              onClick={() => void start("reply")}
            >
              <Icon name="mail" size={14} />
              {t("Draft reply")}
            </button>
            <button
              disabled={blocked}
              title={t("New session: sync connected mailboxes and data sources, then summarize what's new")}
              onClick={() => void start("sync")}
            >
              <Icon name="refresh" size={14} />
              {t("Sync & summarize")}
            </button>
          </div>
        </section>
        {current && (current.task || activity.length > 0) && (
          <section className="agent-activity" aria-label={t("Agent progress")}>
            <button
              className="agent-activity-head"
              aria-expanded={showActivity}
              onClick={() => setShowActivity(!showActivity)}
            >
              {current.task && (
                <span className={`agent-task ${current.task.status}`}>
                  {taskStatuses[current.task.status] || current.task.status}
                  {current.task.status === "working" && latestProgress && ` ${latestProgress}`}
                </span>
              )}
              <span className="grow">
                {current.task?.summary || (latest ? describe(latest) : "")}
              </span>
              <small>{t("{n} steps", { n: activity.length })}</small>
              <Icon name={showActivity ? "up" : "down"} size={13} />
            </button>
            {showActivity && (
              <ol className="agent-activity-list">
                {[...activity].reverse().map((a) => {
                  const target = openable(a);
                  return (
                    <li key={a.id} className={a.ok ? "" : "err"}>
                      <button
                        disabled={!target || !show || documentBusy}
                        title={a.ok ? target?.target || "" : a.detail}
                        onClick={() => target && show && void show(target)}
                      >
                        <span className="grow">{describe(a)}</span>
                        <time>
                          {new Date(a.at).toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" })}
                        </time>
                      </button>
                    </li>
                  );
                })}
              </ol>
            )}
          </section>
        )}
        {missing && (
          <p className="agent-inline-notice">
            {t("{agent} CLI not found. Install it and sign in first, switch assistants in the session list, or set the path in Settings → Advanced.", { agent: names[agent] })}
          </p>
        )}
        {error && (
          <div role="alert" className="agent-inline-error">
            {error}
            <button onClick={() => setError("")}>{t("Close")}</button>
          </div>
        )}
        <div className="agent-work-area">
          <section className="agent-conversation" aria-label={t("Session and execution log")}>
            {current ? (
              <>
                {current.status !== "running" && (
                  <section
                    className="agent-history-notice"
                    aria-label={t("Session ended notice")}
                  >
                    <div>
                      <strong>
                        {statuses[current.status] || t("Session ended")} · {t("Read-only log")}
                      </strong>
                      <p>{t("You can't type or approve permissions here. Start a new session to ask again.")}</p>
                      {current.error && <small>{current.error}</small>}
                    </div>
                    <button
                      className="primary"
                      disabled={
                        busy ||
                        documentBusy ||
                        available[current.agent]?.available === false
                      }
                      onClick={() => void start("", current.agent)}
                    >
                      {t("Start an interactive session")}
                    </button>
                  </section>
                )}
                <TerminalView
                  key={current.id}
                  root={root}
                  session={current}
                  visible={visible}
                  terminal={terminal}
                  onError={setError}
                />
                <footer className="agent-terminal-footer">
                  <span>
                    <i
                      className={
                        current.status === "running" ? "activity-dot" : ""
                      }
                    />
                    {statuses[current.status] || current.status}
                  </span>
                  <button
                    onClick={() => {
                      const term = terminal.current;
                      if (term)
                        void copy(
                          Array.from(
                            { length: term.buffer.active.length },
                            (_, i) =>
                              term.buffer.active
                                .getLine(i)
                                ?.translateToString(true) || "",
                          ).join("\n"),
                          "screen",
                        );
                    }}
                  >
                    {copied === "screen" ? t("Copied") : t("Copy log")}
                  </button>
                  {current.status === "running" && (
                    <button
                      disabled={busy}
                      onClick={() =>
                        void (async () => {
                          if (
                            !(await confirm(
                              t("End “{name}”? Running tasks will stop; the output log is kept.", { name: label(current) }),
                              { title: t("End session") },
                            ))
                          )
                            return;
                          setBusy(true);
                          try {
                            await call("terminal.stop", { id: current.id });
                            await refresh();
                          } catch (e) {
                            setError(String(e));
                          } finally {
                            setBusy(false);
                          }
                        })()
                      }
                    >
                      {t("End session")}
                    </button>
                  )}
                </footer>
              </>
            ) : (
              <div className="agent-welcome">
                <span className="agent-welcome-icon">
                  <Icon name="terminal" size={26} />
                </span>
                <h2>{t("What can the Agent help with?")}</h2>
                <p>
                  {t("Pick a quick task above, or start a chat and ask directly. The Agent reads and writes contact data and documents, showing every step here; drafts are only sent after you approve them.")}
                </p>
                <div className="agent-start">
                  <select
                    aria-label={t("Terminal assistant")}
                    value={agent}
                    disabled={busy}
                    onChange={(e) => setAgent(e.target.value)}
                  >
                    <option value="codex">Codex</option>
                    <option value="claude">Claude Code</option>
                  </select>
                  <button
                    className="primary"
                    disabled={blocked}
                    onClick={() => void start()}
                  >
                    {busy ? t("Starting…") : t("Start chat")}
                  </button>
                </div>
                <div className="agent-examples">
                  <span>{t("“Where is this person stuck lately?”")}</span>
                  <span>{t("“Add the key points of this call to their record”")}</span>
                  <span>{t("“Who needs a follow-up this week?”")}</span>
                </div>
                <small>{t("Uses the Codex or Claude Code you're signed in to on this machine")}</small>
              </div>
            )}
          </section>
        </div>
        <div className="agent-dock-hint">
          {t("⌘ / Ctrl+J opens it anytime · sessions keep running when hidden")}
        </div>
      </aside>
    </>
  );
}

export function WorkspaceDocument({
  root,
  path,
  busy,
  close,
  open,
  fail,
}: {
  root: string;
  path: string;
  busy: boolean;
  close: () => Promise<unknown>;
  open: (path: string) => Promise<unknown>;
  fail: (message: string) => void;
}) {
  const [dirty, setDirty] = useState(false),
    [ready, setReady] = useState(false),
    [saving, setSaving] = useState(false);
  return (
    <section className="agent-document" aria-label={t("Workspace document editor")}>
      <header>
        <div>
          <strong title={path}>{path.split("/").pop()}</strong>
          <small title={path}>{path}</small>
        </div>
        <span role="status">
          {saving
            ? t("Saving…")
            : !ready
              ? t("Opening…")
              : dirty
                ? t("Unsaved changes")
                : t("Saved")}
        </span>
        <button
          disabled={!ready || busy || saving}
          onClick={() =>
            void (async () => {
              setSaving(true);
              try {
                await saveEditors();
              } catch (e) {
                fail(String(e));
              } finally {
                setSaving(false);
              }
            })()
          }
        >
          {t("Save document")}
        </button>
        <button
          className="icon-button"
          aria-label={t("Save and close workspace document")}
          disabled={busy || saving}
          onClick={() => void close()}
        >
          <Icon name="close" size={16} />
        </button>
      </header>
      <div className="agent-document-editor">
        <TableEditor
          root={root}
          path={path}
          onError={fail}
          onState={(state) => setDirty(state.dirty)}
          onConnection={(connection) => setReady(!!connection)}
          onOpen={open}
        />
      </div>
    </section>
  );
}
function TerminalView({
  root,
  session,
  visible,
  terminal,
  onError,
}: {
  root: string;
  session: Session;
  visible: boolean;
  terminal: React.MutableRefObject<Terminal | null>;
  onError: (e: string) => void;
}) {
  const element = useRef<HTMLDivElement>(null);
  const search = useRef<SearchAddon | null>(null);
  const searchInput = useRef<HTMLInputElement>(null);
  const [showSearch, setShowSearch] = useState(false);
  const [query, setQuery] = useState("");
  const [found, setFound] = useState(false);
  const find = (backward = false) => {
    if (!query) {
      search.current?.clearDecorations();
      terminal.current?.clearSelection();
      setFound(false);
      return;
    }
    setFound(
      !!(backward
        ? search.current?.findPrevious(query)
        : search.current?.findNext(query)),
    );
  };
  const active = useRef(visible);
  active.current = visible;
  const status = useRef(session.status);
  status.current = session.status;
  const fitRef = useRef<() => void>(() => {});
  const scheme = useScheme();
  useEffect(() => {
    if (terminal.current) terminal.current.options.theme = terminalTheme(scheme);
  }, [scheme]);
  useEffect(() => {
    const term = new Terminal({
      cols: session.cols,
      rows: session.rows,
      scrollback: 20000,
      fontFamily: "'JetBrains Mono', Menlo, Monaco, 'SFMono-Regular', monospace",
      fontSize: 12,
      lineHeight: 1.3,
      cursorBlink: status.current === "running",
      disableStdin: status.current !== "running",
      convertEol: false,
      allowProposedApi: false,
      theme: terminalTheme(currentScheme()),
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    const searchAddon = new SearchAddon();
    term.loadAddon(searchAddon);
    search.current = searchAddon;
    term.open(element.current!);
    terminal.current = term;
    // Never allow remote terminal escape sequences to write to the clipboard.
    const clipboard = term.parser.registerOscHandler(52, () => true);
    let disposed = false,
      after = 0,
      timer: ReturnType<typeof setTimeout>,
      writeQueue = Promise.resolve(),
      resizeQueue = Promise.resolve(),
      resized = "";
    const call = <T,>(action: string, args: Record<string, unknown> = {}) =>
      invoke<T>("app_call", {
        root,
        action,
        args: { id: session.id, ...args },
      });
    const input = term.onData((text) => {
      if (status.current !== "running") return;
      writeQueue = writeQueue
        .then(async () => {
          // Bound each IPC write while preserving terminal input order.
          const points = Array.from(text);
          for (let offset = 0; offset < points.length; offset += 8000)
            await call("terminal.write", {
              text: points.slice(offset, offset + 8000).join(""),
            });
        })
        .catch((e) => {
          if (!disposed) onError(String(e));
        });
    });
    // Let the app shortcut work even with focus inside the terminal.
    term.attachCustomKeyEventHandler((e) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f") {
        if (e.type === "keydown") {
          e.preventDefault();
          setShowSearch(true);
        }
        return false;
      }
      return !((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "j");
    });
    const fitNow = () => {
      if (!active.current || disposed || !element.current?.clientWidth) return;
      fit.fit();
      const dimensions = `${term.cols}:${term.rows}`;
      if (status.current === "running" && dimensions !== resized) {
        resized = dimensions;
        resizeQueue = resizeQueue
          .then(async () => {
            if (
              disposed ||
              status.current !== "running" ||
              dimensions !== resized
            )
              return;
            await call("terminal.resize", { cols: term.cols, rows: term.rows });
          })
          .catch((e) => {
            if (!disposed && status.current === "running") onError(String(e));
          });
      }
    };
    fitRef.current = fitNow;
    const observer = new ResizeObserver(fitNow);
    observer.observe(element.current!);
    const poll = async () => {
      if (disposed) return;
      let delay = 500;
      try {
        if (active.current) {
          const output = await call<{
            chunks: { seq: number; bytes: number[] }[];
            status: string;
            error: string;
          }>("terminal.read", { after });
          if (disposed) return;
          for (const chunk of output.chunks) {
            if (chunk.seq <= after) continue;
            await new Promise<void>((resolve) =>
              term.write(new Uint8Array(chunk.bytes), resolve),
            );
            after = chunk.seq;
          }
          if (output.chunks.length === 32) delay = 0;
          if (output.status !== "running" && output.chunks.length < 32) {
            term.options.disableStdin = true;
            fitNow();
            return;
          }
          fitNow();
        }
      } catch (e) {
        if (!disposed) onError(String(e));
        delay = 2000;
      }
      if (!disposed) timer = setTimeout(() => void poll(), delay);
    };
    void poll();
    if (active.current && status.current === "running") term.focus();
    return () => {
      disposed = true;
      clearTimeout(timer);
      observer.disconnect();
      input.dispose();
      clipboard.dispose();
      term.dispose();
      search.current = null;
      terminal.current = null;
    };
  }, [root, session.id]);
  useEffect(() => {
    if (visible) {
      requestAnimationFrame(() => {
        fitRef.current();
        if (status.current === "running") terminal.current?.focus();
      });
    }
  }, [visible]);
  useEffect(() => {
    const term = terminal.current;
    if (!term) return;
    term.options.disableStdin = session.status !== "running";
    term.options.cursorBlink = session.status === "running";
    if (session.status !== "running") term.blur();
  }, [session.status]);
  useEffect(() => {
    if (showSearch) searchInput.current?.focus();
    else {
      search.current?.clearDecorations();
      terminal.current?.clearSelection();
    }
  }, [showSearch]);
  useEffect(() => {
    if (showSearch) find();
  }, [query, showSearch]);
  return (
    <>
      <div className="agent-terminal-toolbar">
        <span>
          {session.status === "running"
            ? t("Terminal")
            : t("History · read-only")}
        </span>
        {session.status === "running" && (
          <button
            aria-label={t("Send Enter to the current terminal")}
            title={t("Sends one Enter key press; you can also click the terminal and type")}
            onClick={() => {
              if (status.current !== "running") return;
              terminal.current?.input("\r", true);
              terminal.current?.focus();
            }}
          >
            {t("Enter ↵")}
          </button>
        )}
        <button
          title="⌘ / Ctrl+F"
          aria-expanded={showSearch}
          onClick={() => setShowSearch(!showSearch)}
        >
          <Icon name="search" size={14} />
          {t("Find in log")}
        </button>
        <button
          onClick={() => {
            terminal.current?.scrollToBottom();
            terminal.current?.focus();
          }}
        >
          {t("Jump to latest")}
        </button>
      </div>
      {showSearch && (
        <div className="agent-search">
          <input
            ref={searchInput}
            aria-label={t("Search terminal log")}
            value={query}
            placeholder={t("Search this session's output…")}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                find(e.shiftKey);
              }
              if (e.key === "Escape") {
                e.preventDefault();
                e.stopPropagation();
                setShowSearch(false);
                terminal.current?.focus();
              }
            }}
          />
          <span role="status" className="agent-search-result">
            {query ? (found ? t("Found") : t("Not found")) : ""}
          </span>
          <button disabled={!query} onClick={() => find(true)}>
            {t("Previous match")}
          </button>
          <button disabled={!query} onClick={() => find()}>
            {t("Next match")}
          </button>
          <button
            className="icon-button"
            aria-label={t("Close log search")}
            onClick={() => setShowSearch(false)}
          >
            <Icon name="close" size={14} />
          </button>
        </div>
      )}
      <div
        ref={element}
        className={`agent-terminal ${session.status === "running" ? "" : "is-history"}`}
        title={
          session.status === "running"
            ? t("Click here to type; ↑ ↓ to choose, Enter to confirm")
            : t("Read-only history; you can't type or approve permissions")
        }
        aria-label={t("{agent} terminal", { agent: names[session.agent] || session.agent })}
      />
    </>
  );
}

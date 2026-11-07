import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open, confirm, save } from "@tauri-apps/plugin-dialog";
import { TableEditor, saveEditors } from "../kits/TableEditor";
import { Icon, Logo, Modal, Kbd, channelKinds, channelNames } from "../components/ui";
import { DraftReview } from "../components/DraftReview";
import { IntakeForm } from "../components/IntakeForm";
import { AgentDock, WorkspaceDocument, type AgentTask } from "../components/AgentDock";
import { TodayPage } from "../pages/TodayPage";
import { PeoplePage, type PeopleView } from "../pages/PeoplePage";
import { PersonPage } from "../pages/PersonPage";
import { MailPage } from "../pages/MailPage";
import { SettingsPage } from "../pages/SettingsPage";
import { SourcesPage } from "../pages/SourcesPage";
import { ListsPage } from "../pages/ListsPage";
import { HelpPage } from "../pages/HelpPage";
import { Sidebar } from "./Sidebar";
import { ChatImportDialog, ImportChooser, ImportDialog, TemplatePicker, type ImportKind, type Template } from "../components/WorkspaceDialogs";
import { MergeDialog } from "../components/MergeDialog";
import { CommandPalette, type Command } from "../components/CommandPalette";
import { setPrefs, currentScheme } from "./prefs";
import { copyText } from "./clipboard";
import { t } from "./i18n";
import { awaitsReply, matches, needsFollowUp, setWorkspace, type Row, type V, type WorkspaceInfo } from "./domain";

export type View = "today" | "people" | "person" | "table" | "lists" | "mail" | "sources" | "settings" | "help";

const agentOpenKey = "gc.agentOpen";

export function App() {
  const [root, setRoot] = useState(""),
    [view, setView] = useState<View>("today"),
    [rows, setRows] = useState<Row[]>([]);
  const [selected, setSelected] = useState(""),
    [detail, setDetail] = useState<V | null>(null),
    [query, setQuery] = useState(""),
    [stage, setStage] = useState(""),
    [peopleView, setPeopleView] = useState<PeopleView>("all");
  const [error, setError] = useState(""),
    [busy, setBusy] = useState(""),
    [notice, setNotice] = useState(""),
    [version, setVersion] = useState(0);
  const [config, setConfig] = useState<V>({}),
    [agents, setAgents] = useState<V>({}),
    [accounts, setAccounts] = useState<V>({ imap: [], smtp: [] });
  const [tableOpen, setTableOpen] = useState(false),
    [notePath, setNotePath] = useState(""),
    [dirty, setDirty] = useState(false),
    [ready, setReady] = useState(false);
  const [draft, setDraft] = useState<V | null>(null),
    [draftChannel, setDraftChannel] = useState("all"),
    [smtp, setSmtp] = useState("");
  const [allDrafts, setAllDrafts] = useState<V[]>([]),
    [sources, setSources] = useState<V[]>([]),
    [facts, setFacts] = useState<V[]>([]),
    [fields, setFields] = useState<V[]>([]);
  const [mail, setMail] = useState<V[]>([]),
    [newContact, setNewContact] = useState(false),
    [intake, setIntake] = useState(false),
    [channelForm, setChannelForm] = useState(false),
    [editContact, setEditContact] = useState(false),
    [merging, setMerging] = useState(false),
    [palette, setPalette] = useState(false),
    [listFocus, setListFocus] = useState("");
  const [tab, setTab] = useState("overview");
  const [templates, setTemplates] = useState<Template[]>([]),
    [creating, setCreating] = useState(false),
    [importPreview, setImportPreview] = useState<V | null>(null),
    [importChooser, setImportChooser] = useState(false),
    [chatImport, setChatImport] = useState<{ kind: string; path: string; preview: V } | null>(null),
    [, setWorkspaceInfo] = useState<WorkspaceInfo | null>(null);
  const [agentOpen, setAgentOpen] = useState(() => localStorage.getItem(agentOpenKey) !== "0");
  const [agentDocument, setAgentDocument] = useState("");
  const [agentSessions, setAgentSessions] = useState(0);
  const [agentTask, setAgentTask] = useState<AgentTask | null>(null);
  // What the Agent can see beyond the current person (MCP context.get), and what it reports back.
  const [agentRecent, setAgentRecent] = useState<V[]>([]);
  // Confirmations fade out on their own; errors stay until dismissed.
  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => setNotice(""), 6000);
    return () => clearTimeout(timer);
  }, [notice]);
  // Known email addresses of the open draft's person, for the recipient check.
  const [draftAddresses, setDraftAddresses] = useState<string[] | null>(null);
  const draftContact = draft?.metadata?.contact_id || "";
  useEffect(() => {
    setDraftAddresses(null);
    if (!root || !draftContact) return;
    let alive = true;
    void invoke<string[]>("app_call", { root, action: "contact.addresses", args: { slug: draftContact } })
      .then((a) => alive && setDraftAddresses(a))
      .catch(() => alive && setDraftAddresses(null));
    return () => {
      alive = false;
    };
  }, [root, draftContact, version]);
  const [peopleSelection, setPeopleSelection] = useState<string[]>([]),
    [currentList, setCurrentList] = useState("");
  const [agentWork, setAgentWork] = useState<Record<string, { session: string; at: number }>>({}),
    [agentSuggestion, setAgentSuggestion] = useState<{ kind: string; target: string } | null>(null),
    [agentFocus, setAgentFocus] = useState<{ session: string; id: number } | null>(null);
  const agentEvents = useRef<{ open: (data: V) => void; activity: (data: V) => void }>({ open: () => {}, activity: () => {} });
  const workspaceRoot = useRef(root);
  workspaceRoot.current = root;
  const saveCurrent = useRef<() => Promise<unknown>>(() => saveEditors());
  const draftCache = useRef(new Map<string, V>());
  const draftRef = useRef(draft);
  draftRef.current = draft;
  const draftKey = (d: V) => `${root}:${d.metadata.contact_id}:${d.path || "new"}`;
  const closeDraft = () => {
    if (draft?._edited) draftCache.current.set(draftKey(draft), draft);
    setDraft(null);
  };
  const openDraft = (d: V) => setDraft(structuredClone(draftCache.current.get(draftKey(d)) || d));
  const changeDraft = (d: V) => {
    const changed = { ...d, _edited: true };
    draftCache.current.set(draftKey(changed), changed);
    setDraft(changed);
  };
  const rememberSaved = (saved: V) => {
    if (draft) draftCache.current.delete(draftKey(draft));
    draftCache.current.delete(draftKey(saved));
    setDraft(saved);
  };
  const fail = useCallback((message: string) => setError(message), []);
  const call = useCallback(<T,>(action: string, args: V = {}) => invoke<T>("app_call", { root, action, args }), [root]);
  // Every analysis runs in the right-hand Agent, which reads and writes through MCP.
  const askAgent = (task: AgentTask["task"], extra: Partial<AgentTask> = {}) => {
    setAgentOpen(true);
    setAgentTask({ task, contact: selected, ...extra, id: Date.now() });
  };
  const refresh = useCallback(() => setVersion((v) => v + 1), []);
  const followupDays = config.followupDays || 7;
  const stale = (r: Row) => needsFollowUp(r, followupDays);

  useEffect(() => {
    try {
      localStorage.setItem(agentOpenKey, agentOpen ? "1" : "0");
    } catch {
      /* optional */
    }
  }, [agentOpen]);
  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => setNotice(""), 5000);
    return () => clearTimeout(timer);
  }, [notice]);
  const working = useRef(false);
  const work = async (label: string, fn: () => Promise<unknown>) => {
    if (working.current) return;
    working.current = true;
    setBusy(label);
    setError("");
    try {
      await fn();
    } catch (e) {
      setError(String(e));
    } finally {
      working.current = false;
      setBusy("");
    }
  };
  useEffect(() => {
    void call<V>("bootstrap")
      .then(async (data) => {
        setConfig(data.config);
        setAgents(data.agents);
        setTemplates(data.templates || []);
        if (data.lastWorkspace) {
          const info = await invoke<V>("workspace_open", { path: data.lastWorkspace, create: false });
          setRoot(info.root);
        }
      })
      .catch((e) => fail(String(e)));
  }, []);
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout>;
    let disposed = false;
    const stops: (() => void)[] = [];
    const on = <T,>(name: string, fn: (data: T) => void) =>
      void listen<T>(name, ({ payload }) => fn(payload)).then((stop) => (disposed ? stop() : stops.push(stop)));
    on("workspace-changed", () => {
      clearTimeout(timer);
      timer = setTimeout(refresh, 500);
    });
    on<V>("background-error", (data) => fail(data.message));
    on<V>("agent-open", (data) => agentEvents.current.open(data));
    on<V>("agent-activity", (data) => agentEvents.current.activity(data));
    const key = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "j") {
        e.preventDefault();
        setAgentOpen((open) => !open);
      }
      // ⌘K opens search everywhere except inside the terminal, where it belongs to the shell.
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k" && !(e.target as HTMLElement)?.closest?.(".xterm")) {
        e.preventDefault();
        if (workspaceRoot.current) setPalette((open) => !open);
      }
      if ((e.metaKey || e.ctrlKey) && e.key === "s") {
        if ((e.target as HTMLElement)?.closest?.(".xterm")) return;
        e.preventDefault();
        void saveCurrent.current().catch((e) => fail(String(e)));
      }
    };
    const toggleAgent = () => setAgentOpen((open) => !open);
    window.addEventListener("globalcontact-agent-toggle", toggleAgent);
    window.addEventListener("keydown", key);
    void getCurrentWindow()
      .onCloseRequested(async (e) => {
        e.preventDefault();
        try {
          await saveEditors();
          if (
            (draftRef.current?._edited || draftCache.current.size) &&
            !(await confirm(t("You have unsaved draft edits. Quitting will discard them. Quit anyway?"), { title: t("Unsaved draft"), kind: "warning" }))
          )
            return;
          const terminals = await invoke<number>("app_call", { root: workspaceRoot.current, action: "terminal.running", args: {} });
          if (terminals > 0 && !(await confirm(t("Agent terminals are still running. Quitting will end these tasks; terminal logs are kept."), { title: t("Quit OutreachDex") })))
            return;
          await getCurrentWindow().destroy();
        } catch (error) {
          fail(String(error));
        }
      })
      .then((stop) => (disposed ? stop() : stops.push(stop)))
      .catch(() => {});
    return () => {
      disposed = true;
      clearTimeout(timer);
      stops.forEach((s) => s());
      window.removeEventListener("globalcontact-agent-toggle", toggleAgent);
      window.removeEventListener("keydown", key);
    };
  }, [fail, refresh]);
  useEffect(() => {
    if (!root) return;
    let alive = true;
    void call<Row[]>("contacts.list")
      .then((data) => alive && setRows(data))
      .catch((e) => fail(String(e)));
    return () => {
      alive = false;
    };
  }, [root, version, call, fail]);
  useEffect(() => {
    if (!root) {
      setWorkspace(null);
      setWorkspaceInfo(null);
      return;
    }
    let alive = true;
    void call<WorkspaceInfo>("workspace.config")
      .then((info) => {
        if (!alive) return;
        // Domain helpers read the module store; the state update re-renders with it.
        setWorkspace(info);
        setWorkspaceInfo(info);
      })
      .catch((e) => fail(String(e)));
    return () => {
      alive = false;
    };
  }, [root, version, call, fail]);
  useEffect(() => {
    if (!root || !selected) {
      setDetail(null);
      return;
    }
    let alive = true;
    void call<V>("contact.get", { slug: selected })
      .then((data) => alive && setDetail(data))
      .catch((e) => fail(String(e)));
    void call<V[]>("source.person", { slug: selected })
      .then((data) => alive && setFacts(data))
      .catch(() => alive && setFacts([]));
    return () => {
      alive = false;
    };
  }, [root, selected, version, call, fail]);
  useEffect(() => {
    if (!root) return;
    let alive = true;
    void call<V>("mail.accounts")
      .then((data) => alive && setAccounts(data))
      .catch((e) => fail(String(e)));
    void call<V[]>("contact.fields")
      .then((data) => alive && setFields(data))
      .catch(() => {});
    void call<V[]>("source.list")
      .then((data) => alive && setSources(data))
      .catch(() => alive && setSources([]));
    if (view === "today") {
      void call<V[]>("draft.list")
        .then((data) => alive && setAllDrafts(data))
        .catch((e) => fail(String(e)));
      void call<V[]>("agent.recent", { limit: 6 })
        .then((data) => alive && setAgentRecent(data))
        .catch(() => alive && setAgentRecent([]));
    }
    if (view === "mail")
      void call<V[]>("mail.messages")
        .then((data) => alive && setMail(data))
        .catch((e) => fail(String(e)));
    return () => {
      alive = false;
    };
  }, [root, view, version, call, fail]);

  const choose = (create: boolean, template?: string) => {
    if (create && !template) return setCreating(true);
    return work(create ? t("New workspace…") : t("Open workspace…"), async () => {
      const path = await open({ directory: true, multiple: false, title: create ? t("Choose a folder for the new workspace") : t("Choose a workspace folder") });
      if (typeof path !== "string") return;
      await saveEditors();
      setTableOpen(false);
      setNotePath("");
      setAgentDocument("");
      setSelected("");
      setDraft(null);
      setDetail(null);
      const info = await invoke<V>("workspace_open", { path, create, template: template || null });
      setCreating(false);
      setRoot(info.root);
      setView("today");
      refresh();
    });
  };
  const startImport = () =>
    work(t("Reading file…"), async () => {
      const path = await open({
        multiple: false,
        title: t("Choose a CSV / TSV file to import"),
        filters: [{ name: "CSV / TSV", extensions: ["csv", "tsv", "txt"] }],
      });
      if (typeof path !== "string") return;
      setImportPreview({ ...(await call<V>("import.preview", { path })), path });
    });
  const startChatImport = (kind: "whatsapp" | "linkedin") =>
    work(t("Reading file…"), async () => {
      const path = await open({
        multiple: false,
        title: kind === "whatsapp" ? t("Choose a WhatsApp chat export") : t("Choose your LinkedIn data archive"),
        filters: [kind === "whatsapp" ? { name: "WhatsApp", extensions: ["zip", "txt"] } : { name: "LinkedIn", extensions: ["zip", "csv"] }],
      });
      if (typeof path !== "string") return;
      setChatImport({ kind, path, preview: await call<V>("chat.preview", { kind, path }) });
    });
  const chooseImport = (kind: ImportKind) => {
    setImportChooser(false);
    if (kind === "contacts") void startImport();
    else void startChatImport(kind);
  };
  const navigate = (next: View) =>
    work(t("Saving…"), async () => {
      await saveEditors();
      setNotePath("");
      setAgentDocument("");
      setView(next);
      if (next === "table") setTableOpen(true);
    });
  const openPerson = (slug: string, nextTab = "") =>
    work(t("Opening contact…"), async () => {
      await saveEditors();
      setAgentDocument("");
      if (slug !== selected) {
        setNotePath("");
        setDetail(null);
        setTab(nextTab || "overview");
      } else if (nextTab) setTab(nextTab);
      // The dossier tab without an open document is empty; come back to the overview.
      else if (tab === "dossier" && !notePath) setTab("overview");
      closeDraft();
      setSelected(slug);
      setView("person");
    });
  const showNote = (path: string) =>
    work(t("Opening document…"), async () => {
      await saveEditors();
      setAgentDocument("");
      if (path === "contacts.ktable") {
        setTableOpen(true);
        setView("table");
        setNotePath("");
      } else if (view === "mail") {
        setNotePath(path);
      } else {
        setNotePath(path);
        setView("person");
        setTab("dossier");
      }
    });
  const openExternalOrNote = (source: string, fallback: () => unknown) => {
    if (/^https?:\/\//i.test(source))
      void work(t("Opening source…"), async () => {
        if (await confirm(t("Open external source: {source}?", { source }), { title: t("View source") })) await invoke("open_external", { root, url: source });
      });
    else void fallback();
  };
  const openAgentDocument = (path: string) =>
    work(t("Opening workspace document…"), async () => {
      await saveEditors();
      if (path === "contacts.ktable") {
        setAgentDocument("");
        setNotePath("");
        setTableOpen(true);
        setView("table");
        return;
      }
      setNotePath("");
      setAgentDocument(path);
      setView("person");
    });
  const update = (values: V) =>
    work(t("Saving contact…"), async () => {
      await call("contact.update", { slug: selected, values });
      refresh();
    });
  const saveDraft = () =>
    work(t("Saving draft…"), async () => {
      const result = await call<V>("draft.save", { ...draft!.metadata, path: draft!.path, body: draft!.body });
      rememberSaved(result);
      setNotice(t("Draft saved"));
      refresh();
    });
  saveCurrent.current = () =>
    draftRef.current && !["sent", "sending", "uncertain"].includes(draftRef.current.metadata.status) ? saveDraft() : saveEditors();
  const statusDraft = (status: string) =>
    work(t("Updating draft…"), async () => {
      await saveEditors();
      await call("draft.status", { path: draft!.path, status });
      const list = await call<V[]>("draft.list", { slug: draft!.metadata.contact_id });
      const saved = list.find((d) => d.path === draft!.path);
      if (saved) rememberSaved(saved);
      refresh();
    });

  // Previous / next person in the current people view, for J / K on the person page.
  const sequence = rows.filter(
    (r) =>
      matches(r, query) &&
      (!stage || r.values.Stage === stage) &&
      (peopleView === "all" || (peopleView === "reply" ? awaitsReply(r) : stale(r))),
  );
  const position = sequence.findIndex((r) => r.values.Slug === selected);
  const step = (delta: number) => {
    const next = sequence[position + delta];
    if (next) void openPerson(next.values.Slug, tab === "dossier" ? "overview" : tab);
  };
  const stepRef = useRef(step);
  stepRef.current = step;
  useEffect(() => {
    if (view !== "person") return;
    const key = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      if (e.metaKey || e.ctrlKey || e.altKey || target.closest("input, textarea, select, [contenteditable], .xterm, iframe")) return;
      if (draftRef.current || document.querySelector(".overlay")) return;
      if (e.key === "j") stepRef.current(1);
      if (e.key === "k") stepRef.current(-1);
      if (e.key === "Escape") setView("people");
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [view]);

  const contactName = rows.find((r) => r.values.Slug === selected)?.values.Name || "";
  // Shows what the Agent asked for (ui.open) beside the terminal, keeping the keyboard there.
  const showAgentResult = async ({ kind, target }: { kind: string; target: string }) => {
    setAgentSuggestion(null);
    const active = document.activeElement;
    const keep = active instanceof HTMLElement && active.closest(".agent-dock") ? active : null;
    if (kind === "contact") await openPerson(target);
    else if (kind === "document") await openAgentDocument(target);
    else if (kind === "list") {
      setListFocus(target);
      await navigate("lists");
    } else if (kind === "draft") {
      const found = (await call<V[]>("draft.list")).find((d) => d.path === target);
      if (!found) return fail(t("The draft the Agent wanted to open no longer exists."));
      if (found.metadata.contact_id !== selected) await openPerson(found.metadata.contact_id, "drafts");
      openDraft(found);
    }
    if (keep) for (const delay of [0, 150, 500]) setTimeout(() => keep.isConnected && keep.focus(), delay);
  };
  agentEvents.current = {
    open: (data) => {
      if (data.root !== root) return;
      // Never replace something the user is in the middle of editing; offer it instead.
      if (draft?._edited || dirty || busy) setAgentSuggestion({ kind: data.kind, target: data.target });
      else void showAgentResult(data as { kind: string; target: string });
    },
    activity: (data) => {
      if (data.root !== root) return;
      if (data.contact) setAgentWork((work) => ({ ...work, [data.contact]: { session: data.session, at: data.at } }));
      if (data.tool === "task.report" && data.ok && data.status !== "working") {
        // Finished or waiting for the user: this person is no longer being worked on.
        setAgentWork((work) => Object.fromEntries(Object.entries(work).filter(([, w]) => w.session !== data.session)));
        if (!agentOpen) setNotice(t("Agent: {summary}", { summary: data.detail }));
      }
    },
  };
  const personWork = agentWork[selected];
  const agentWorking = personWork && Date.now() - personWork.at < 5 * 60 * 1000 ? personWork : null;
  const pendingToday =
    rows.filter(awaitsReply).length +
    rows.filter(stale).length +
    allDrafts.filter((d) => ["draft", "approved", "uncertain"].includes(d.metadata.status)).length;
  const titles: Record<View, string> = {
    today: t("Today"),
    people: t("People"),
    person: t("People"),
    table: t("Table editor"),
    mail: t("Mail"),
    sources: t("Sources"),
    lists: t("Lists"),
    help: t("Help"),
    settings: t("Settings"),
  };

  return (
    <main className={`app ${agentOpen && root ? "with-agent" : ""}`}>
      <Sidebar
        root={root}
        view={view}
        busy={!!busy}
        counts={{ today: pendingToday, people: rows.length, sources: sources.reduce((n, s) => n + (s.counts?.pending || 0), 0) }}
        agentOpen={agentOpen}
        agentSessions={agentSessions}
        navigate={(v) => void navigate(v)}
        toggleAgent={() => setAgentOpen((o) => !o)}
        choose={(create) => void choose(create)}
        openSearch={() => setPalette(true)}
      />
      <section className="main">
        <header className="topbar">
          <div className="crumb">
            {view === "person" ? (
              <>
                <button className="link plain" onClick={() => void navigate("people")}>
                  {t("People")}
                </button>
                <Icon name="chevron" size={12} />
                <b>{contactName || t("Contacts")}</b>
                {position >= 0 && (
                  <span className="stepper">
                    <button className="icon-btn sm" aria-label={t("Previous")} title={t("Previous · K")} disabled={position <= 0 || !!busy} onClick={() => step(-1)}>
                      <Icon name="up" size={14} />
                    </button>
                    <button
                      className="icon-btn sm"
                      aria-label={t("Next")}
                      title={t("Next · J")}
                      disabled={position >= sequence.length - 1 || !!busy}
                      onClick={() => step(1)}
                    >
                      <Icon name="down" size={14} />
                    </button>
                    <span className="faint small">
                      {position + 1} / {sequence.length}
                    </span>
                  </span>
                )}
              </>
            ) : (
              <b>{titles[view]}</b>
            )}
          </div>
          <span className="sp" />
          {config.sendingPaused && (
            <button className="pill paused" onClick={() => void navigate("settings")} title={t("All sending is paused. Click to resume in Settings.")}>
              {t("Sending paused")}
            </button>
          )}
          <span role="status" className="status-text">
            {busy || (view === "table" && tableOpen ? (ready ? (dirty ? t("Unsaved") : t("Saved")) : t("Opening…")) : "")}
          </span>
          {view === "table" && tableOpen && (
            <button className="btn" disabled={!!busy} onClick={() => void work(t("Saving…"), saveEditors)}>
              {t("Save")}
            </button>
          )}
          {root && (
            <button
              className={`btn ghost ${agentOpen ? "on" : ""}`}
              aria-pressed={agentOpen}
              title="Agent · ⌘ / Ctrl+J"
              onClick={() => setAgentOpen((o) => !o)}
            >
              <Icon name="terminal" />
              Agent
              <Kbd>⌘J</Kbd>
            </button>
          )}
        </header>
        {error && !draft && (
          <div role="alert" className="banner error">
            <span className="grow">{t(error)}</span>
            <button className="icon-btn sm" aria-label={t("Close")} onClick={() => setError("")}>
              <Icon name="close" size={14} />
            </button>
          </div>
        )}
        {agentSuggestion && (
          <div role="status" className="banner notice">
            <Icon name="terminal" />
            <span className="grow">
              {t("The Agent wants to show you: ")}
              <b>{agentSuggestion.kind === "contact" ? rows.find((r) => r.values.Slug === agentSuggestion.target)?.values.Name || agentSuggestion.target : agentSuggestion.target}</b>
            </span>
            <button className="btn" disabled={!!busy} onClick={() => void showAgentResult(agentSuggestion)}>
              {t("Open")}
            </button>
            <button className="icon-btn sm" aria-label={t("Dismiss")} onClick={() => setAgentSuggestion(null)}>
              <Icon name="close" size={14} />
            </button>
          </div>
        )}
        {notice && (
          <div role="status" className="banner notice">
            <Icon name="check" />
            <span className="grow">{t(notice)}</span>
            <button className="icon-btn sm" aria-label={t("Close")} onClick={() => setNotice("")}>
              <Icon name="close" size={14} />
            </button>
          </div>
        )}
        <div className="content">
          {view === "help" && <HelpPage openUrl={(url) => openExternalOrNote(url, () => undefined)} />}
          {!root && view !== "settings" && view !== "help" && (
            <div className="welcome">
              <Logo size={48} />
              <h2>{t("Put people at the center of every conversation")}</h2>
              <p className="muted">{t("Contacts, profiles, activity and drafts awaiting review all live in your own workspace folder.")}</p>
              <div className="row-gap">
                <button className="btn primary" onClick={() => void choose(true)}>
                  {t("New workspace")}
                </button>
                <button className="btn" onClick={() => void choose(false)}>
                  {t("Open existing workspace")}
                </button>
              </div>
            </div>
          )}
          {root && view === "today" && (
            <TodayPage
              agentRecent={agentRecent}
              openAgentSession={(session, contact) => {
                setAgentOpen(true);
                setAgentFocus({ session, id: Date.now() });
                if (contact) void openPerson(contact);
              }}
              rows={rows}
              drafts={allDrafts}
              stale={stale}
              followupDays={followupDays}
              mailboxes={accounts.imap.length}
              agents={agents}
              root={root}
              openSettings={() => void navigate("settings")}
              importContacts={() => {
                void navigate("people");
                void startImport();
              }}
              sources={sources}
              openSources={() => void navigate("sources")}
              open={(slug, nextTab) => void openPerson(slug, nextTab)}
              openDraft={(d) =>
                void work(t("Opening draft…"), async () => {
                  await saveEditors();
                  if (d.metadata.contact_id !== selected) setDetail(null);
                  setSelected(d.metadata.contact_id);
                  setView("person");
                  setTab("drafts");
                  openDraft(d);
                })
              }
              openPeople={(v) => {
                setPeopleView(v);
                void navigate("people");
              }}
              askAgent={(task) => askAgent(task)}
              connectMail={() => void navigate("settings")}
            />
          )}
          {root && view === "people" && (
            <PeoplePage
              onSelection={setPeopleSelection}
              rows={rows}
              view={peopleView}
              setView={setPeopleView}
              query={query}
              setQuery={setQuery}
              stage={stage}
              setStage={setStage}
              selected={selected}
              stale={stale}
              busy={!!busy}
              open={(slug) => void openPerson(slug)}
              create={() => setNewContact(true)}
              importFile={() => setImportChooser(true)}
              bulkStage={(slugs, stage) =>
                work(t("Changing stage for {n} people…", { n: slugs.length }), async () => {
                  await call("contact.bulk_update", { slugs, values: { Stage: stage } });
                  setNotice(t("Moved {n} people to “{stage}”", { n: slugs.length, stage }));
                  refresh();
                })
              }
              bulkFollowUp={(slugs, days) =>
                work(t("Scheduling follow-up…"), async () => {
                  await call("contact.bulk_follow_up", { slugs, days });
                  setNotice(days === null ? t("Cleared follow-up for {n} people", { n: slugs.length }) : t("Follow-up scheduled in {days} days for {n} people", { n: slugs.length, days }));
                  refresh();
                })
              }
              exportCsv={(slugs) =>
                void work(t("Exporting CSV…"), async () => {
                  const path = await save({ title: t("Export contacts"), defaultPath: "contacts.csv", filters: [{ name: "CSV", extensions: ["csv"] }] });
                  if (!path) return;
                  const r = await call<V>("contacts.export", { path, ...(slugs ? { slugs } : {}) });
                  setNotice(t("Exported {n} people to {path}", { n: r.exported, path: r.path }));
                })
              }
              openTable={() => void navigate("table")}
            />
          )}
          {root && view === "person" && (
            <PersonPage
              workspaceDocument={
                agentDocument ? (
                  <WorkspaceDocument
                    key={`${root}:${agentDocument}`}
                    root={root}
                    path={agentDocument}
                    busy={!!busy}
                    open={openAgentDocument}
                    fail={fail}
                    close={() =>
                      work(t("Saving document…"), async () => {
                        await saveEditors();
                        setAgentDocument("");
                        if (!selected) setView("today");
                      })
                    }
                  />
                ) : undefined
              }
              selected={selected}
              detail={detail}
              stale={!!rows.find((r) => r.values.Slug === selected && stale(r))}
              facts={facts}
              fields={fields as never}
              saveField={(title, value) =>
                work(t("Saving profile…"), async () => {
                  await call("contact.update", { slug: selected, values: { [title]: value } });
                  refresh();
                })
              }
              addField={(title, type, choices) =>
                work(t("Adding field…"), async () => {
                  await call("contact.field_add", { title, type, choices });
                  setNotice(t("Added field “{title}”; every contact can now fill it in", { title }));
                  refresh();
                })
              }
              addChoice={(title, choice) => call("contact.field_choice", { title, choice })}
              loadHistory={async () => (await call<V>("contact.history", { slug: selected })).items}
              reply={() => askAgent("reply")}
              loadMemory={() => call<V>("memory.get", { slug: selected })}
              loadAnalysis={() => call<V>("agent.analysis", { slug: selected })}
              openSource={(href) => openExternalOrNote(href, () => showNote(href))}
              showSession={(session) => {
                setAgentOpen(true);
                setAgentFocus({ session, id: Date.now() });
              }}
              agentWorking={!!agentWorking}
              showAgentWork={() => {
                setAgentOpen(true);
                if (agentWorking) setAgentFocus({ session: agentWorking.session, id: Date.now() });
              }}
              followUp={(value) =>
                void work(t("Updating follow-up…"), async () => {
                  await call("contact.follow_up", { slug: selected, ...value });
                  refresh();
                })
              }
              decide={(item, action) =>
                void work(t("Working…"), async () => {
                  await call("identity.decide", { source: item.source, external_id: item.external_id, action });
                  setNotice(action === "reject" ? t("Marked as a different person") : t("Link confirmed; activity updated"));
                  refresh();
                })
              }
              tab={tab}
              root={root}
              notePath={notePath}
              busy={busy}
              draftChannel={draftChannel}
              setTab={setTab}
              setDraftChannel={setDraftChannel}
              update={update}
              edit={() => setEditContact(true)}
              merge={() => setMerging(true)}
              addChannel={() => setChannelForm(true)}
              intake={() => setIntake(true)}
              newDraft={() =>
                openDraft({
                  metadata: { contact_id: selected, channel: "email", to: detail?.contact.values.Email || "", subject: "", status: "draft" },
                  body: "",
                })
              }
              openDraft={openDraft}
              showNote={showNote}
              closeNote={() =>
                void work(t("Saving…"), async () => {
                  await saveEditors();
                  setNotePath("");
                  setTab("overview");
                })
              }
              fail={fail}
              analyze={() => askAgent("overview")}
              resumeDraft={draftCache.current.has(`${root}:${selected}:new`)}
              hasDraftEdit={(d) => draftCache.current.has(draftKey(d))}
            />
          )}
          {tableOpen && (
            <div className={`page-fill table-host ${view !== "table" ? "hidden" : ""}`}>
              <TableEditor
                root={root}
                generate={!!agents.claude?.available}
                onError={fail}
                onState={(s) => setDirty(s.dirty)}
                onConnection={(c) => setReady(!!c)}
              />
            </div>
          )}
          {root && view === "mail" && (
            <MailPage
              root={root}
              accounts={accounts}
              mail={mail}
              rows={rows}
              notePath={notePath}
              busy={!!busy}
              sync={(id) =>
                void work(t("Syncing mail…"), async () => {
                  const result = await call<V | null>("mail.sync", { id });
                  if (result) setNotice(t("Sync complete: {imported} new, {updated} updated", { imported: result.imported, updated: result.updated }) + (result.failed ? t(", {n} failed", { n: result.failed }) : ""));
                  refresh();
                })
              }
              read={(path) => void showNote(path)}
              assign={(slug, email) =>
                void work(t("Filing message…"), async () => {
                  await call("mail.assign", { slug, email });
                  setNotice(t("Filed to contact; activity updated"));
                  refresh();
                })
              }
              closeNote={() =>
                void work(t("Saving…"), async () => {
                  await saveEditors();
                  setNotePath("");
                })
              }
              showNote={showNote}
              connect={() => void navigate("settings")}
              fail={fail}
            />
          )}
          {root && view === "lists" && (
            <ListsPage
              rows={rows}
              sources={sources}
              busy={busy}
              call={call}
              work={work}
              version={version}
              refresh={refresh}
              notify={setNotice}
              openPerson={(slug) => void openPerson(slug)}
              openDraft={openDraft}
              askAgent={(list) => askAgent("list", { list, contact: "" })}
              onCurrent={setCurrentList}
              focus={listFocus}
            />
          )}
          {root && view === "sources" && (
            <SourcesPage
              sources={sources}
              rows={rows}
              busy={busy}
              call={call}
              work={work}
              refresh={refresh}
              openPerson={(slug) => void openPerson(slug)}
              openUrl={(url) => openExternalOrNote(url, () => undefined)}
              notify={setNotice}
              askAgent={(source, external_id, label) => askAgent("source_person", { source, external_id, label, contact: "" })}
              openDocument={(path) => void openAgentDocument(path)}
            />
          )}
          {/* Settings stay mounted so an unfinished mailbox form survives switching pages. */}
          <div className={view === "settings" ? "" : "hidden"}>
            <SettingsPage
              root={root}
              config={config}
              setConfig={setConfig}
              agents={agents}
              accounts={accounts}
              call={call}
              work={work}
              refresh={refresh}
              fail={fail}
              save={() =>
                void work(t("Saving settings…"), async () => {
                  await call("settings.save", config);
                  const data = await call<V>("settings.get");
                  setConfig(data.config);
                  setAgents(data.agents);
                  setNotice(t("Settings saved"));
                })
              }
            />
          </div>
        </div>
      </section>
      {root && (
        <AgentDock
          key={root}
          root={root}
          visible={agentOpen}
          close={() => setAgentOpen(false)}
          contact={selected}
          contactName={contactName}
          document={agentDocument || notePath || (view === "table" ? "contacts.ktable" : "")}
          version={version}
          openDocument={openAgentDocument}
          documentPath={agentDocument}
          documentBusy={!!busy}
          defaultAgent={config.defaultAgent}
          view={view}
          list={view === "lists" ? currentList : ""}
          draft={draft?.path || ""}
          selection={view === "people" ? peopleSelection : []}
          names={Object.fromEntries(rows.map((r) => [r.values.Slug, r.values.Name]))}
          focusSession={agentFocus}
          show={showAgentResult}
          onSessionCount={setAgentSessions}
          request={agentTask}
        />
      )}
      {root && (
        <CommandPalette
          open={palette}
          close={() => setPalette(false)}
          rows={rows}
          call={call}
          pages={(
            [
              ["today", t("Today"), "today"],
              ["people", t("People"), "people"],
              ["lists", t("Lists"), "list"],
              ["mail", t("Mail"), "mail"],
              ["sources", t("Sources"), "link"],
              ["settings", t("Settings"), "settings"],
              ["help", t("Help"), "help"],
            ] as [View, string, string][]
          ).map(([v, title, icon]): Command => ({ id: `v:${v}`, group: t("Pages"), title, icon, run: () => void navigate(v) }))}
          actions={[
            { id: "a:new", group: t("Actions"), title: t("New contact"), icon: "plus", run: () => void navigate("people").then(() => setNewContact(true)) },
            { id: "a:import", group: t("Actions"), title: t("Import contacts (CSV / TSV)"), icon: "inbox", run: () => void startImport() },
            { id: "a:import-whatsapp", group: t("Actions"), title: t("Import a WhatsApp chat"), icon: "inbox", run: () => void startChatImport("whatsapp") },
            { id: "a:import-linkedin", group: t("Actions"), title: t("Import LinkedIn messages"), icon: "inbox", run: () => void startChatImport("linkedin") },
            { id: "a:workspace", group: t("Actions"), title: t("New workspace…"), icon: "folder", run: () => void choose(true) },
            {
              id: "a:theme",
              group: t("Actions"),
              title: t("Toggle dark / light"),
              icon: "moon",
              run: () => setPrefs({ theme: currentScheme() === "dark" ? "light" : "dark" }),
            },
          ]}
          openPerson={(slug) => void openPerson(slug)}
          openList={(id) => {
            setListFocus(id);
            void navigate("lists");
          }}
          openDraft={(d) =>
            void work(t("Opening draft…"), async () => {
              await saveEditors();
              if (d.metadata.contact_id !== selected) setDetail(null);
              setSelected(d.metadata.contact_id);
              setView("person");
              setTab("drafts");
              openDraft(d);
            })
          }
        />
      )}
      {merging && detail && (
        <MergeDialog
          winner={detail.contact}
          rows={rows}
          busy={busy}
          call={call}
          close={() => setMerging(false)}
          merge={(loser) =>
            void work(t("Merging contacts…"), async () => {
              // Save and release open documents first; the merge moves and removes files.
              await saveEditors();
              setNotePath("");
              setAgentDocument("");
              await call("contact.merge", { winner: selected, loser });
              setMerging(false);
              setNotice(t("Merged: activity, drafts and profile now belong to this person"));
              refresh();
            })
          }
        />
      )}
      {creating && (
        <TemplatePicker
          templates={templates}
          busy={!!busy}
          close={() => setCreating(false)}
          pick={(id) => void choose(true, id)}
        />
      )}
      {importChooser && <ImportChooser close={() => setImportChooser(false)} pick={chooseImport} />}
      {chatImport && (
        <ChatImportDialog
          preview={chatImport.preview}
          contacts={rows.map((r) => ({ slug: r.values.Slug, name: r.values.Name || r.values.Slug })).sort((a, b) => a.name.localeCompare(b.name))}
          busy={busy}
          close={() => setChatImport(null)}
          reload={(options) =>
            void work(t("Reading file…"), async () => {
              const preview = await call<V>("chat.preview", { kind: chatImport.kind, path: chatImport.path, ...options });
              setChatImport({ ...chatImport, preview });
            })
          }
          run={(assign) =>
            void work(t("Importing…"), async () => {
              const { kind, path, preview } = chatImport;
              const result = await call<V>("chat.import", { kind, path, assign, me: preview.me, date_order: preview.dateOrder });
              setRows(await call<Row[]>("contacts.list"));
              setChatImport(null);
              setNotice(
                t("Chats imported · Conversations: {conversations} · New messages: {added} · New contacts: {created}", {
                  conversations: result.conversations,
                  added: result.added,
                  created: result.created,
                }),
              );
              refresh();
            })
          }
        />
      )}
      {importPreview && (
        <ImportDialog
          preview={importPreview}
          busy={busy}
          close={() => setImportPreview(null)}
          run={(mapping) =>
            void work(t("Importing…"), async () => {
              const result = await call<V>("import.run", { path: importPreview.path, mapping });
              // Show the imported people together with the result, not a refresh later.
              setRows(await call<Row[]>("contacts.list"));
              setImportPreview(null);
              setNotice(
                t("Import complete: {created} created, {updated} updated", { created: result.created, updated: result.updated }) +
                  (result.ignored.length ? t("; {n} notes: {items}", { n: result.ignored.length, items: result.ignored.slice(0, 3).join(t("; ")) }) : ""),
              );
              refresh();
            })
          }
        />
      )}
      {newContact && (
        <Modal title={t("New contact")} close={busy ? undefined : () => setNewContact(false)} protectChanges>
          <Form
            busy={busy}
            fields={[
              ["Name", t("Name")],
              ["Native name", t("Native name")],
              ["Affiliation", t("Affiliation")],
              ["Email", t("Email")],
              ["Hook", t("Hook")],
              ["Notes", t("Notes")],
            ]}
            long={["Hook", "Notes"]}
            onSubmit={(data) =>
              void work(t("Creating contact…"), async () => {
                const result = await call<V>("contact.create", data);
                if (data.Email)
                  await call("channel.add", { slug: result.slug, kind: "email", value: data.Email, source: "Manual", verified: false });
                setDetail(null);
                setSelected(result.slug);
                setView("person");
                setTab("overview");
                setNewContact(false);
                refresh();
              })
            }
          />
        </Modal>
      )}
      {editContact && detail && (
        <Modal title={t("Edit contact")} close={busy ? undefined : () => setEditContact(false)} protectChanges>
          <Form
            busy={busy}
            values={detail.contact.values}
            fields={[
              ["Name", t("Name")],
              ["Native name", t("Native name")],
              ["Affiliation", t("Affiliation")],
              ["Email", t("Email")],
              ["Hook", t("Hook")],
              ["Next action", t("Next action")],
              ["Notes", t("Notes")],
            ]}
            long={["Hook", "Next action", "Notes"]}
            onSubmit={(values) =>
              void work(t("Saving contact…"), async () => {
                await call("contact.update", { slug: selected, values });
                setEditContact(false);
                refresh();
              })
            }
          />
        </Modal>
      )}
      {channelForm && (
        <Modal title={t("Add channel")} close={busy ? undefined : () => setChannelForm(false)} protectChanges>
          <Form
            busy={busy}
            fields={[
              ["kind", t("Channel")],
              ["value", t("Address or handle")],
              ["source", t("Where you found it")],
            ]}
            choices={{ kind: channelKinds.map((c) => [c, channelNames[c]]) }}
            onSubmit={(data) =>
              void work(t("Saving channel…"), async () => {
                await call("channel.add", { ...data, slug: selected, verified: false });
                setChannelForm(false);
                refresh();
              })
            }
          />
        </Modal>
      )}
      {intake && (
        <Modal title={t("Log info / feedback")} close={busy ? undefined : () => setIntake(false)} protectChanges>
          <IntakeForm
            busy={busy}
            submit={(data, analyze, category) =>
              void work(t("Saving profile…"), async () => {
                await call("intake.add", { ...data, slug: selected });
                setIntake(false);
                setNotice(t("Saved to activity"));
                if (analyze) {
                  setTab("timeline");
                  askAgent("overview", { note: `The user just recorded “${data.title}” (${category}). Take it into account when analyzing the next step.` });
                } else setTab("timeline");
                refresh();
              })
            }
          />
        </Modal>
      )}
      {draft && (
        <DraftReview
          draft={draft}
          drafts={detail?.drafts || []}
          name={rows.find((r) => r.values.Slug === draft.metadata.contact_id)?.values.Name || detail?.contact.values.Name || ""}
          busy={busy}
          error={error}
          smtp={smtp}
          accounts={accounts}
          change={changeDraft}
          close={closeDraft}
          setSmtp={setSmtp}
          save={() => void saveDraft()}
          approve={() =>
            void work(t("Approving draft…"), async () => {
              const saved = await call<V>("draft.save", { ...draft.metadata, path: draft.path, body: draft.body });
              rememberSaved(saved);
              await call("draft.status", { path: saved.path, status: "approved" });
              rememberSaved({ ...saved, metadata: { ...saved.metadata, status: "approved" } });
              refresh();
            })
          }
          discard={() =>
            void confirm(t("Discard this draft? Unsaved changes will be lost.")).then((ok) => {
              if (ok) void statusDraft("discarded");
            })
          }
          send={() =>
            void work(t("Sending…"), async () => {
              if (!(await confirm(t("Send “{subject}” to {to}?", { to: draft.metadata.to, subject: draft.metadata.subject }), { title: t("Send email"), kind: "warning" }))) return;
              // The review dialog only enables send for a stale draft after the person acknowledged it.
              const send = (acknowledge_recipient: boolean) =>
                call("draft.send", { path: draft.path, connection: smtp, acknowledge_stale: true, acknowledge_recipient });
              try {
                await send(false);
              } catch (e) {
                // Not one of the person's known addresses: send only after an explicit confirmation.
                // Matches the backend recipient guard (gc-service contacts.rs); older builds wrote it in Chinese.
                if (!/known e-?mail|known address|已知邮箱/i.test(String(e))) throw e;
                if (!(await confirm(t("{to} is not a known address for this person. Send anyway?", { to: draft.metadata.to }), { title: t("Confirm recipient"), kind: "warning" }))) return;
                await send(true);
              }
              draftCache.current.delete(draftKey(draft));
              setDraft(null);
              setNotice(t("Email sent"));
              refresh();
            })
          }
          lastReply={rows.find((r) => r.values.Slug === draft.metadata.contact_id)?.values["Last reply"]}
          addresses={draftAddresses}
          language={rows.find((r) => r.values.Slug === draft.metadata.contact_id)?.values.Language}
          remember={(note) =>
            work(t("Remembering preference…"), async () => {
              await call("preferences.add", { note });
              setNotice(t("Remembered. Future analysis and drafts will follow it (edit in Settings → Writing preferences)"));
            })
          }
          copy={() =>
            void copyText(draft.body)
              .then(() => setNotice(t("Copied. Send it on the matching channel.")))
              .catch((e) => fail(String(e)))
          }
          markSent={() =>
            void work(t("Recording send…"), async () => {
              if (!(await confirm(t("Confirm you have sent it on that channel?")))) return;
              await call("draft.mark_sent", { path: draft.path, acknowledge_stale: true });
              draftCache.current.delete(draftKey(draft));
              setDraft(null);
              refresh();
            })
          }
        />
      )}
    </main>
  );
}

function Form({
  fields,
  long = [],
  choices = {},
  values = {},
  busy,
  onSubmit,
}: {
  fields: [string, string][];
  long?: string[];
  /** Fields picked from a fixed list: value → label. */
  choices?: Record<string, [string, string][]>;
  values?: V;
  busy: string;
  onSubmit: (data: V) => void;
}) {
  return (
    <form
      className="form"
      onSubmit={(e) => {
        e.preventDefault();
        onSubmit(Object.fromEntries(new FormData(e.currentTarget).entries()));
      }}
    >
      <fieldset disabled={!!busy} aria-busy={!!busy}>
        {fields.map(([name, label], i) => (
          <label key={name}>
            {label}
            {choices[name] ? (
              <select name={name} required={i === 0} defaultValue={values[name] || choices[name][0]?.[0]}>
                {choices[name].map(([value, text]) => (
                  <option key={value} value={value}>
                    {text}
                  </option>
                ))}
              </select>
            ) : long.includes(name) ? (
              <textarea name={name} defaultValue={values[name] || ""} />
            ) : (
              <input name={name} required={i === 0} defaultValue={values[name] || ""} />
            )}
          </label>
        ))}
        <div className="row-gap end">
          <button className="btn primary" disabled={!!busy}>
            {busy || t("Save")}
          </button>
        </div>
      </fieldset>
    </form>
  );
}

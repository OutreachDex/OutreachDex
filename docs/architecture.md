# Architecture

## Goals and boundaries

OutreachDex does **one-to-one, high-quality outreach**, not mass mailing:

- Each contact has several channels (email, X, Instagram, homepage, WeChat, WhatsApp, TikTok, Facebook / Messenger, LinkedIn…), and the history of every channel is gathered on that person's timeline.
- Mail is synced automatically. Channels without a usable API, such as X or Instagram, are handled by **recording** content (a link or text); the Agent can add more from public web pages.
- People and events from your own backends arrive through the read-only [GCSP/1](gcsp.md) data source protocol.
- Analysis and drafting are done by the user's own Claude Code or Codex in the Agent terminal, through the OutreachDex MCP tools. They update the person's dossier, summary and Agent memory, and save drafts for review.
- **Sending is always confirmed by a person.** The Agent can only write drafts; it has no sending tool.

Out of scope: mass-mail sequences, mailbox warm-up, automated scraping of X or Instagram, tracking pixels.

## Processes

```
┌──────────────────────── Tauri desktop process (Rust) ───────────────────────────┐
│  gc-app (src-tauri)  Tauri commands, events, windows; binary `globalcontact`    │
│  gc-service   business layer: contacts, timeline, drafts, mail, lists, sources, │
│               memory, merge; Agent terminals (PTY) and the MCP tool catalog     │
│  gc-core      workspace, configuration, schema, channels, atomic file writes    │
│  gc-docs      open-document registry: path → in-process document / editor iframe│
│  gc-mail      IMAP sync                                                         │
│  gc-connect   SMTP, proxies and the encryption key for secrets                  │
│  gc-source    GCSP/1 client and conformance checker                             │
│  gc-store     the app's own SQLite (connections, sync state, terminals, …)      │
│  table-sdk / note-sdk   TableDocument / NoteDocument in process                 │
└─────────────────────────────────────────────────────────────────────────────────┘
        │ PTY (portable-pty)
        ▼
  codex / claude (the user's interactive CLI)
        │ MCP (stdio)
        ▼
  globalcontact mcp (gc-mcp)
        │ local socket, token-authenticated (GC_MCP_ENDPOINT / GC_MCP_TOKEN)
        ▼
  desktop process (the owner of the workspace) executes the tool
        │ Kit Host Protocol (kit-host)
        ▼
  kit-table / kit-note (headless)

┌──────────────────── WebView: React + Tailwind ──────────────────┐
│  Native UI: Today, People, person page, Lists, Mail, Sources,   │
│  Settings, help center, the right-hand Agent pane (xterm.js)    │
│  iframe kit.table: the contacts workbook (kit-host)             │
│  iframe kit.note:  dossier / draft / document Markdown          │
└─────────────────────────────────────────────────────────────────┘
```

- **Agent terminals.** `gc-service::terminal` uses portable-pty to run the local Codex / Claude interactive CLI; xterm.js in the WebView shows input and output. The CLI connects to `globalcontact mcp`, which forwards every request to the desktop process that owns the workspace; there is no separate business or document client. Output is stored in local SQLite as ordered byte chunks, and hiding the pane does not affect the process. See [Agent bridge](agent-bridge.md).
- **The contacts table is an embedded table component; there is no separate client.** It is used through three surfaces:

  | Surface | Used by | How it is connected | What it exercises |
  |---|---|---|---|
  | In-process `TableDocument` | Host features: mail sync, timeline, import, AI fields | `table-sdk`; `KitDocument::open/create`, `execute_as("table.<method>", params, origin)`, saving bytes | Engine, request layer, `ai.prepare/ai.apply`, `import.*`, `changes.since`, origins and undo |
  | Headless `kit-table` | The Agent (Claude Code, Codex) | Started through `kit-host` (`HeadlessKit`); the table's `commands.json` is exposed unchanged as MCP tools | kit.json, command schemas, `model.describe`, `document.check` |
  | iframe `kit.table` editor | People | Mounted with `kit-host`; the host implements `host/read`, `host/write`, `host/generate` | Editor, Kit UI protocol, `ai/run`, `replica/*`, `document/reveal` |

  Routing: while a `.ktable` is open in an editor, host writes are sent to that editor through `ai/run` (the user sees them immediately and there is only one copy of the document); otherwise they go to the in-process `TableDocument` and are written back to the file atomically. gc-docs only does this registration and routing. It does not parse tables or wrap them in another API; requests and results are the table's own `{ result, change }`.
- kit.note works the same way: structured edits to dossiers, drafts and documents (property blocks, section replacement) use the in-process `NoteDocument`, and people edit in the `kit.note` iframe.
- Incompatibilities found in the table or note component are fixed in that component, not worked around here.
- **No model calls in this app.** Analysis and drafting run in the user's own interactive CLI (their own Claude or Codex account). `host/generate` from table AI fields runs the user's Claude Code once without tools (see [agent-bridge.md](agent-bridge.md#table-ai-fields-hostgenerate)). The app bundles neither assistant.
- Host writes to the table always carry an origin `system:<feature>` (such as `system:emailsync` or `system:agent`) and never enter the user's undo history.

## Repository layout

```
globalcontact/
├─ Cargo.toml                 workspace
├─ crates/
│  ├─ gc-core/                workspace, configuration, schema, channels, templates, atomic file writes
│  ├─ gc-service/             business layer, Agent terminals, MCP tool execution
│  ├─ gc-store/               SQLite connection pool and migrations
│  ├─ gc-docs/                open-document registry and routing (in process / editor)
│  ├─ gc-mail/                IMAP sync
│  ├─ gc-connect/             SMTP, proxies, secrets
│  ├─ gc-source/              GCSP/1 client and checker
│  └─ gc-mcp/                 stdio MCP endpoint (`globalcontact mcp`)
├─ src-tauri/                 gc-app: Tauri entry point and commands; bundles the kit binaries
├─ web/                       React + Tailwind + Vite
│  └─ src/
│     ├─ app/                 shell, sidebar, i18n, shared client code
│     ├─ pages/               Today, People, person page, Lists, Mail, Sources, Settings, Help
│     ├─ components/          Agent pane, draft review, dialogs, command palette
│     ├─ kits/                mounts kit.table / kit.note (kit-host)
│     └─ locales/             UI translations
├─ spec/gcsp/v1/              GCSP/1 JSON Schemas
├─ scripts/                   kit preparation, CI helpers, GCSP mock backend, checks
└─ docs/                      protocol, architecture, data model, help center
```

## External dependencies

| Dependency | Purpose | How it is obtained |
|---|---|---|
| Table and note components | Rust: `table-sdk`, `note-sdk`, `kit-host`; binaries: headless `kit-table`, `kit-note`; front end: the `kit.table` / `kit.note` editors, `kit-host`, `kit-protocol` | Not published yet. Located as sibling directories through Cargo path and pnpm `link:` dependencies; prebuilt artifacts are imported by `scripts/prepare-kits.mjs`, which records version, source revision and hashes in `provenance.json` |
| Claude Code / Codex CLI | Interactive Agent terminals; Claude Code also generates table AI fields | Installed and signed in by the user; not bundled. The app shows the install command when one is missing |

When packaging, `src-tauri` includes the `kit-table` and `kit-note` binaries and the editors' `dist` as resources.

## Main Rust dependencies

`tauri 2`, `tokio`, `rusqlite` (bundled), `r2d2_sqlite`, `async-imap`, `mail-parser`, `rustls`, `aes-gcm`, `portable-pty`, `serde`, `tracing`.

## Security

- Mail passwords, SMTP passwords and source tokens are encrypted with a per-installation key from `gc-connect` (AES-GCM). The key file is in the app data folder, never in the workspace.
- The workspace only contains material that may be shown to the Agent. The Agent's working directory is the workspace; it gets no access to the app data folder.
- Permission prompts of the Agent CLI are shown in its terminal and confirmed by the user; the app never confirms on the user's behalf.
- Approval and sending are desktop commands only; MCP has no route to them.

## Component boundary

The table and note components are consumed as independent products. OutreachDex uses only their public Rust APIs, the Kit UI protocol and build artifacts; their internal structure, dependency versions and editor implementation do not enter business code. Incompatibilities are fixed in the component with a regression test there; internal implementations must not be copied here, and `[patch]` must not be used to hide problems. Local path / link dependencies are only how the components are located during development and will be replaced by pinned, distributed versions.

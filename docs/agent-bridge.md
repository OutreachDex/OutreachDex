# Agent bridge

OutreachDex does not call model APIs. Analysis and drafting are done by the Claude Code or Codex the user already uses. Contact analysis, reply drafting, list preparation and reading the intent of incoming mail all happen in the global **Agent workspace**: like Orca, it runs the user's own CLI interactively in a real **PTY terminal**, keeps the raw terminal output, and the CLI reads and writes the workspace through the OutreachDex MCP server.

ACP is used for one thing only: tool-free text generation for table AI fields (see [Table AI fields](#table-ai-fields-hostgenerate)). There is no ACP-based contact analysis, no background analysis queue and no automatic analysis.

User-facing behavior is described in the help center: [Analysis and playbooks](help/04-analysis.md) and [Agent workspace](help/09-agent.md).

## Current context

The desktop app updates the current contact and document serially with `context.set`; MCP exposes only the read-only `context.get`, and contact names come from the actual records. When a new question refers to "this person", the Agent reads the current selection once and pins that slug for the task; a task that names someone explicitly uses that person instead. Tasks already in progress keep their original target. Context is isolated per workspace, and no text is ever typed into a CLI's permission prompt automatically.

The quick tasks ("Understand", "Draft reply", "Sync & summarize") and the hand-offs from other pages ("Ask Agent", "Agent drafts reply", "Agent reply", list preparation) each start a new CLI session with an initial task passed as a CLI launch argument; nothing is injected into an existing PTY. Replies are only saved as drafts and reference the real incoming Message-ID. `mail.accounts` returns the current workspace's incoming accounts (no credentials) and `mail.sync` reuses the normal incoming sync and contact matching; neither can approve or send. Sync applies to a configured mailbox; with several accounts and no instruction from the user, the Agent asks first.

## Agent workspace

The sidebar button or ⌘ / Ctrl+J opens a fixed right-hand pane that takes part in the layout and never covers the main page. Hiding it does not stop the process. It supports multiple sessions, switching assistants, terminal input and scrollback. The working directory and MCP binding are the current workspace; switching workspaces never rebinds an existing session to the new folder.

Documents open in the central area, the contact list stays on the left and the Agent terminal on the right; in a narrow window the panes stay side by side and the main navigation collapses to icons. The document list can be filtered; the terminal uses the standard xterm.js search add-on to find text in the scrollback. Session names, the chosen assistant and the last selection are stored in per-workspace local UI preferences; PTY output is persisted in SQLite. The connection state only means the CLI process is still running; task progress and permission prompts are shown by the terminal itself.

- `portable-pty` starts the local `codex` / `claude`; the front end renders a real terminal with xterm.js, with IME input, ANSI and resizing. The interactive CLI and the ACP adapter are detected separately; when a CLI is not found, its path can be set in Settings.
- Codex is configured per process with `-c mcp_servers.globalcontact=…` and `env_vars`; Claude uses `--mcp-config`. The bridge token (`GC_MCP_ENDPOINT`, `GC_MCP_TOKEN`) is passed only through the child process environment, never written to a global MCP config or to command-line arguments. `GC_TERMINAL_SESSION` identifies the terminal session so that each MCP call is attributed to it. Codex runs with `--no-alt-screen` to keep scrollback and `--no-daemon` so the app owns the process lifetime, with the `workspace-write` sandbox and the `on-request` approval mode.
- Output is written to the local SQLite tables `terminals` / `terminal_output` as ordered binary chunks, avoiding UTF-8 splits across frames and duplicate replays. The UI lists the latest 100 sessions and keeps 20,000 lines of screen scrollback; every received chunk stays in the local database. At most 6 terminals run at once. Quitting ends the processes; on the next launch old sessions are read-only logs, not resumed CLI processes. Start a new session and use the CLI's own resume feature if needed.
- Document changes go MCP → the desktop workspace owner → gc-docs, which routes them to an open kit editor, or to the in-process SDK when no editor is open. `document.create` / `document.replace` can maintain `notes/` and `campaigns/`; dossiers are still only changed in agent-owned sections, and drafts still go through the draft tools.
- Sign-in, workspace trust and CLI permission prompts are shown in the terminal. The app never confirms on the user's behalf; a plain "New session" waits for user input. Before quitting, the app checks for active terminals in every workspace. Terminal output stays on this computer; no message is ever sent to a real contact.

Codex configuration follows the [official MCP documentation](https://developers.openai.com/codex/mcp/) and the [configuration reference](https://developers.openai.com/codex/config-reference/), checked against the local CLI's options.

## Progress reporting and showing results

- Every MCP call, including refusals, is recorded against its terminal session and shown as readable progress in the Agent pane ("Agent progress").
- The Agent reports status with `task.report` (`working` with optional `done` / `total` for batches, `done`, `needs_input`, `blocked`).
- The Agent shows results with `ui.open` (a contact, a draft, a document or a list). It never takes keyboard focus from the terminal and never replaces something the user is editing; in that case the result is offered as a suggestion instead.
- Every change made for a person (`dossier.write_section`, `contact.update`, `channel.add`, `draft.save`, `list.draft_save`, `contact.follow_up`) is also appended to that person's memory (`memory/<slug>.md`) as a `change` entry.

## Components

```
globalcontact (desktop app, owner of the open workspaces)
  ├─ gc-service::terminal  PTY sessions for codex / claude (portable-pty)
  ├─ gc-service::mcp       MCP tool catalog and execution (business tools + kit-table commands)
  └─ gc-agent              ACP client, used only for tool-free AI field generation
globalcontact mcp --workspace <dir>   (stdio MCP server started by the CLI)
  └─ gc-mcp                forwards each request over a local socket to the desktop app,
                           authenticated with GC_MCP_TOKEN; no business logic of its own
```

- The MCP endpoint refuses to start unless the desktop app is running (`GC_MCP_ENDPOINT` missing), so all writes go through the single workspace owner and its open editors.
- The table component's agent commands (`table_*`, names and schemas from the kit's `commands.json`) are exposed alongside the business tools and executed by the headless `kit-table`. Writes from these tools carry the `agent` origin and show up live when the table is open in an editor.

## MCP tools

Business tools only do what is specific to this app; generic table operations are the table component's own commands.

| Tool | Read / write | Purpose |
|---|---|---|
| `context.get` | read | The contact and document currently shown in the UI for this workspace |
| `workspace.guide` | read | Playbooks (purposes and business contexts), stages with roles, profile fields, outreach daily limit, writing preferences |
| `memory.read` | read | The person's Agent memory: dated facts, insights, preferences, commitments, questions and earlier changes |
| `memory.append` | write | Add one `fact` / `insight` / `preference` / `commitment` / `question` to the person's memory |
| `preferences.add` | write | Remember a writing preference of the workspace owner (`preferences.md`) |
| `contact.get` | read | One contact: table row, all channels, dossier and drafts |
| `contact.search` | read | Find people by name, affiliation, campaign or stage |
| `timeline.get` | read | A person's latest N interactions, including email bodies and recorded notes |
| `campaign.get` | read | A campaign brief (`campaigns/<name>.md`) |
| `dossier.write_section` | write | Replace an agent-owned dossier section (`analysis` or `channels`), wrapped in `<!-- agent:<section> -->`, through a minimal kit.note edit |
| `contact.update` | write | Only `AI summary`, `Next action` and `Stage` (origin `system:agent`) |
| `contact.follow_up` | write | Set (`date` or `days`) or clear the follow-up reminder; nothing is sent |
| `channel.add` | write | A newly found channel with `Verified = false`; `source` is required |
| `draft.save` | write | Create or edit a draft with `status: draft`; cannot approve or send |
| `source.list` | read | Connected data sources with sync status and match counts; no credentials |
| `source.sync` | write | Pull new people and events from a data source (read-only toward the source) |
| `source.facts` | read | Declared source attributes for a contact, with source name and sync time |
| `source.people` | read | Search all source users, including people who are not contacts yet |
| `source.person` | read | One source person: facts, attribute changes and events with evidence paths |
| `mail.accounts` | read | Connected incoming mailboxes and sync status; no credentials |
| `mail.sync` | write | Sync a connected incoming mailbox and match messages to contacts; never sends |
| `list.all` / `list.get` | read | Outreach lists, their filters, counts and members (with why each was selected) |
| `list.draft_save` | write | Save the draft for one list member (creating the contact from the source if needed) |
| `list.decline` | write | Mark a list member as not worth contacting now, with a reason |
| `task.report` | — | Report task status to the Agent pane |
| `ui.open` | — | Show a contact, draft, document or list in the main area without taking focus |
| `document.list` / `document.read` | read | List workspace Markdown; reading includes live editor contents |
| `document.create` | write | Create Markdown in `notes/` or `campaigns/`; never overwrites an existing file |
| `document.replace` | write | Replace a unique exact match in `notes/` or `campaigns/`; rejects stale or ambiguous edits |

**There is no sending tool.** The Agent cannot move a draft to `approved` or `sent`. Approval and sending are desktop commands only and have no MCP route. Approval stores the file hash; any change to the content invalidates it.

The generic table tools can reorganize the whole contacts table, and the field restrictions on business tools are not a file-system sandbox. Shell and file permissions of the CLI remain the CLI's own permission prompts, decided by the user. Do not give an untrusted agent unrestricted shell access.

## Table AI fields (`host/generate`)

`AI summary` in the `contacts` table is a table AI field. When the user clicks "Run" in the editor, the kit sends `host/generate` (purpose `ai-field`) to the host, which forwards it to Claude ACP (its public session options support `tools: []`): a short session **with no MCP and no tools**, using the messages prepared by the kit as the prompt. The final text is returned to the kit, which then calls `ai.apply`. Built-in tools and project setting sources are disabled and permission requests are refused. This path exercises the table's `ai.prepare` / `ai.apply` and the editor's AI field UI.

When Claude ACP is not available, the host does not declare the `generate` capability, and the editor hides the AI field's run button according to the table's rules.

# Agent workspace

The Agent on the right is a real command-line assistant (the Codex or Claude Code on this computer) that reads and writes contacts and documents through OutreachDex's tools.

## Basics

- **Show / hide**: ⌘ / Ctrl+J, or click Agent in the top bar. Sessions keep running while hidden.
- **Resize**: drag the left edge, or focus the divider and use the arrow keys; double-click to restore the default.

The panel has just three parts, top to bottom:

1. **Header**: a green dot means a session is running, followed by the current session's name. On the right: "Sessions", "Workspace documents", "New session", "Collapse".
2. **Quick tasks**: shows the person you are looking at, with three buttons below — "Understand", "Draft reply" and "Sync & summarize" (sync the connected mailboxes and data sources and summarize who has new activity). Each button starts a new session.
3. **Terminal**: every step the Agent takes is shown here, and you can type follow-up questions directly; "Enter ↵" sends one Enter key.

- **Sessions**: switch to earlier sessions, rename the current one, and choose whether new sessions use Codex or Claude Code.
- **Workspace documents**: open a Markdown document from the workspace. It opens in the center, so you can read it while the Agent edits it.
- Other pages hand tasks over here as well: "Ask Agent" and "Agent drafts reply" on the person page, "Prepare N people" on Lists, "Sync and summarize" on Today.

## Context

- The contact and document you are viewing are shared with the Agent. When a new question says "this person", the Agent reads the currently selected person.
- A task already in progress keeps the target it started with; switching contacts does not change it.

## Progress and results

- The "Agent progress" line above the terminal shows the status the Agent reports itself: In progress (batches show progress such as 3/12), Done, Awaiting your reply, or Blocked, plus the latest step. Expand it to see each step in plain words, e.g. "Read activity · Maya Chen" or "Save draft · Maya Chen"; click a step to open that person, draft or document.
- After saving a draft or writing a report, the Agent can open it in the main area for you. The keyboard stays in the terminal so you can keep typing. If you are editing something, it does not interrupt you; a banner at the top offers "The Agent wants to show you: …" and you decide whether to open it.
- Dialogs never cover the Agent pane, so you can keep talking to the Agent while reviewing a draft.
- While the Agent works on a person, their page shows "Agent working"; click it to jump to that session.
- When the Agent finishes or needs your reply while the pane is hidden, a banner shows its one-line result; Today also lists it under "Recent Agent results".
- Up to 6 sessions run at once. When full, starting a new task offers to end the oldest finished or waiting session; its record is kept.
- Pick contacts on the People page and just tell the Agent "for these people…" — it can see your selection. The Sessions list also shows each session's latest status.

## Records and permissions

- Terminal output is stored on this computer. Finished or interrupted sessions are shown as "Read-only log"; you can search and copy them, then click "Start an interactive session" to continue.
- First-time trust prompts, sign-in and tool permissions are all confirmed by you in the terminal.
- The Agent can only save drafts pending review; it has no sending tool.
- Changes the Agent makes are recorded automatically in that person's **Agent memory**.

## Tools available to the Agent (MCP)

| Purpose | Tools |
|---|---|
| Understand the workspace | `workspace.guide` (playbooks, stages, fields, writing preferences), `context.get` |
| Understand a person | `memory.read`, `contact.get`, `timeline.get`, `source.facts`, `contact.search` |
| Understand source users | `source.people` (search all source users, including unlinked), `source.person` (facts, changes, events) |
| Write things down | `memory.append`, `dossier.write_section`, `contact.update`, `contact.follow_up`, `channel.add`, `preferences.add` |
| Drafts | `draft.save` (save only; cannot approve or send) |
| Talk to you | `task.report` (progress / done / awaiting your reply / blocked), `ui.open` (show a person, draft, document or list in the main area) |
| Lists | `list.all`, `list.get`, `list.draft_save`, `list.decline` |
| Sync | `mail.accounts`, `mail.sync`, `source.list`, `source.sync` |
| Documents | `document.list`, `document.read`, `document.create`, `document.replace` |

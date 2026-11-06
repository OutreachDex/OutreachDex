# Data model

All data lives in one **workspace** folder made of ordinary files (`.ktable` tables and Markdown notes) that can be put in git. The app's own state (connections, keys, sync cursors, terminal sessions) is in SQLite in the app data folder, not in the workspace.

## Workspace layout

```
<workspace>/
├─ globalcontact.toml           workspace configuration: stages and their roles, campaign choices, AI summary prompt, extra fields, daily limit
├─ playbooks/
│  └─ <id>.md                   analysis purposes (kind: purpose) and business contexts (kind: context)
├─ contacts.ktable              the contacts workbook (several tables, see below)
├─ preferences.md               the owner's writing preferences, read by every analysis and draft
├─ lists/
│  └─ <id>.toml                 outreach lists: candidate source, channel, analysis purpose, filters
├─ suppression.txt              do-not-contact addresses, excluded from lists and sending
├─ sources/
│  └─ <source-id>/
│     ├─ <yyyy>/<mm>/<event-id>.md  evidence file for a GCSP source event (overwritten on sync; deleted events are marked)
│     └─ people/<external-id>.md    current attributes of a source person and a timestamped change log
├─ people/
│  └─ <slug>.md                 one dossier per person (kit.note); contact_id in the property block
├─ memory/
│  └─ <slug>.md                 Agent memory: append-only, dated log per person
├─ intake/
│  └─ <slug>/<date>-<source>.md recorded notes and pasted content (X posts, Instagram, homepage excerpts, feedback…)
├─ drafts/
│  └─ <slug>/<date>-<channel>.md drafts; status lives in the property block
├─ notes/
│  └─ <name>.md                 reports and summaries created from the Agent workspace (folder created on demand)
├─ mail/
│  ├─ email.ktable              mail index (maintained by gc-mail, table name email_messages)
│  └─ <account>/<yyyy>/<mm>/<id>.md  one Markdown file per email, attachments next to it
└─ campaigns/
   └─ <name>.md                 background material for an outreach campaign (for example a one-page project brief)
```

`<slug>` is generated from the name (lower case, hyphenated), with a numeric suffix for duplicates; renaming a contact does not change the slug.

The Agent can create and edit Markdown in `notes/` and `campaigns/` with the generic document tools; edits must quote a unique exact match of the original text. Dossiers and drafts are still changed only through their own restricted business tools. Terminal session metadata and the ordered output chunks are stored in the local app database (`terminals`, `terminal_output`), never in the workspace.

## Agent memory `memory/<slug>.md`

An append-only log of what the Agent learned about a person and what it changed for them. Every entry is one dated line plus the note:

```markdown
- 2026-11-03 14:20 · Fact · Agent
  Uses the export feature weekly (intake/maya-chen/2026-11-01-feedback.md).
- 2026-11-03 14:21 · Change · Agent
  Set follow-up date 2026-11-10
```

Entry kinds are `Fact`, `Insight`, `Preference`, `Commitment`, `Question` (written by the Agent through `memory.append`) and `Change` (written by the host for every MCP write made for the person). Every new analysis reads the memory first through `memory.read`. When contacts are merged, their memories are combined. Logs written by older versions may carry Chinese labels; they are kept as written, since the host never parses labels back.

## Workspace configuration and playbooks

Business settings live in the workspace, not in code. A new workspace is created from a template (Blank / Expert outreach / Product user relationships); a template is a `globalcontact.toml` plus `playbooks/`. When a workspace from an early version without a configuration file is opened for the first time, a configuration matching its existing stages is generated (`template = "legacy"`) and the common playbooks are added.

```toml
version = 1
template = "user-relationships"
stages = [
  { label = "New user", role = "open" },
  { label = "To contact", role = "open" },
  { label = "Sent", role = "waiting" },
  { label = "Replied", role = "active" },
  { label = "Following up", role = "active" },
  { label = "Resolved", role = "closed" },
  { label = "Do not contact", role = "suppressed" },
]
[outreach]
daily_limit = 20
[contacts]
campaigns = []
summary_prompt = "…"
[[fields]]          # extra fields added to contacts when the table is created
title = "Plan"
type = "single_select"
options = []
```

Code only looks at the role: new contacts enter the first `open` stage; after a message is sent they move to the first `waiting` stage, and to the first `active` stage when a reply arrives; `closed` / `suppressed` contacts get no follow-up reminders. Stage labels can be anything, but at least the open, waiting and active roles are required, and a written Stage must be one of the configured stages. Workspaces created by older versions (including `template = "legacy"`) may use Chinese stage labels such as `待联系` / `已发` / `已回复` / `不再联系`; they behave the same because only the role matters.

A playbook is Markdown with a property block: `kind: purpose` makes it an analysis purpose, `kind: context` a business context; `creates_drafts` and `requires_prompt` control whether it may draft and whether a question is required. The Agent reads all playbooks through `workspace.guide`. The playbook body is the task description; safety rules (never send, only write drafts, separate facts from inferences) are always added by the code and cannot be overridden by a playbook.

## contacts.ktable

### Table `contacts`

| Field | Type | Notes |
|---|---|---|
| Name | text | Display name |
| Native name | text | Name in the person's own script, optional |
| Slug | text | Dossier file name, unique, maintained by the host |
| Affiliation | text | Organization and role |
| Campaign | single_select | Project / outreach campaign; choices from `globalcontact.toml` |
| Stage | single_select | A stage from `globalcontact.toml`, updated automatically by role |
| Language | single_select | `Chinese` / `English` (workspaces from older versions use `中文` for Chinese; both are accepted) |
| Primary channel | single_select | Email / X / Instagram / WeChat / WhatsApp / TikTok / Facebook / LinkedIn / Other |
| Email | text | Primary email |
| Homepage | url | |
| X | text | `@handle` |
| Instagram | text | `@handle` |
| Verified | checkbox | Contact details verified on the person's own page |
| Hook | long_text | Why this person, and what you can offer them |
| AI summary | long_text with `ai_config` (AI field) | One-paragraph summary from the latest analysis; see [agent-bridge](agent-bridge.md) |
| Next action | long_text | Next step suggested by the Agent; editable |
| Follow up | date | Next follow-up reminder |
| Keep in touch | number | Keep-in-touch interval in days |
| Last contacted | datetime | Last message we sent |
| Last reply | datetime | Last reply from them |
| Notes | long_text | Manual notes |

Extra fields from `[[fields]]` follow these core fields. `Stage`, `Last contacted` and `Last reply` are updated automatically by the host from the timeline (origin `system:timeline`); people can also change them by hand.

### Table `channels`

A person can have several email addresses and accounts. Email, X and Instagram in `contacts` are shortcut columns for the primary channels; the full list is here.

| Field | Type | Notes |
|---|---|---|
| Contact | text | Slug in contacts |
| Kind | single_select | email / x / instagram / homepage / scholar / github / wechat / whatsapp / tiktok / facebook / linkedin / other |
| Value | text | Address, handle or URL |
| Source | text | Where it was found (homepage, paper, CV…) |
| Verified | checkbox | |

Mail is filed by matching all addresses with `Kind = email`.

Channel kinds are defined in one place, `gc-core/src/channels.rs` (`facebook` also stands for Messenger). When a workspace created earlier is opened, the host appends any missing kinds to the choices of `Primary channel`, `channels.Kind` and `interactions.Channel` with `table.fields.update` (before `other`); existing choices and data are unchanged.

### Table `identities`

People from GCSP data sources, one row per `Source + External ID` (see [GCSP](gcsp.md)).

| Field | Notes |
|---|---|
| Name, Source, External ID | Display name in the source, source identifier (`source.id` in the manifest), external identifier |
| Contact | Slug of the linked contact; for `pending`, the candidate contact |
| Match | `auto` (verified email matches, linked automatically) / `confirmed` / `rejected` (decided by a person, kept across re-syncs) / `pending` (to confirm) / `unlinked` |
| Emails | Emails provided by the source; verified ones are marked ✓ |
| Attributes | Only attributes declared in the manifest (JSON) |
| URL, Updated at, Synced at, Deleted | Backend detail page, source update time, sync time, tombstone |

Source events of linked people are written to interactions with `Kind = intake`, `Direction = note`, `Message-ID = gcsp:<source>:<event>`. Source connections, tokens (encrypted), cursors and sync runs are stored in the local app database (`sources`, `source_cursors`, `source_runs`), never in the workspace.

### Table `interactions`

The timeline index. Bodies are not in the table but in Markdown files.

| Field | Type | Notes |
|---|---|---|
| Contact | text | Slug |
| At | datetime | |
| Direction | single_select | in / out / note |
| Channel | single_select | email / x / instagram / wechat / whatsapp / tiktok / facebook / linkedin / other |
| Kind | single_select | message / post / intake / draft / analysis |
| Title | text | Email subject or a one-line summary |
| File | text | Workspace-relative path (mail md, intake md, draft md) |
| Message-ID | text | Email only; used for de-duplication and threading |

Where rows come from:

- **Mail**: after gc-mail syncs a message, its sender and recipients are matched against `channels`; on a match an interaction is written (`system:emailsync`). Unmatched mail is not written here; it stays in `email.ktable` and can be assigned to a person in the UI.
- **Recorded notes**: an `intake/...md` file is written, then a row with `Kind = intake`.
- **Sent**: after a successful send, the draft status becomes `sent` and a row with `Direction = out` is written.

## Dossier `people/<slug>.md`

kit.note Markdown: structured information in the property block (Obsidian-style frontmatter), the body in sections. The Agent may only change the sections it owns; sections written by people are never touched.

```markdown
---
contact_id: elena-park
campaign: Research paper
updated_by: agent
updated_at: 2026-10-24T20:00:00+08:00
---

# Elena Park

## Background
(written by you, or by the Agent's first research: role, field, notable work)

## Hook
(why this person, and what you can offer them)

## AI analysis
<!-- agent:analysis -->
(replaced on every analysis: recent activity, tone, what they care about, risks)
<!-- /agent:analysis -->

## Channel suggestions
<!-- agent:channels -->
(what to say and not say on each channel)
<!-- /agent:channels -->

## My notes
```

The Agent writes dossiers only with `dossier.write_section` (see [agent-bridge](agent-bridge.md)); the write is a minimal kit.note edit, so opening and saving never changes other bytes. Dossiers created by older versions may have Chinese headings (`背景`, `切入点`, `AI 分析`, `渠道建议`, `我的备注`); the agent-owned sections are found by their `<!-- agent:… -->` markers, not by heading text.

## Drafts `drafts/<slug>/<date>-<channel>.md`

```markdown
---
contact_id: elena-park
channel: email
to: elena.park@university.example
subject: An idea for a joint evaluation
status: draft        # draft / approved / sent / discarded
in_reply_to: <message-id, optional>
created_by: claude    # claude / codex / me
---

Body…
```

Only email drafts with `status: approved` get a "Send email" button. Drafts for X, Instagram, WeChat, WhatsApp, TikTok, Facebook / Messenger and LinkedIn only offer "Copy body"; the person sends them and then clicks "Mark as sent", which records an interaction.

## App database (app data folder, gc-store)

| Table | Owner | Contents |
|---|---|---|
| `connections` | gc-connect | SMTP / IMAP connection settings; secret fields encrypted |
| `workflows`, `sync_items`, `sync_runs` | gc-mail | Mail sync jobs, per-message items, runs and UID cursors |
| `settings` | gc-service | App settings (assistant programs, follow-up reminder days, pause all sending…) |
| `approvals` | gc-service | Approved draft paths and their content hashes; any change invalidates approval |
| `sent_receipts` | gc-service | Send results per draft, used to lock drafts whose send result is unknown |
| `outreach_sends` | gc-service | List messages sent, for the daily limit |
| `list_items` | gc-service | Per-list member state (queued, preparing, ready, skipped…), evidence, draft, session |
| `sources`, `source_cursors`, `source_runs` | gc-service / gc-source | GCSP connections (token encrypted), stream cursors and sync runs |
| `terminals`, `terminal_output` | gc-service::terminal | Agent terminal sessions and their ordered output chunks |
| `agent_activity` | gc-service::mcp | Every MCP call per terminal session, shown as progress in the Agent pane |
| `agent_sessions`, `analysis_queue` | earlier versions | Records from the former ACP contact analysis; kept for history, no longer written |

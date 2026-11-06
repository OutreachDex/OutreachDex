# Workspace and configuration

A workspace is an ordinary folder that you can back up or put in git. It contains:

```
<workspace>/
├─ globalcontact.toml   workspace configuration: stages, fields, summary prompt, daily limit
├─ playbooks/           analysis purposes and business contexts
├─ lists/               outreach lists
├─ suppression.txt      do-not-contact addresses
├─ preferences.md       your writing preferences, followed by all drafting and analysis
├─ contacts.ktable      contacts, channels, source identities, activity (opens in the table editor)
├─ people/              one dossier per person
├─ memory/              Agent memory: one file per person, appended in time order
├─ drafts/              drafts
├─ intake/              recorded notes and feedback
├─ mail/                synced mail
├─ sources/             evidence files for data source events
└─ notes/               reports written by the Agent, etc.
```

Mail passwords, source tokens, terminal records and sync progress are stored in the app data folder on this computer, **not in the workspace**.

## globalcontact.toml

```toml
stages = [
  { label = "To contact", role = "open" },
  { label = "Sent", role = "waiting" },
  { label = "Replied", role = "active" },
  { label = "In progress", role = "active" },
  { label = "On hold", role = "closed" },
  { label = "Do not contact", role = "suppressed" },
]

[outreach]
daily_limit = 20

[contacts]
campaigns = []
summary_prompt = "Based on this contact's profile, summarize…"
```

- **Stage names can be anything you like**; the app only looks at `role`:
  - `open`: not contacted;
  - `waiting`: sent, waiting for a reply;
  - `active`: replied or in progress;
  - `closed`: paused or resolved;
  - `suppressed`: do not contact.
- At least the open, waiting and active roles are required. New contacts enter the first open stage; after you send a message they move to the first waiting stage, and to the first active stage when they reply.
- `[[fields]]` defines extra fields added when the table is created (only takes effect for new workspaces).
- Reopen the workspace for changes to take effect. If the format is wrong, you are told exactly why when opening it.
- Workspaces created with older versions may still use Chinese stage labels (such as `待联系` or `不再联系`). They keep working because only `role` matters; rename them here if you like, but also update the Stage values of existing contacts so they still match.

## Open another workspace

Click the workspace name in the top-left → Open workspace…. A workspace can only be open in one OutreachDex window at a time.

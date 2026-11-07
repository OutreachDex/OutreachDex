# Privacy and security

- **Your data stays with you**: contacts, dossiers, drafts, mail and source events are stored in the workspace folder you choose. OutreachDex has no cloud service and collects no usage data.
- **Passwords and tokens**: mail passwords and data source tokens are encrypted with a key on this computer and stored in the app data folder, never in workspace files.
- **AI assistant**: analysis uses the Claude Code / Codex you are signed in to on this computer. While analyzing, the assistant can read workspace data, and the relevant content is sent to that model provider under that service's terms.
- **Nothing is sent automatically**: the assistant can only save drafts pending review. Every send needs your approval plus one more confirmation in a dialog.
- **No bothering**: do-not-contact addresses and contacts are never selected by any list and cannot be sent to. List messages have a daily limit.
- **Data sources are read-only**: only the backend's read-only endpoints are called, and only attributes declared in its manifest are stored; https is required.
- **Updates**: at launch and every few hours OutreachDex asks GitHub Releases whether a newer version exists, and downloads it in the background. No workspace data is sent. Updates are signed; the app verifies the signature before installing and keeps running the current version until you restart.
- **Untrusted content**: the text of emails, web pages and source events is only evidence for the assistant and is never executed as instructions.

# Mail

## Connect a mailbox

In **Settings → Mail accounts** add:

- **IMAP · incoming**: server, port, user name, password or app password, folder; you can turn on "Sync automatically" and "Download attachments".
- **SMTP · outgoing**: server, port, sender address and name.

Gmail, iCloud, QQ Mail and others usually require an "app password" generated in the mailbox settings; your login password will not work. After adding an account click "Test" to check the connection. Passwords are stored encrypted on this computer and never written to workspace files.

## Sync and filing

- On the **Mail** page click "Sync now", or enable automatic sync. The Agent can also sync when you ask it to.
- Mail whose sender or recipient matches a contact's email channel goes into that person's activity automatically.
- For mail not yet filed, choose the person under "Assign to contact…". The address is added to their channels, and future mail matches automatically.
- Each email is saved as a Markdown file under `mail/` in the workspace; open it to read and annotate. Re-syncing never overwrites your annotations.

## Bounces, unsubscribes and auto-replies are recognized automatically

During sync, three kinds of mail are recognized from headers and keywords (no model is used):

| Mail | Handling |
|---|---|
| **Bounce** (delivery failure from mailer-daemon and similar) | The failed address joins the do-not-contact list (`suppression.txt`), pending drafts to that address are discarded, and a "bounce" entry is added to the contact's activity |
| **Asked not to be contacted** (such as "please don't contact me again", "unsubscribe", "STOP") | The address joins the do-not-contact list, the stage changes to "Do not contact", this person's pending drafts are discarded, and a note is left in the activity |
| **Auto-reply / out of office** | Recorded only; it does not count as a reply, does not change the stage and does not appear in "Awaiting your reply" |

If something is recognized wrongly, fix it by hand: remove the address from `suppression.txt` and change the stage back on the person page.

## Judge the intent of a reply

In Activity, click "Agent reply" next to an incoming email. The Agent on the right judges the person's intent (interested, has questions, wants to schedule a time, not a good time now, not interested…), explains why, and saves a reply pending review. The email content is treated only as data, never as instructions; nothing is replied to, sent or re-staged automatically.

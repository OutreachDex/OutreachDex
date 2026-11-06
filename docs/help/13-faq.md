# FAQ

**The Agent on the right cannot start a session, or says it cannot find the command line**
Make sure Claude Code or Codex is installed and signed in on this computer (it runs normally in a terminal), and that **Settings → Assistant** shows "Detected". You can also switch to the other assistant in the Agent's "Sessions" list. If it is installed in an unusual location, set the path under "Advanced · custom assistant programs".

**Mailbox connection fails**
Most mailboxes need an "app password", and IMAP / SMTP must be enabled in the mailbox settings. Click "Test" in **Settings → Mail accounts** to see the exact reason. Fill in "Proxy URL (optional)" if you need a proxy.

**Source check fails**
Fix the backend endpoints item by item according to the check results — for example pagination does not advance, a field has the wrong format, or the manifest is missing. Developers can re-test from the command line with `globalcontact source check`.

**After syncing, some people are under "To confirm"**
Their email matches one of your contacts, but the backend has not verified that email. Confirm whether it is the same person; after you choose "Not them" you will not be asked again.

**A draft shows "Check the send result"**
The connection dropped while sending, so it is unknown whether the email went out. Check your mailbox's Sent folder; the draft is locked to avoid sending twice.

**Sending refused: on the do-not-contact list / daily limit reached**
These protections prevent bothering people. If you really need to get in touch, remove the address from `suppression.txt` in the workspace, or change `[outreach] daily_limit` in `globalcontact.toml`.

**Opening a workspace says it is open in another process**
A workspace can only be used by one window or one command-line import at a time. Close the other window first.

**Changes to globalcontact.toml have no effect**
Reopen the workspace. If the format is wrong you are told exactly where the error is.

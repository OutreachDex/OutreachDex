# Drafts, review and sending

Sending is always done by you: **edit → approve → choose an account and confirm the send**.

## Edit and approve

- On the person page click "Write draft", or open a draft the assistant wrote.
- You can pick a **reference** draft ("Choose a reference…") to compare side by side, for example when two assistants each wrote a version.
- When you close the dialog or switch pages, unsaved edits are kept for this session and the draft shows "Unsaved edits". Keeping is not saving; you are reminded before quitting the app.
- Click "Save and approve" to move the draft to approved. **Any change after approval requires approving again.**

## Hints during review

- **AI original**: drafts written by the assistant keep the original text. After you edit, choose "AI original (before edits)" as the reference to compare side by side.
- **May be stale**: if the person wrote again after the draft was created, the draft shows "May be stale". Before sending or marking it sent you must tick the box confirming you have read the latest message, and the backend checks again.
- **Copy check**: hints below the body flag cliché openings, AI-sounding words, excessive length, too many questions or links, and too many exclamation marks or dashes. These are hints only; nothing is changed automatically.
- **Unreplaced placeholders**: such as `{{name}}` or `[Name]` are blocked and cannot be sent.

## Make the assistant write more like you

While reviewing a draft, click "**Remember a writing preference**" below the body, for example "Skip the small talk; state the purpose up front" or "Keep messages under 100 words".

- Preferences are saved in the workspace's `preferences.md`;
- later analysis, drafting and list preparation all happen in the Agent terminal and all follow them;
- view, edit or delete them in **Settings → Writing preferences**.

## Recipient check

An email draft must go to one of the person's known addresses: the email on their profile, an email channel, or an email from a linked source. If the address is mistyped, or the Agent used one that does not match, a yellow note appears under the recipient with the known addresses; click one to fix it. To send to a new address anyway, you confirm once more before sending.

## Sending

- **Email**: for an approved email draft, choose a sending account (SMTP), click "Send email" and confirm once more in the dialog.
- **Other channels** (X, Instagram, WeChat, WhatsApp, TikTok, Facebook / Messenger, LinkedIn and so on): the app does not send on your behalf and does not connect to these platforms. Click "Copy body", send it yourself on that platform, then come back and click "Mark as sent".

Before sending, the app always checks these three things; if any fails, nothing is sent:

1. the recipient address is not on the do-not-contact list (`suppression.txt` in the workspace);
2. the contact's stage is not "Do not contact";
3. messages from lists have not exceeded the daily limit.

When you need to pause everything (for example bounces are increasing or you want to check the sending setup), tick "**Pause all sending**" in **Settings → Assistant**: sending email and "Mark as sent" are both refused, drafts can still be edited and approved, and the top bar keeps showing "Sending paused".

If the network drops while sending, the draft is marked "Check the send result" and locked to avoid sending twice. Check your mailbox's Sent folder.

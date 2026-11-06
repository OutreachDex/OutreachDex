# Lists and cold outreach

Lists help you find **the people worth contacting now** among your contacts and data sources: the assistant researches and drafts for each person, and you review them one by one. Sending is always your decision — **no mass mailing, no automatic sending**.

## Create a list

On the **Lists** page click +, and fill in:

- **Name and purpose**: the purpose tells the assistant why you are contacting these people, for example "Find out where they are stuck and whether they need help".
- **Candidates from**: existing contacts, or the people in a data source (including people not linked yet).
- **Channel, analysis purpose, business context**. The channel can be email, X, Instagram, WeChat, WhatsApp, TikTok, Facebook / Messenger, LinkedIn or other. Only email is sent by the app; for every other channel you copy the draft, send it yourself, then click "Mark as sent".
- **Filters** (all must match):
  - source attributes: for example plan = free, credits left ≤ 0;
  - events: for example "credits exhausted" within 30 days, no "purchased credits";
  - event counts: for example "task failed" at least 2 times within 14 days, "purchased credits" exactly 0 times (leave days empty for all time);
  - contact status: stage, never contacted, last contacted more than N days ago;
  - contact fields: for example project or affiliation.

While editing, the number of matching people and the first few are shown live. People marked "Do not contact" are excluded automatically. Lists are saved in the workspace's `lists/` folder.

## Batch preparation

Check some people and click "Prepare N selected", or just click "Prepare N people" for everyone not prepared yet. They are queued and handed to the Agent on the right, which handles them **one at a time** (every step is shown in the terminal):

1. people from a source who are not linked yet are created as contacts first;
2. the reasons they were selected are verified (source facts and activity);
3. if contacting them now would genuinely help them, a message for review is drafted, opening with why you are getting in touch now; if the information is insufficient, nothing is drafted and the reason is written in the member table.

Preparation uses the Claude Code / Codex you are signed in to on this computer and consumes your model quota. If the Agent is interrupted, click "Let the Agent continue" above the list to carry on; you can also "Clear queue". Why each person was selected is shown in the member table.

## Review one by one

Click "Review one by one":

- On the left, "Why reach out now" and the source facts; on the right, the draft.
- Actions: **Save**, **Approve and next**, **Skip**, **Do not contact**.
- Approving does not send. To send, click "Go to send…" in the draft, choose a sending account and confirm.

## Avoid bothering people

- **Do not contact**: the address is written to `suppression.txt` in the workspace, the contact's stage changes to "Do not contact" and this draft is discarded. Every list excludes them from then on, and sending is refused.
- **Daily limit**: the maximum number of list messages sent per day (default 20; both emails and manual "Mark as sent" count). Change it with `[outreach] daily_limit` in `globalcontact.toml`. The top-right of the list page shows how many have been sent today.

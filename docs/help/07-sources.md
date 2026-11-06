# Connect data sources

A data source lets your business backend (for example your product's user admin or a ticketing system) hand "people" and "events" to OutreachDex. You can then see each person's plan, usage and feedback, and analyze and reach out based on them.

## What your backend needs to do

The backend implements 3 read-only endpoints following the **GCSP/1** spec: `/manifest`, `/people` and `/events`, and issues a read-only token. The full spec is `docs/gcsp.md` in the project repository, with reference implementations in Go and Node. Developers can test from the command line first:

```sh
globalcontact source check https://your-backend.example.com/gcsp --token <token>
```

## Connect

1. Open the **Sources** page → "Connect source".
2. Enter the "Source URL" and token; the "Header" defaults to `Authorization` — change it only if your backend uses a different header.
3. Click "Check": the app checks each endpoint against the spec and previews the first few people.
   - **Failed** items must be fixed in the backend;
   - **Warning** items still let you connect, but the backend should fix them later.
4. Click "Connect and sync". After that it syncs incrementally at the configured interval (60 minutes by default), and you can click "Sync now" any time.

The URL must be `https://` (`http://127.0.0.1` is allowed for local testing). The token is stored encrypted on this computer. OutreachDex only calls the read-only endpoints and never writes anything to your backend.

## Identity matching

Each synced person is matched to a contact by these rules:

| Situation | Result |
|---|---|
| An email verified by the backend matches a contact | **Auto-linked** |
| The email matches, but the backend has not verified it | Goes to "**To confirm**" for you to decide whether it is the same person |
| No match | Stays "**Unlinked**"; you can "Create contact" or "Link to…" an existing contact |

- Matching names are never merged automatically.
- After you choose "Not them", re-syncing will not suggest it again.
- Once linked, this person's source events are added to their activity and the source email joins their channels.

## Understand users who are not contacts yet

On the Sources page, click anyone (including unlinked people) to see their facts, how their attributes changed, and their events; click an event to open the original. Click "Ask Agent" and the Agent on the right reads all of this, summarizes their stage, payments, problems and activity, and judges whether it is worth reaching out now, without making them a contact first. If it is, use "Create contact" and draft from there.

The Agent can also search all source users, for example: "Which free users ran out of credits but are still active?"

## Source facts

For linked people, the "Source facts" card on the person page shows the attributes the backend provides, such as plan, credits or sign-up date, labelled with the source and sync time. Attributes the backend marks as personal information are flagged. Only attributes declared in the backend's manifest are stored.

## Changes

On every sync OutreachDex compares the person's attributes and records changes, for example "plan: free → pro" or "credits left: 20 → 0".

- Click "Changes" in the "Source facts" card on the person page to see recent changes.
- The full record is stored in `sources/<source>/people/<id>.md` in the workspace: the current attributes at the top and a timestamped change log below. Open it to read, or hand it to the Agent to analyze.
- Only attributes declared in the backend's manifest are recorded.

## Language of new contacts

Contacts created from a source get a language, and the Agent drafts in it. The order is: the person's language from the source → "Language for new contacts" in the source's "…" menu → the source's declared default → the workspace default. For example, if the users of a source mostly speak English, set "Language for new contacts" to English. Review shows a hint when a draft's language does not match the contact's.


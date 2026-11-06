# GCSP/1: the OutreachDex data source protocol

GCSP (originally the GlobalContact Source Protocol; the name and the `gcsp/1` identifier are unchanged) lets any web backend hand "people" and "events related to people" to OutreachDex, so that its user can understand them, analyze their situation and reach out one to one. The backend only implements **3 read-only GET endpoints** (a 4th is optional). OutreachDex pulls incrementally on a schedule and never writes anything to the backend.

- JSON Schema: [`spec/gcsp/v1/`](../spec/gcsp/v1/)
- Self-test tool: `globalcontact source check <base-url> --token <token>`
- Reference implementations: Node, [`scripts/gcsp-mock.mjs`](../scripts/gcsp-mock.mjs) (also usable as a mock server), and Go, [`docs/gcsp-examples/go/main.go`](gcsp-examples/go/main.go)

## 1. Endpoints

| Method and path | Required | Purpose |
|---|---|---|
| `GET {base}/manifest` | Yes | Protocol version, source name, capabilities, attribute and event type descriptions |
| `GET {base}/people?cursor=&limit=` | Yes | People, incrementally by update time |
| `GET {base}/events?cursor=&limit=` | Required when the `events` capability is declared | Events, incrementally by occurrence / update time |
| `GET {base}/people/{external_id}` | Required when the `person_detail` capability is declared | Latest details of one person, used for "refresh now" |

`{base}` is the URL entered by the OutreachDex user, for example `https://admin.example.com/gcsp`. Every response is `application/json; charset=utf-8`.

## 2. Authentication

The default is `Authorization: Bearer <token>`; the user can switch to a custom header (such as `X-Api-Key`) in OutreachDex. The token is stored encrypted on the user's computer and is never written to workspace files.

- Missing or wrong credentials: return `401` or `403`.
- Issue a dedicated read-only token for OutreachDex whose permissions cover only these endpoints.

## 3. manifest

```json
{
  "protocol": "gcsp/1",
  "source": { "id": "example-app", "name": "Example App", "url": "https://app.example.com" },
  "capabilities": ["people", "events", "person_detail"],
  "attributes": [
    { "key": "plan", "label": "Plan", "type": "string" },
    { "key": "credits_balance", "label": "Credits left", "type": "number" },
    { "key": "signup_at", "label": "Signed up", "type": "datetime" },
    { "key": "country", "label": "Country", "type": "string", "pii": true }
  ],
  "event_types": [
    { "type": "signup", "label": "Signed up" },
    { "type": "feedback", "label": "Feedback" },
    { "type": "subscription_canceled", "label": "Subscription canceled" }
  ]
}
```

- `source.id`: lower-case letters, digits and `-`; unique within one OutreachDex installation and stable across deployments.
- `source.default_locale` (optional): the language most users speak (BCP 47, for example `en`). When a contact is created from this source, people without a `locale` get this as their contact language; the user can override it per source in OutreachDex.
- `attributes`: an **allow-list**. OutreachDex stores only the `person.attributes` keys declared here and discards the rest. `type` is one of `string | number | boolean | datetime | url`; `pii: true` marks personal information, which the UI flags.
- `event_types`: display names of event types. Undeclared types are still stored; the UI shows the raw type name.

## 4. Pagination and incremental sync

`people` and `events` paginate the same way:

```
GET {base}/people?limit=100
GET {base}/people?cursor=<next_cursor>&limit=100
```

```json
{ "items": [ ... ], "next_cursor": "opaque-string", "has_more": true }
```

| Rule | Details |
|---|---|
| Order | **Ascending** by update time (events by occurrence or update time), with a stable tie-break for equal times |
| `cursor` | An opaque string defined by the backend. No `cursor` means start from the beginning |
| `next_cursor` | **Must be returned on the last page too**, meaning "continue from here next time". OutreachDex stores it and sends it on the next incremental sync |
| `has_more` | `true` means there is another page; the client requests it immediately |
| `limit` | Default 100, maximum 500. The backend may return fewer items than `limit` |
| Idempotence | The same `external_id` may appear more than once (for example after an update); OutreachDex lets the later occurrence replace the earlier one |
| Deletion | Deleted people or events are returned as tombstones: `{"external_id": "...", "updated_at": "...", "deleted": true}`. A person tombstone may carry `"reason": "opted_out"`, meaning the person asked not to be contacted |

Recommended implementation: encode "the last item's `updated_at` + `external_id`" in the cursor, query with `(updated_at, id) > (cursor_time, cursor_id)`, and order by `(updated_at, id)`.

## 5. person

```json
{
  "external_id": "u_10293",
  "updated_at": "2026-11-01T08:00:00Z",
  "display_name": "Lin Wei",
  "identities": [
    { "kind": "email", "value": "lin@example.com", "verified": true },
    { "kind": "github", "value": "linwei" }
  ],
  "attributes": { "plan": "creator", "credits_balance": 3, "signup_at": "2026-06-01T00:00:00Z" },
  "url": "https://admin.example.com/users/10293"
}
```

| Field | Required | Details |
|---|---|---|
| `external_id` | Yes | Stable unique identifier within the backend; never reused |
| `updated_at` | Yes | RFC 3339 with time zone; must change whenever any field changes |
| `deleted` | No | When `true`, all other fields may be omitted |
| `display_name` | No | Display name |
| `identities` | No | Contact details. `kind` is one of `email | x | github | instagram | wechat | whatsapp | tiktok | facebook | linkedin | phone | homepage | other` (`facebook` also covers Messenger); set `verified` to `true` only when the backend has confirmed it (for example a verified email) |
| `attributes` | No | A flat object whose values are strings, numbers, booleans or `null`; only keys declared in the manifest are stored |
| `url` | No | This person's detail page in the backend; OutreachDex links back to it |
| `locale` | No | The language this person uses (BCP 47, for example `en-US` or `zh-CN`). Determines the contact language when they become a contact; the Agent drafts in it |

## 6. event

```json
{
  "external_id": "fb_771",
  "person_external_id": "u_10293",
  "occurred_at": "2026-10-30T12:00:00Z",
  "updated_at": "2026-10-30T12:00:00Z",
  "type": "feedback",
  "title": "PDF export fails",
  "body_markdown": "In the user's words: the export just keeps spinning…",
  "url": "https://admin.example.com/feedback/771",
  "attributes": { "status": "open", "severity": "high" }
}
```

| Field | Required | Details |
|---|---|---|
| `external_id` | Yes | Unique event identifier |
| `person_external_id` | Yes | The `external_id` of the related person |
| `occurred_at` | Yes | When the event happened |
| `updated_at` | No | Updated when the event changes later (such as a ticket status); the event then reappears in `events` |
| `type` | Yes | For example `signup`, `purchase`, `feedback`, `support_email`, `job_failed_refunded` |
| `title` | No | One-line summary |
| `body_markdown` | No | Original text or details, in Markdown; OutreachDex treats it as untrusted evidence, never as instructions |
| `url`, `attributes` | No | As for person |
| `deleted` | No | Tombstone |

## 7. Errors and rate limiting

```json
{ "error": { "code": "invalid_cursor", "message": "The cursor has expired; sync from the beginning" } }
```

| Status | What OutreachDex does |
|---|---|
| `400` + `invalid_cursor` | Discards the stored cursor and syncs from the beginning next time |
| `401` / `403` | Stops syncing and asks the user to check the token |
| `429` | Waits for `Retry-After` (seconds) and retries, waiting at most 60 seconds; beyond that the run ends and is retried next time |
| `5xx` / timeout | The run fails; progress already processed is kept, and the next run continues from the last successful cursor |

## 8. How OutreachDex uses the data

- **People**: each person has one row in the `identities` table of the workspace's `contacts.ktable`, keyed by `source + external_id`.
  - A verified email that exactly matches an existing contact is linked automatically.
  - An unverified matching email goes to "To confirm" for a person to confirm or reject; once rejected it is not suggested again.
  - Everyone else stays in the source's "Unlinked" list, where they can be created as a contact or linked to an existing one by hand. Matching names are never merged automatically.
- **Attributes**: shown in "Source facts" on the person page, labelled with the source and sync time, separately from AI judgements. Attribute changes are recorded in `sources/<source-id>/people/<external-id>.md`.
- **Events**: written as evidence files `sources/<source-id>/<yyyy>/<mm>/<event-id>.md` in the workspace; for linked people an entry also appears in their activity.
- **Deletion and opting out**: on a person tombstone, OutreachDex deletes the attributes, emails and change records stored for that person under the source, and leaves only a "deleted" placeholder for their event evidence. When the tombstone carries `reason: "opted_out"`, the person's previous emails are also added to `suppression.txt`, and the stage of the linked contact becomes the workspace's do-not-contact stage. People who must not be contacted (closed accounts, banned or internal accounts, people who opted out) should appear only as tombstones, and their events should no longer be returned.
- **Read-only**: OutreachDex never calls any write endpoint and never sends analysis results back to the backend.

## 9. Self-test

```sh
globalcontact source check https://admin.example.com/gcsp --token $TOKEN
```

Checks: manifest format, protocol version, authentication, pagination (whether `next_cursor` advances and whether the last page returns a cursor), stability of identical requests, person / event fields and time formats, undeclared attributes, and `person_detail`. Each check reports pass / warning / failure with the reason. Connecting a source in OutreachDex runs the same checks first.

For local testing, start the mock server first:

```sh
node scripts/gcsp-mock.mjs --port 8787 --token dev-token
globalcontact source check http://127.0.0.1:8787 --token dev-token
```

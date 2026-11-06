# People and the person page

## The People page

All contacts are shown as a table.

- **Views**: All, Awaiting reply (they replied after your last message), Follow up due (no reply for more than the configured number of days after you wrote).
- **Search and filter**: search by name, affiliation or email; filter by stage.
- **Keyboard**: ↑ ↓ or J / K to move, Enter to open.
- **Bulk actions**: check several people (the header checkbox selects the whole current list) to change their stage, schedule a follow-up (tomorrow / 3 days / 1 week / 1 month / clear), or export CSV.
- **Export**: "Export" writes every field of the current list to CSV (with a BOM, so Excel opens it correctly).
- **Table editor**: tidy up contacts, channels, source identities and activity in bulk directly in the table editor.

## The person page

Content is on the left; properties and contact channels are on the right. Besides email, homepage, X, Instagram and WeChat, channels can also hold WhatsApp, TikTok, Facebook / Messenger, LinkedIn and other accounts.

| Section | Content |
|---|---|
| Overview | Next step, source facts, hook, AI background summary, Agent memory, things to note, pending drafts, recent activity |
| Draft | All drafts for this person, filterable by channel |
| Activity | Emails, recorded notes and feedback, source events |
| Dossier | This person's Markdown dossier, editable in place |

- **Facts and inferences are separate**: items tagged "Profile" or "Sources" are recorded facts; items tagged "AI inferred" are the assistant's judgement — verify them before relying on them.
- **Switch contacts**: ↑ ↓ in the top bar, or K / J on the keyboard, go to the previous / next person; Esc returns to the list.
- **Stage**: change it in the properties on the right. After you send a message it moves to a "sent" stage automatically, and to a "replied" stage when they reply. Stage names can be changed in the workspace configuration (see [Workspace and configuration](10-workspace.md)).

## Profile: custom fields

"Profile" in the right column of the person page lists every field for this person. Click to edit; **changes save automatically when you leave the input**.

- **Add fields**: phone, title, birthday, tags, relationship and so on — click "Add field" to create your own.
  - Types: Text, Long text, Number, Date, Link, Yes / No, Single select, Multi-select (tags).
  - A new field is added to every contact and stored in the workspace's contacts table.
- **Options**: in a single-select field choose "+ New option…" to add one; in a multi-select field just type, separated by commas, and new tags are added to the options automatically.
- **Fields maintained by the app**: last contacted / last reply time, AI summary and the ID are maintained by the app and are not edited here.
- **Change history**: "Change history" at the bottom of the right column lists when each field changed and from what to what.

## Merge duplicate contacts

When one person has two records, open the person page of the record you want to keep, click "Merge…" and choose the other record:

- Before merging you see what will move: channels, activity, source identities, drafts, recorded notes, and the empty fields that will be filled in.
- When merging:
  - channels are de-duplicated;
  - empty fields are filled from the other record; existing values are kept;
  - the newer last contacted / last reply time wins;
  - the merged record's dossier text is appended at the end.
- The merged record is deleted and this cannot be undone (restore the workspace from git or a backup if needed). Approved drafts must be approved again after a merge.

## Record notes and feedback

On the person page click "Log info / feedback" to paste a web excerpt, a social post, a user's feedback in their own words, or the result of a call, and note the source. Choose "Save and ask the Agent" and the Agent on the right analyzes the next step with this note in mind.

Saving only a link does not fetch the page; please paste the original text.

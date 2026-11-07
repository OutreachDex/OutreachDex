# Import contacts

## Import from CSV / TSV

1. On the **People** page click "Import", choose "Contacts (CSV / TSV)" and pick a file. In Excel, first use "Save as CSV UTF-8".
2. The app previews the first few rows and maps columns to fields automatically by their names. English and Chinese column names are both recognized, for example Name / 姓名, Email / 邮箱, Affiliation / 公司, Notes / 备注.
3. Adjust the field above each column, or choose "Don't import". One column must map to "Name".
4. Click "Import".

Import rules:

- Existing contacts are matched first by **email**, then by **name**; on a match the non-empty fields are updated, otherwise a new contact is created.
- Email, homepage, X and Instagram are also added as contact channels.
- WhatsApp, TikTok, Facebook (a column named Messenger also works) and LinkedIn columns are added only as contact channels and do not take up contact fields.
- Values in a "Stage" column must be stage names from the workspace configuration; others are ignored with a warning.

## Import WhatsApp and LinkedIn conversations

Conversation history comes from files you export yourself. OutreachDex never signs in to WhatsApp or LinkedIn, so there is no risk to your accounts and no rate limit to hit.

On the **People** page click "Import" and choose:

- **WhatsApp chat**: in WhatsApp, open a one-to-one chat, tap the person's name, choose "Export chat" → "Without media", and pick the `.zip` (iPhone) or `.txt` (Android). Group chats can't be imported; export the chat with each person instead.
- **LinkedIn messages**: on LinkedIn go to Settings → Data privacy → "Get a copy of your data", select Messages and Connections, and pick the `.zip` LinkedIn emails you (it can take up to a day).

The preview shows each one-to-one conversation and the contact it will be filed under:

- People are matched by their WhatsApp number or LinkedIn profile among a contact's channels, then by an email from your LinkedIn connections, then by exact name. Change the contact, create a new one, or choose "Don't import".
- If the app can't tell which participant is you, it asks. For WhatsApp exports where every date could be read either way (01/02), choose whether the day or the month comes first.

What the import writes:

- Messages are saved by day in `chats/whatsapp/…` or `chats/linkedin/…` in the workspace, one Markdown file per conversation and day.
- The person's timeline gets one entry per day and direction ("WhatsApp: last message (+3 more)"); open it to read the whole day.
- "Last contacted" and "Last reply" move forward, and the stage follows whoever wrote last. People in a closed or do-not-contact stage keep their stage.
- The WhatsApp number or LinkedIn profile is added to the person's channels, so the next import matches automatically.

Import a newer export of the same chat or archive whenever you like: messages already imported are recognized and only new ones are added.

## Import from the command line

```sh
globalcontact import contacts.csv --workspace /path/to/workspace
```

The target workspace must not be in use by the desktop app. When creating a new workspace you can add `--template user-relationships` and similar to choose a template.

## Tidy up in the table

"Table editor" on the **People** page opens the contacts table, where you can add rows and change fields directly — handy for bulk clean-up.

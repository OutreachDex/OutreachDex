# Import contacts

## Import from CSV / TSV

1. On the **People** page click "Import" and choose a file. In Excel, first use "Save as CSV UTF-8".
2. The app previews the first few rows and maps columns to fields automatically by their names. English and Chinese column names are both recognized, for example Name / 姓名, Email / 邮箱, Affiliation / 公司, Notes / 备注.
3. Adjust the field above each column, or choose "Don't import". One column must map to "Name".
4. Click "Import".

Import rules:

- Existing contacts are matched first by **email**, then by **name**; on a match the non-empty fields are updated, otherwise a new contact is created.
- Email, homepage, X and Instagram are also added as contact channels.
- WhatsApp, TikTok, Facebook (a column named Messenger also works) and LinkedIn columns are added only as contact channels and do not take up contact fields.
- Values in a "Stage" column must be stage names from the workspace configuration; others are ignored with a warning.

## Import from the command line

```sh
globalcontact import contacts.csv --workspace /path/to/workspace
```

The target workspace must not be in use by the desktop app. When creating a new workspace you can add `--template user-relationships` and similar to choose a template.

## Tidy up in the table

"Table editor" on the **People** page opens the contacts table, where you can add rows and change fields directly — handy for bulk clean-up.

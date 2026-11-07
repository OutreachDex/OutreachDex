//! Imports one-to-one conversations from WhatsApp chat exports and LinkedIn data archives.
//! The person downloads the export themselves; nothing signs in to their accounts.
//!
//! Messages are written as day transcripts, `chats/<channel>/<conversation>/<yyyy>/<mm>/<dd>.md`,
//! one block per message keyed by a hash, so importing the same or a longer export again only
//! adds what is new. The timeline gets one row per person, day and direction (not per message),
//! and every import writes the interactions and contacts tables once, in one batch each.
use crate::chat_parse::{self, DateOrder, Message, Person};
use crate::contacts::{Row, slugify};
use crate::Service;
use chrono::{DateTime, Local, NaiveDate, Utc};
use gc_core::config::Role;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    WhatsApp,
    LinkedIn,
}

impl Kind {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "whatsapp" => Ok(Self::WhatsApp),
            "linkedin" => Ok(Self::LinkedIn),
            _ => Err("Choose a WhatsApp chat export or a LinkedIn data archive.".into()),
        }
    }
    fn channel(self) -> &'static str {
        match self {
            Self::WhatsApp => "whatsapp",
            Self::LinkedIn => "linkedin",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::WhatsApp => "WhatsApp",
            Self::LinkedIn => "LinkedIn",
        }
    }
}

/// A one-to-one conversation ready to import: the partner and each message's direction.
struct Planned {
    key: String,
    title: String,
    partner: Person,
    /// `(message, outgoing)`.
    messages: Vec<(Message, bool)>,
}

struct Plan {
    conversations: Vec<Planned>,
    /// Group conversations, which are not imported.
    groups: usize,
    spam: usize,
    /// Candidates for "which one is you" when the owner couldn't be determined.
    me_choices: Vec<Value>,
    me: Option<String>,
    date_order: Option<DateOrder>,
    order_certain: bool,
}

fn short_hash(value: &str) -> String {
    Sha256::digest(value.as_bytes()).iter().take(4).map(|b| format!("{b:02x}")).collect()
}

fn same_name(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim(), b.trim());
    !a.is_empty() && a.eq_ignore_ascii_case(b)
}

fn same_phone(a: &str, b: &str) -> bool {
    let (a, b) = (chat_parse::phone_digits(a), chat_parse::phone_digits(b));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    // Saved with or without the country code or a national 0: compare the last nine digits.
    let n = a.len().min(b.len()).min(9);
    n >= 8 && a[a.len() - n..] == b[b.len() - n..]
}

fn plan(path: &Path, kind: Kind, me: Option<&str>, order: Option<DateOrder>) -> Result<Plan, String> {
    match kind {
        Kind::WhatsApp => plan_whatsapp(path, me, order),
        Kind::LinkedIn => plan_linkedin(path, me),
    }
}

fn plan_whatsapp(path: &Path, me: Option<&str>, order: Option<DateOrder>) -> Result<Plan, String> {
    let export = chat_parse::read_whatsapp(path, order)?;
    let authors: Vec<&str> = export.authors.iter().map(|(a, _)| a.as_str()).collect();
    let me_choices = export.authors.iter().map(|(a, n)| json!({"id": a, "name": a, "messages": n})).collect();
    let mut plan = Plan {
        conversations: Vec::new(),
        groups: 0,
        spam: 0,
        me_choices,
        me: None,
        date_order: Some(export.order),
        order_certain: export.order_certain,
    };
    if authors.len() > 2 {
        plan.groups = 1;
        return Ok(plan);
    }
    let title = export.title.clone();
    let is_title = |a: &str| title.as_deref().is_some_and(|t| same_name(a, t) || same_phone(a, t));
    // The owner: chosen, or the author who isn't the chat partner named in the file name.
    let me = match me.filter(|m| !m.is_empty()) {
        Some(m) if authors.contains(&m) => Some(m.to_owned()),
        Some(_) => return Err("Choose which of the chat's authors is you.".into()),
        None => match authors.as_slice() {
            [a, b] if is_title(a) && !is_title(b) => Some((*b).to_owned()),
            [a, b] if is_title(b) && !is_title(a) => Some((*a).to_owned()),
            [a] if title.is_some() && !is_title(a) => Some((*a).to_owned()),
            [a] if is_title(a) => Some(String::new()),
            _ => None,
        },
    };
    let Some(me) = me else { return Ok(plan) };
    let partner = authors
        .iter()
        .find(|a| **a != me)
        .map(|a| (*a).to_owned())
        .or(title.clone())
        .ok_or("Only your own messages are in this chat, and the file name doesn't say who it was with. Rename the file to “WhatsApp Chat with <name>.txt”.")?;
    let messages = export.messages.into_iter().map(|m| {
        let out = !me.is_empty() && m.author == me;
        (m, out)
    });
    plan.conversations.push(Planned {
        key: chat_parse::whatsapp_key(&partner),
        title: partner.clone(),
        partner: Person { handle: chat_parse::phone_digits(&partner), name: partner, ..Person::default() },
        messages: messages.collect(),
    });
    plan.me = Some(me);
    Ok(plan)
}

fn plan_linkedin(path: &Path, me: Option<&str>) -> Result<Plan, String> {
    let export = chat_parse::read_linkedin(path)?;
    let mut counts: HashMap<String, (usize, Person)> = HashMap::new();
    for c in &export.conversations {
        for p in &c.participants {
            let id = if p.handle.is_empty() { p.name.clone() } else { p.handle.clone() };
            counts.entry(id).or_insert_with(|| (0, p.clone())).0 += 1;
        }
    }
    let mut choices: Vec<_> = counts.iter().map(|(id, (n, p))| (id.clone(), *n, p.name.clone())).collect();
    choices.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)));
    let me_choices = choices.iter().take(8).map(|(id, n, name)| json!({"id": id, "name": name, "conversations": n})).collect();
    let mut plan = Plan {
        conversations: Vec::new(),
        groups: 0,
        spam: export.spam_skipped,
        me_choices,
        me: None,
        date_order: None,
        order_certain: true,
    };
    let owner: Option<Person> = match me.filter(|m| !m.is_empty()) {
        Some(id) => Some(counts.get(id).map(|(_, p)| p.clone()).ok_or("Choose which participant is you.")?),
        None => export.me.clone(),
    };
    let Some(owner) = owner else { return Ok(plan) };
    let is_owner = |p: &Person| {
        if owner.handle.is_empty() || p.handle.is_empty() { same_name(&p.name, &owner.name) } else { p.handle == owner.handle }
    };
    for c in export.conversations {
        let others: Vec<&Person> = c.participants.iter().filter(|p| !is_owner(p)).collect();
        let [partner] = others.as_slice() else {
            plan.groups += usize::from(others.len() > 1);
            continue;
        };
        let mut partner = (*partner).clone();
        if let Some(known) = export.connections.get(&partner.handle) {
            partner.email.clone_from(&known.email);
            partner.affiliation.clone_from(&known.affiliation);
            if partner.name.is_empty() {
                partner.name.clone_from(&known.name);
            }
        }
        let messages = c
            .messages
            .into_iter()
            .map(|m| {
                let out = if m.author_url.is_empty() || owner.handle.is_empty() {
                    same_name(&m.author, &owner.name)
                } else {
                    m.author_url == owner.handle
                };
                (m, out)
            })
            .collect();
        let title = if c.title.is_empty() { partner.name.clone() } else { c.title };
        plan.conversations.push(Planned { key: c.key, title, partner, messages });
    }
    plan.me = Some(if owner.handle.is_empty() { owner.name } else { owner.handle });
    Ok(plan)
}

/// The existing contact a conversation partner belongs to, and what matched.
fn find_contact(kind: Kind, partner: &Person, contacts: &[Row], channels: &[Row]) -> Option<(String, &'static str)> {
    let channel = |r: &&Row| match kind {
        Kind::WhatsApp => r.text("Kind") == "whatsapp" && same_phone(r.text("Value"), &partner.name),
        Kind::LinkedIn => {
            r.text("Kind") == "linkedin"
                && !partner.handle.is_empty()
                && chat_parse::normalize_linkedin(r.text("Value")) == partner.handle
        }
    };
    if let Some(row) = channels.iter().find(channel) {
        return Some((row.text("Contact").to_owned(), kind.channel()));
    }
    let email = partner.email.trim().to_lowercase();
    if !email.is_empty() {
        let by_email = contacts
            .iter()
            .find(|r| r.text("Email").eq_ignore_ascii_case(&email))
            .map(|r| r.text("Slug").to_owned())
            .or_else(|| {
                channels
                    .iter()
                    .find(|r| r.text("Kind") == "email" && r.text("Value").eq_ignore_ascii_case(&email))
                    .map(|r| r.text("Contact").to_owned())
            });
        if let Some(slug) = by_email {
            return Some((slug, "email"));
        }
    }
    contacts
        .iter()
        .find(|r| same_name(r.text("Name"), &partner.name) || same_name(r.text("Native name"), &partner.name))
        .map(|r| (r.text("Slug").to_owned(), "name"))
}

/// One message block of a day transcript, as stored.
struct Block {
    at: String,
    outgoing: bool,
    text: String,
    body: String,
}

const MARK: &str = "<!-- m:";

fn render(message: &Message, outgoing: bool) -> Block {
    let at = message.at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let time = message.at.with_timezone(&Local).format("%H:%M");
    let author = if outgoing { "Me" } else { message.author.as_str() };
    // A message must not be able to start another block or close the marker.
    let text = message.text.replace("<!--", "&lt;!--").replace("-->", "--&gt;");
    let body = format!("**{time} · {author}**\n\n{}\n", if text.trim().is_empty() { "…" } else { text.trim_end() });
    Block { at, outgoing, text: message.text.clone(), body }
}

/// Blocks already in a day transcript, by key.
fn existing_blocks(text: &str) -> BTreeMap<String, Block> {
    let mut out = BTreeMap::new();
    for part in text.split(MARK).skip(1) {
        let Some((head, rest)) = part.split_once("-->") else { continue };
        let mut fields = head.split_whitespace();
        let (Some(key), Some(at), Some(dir)) = (fields.next(), fields.next(), fields.next()) else { continue };
        let body = rest.trim_start_matches('\n').trim_end().to_owned() + "\n";
        let text = body.split_once("\n\n").map_or("", |(_, t)| t).trim().to_owned();
        out.insert(key.to_owned(), Block { at: at.to_owned(), outgoing: dir == "out", text, body });
    }
    out
}

fn snippet(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("…");
    let mut out: String = line.chars().take(70).collect();
    if line.chars().count() > 70 {
        out.push('…');
    }
    out
}

/// A timeline row for one person, day and direction.
struct DayRow {
    slug: String,
    message_id: String,
    at: String,
    outgoing: bool,
    title: String,
    file: String,
}

impl Service {
    /// What an export contains and how its conversations would be filed, without writing.
    pub async fn chat_preview(&self, args: &Value) -> Result<Value, String> {
        let kind = Kind::parse(args["kind"].as_str().unwrap_or(""))?;
        let path = Path::new(args["path"].as_str().ok_or("path")?);
        let order = args["date_order"].as_str().and_then(DateOrder::parse);
        let plan = plan(path, kind, args["me"].as_str(), order)?;
        let contacts = self.rows("contacts").await?;
        let channels = self.rows("channels").await?;
        let names: HashMap<&str, &str> = contacts.iter().map(|r| (r.text("Slug"), r.text("Name"))).collect();
        let conversations: Vec<Value> = plan
            .conversations
            .iter()
            .map(|c| {
                let matched = find_contact(kind, &c.partner, &contacts, &channels);
                let outgoing = c.messages.iter().filter(|(_, out)| *out).count();
                json!({
                    "key": c.key,
                    "title": c.title,
                    "partner": {"name": c.partner.name, "handle": c.partner.handle, "email": c.partner.email, "affiliation": c.partner.affiliation},
                    "messages": c.messages.len(),
                    "outgoing": outgoing,
                    "incoming": c.messages.len() - outgoing,
                    "first": c.messages.iter().map(|(m, _)| m.at).min().map(|t| t.to_rfc3339()),
                    "last": c.messages.iter().map(|(m, _)| m.at).max().map(|t| t.to_rfc3339()),
                    "match": matched.map(|(slug, by)| json!({"slug": slug, "name": names.get(slug.as_str()).copied().unwrap_or(""), "by": by})),
                })
            })
            .collect();
        Ok(json!({
            "kind": kind.channel(),
            "file": path.file_name().map(|n| n.to_string_lossy().into_owned()),
            "me": plan.me,
            "meChoices": plan.me_choices,
            "dateOrder": plan.date_order.map(DateOrder::as_str),
            "dateOrderCertain": plan.order_certain,
            "groups": plan.groups,
            "spam": plan.spam,
            "conversations": conversations,
        }))
    }

    /// Imports the conversations `assign` maps to a contact slug or `"new"`; others are skipped.
    pub async fn chat_import(&self, args: &Value) -> Result<Value, String> {
        let kind = Kind::parse(args["kind"].as_str().unwrap_or(""))?;
        let path = Path::new(args["path"].as_str().ok_or("path")?);
        let order = args["date_order"].as_str().and_then(DateOrder::parse);
        let plan = plan(path, kind, args["me"].as_str(), order)?;
        plan.me.as_ref().ok_or("Choose which participant is you.")?;
        let assign = args["assign"].as_object().cloned().unwrap_or_default();
        let source = format!("Import: {} {}", kind.label(), if kind == Kind::WhatsApp { "chat" } else { "data" });

        // Contacts and channels first: these take the mutation lock themselves.
        let mut targets: Vec<(&Planned, String)> = Vec::new();
        let mut created = 0;
        let known: Vec<String> = self.rows("contacts").await?.iter().map(|r| r.text("Slug").to_owned()).collect();
        for c in &plan.conversations {
            let slug = match assign.get(&c.key).and_then(Value::as_str).unwrap_or("") {
                "" => continue,
                "new" => {
                    let mut values = Map::new();
                    let name = if c.partner.name.is_empty() { c.title.clone() } else { c.partner.name.clone() };
                    values.insert("Name".into(), json!(name));
                    if !c.partner.affiliation.is_empty() {
                        values.insert("Affiliation".into(), json!(c.partner.affiliation));
                    }
                    created += 1;
                    self.create_contact(values).await?
                }
                slug if known.iter().any(|k| k == slug) => slug.to_owned(),
                slug => return Err(format!("Contact not found: {slug}")),
            };
            match kind {
                Kind::WhatsApp if !c.partner.handle.is_empty() => {
                    self.add_channel(&slug, "whatsapp", &c.partner.name, &source, false).await?;
                }
                Kind::LinkedIn if !c.partner.handle.is_empty() => {
                    self.add_channel(&slug, "linkedin", &format!("https://www.{}", c.partner.handle), &source, false).await?;
                }
                _ => {}
            }
            if kind == Kind::LinkedIn && c.partner.email.contains('@') {
                self.add_channel(&slug, "email", &c.partner.email, "LinkedIn connections", false).await?;
            }
            targets.push((c, slug));
        }

        // Day transcripts: merge with what earlier imports wrote.
        let mut days: Vec<DayRow> = Vec::new();
        let (mut total, mut added, mut files) = (0usize, 0usize, 0usize);
        for (c, slug) in &targets {
            let folder = format!(
                "chats/{}/{}-{}",
                kind.channel(),
                slugify(&c.title).chars().take(40).collect::<String>().trim_end_matches('-'),
                short_hash(&c.key)
            );
            let mut by_day: BTreeMap<NaiveDate, Vec<&(Message, bool)>> = BTreeMap::new();
            for m in &c.messages {
                by_day.entry(m.0.at.with_timezone(&Local).date_naive()).or_default().push(m);
            }
            for (date, messages) in by_day {
                let file = format!("{folder}/{}.md", date.format("%Y/%m/%d"));
                let workspace = &self.docs.workspace;
                let stamp = workspace.stamp(&file).map_err(|e| e.to_string())?;
                let old = match &stamp {
                    Some(_) => String::from_utf8_lossy(&workspace.read(&file, None, None).map_err(|e| e.to_string())?.bytes).into_owned(),
                    None => String::new(),
                };
                let mut blocks = existing_blocks(&old);
                for (message, outgoing) in messages {
                    total += 1;
                    if let std::collections::btree_map::Entry::Vacant(slot) = blocks.entry(message.key()) {
                        added += 1;
                        slot.insert(render(message, *outgoing));
                    }
                }
                let mut ordered: Vec<(&String, &Block)> = blocks.iter().collect();
                ordered.sort_by(|a, b| a.1.at.cmp(&b.1.at).then(a.0.cmp(b.0)));
                let meta = json!({"contact_id": slug, "channel": kind.channel(), "conversation": c.title, "date": date.to_string()});
                let mut body = format!("# {} · {} · {date}\n", kind.label(), c.title);
                for (key, block) in &ordered {
                    body.push_str(&format!("\n{MARK}{key} {} {} -->\n{}", block.at, if block.outgoing { "out" } else { "in" }, block.body));
                }
                let text = crate::notes::frontmatter(&meta, &body)?;
                if text != old {
                    let mode = if stamp.is_some() { "save" } else { "create" };
                    workspace
                        .write(&file, text.as_bytes(), mode, stamp.as_deref())
                        .map_err(|e| format!("Couldn't write {file}: {e}"))?;
                    files += 1;
                }
                for outgoing in [false, true] {
                    let side: Vec<&Block> = ordered.iter().map(|(_, b)| *b).filter(|b| b.outgoing == outgoing).collect();
                    let Some(last) = side.last() else { continue };
                    let count = side.len();
                    let title = if count == 1 {
                        format!("{}: {}", kind.label(), snippet(&last.text))
                    } else {
                        format!("{}: {} (+{} more)", kind.label(), snippet(&last.text), count - 1)
                    };
                    let at = DateTime::parse_from_rfc3339(&last.at)
                        .map(|t| t.with_timezone(&Utc).to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
                        .unwrap_or_else(|_| last.at.clone());
                    days.push(DayRow {
                        slug: slug.clone(),
                        message_id: format!("chat:{}:{}:{date}:{}", kind.channel(), short_hash(&c.key), if outgoing { "out" } else { "in" }),
                        at,
                        outgoing,
                        title,
                        file: file.clone(),
                    });
                }
            }
        }

        // One batch for the timeline and one for the contacts.
        let _guard = self.mutations.lock().await;
        let existing: HashMap<String, Row> = self
            .rows("interactions")
            .await?
            .into_iter()
            .filter(|r| r.text("Message-ID").starts_with("chat:"))
            .map(|r| (r.text("Message-ID").to_owned(), r))
            .collect();
        let mut writes = Vec::new();
        for d in &days {
            let values = json!({"Contact": d.slug, "Channel": kind.channel(), "Kind": "message", "Direction": if d.outgoing {"out"} else {"in"}, "Title": d.title, "File": d.file, "Message-ID": d.message_id, "At": d.at});
            let values = values.as_object().cloned().unwrap_or_default();
            match existing.get(&d.message_id) {
                Some(row) if ["Contact", "Title", "File", "At"].iter().all(|k| row.text(k) == values[*k].as_str().unwrap_or("")) => {}
                Some(row) => writes.push((Some(row.id), values)),
                None => writes.push((None, values)),
            }
        }
        let timeline = writes.len();
        self.write_rows("interactions", writes, "system:chatimport").await?;

        let config = self.workspace_config()?;
        let mut latest: HashMap<&str, (String, String)> = HashMap::new();
        for d in &days {
            let entry = latest.entry(d.slug.as_str()).or_default();
            let slot = if d.outgoing { &mut entry.1 } else { &mut entry.0 };
            if *slot < d.at {
                slot.clone_from(&d.at);
            }
        }
        let mut updates = Vec::new();
        for row in self.rows("contacts").await? {
            let Some((reply, contacted)) = latest.get(row.text("Slug")) else { continue };
            let mut values = Map::new();
            let last_reply = if reply.as_str() > row.text("Last reply") { values.insert("Last reply".into(), json!(reply)); reply.as_str() } else { row.text("Last reply") };
            let last_contacted = if contacted.as_str() > row.text("Last contacted") { values.insert("Last contacted".into(), json!(contacted)); contacted.as_str() } else { row.text("Last contacted") };
            if values.is_empty() {
                continue;
            }
            // The stage follows whoever wrote last, except for closed and do-not-contact people.
            let role = config.role(row.text("Stage"));
            if !matches!(role, Some(Role::Closed | Role::Suppressed)) {
                let next = if last_reply > last_contacted { Role::Active } else { Role::Waiting };
                if role != Some(next) {
                    values.insert("Stage".into(), json!(config.first(next)));
                }
            }
            updates.push((Some(row.id), values));
        }
        let contacts_updated = updates.len();
        self.write_rows("contacts", updates, "system:chatimport").await?;
        Ok(json!({
            "conversations": targets.len(),
            "messages": total,
            "added": added,
            "files": files,
            "timeline": timeline,
            "created": created,
            "contactsUpdated": contacts_updated,
            "skipped": plan.conversations.len() - targets.len(),
        }))
    }
}

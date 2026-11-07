//! Parsers for the conversation exports people can download themselves: a WhatsApp "Export
//! chat" file (iOS `.zip` with `_chat.txt`, Android `.txt`) and LinkedIn's "Get a copy of your
//! data" archive (`messages.csv`, `Connections.csv`, `Profile.csv`). Nothing here touches the
//! network or an account; the importer in `chats.rs` decides what is written.
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::Path;
use std::sync::LazyLock;

/// Exports above this size are refused before being read into memory.
pub const MAX_BYTES: u64 = 200 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub at: DateTime<Utc>,
    /// Display name in the export (WhatsApp: saved name or phone number; LinkedIn: FROM).
    pub author: String,
    /// LinkedIn profile URL of the sender, normalized; empty for WhatsApp.
    pub author_url: String,
    pub text: String,
}

impl Message {
    /// Stable identity of a message across re-imports of the same or a longer export.
    pub fn key(&self) -> String {
        let digest = Sha256::digest(format!("{}\u{1f}{}\u{1f}{}", self.at.timestamp(), self.author, self.text));
        digest.iter().take(6).map(|b| format!("{b:02x}")).collect()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Person {
    pub name: String,
    /// WhatsApp: the phone number when the export shows one; LinkedIn: normalized profile URL.
    pub handle: String,
    pub email: String,
    pub affiliation: String,
}

#[derive(Clone, Debug)]
pub struct Conversation {
    /// Stable key: LinkedIn's conversation ID, or a hash of the WhatsApp chat partner.
    pub key: String,
    pub title: String,
    pub messages: Vec<Message>,
    /// Everyone who wrote or received a message, keyed by author name (WhatsApp) or URL.
    pub participants: Vec<Person>,
}

/// Day/month order of WhatsApp dates; exports carry no marker, so it is inferred or chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateOrder {
    Dmy,
    Mdy,
    Ymd,
}

impl DateOrder {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "dmy" => Some(Self::Dmy),
            "mdy" => Some(Self::Mdy),
            "ymd" => Some(Self::Ymd),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dmy => "dmy",
            Self::Mdy => "mdy",
            Self::Ymd => "ymd",
        }
    }
}

/// Lower-cased `linkedin.com/in/<id>` without scheme, `www.`, query or trailing slash.
pub fn normalize_linkedin(url: &str) -> String {
    let url = url.trim().to_lowercase();
    let url = url.split(['?', '#']).next().unwrap_or("");
    let url = url.trim_start_matches("https://").trim_start_matches("http://");
    let url = url.trim_start_matches("www.").trim_start_matches("m.");
    url.trim_end_matches('/').to_owned()
}

/// Digits of a phone number (with a leading `+` dropped), or empty when `value` isn't one.
pub fn phone_digits(value: &str) -> String {
    let value = value.trim().trim_start_matches('\u{202a}').trim_end_matches('\u{202c}');
    if !value.chars().all(|c| c.is_ascii_digit() || " +-().\u{a0}".contains(c)) {
        return String::new();
    }
    let digits: String = value.chars().filter(char::is_ascii_digit).collect();
    if digits.len() >= 6 { digits } else { String::new() }
}

fn read_limited(path: &Path) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("Couldn't read the file: {e}"))?;
    if meta.len() > MAX_BYTES {
        return Err("The export is over 200 MB. Export the chat without media.".into());
    }
    std::fs::read(path).map_err(|e| format!("Couldn't read the file: {e}"))
}

/// Text files inside a `.zip`, by lower-cased file name (the last path component).
fn zip_texts(bytes: &[u8], wanted: &dyn Fn(&str) -> bool) -> Result<HashMap<String, String>, String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|_| "The file isn't a readable .zip archive.".to_string())?;
    let mut out = HashMap::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("Couldn't read the archive: {e}"))?;
        let name = entry.name().rsplit('/').next().unwrap_or("").to_lowercase();
        if !entry.is_file() || !wanted(&name) {
            continue;
        }
        if entry.size() > MAX_BYTES {
            return Err("A file in the archive is too large.".into());
        }
        let mut text = String::new();
        entry
            .read_to_string(&mut text)
            .map_err(|_| format!("{name} in the archive isn't UTF-8 text."))?;
        out.insert(name, text);
    }
    Ok(out)
}

fn utf8(bytes: Vec<u8>) -> Result<String, String> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").map(<[u8]>::to_vec).unwrap_or(bytes);
    String::from_utf8(bytes).map_err(|_| "The file isn't UTF-8 text.".to_string())
}

// ---------------------------------------------------------------------------------------------
// WhatsApp

/// One line that starts a message or system notice:
/// iOS `[06/03/2017, 00:45:12] Name: text`, Android `06/03/2017, 00:45 - Name: text`,
/// with 12-hour times (`9:05 PM`, `下午9:05`) and `/`, `.` or `-` date separators.
static WA_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^[\u{200e}\u{200f}\u{202a}\u{202c}\u{feff}]*\[?(\d{1,4})[./-] ?(\d{1,2})[./-] ?(\d{1,4})\.?,? *(上午|下午|午前|午後|오전|오후)? ?(\d{1,2})[:.](\d{2})(?:[:.](\d{2}))? ?([AaPp]\.? ?[Mm]\.?)?\]? ?(?:- )?(.*)$",
    )
    .expect("WhatsApp line pattern")
});

struct WaLine {
    date: (u32, u32, u32),
    hour: u32,
    minute: u32,
    second: u32,
    pm: Option<bool>,
    rest: String,
}

fn wa_line(line: &str) -> Option<WaLine> {
    let c = WA_LINE.captures(line)?;
    let num = |i: usize| c.get(i).and_then(|m| m.as_str().parse::<u32>().ok());
    let marker = c.get(4).map(|m| m.as_str()).or(c.get(8).map(|m| m.as_str()));
    let pm = marker.map(|m| {
        let m = m.to_lowercase();
        m.starts_with('p') || ["下午", "午後", "오후"].contains(&m.as_str())
    });
    Some(WaLine {
        date: (num(1)?, num(2)?, num(3)?),
        hour: num(5)?,
        minute: num(6)?,
        second: num(7).unwrap_or(0),
        pm,
        rest: c.get(9).map_or("", |m| m.as_str()).to_owned(),
    })
}

/// Infers the date order from every date in the export: a 4-digit first part means
/// year-first; a first part above 12 means day-first, a second part above 12 month-first;
/// otherwise day-first, which most locales use.
pub fn infer_order(dates: &[(u32, u32, u32)]) -> (DateOrder, bool) {
    if dates.iter().any(|d| d.0 > 31) {
        return (DateOrder::Ymd, true);
    }
    if dates.iter().any(|d| d.0 > 12) {
        return (DateOrder::Dmy, true);
    }
    if dates.iter().any(|d| d.1 > 12) {
        return (DateOrder::Mdy, true);
    }
    (DateOrder::Dmy, dates.is_empty())
}

fn wa_date(d: (u32, u32, u32), order: DateOrder) -> Option<NaiveDate> {
    let (y, m, day) = match order {
        DateOrder::Dmy => (d.2, d.1, d.0),
        DateOrder::Mdy => (d.2, d.0, d.1),
        DateOrder::Ymd => (d.0, d.1, d.2),
    };
    let y = if y < 100 { 2000 + y } else { y };
    NaiveDate::from_ymd_opt(i32::try_from(y).ok()?, m, day)
}

/// iOS attributes some system notices to the chat partner, marked with a leading U+200E; they
/// are not messages (and would otherwise count as a reply).
static WA_NOTICE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)end-to-end|端到端|エンドツーエンド|종단간|ende-zu-ende|extremo a extremo|ponta a ponta|security code|安全码|安全代码|disappearing messages|限时消息|消失消息")
        .expect("WhatsApp notice pattern")
});

/// Splits `Name: text`; a line without an author is a system notice (encryption, group
/// changes) and is not a message.
fn wa_author(rest: &str) -> Option<(String, String)> {
    let (author, text) = rest.split_once(": ")?;
    let author = author.trim_matches(|c: char| c.is_whitespace() || "\u{200e}\u{200f}\u{202a}\u{202c}".contains(c));
    if author.is_empty() || author.chars().count() > 80 {
        return None;
    }
    if text.starts_with('\u{200e}') && WA_NOTICE.is_match(text) {
        return None;
    }
    Some((author.to_owned(), text.trim_start_matches('\u{200e}').to_owned()))
}

/// The chat partner's name from the export file name, when it follows WhatsApp's pattern
/// (`WhatsApp Chat with Maya Chen.txt`, `WhatsApp Chat - Maya Chen.zip`).
pub fn whatsapp_title(file_name: &str) -> Option<String> {
    let stem = file_name.trim_end_matches(".zip").trim_end_matches(".txt").trim();
    for prefix in ["WhatsApp Chat with ", "WhatsApp Chat - ", "WhatsApp-Chat mit ", "Chat de WhatsApp con ", "Conversa do WhatsApp com "] {
        if let Some(name) = stem.strip_prefix(prefix) {
            let name = name.trim();
            if !name.is_empty() {
                return Some(name.to_owned());
            }
        }
    }
    None
}

pub struct WhatsAppExport {
    pub title: Option<String>,
    pub order: DateOrder,
    /// Whether the order was decided by the dates themselves rather than defaulted.
    pub order_certain: bool,
    pub messages: Vec<Message>,
    /// Authors with their message counts, most active first.
    pub authors: Vec<(String, usize)>,
}

/// Reads a WhatsApp export (`.zip` or `.txt`). `order` overrides the inferred date order.
/// Times carry no time zone in the export; they are read as this computer's local time.
pub fn read_whatsapp(path: &Path, order: Option<DateOrder>) -> Result<WhatsAppExport, String> {
    let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let bytes = read_limited(path)?;
    let text = if bytes.starts_with(b"PK") {
        let mut texts = zip_texts(&bytes, &|name| name.ends_with(".txt"))?;
        let name = texts
            .keys()
            .find(|n| n.as_str() == "_chat.txt")
            .or_else(|| texts.keys().next())
            .cloned()
            .ok_or("The archive has no chat text file. Export the chat again from WhatsApp.")?;
        texts.remove(&name).unwrap_or_default()
    } else {
        utf8(bytes)?
    };
    parse_whatsapp(&text, whatsapp_title(&file_name), order)
}

pub fn parse_whatsapp(text: &str, title: Option<String>, order: Option<DateOrder>) -> Result<WhatsAppExport, String> {
    // First pass: split into entries (a header line plus continuation lines).
    let mut entries: Vec<(WaLine, String)> = Vec::new();
    for line in text.lines() {
        match wa_line(line) {
            Some(head) => {
                let rest = head.rest.clone();
                entries.push((head, rest));
            }
            None => {
                if let Some(last) = entries.last_mut() {
                    last.1.push('\n');
                    last.1.push_str(line);
                }
            }
        }
    }
    if entries.is_empty() {
        return Err("This doesn't look like a WhatsApp chat export. In WhatsApp, open the chat → More → Export chat.".into());
    }
    let dates: Vec<_> = entries.iter().map(|(h, _)| h.date).collect();
    let (inferred, certain) = infer_order(&dates);
    let order = order.unwrap_or(inferred);
    let mut messages = Vec::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for (head, body) in entries {
        let Some((author, text)) = wa_author(&body) else { continue };
        let Some(date) = wa_date(head.date, order) else {
            return Err(format!("The date {}/{}/{} doesn't fit the chosen date order.", head.date.0, head.date.1, head.date.2));
        };
        let mut hour = head.hour;
        match head.pm {
            Some(true) if hour < 12 => hour += 12,
            Some(false) if hour == 12 => hour = 0,
            _ => {}
        }
        let Some(naive) = date.and_hms_opt(hour, head.minute, head.second) else { continue };
        let at = local_to_utc(naive);
        *counts.entry(author.clone()).or_default() += 1;
        messages.push(Message { at, author, author_url: String::new(), text: text.trim_end().to_owned() });
    }
    if messages.is_empty() {
        return Err("The export has no messages.".into());
    }
    let mut authors: Vec<_> = counts.into_iter().collect();
    authors.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    Ok(WhatsAppExport { title, order, order_certain: certain || order != inferred, messages, authors })
}

fn local_to_utc(naive: NaiveDateTime) -> DateTime<Utc> {
    Local
        .from_local_datetime(&naive)
        .earliest()
        .map_or_else(|| Utc.from_utc_datetime(&naive), |t| t.with_timezone(&Utc))
}

/// Stable conversation key for a one-to-one WhatsApp chat: its partner's identity.
pub fn whatsapp_key(partner: &str) -> String {
    let id = match phone_digits(partner) {
        d if !d.is_empty() => d,
        _ => partner.trim().to_lowercase(),
    };
    let digest = Sha256::digest(id.as_bytes());
    digest.iter().take(6).map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------------------------------------
// LinkedIn

pub struct LinkedInExport {
    pub conversations: Vec<Conversation>,
    /// The archive owner, from Profile.csv or as the participant of most conversations.
    pub me: Option<Person>,
    /// Profile URL → connection details from Connections.csv.
    pub connections: HashMap<String, Person>,
    pub spam_skipped: usize,
}

/// Parses CSV text whose header row may follow a few note lines (Connections.csv starts with
/// "Notes:"). `header` recognizes the header row.
fn csv_rows(text: &str, header: &dyn Fn(&[String]) -> bool) -> Result<(Vec<String>, Vec<Vec<String>>), String> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());
    let mut head: Option<Vec<String>> = None;
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|e| format!("The CSV is malformed: {e}"))?;
        let cells: Vec<String> = record.iter().map(|c| c.trim().to_owned()).collect();
        match &head {
            None => {
                let upper: Vec<String> = cells.iter().map(|c| c.to_uppercase()).collect();
                if header(&upper) {
                    head = Some(upper);
                }
            }
            Some(_) => {
                if cells.iter().any(|c| !c.is_empty()) {
                    rows.push(cells);
                }
            }
        }
    }
    Ok((head.unwrap_or_default(), rows))
}

fn column(head: &[String], name: &str) -> Option<usize> {
    head.iter().position(|h| h == name)
}

fn linkedin_date(value: &str) -> Option<DateTime<Utc>> {
    let value = value.trim().trim_end_matches(" UTC").trim_end_matches('Z');
    for format in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M", "%m/%d/%Y %H:%M"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(value, format) {
            return Some(Utc.from_utc_datetime(&naive));
        }
    }
    DateTime::parse_from_rfc3339(value).ok().map(|t| t.with_timezone(&Utc))
}

/// Reads LinkedIn's data archive (`.zip`) or a `messages.csv` on its own.
pub fn read_linkedin(path: &Path) -> Result<LinkedInExport, String> {
    let bytes = read_limited(path)?;
    let texts = if bytes.starts_with(b"PK") {
        zip_texts(&bytes, &|name| ["messages.csv", "connections.csv", "profile.csv"].contains(&name))?
    } else {
        HashMap::from([("messages.csv".to_owned(), utf8(bytes)?)])
    };
    let messages = texts
        .get("messages.csv")
        .ok_or("The archive has no messages.csv. On LinkedIn, request your data with “Messages” selected.")?;
    parse_linkedin(messages, texts.get("connections.csv").map(String::as_str), texts.get("profile.csv").map(String::as_str))
}

pub fn parse_linkedin(messages: &str, connections: Option<&str>, profile: Option<&str>) -> Result<LinkedInExport, String> {
    let (head, rows) = csv_rows(messages, &|h| h.contains(&"FROM".to_owned()) && h.contains(&"DATE".to_owned()))?;
    let (Some(from), Some(date)) = (column(&head, "FROM"), column(&head, "DATE")) else {
        return Err("messages.csv has no FROM and DATE columns.".into());
    };
    let cell = |row: &Vec<String>, name: &str| column(&head, name).and_then(|i| row.get(i)).cloned().unwrap_or_default();
    let mut connection_map = HashMap::new();
    if let Some(text) = connections {
        let (head, rows) = csv_rows(text, &|h| h.contains(&"FIRST NAME".to_owned()) && h.contains(&"URL".to_owned()))?;
        let get = |row: &Vec<String>, name: &str| column(&head, name).and_then(|i| row.get(i)).cloned().unwrap_or_default();
        for row in rows {
            let url = normalize_linkedin(&get(&row, "URL"));
            if url.is_empty() {
                continue;
            }
            let name = format!("{} {}", get(&row, "FIRST NAME"), get(&row, "LAST NAME")).trim().to_owned();
            let affiliation = [get(&row, "POSITION"), get(&row, "COMPANY")]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(", ");
            connection_map.insert(url.clone(), Person { name, handle: url, email: get(&row, "EMAIL ADDRESS"), affiliation });
        }
    }
    let profile_name = profile.and_then(|text| {
        let (head, rows) = csv_rows(text, &|h| h.contains(&"FIRST NAME".to_owned())).ok()?;
        let row = rows.first()?;
        let get = |name: &str| column(&head, name).and_then(|i| row.get(i)).cloned().unwrap_or_default();
        Some(format!("{} {}", get("FIRST NAME"), get("LAST NAME")).trim().to_owned()).filter(|n| !n.is_empty())
    });

    let mut spam = 0;
    let mut grouped: BTreeMap<String, Conversation> = BTreeMap::new();
    for row in &rows {
        if cell(row, "FOLDER").to_lowercase().contains("spam") {
            spam += 1;
            continue;
        }
        let Some(at) = row.get(date).and_then(|d| linkedin_date(d)) else { continue };
        let author = row.get(from).cloned().unwrap_or_default();
        let author_url = normalize_linkedin(&cell(row, "SENDER PROFILE URL"));
        let to_names: Vec<String> = cell(row, "TO").split(',').map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()).collect();
        let to_urls: Vec<String> = cell(row, "RECIPIENT PROFILE URLS").split(',').map(normalize_linkedin).filter(|s| !s.is_empty()).collect();
        let subject = cell(row, "SUBJECT");
        let content = cell(row, "CONTENT");
        let text = match (subject.is_empty(), content.is_empty()) {
            (false, false) => format!("{subject}\n\n{content}"),
            (false, true) => subject,
            _ => content,
        };
        // Without a conversation ID, the participants identify the conversation.
        let key = match cell(row, "CONVERSATION ID") {
            id if !id.is_empty() => id,
            _ => {
                let mut ids: Vec<String> = to_urls.iter().cloned().chain([author_url.clone()]).collect();
                ids.sort();
                ids.join("|")
            }
        };
        let conversation = grouped.entry(key.clone()).or_insert_with(|| Conversation {
            key,
            title: cell(row, "CONVERSATION TITLE"),
            messages: Vec::new(),
            participants: Vec::new(),
        });
        let mut add = |name: &str, url: &str| {
            let handle = url.to_owned();
            let exists = conversation.participants.iter().any(|p| {
                if handle.is_empty() { p.handle.is_empty() && p.name == name } else { p.handle == handle }
            });
            if !exists && (!name.is_empty() || !handle.is_empty()) {
                conversation.participants.push(Person { name: name.to_owned(), handle, ..Person::default() });
            }
        };
        add(&author, &author_url);
        for (i, url) in to_urls.iter().enumerate() {
            add(to_names.get(i).map_or("", String::as_str), url);
        }
        if to_urls.is_empty() {
            for name in &to_names {
                add(name, "");
            }
        }
        conversation.messages.push(Message { at, author, author_url, text });
    }
    let mut conversations: Vec<Conversation> = grouped.into_values().collect();
    for c in &mut conversations {
        c.messages.sort_by_key(|m| m.at);
        c.messages.dedup_by(|a, b| a.at == b.at && a.author == b.author && a.text == b.text);
    }

    // The owner takes part in every conversation; prefer the name from Profile.csv.
    let mut seen: HashMap<String, (usize, Person)> = HashMap::new();
    for c in &conversations {
        for p in &c.participants {
            let id = if p.handle.is_empty() { p.name.clone() } else { p.handle.clone() };
            seen.entry(id).or_insert_with(|| (0, p.clone())).0 += 1;
        }
    }
    let me = profile_name
        .as_ref()
        .and_then(|name| seen.values().filter(|(_, p)| &p.name == name).max_by_key(|(n, _)| *n))
        .or_else(|| {
            let most = seen.values().max_by_key(|(n, _)| *n)?;
            // Only certain when the owner appears in more conversations than anyone else.
            let ties = seen.values().filter(|(n, _)| *n == most.0).count();
            (ties == 1 && most.0 > 1).then_some(most)
        })
        .map(|(_, p)| p.clone());
    Ok(LinkedInExport { conversations, me, connections: connection_map, spam_skipped: spam })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whatsapp_ios_and_android_lines() {
        let ios = "\u{200e}[06/03/2023, 21:45:12] Maya Chen: \u{200e}Messages and calls are end-to-end encrypted.\n[06/03/2023, 21:45:12] Maya Chen: Hi! Saw your talk\n[13/03/2023, 09:01:02] Alex Rivera: Thanks Maya,\nhappy to chat next week\n";
        let export = parse_whatsapp(ios, Some("Maya Chen".into()), None).unwrap();
        assert_eq!(export.order, DateOrder::Dmy);
        assert!(export.order_certain);
        assert_eq!(export.messages.len(), 2, "the encryption notice is not a message");
        assert_eq!(export.messages[1].text, "Thanks Maya,\nhappy to chat next week");
        assert_eq!(export.messages[1].author, "Alex Rivera");
        assert_eq!(export.authors, vec![("Alex Rivera".to_owned(), 1), ("Maya Chen".to_owned(), 1)]);

        let android = "3/14/23, 9:05 PM - Messages and calls are end-to-end encrypted.\n3/14/23, 9:05 PM - +1 555-0100: hello\n3/14/23, 12:10 AM - Me Myself: <Media omitted>\n";
        let export = parse_whatsapp(android, None, None).unwrap();
        assert_eq!(export.order, DateOrder::Mdy);
        assert_eq!(export.messages.len(), 2, "the system notice has no author");
        let local = export.messages[0].at.with_timezone(&Local).naive_local();
        assert_eq!(local.format("%Y-%m-%d %H:%M").to_string(), "2023-03-14 21:05");
        let midnight = export.messages[1].at.with_timezone(&Local).naive_local();
        assert_eq!(midnight.format("%H:%M").to_string(), "00:10");
        assert_eq!(phone_digits(&export.messages[0].author), "15550100");
    }

    #[test]
    fn whatsapp_date_order_and_cjk_markers() {
        // Ambiguous dates default to day-first; an explicit order wins.
        let text = "01/02/2024, 10:00 - A: one\n03/04/2024, 11:00 - B: two\n";
        assert_eq!(parse_whatsapp(text, None, None).unwrap().order, DateOrder::Dmy);
        assert!(!parse_whatsapp(text, None, None).unwrap().order_certain);
        let mdy = parse_whatsapp(text, None, Some(DateOrder::Mdy)).unwrap();
        let first = mdy.messages[0].at.with_timezone(&Local).date_naive();
        assert_eq!(first.to_string(), "2024-01-02");
        // Year-first with a 12-hour marker before the time.
        let zh = "2024/5/4 下午3:12 - 张三: 你好\n2024/5/4 上午12:05 - 我: 在吗\n";
        let export = parse_whatsapp(zh, None, None).unwrap();
        assert_eq!(export.order, DateOrder::Ymd);
        let at = export.messages[0].at.with_timezone(&Local).naive_local();
        assert_eq!(at.format("%Y-%m-%d %H:%M").to_string(), "2024-05-04 15:12");
        let at = export.messages[1].at.with_timezone(&Local).naive_local();
        assert_eq!(at.format("%H:%M").to_string(), "00:05");
        // Dotted European dates with seconds.
        let de = "[24.12.23, 18:30:05] Lena: Frohe Weihnachten\n";
        assert_eq!(parse_whatsapp(de, None, None).unwrap().messages.len(), 1);
        assert!(parse_whatsapp("just some text\n", None, None).is_err());
    }

    #[test]
    fn whatsapp_title_and_keys() {
        assert_eq!(whatsapp_title("WhatsApp Chat with Maya Chen.txt").as_deref(), Some("Maya Chen"));
        assert_eq!(whatsapp_title("WhatsApp Chat - +1 555 0100.zip").as_deref(), Some("+1 555 0100"));
        assert_eq!(whatsapp_title("_chat.txt"), None);
        assert_eq!(whatsapp_key("+1 (555) 0100"), whatsapp_key("15550100"));
        assert_eq!(whatsapp_key("Maya Chen"), whatsapp_key("maya chen "));
        let m = Message { at: Utc::now(), author: "A".into(), author_url: String::new(), text: "x".into() };
        assert_eq!(m.key(), m.clone().key());
        assert_eq!(m.key().len(), 12);
    }

    #[test]
    fn linkedin_messages_connections_and_owner() {
        let messages = "CONVERSATION ID,CONVERSATION TITLE,FROM,SENDER PROFILE URL,TO,RECIPIENT PROFILE URLS,DATE,SUBJECT,CONTENT,FOLDER\n\
c1,,Alex Rivera,https://www.linkedin.com/in/alex-rivera,Maya Chen,https://www.linkedin.com/in/maya-chen/,2024-03-05 18:22:10 UTC,,\"Hi Maya, loved the paper\",INBOX\n\
c1,,Maya Chen,https://www.linkedin.com/in/maya-chen,Alex Rivera,https://www.linkedin.com/in/alex-rivera,2024-03-06 08:00:00 UTC,,Thanks!,INBOX\n\
c2,,Alex Rivera,https://www.linkedin.com/in/alex-rivera,Sam Ortiz,https://www.linkedin.com/in/sam-ortiz,2024-04-01 10:00:00 UTC,Intro,Hello Sam,INBOX\n\
c3,,Spammer,https://www.linkedin.com/in/spam,Alex Rivera,https://www.linkedin.com/in/alex-rivera,2024-04-02 10:00:00 UTC,,Buy now,SPAM\n";
        let connections = "Notes:\n\"When exporting your connection data, you may notice...\"\n\nFirst Name,Last Name,URL,Email Address,Company,Position,Connected On\nMaya,Chen,https://www.linkedin.com/in/maya-chen,maya@example.org,Example Lab,Researcher,05 Mar 2024\n";
        let export = parse_linkedin(messages, Some(connections), None).unwrap();
        assert_eq!(export.spam_skipped, 1);
        assert_eq!(export.conversations.len(), 2);
        let c1 = export.conversations.iter().find(|c| c.key == "c1").unwrap();
        assert_eq!(c1.messages.len(), 2);
        assert_eq!(c1.participants.len(), 2, "URLs are normalized, so Maya appears once");
        assert_eq!(export.me.as_ref().unwrap().handle, "linkedin.com/in/alex-rivera");
        let maya = &export.connections["linkedin.com/in/maya-chen"];
        assert_eq!(maya.email, "maya@example.org");
        assert_eq!(maya.affiliation, "Researcher, Example Lab");
        let c2 = export.conversations.iter().find(|c| c.key == "c2").unwrap();
        assert_eq!(c2.messages[0].text, "Intro\n\nHello Sam");

        // With one conversation the owner can only come from Profile.csv.
        let one = "FROM,SENDER PROFILE URL,TO,RECIPIENT PROFILE URLS,DATE,CONTENT\nAlex Rivera,https://linkedin.com/in/alex-rivera,Maya Chen,https://linkedin.com/in/maya-chen,2024-03-05 18:22:10 UTC,hi\n";
        assert!(parse_linkedin(one, None, None).unwrap().me.is_none());
        let profile = "First Name,Last Name,Headline\nAlex,Rivera,Builder\n";
        let export = parse_linkedin(one, None, Some(profile)).unwrap();
        assert_eq!(export.me.unwrap().name, "Alex Rivera");
        assert!(parse_linkedin("A,B\n1,2\n", None, None).is_err());
    }

    #[test]
    fn linkedin_urls_and_phones() {
        assert_eq!(normalize_linkedin("https://www.linkedin.com/in/Maya-Chen/?trk=x"), "linkedin.com/in/maya-chen");
        assert_eq!(normalize_linkedin("linkedin.com/in/maya-chen"), "linkedin.com/in/maya-chen");
        assert_eq!(phone_digits("Maya Chen"), "");
        assert_eq!(phone_digits("\u{202a}+44 7700 900123\u{202c}"), "447700900123");
    }
}

//! Recognizes bounces, unsubscribe requests and automatic replies in synced mail, from headers
//! and keywords only (no model). Bounces and opt-outs feed the do-not-contact list; automatic
//! replies are recorded but never count as the person replying.
use crate::{Service, notes::metadata};
use serde_json::json;

#[derive(Debug, PartialEq, Eq)]
pub enum Signal {
    /// Delivery failed for these addresses.
    Bounce(Vec<String>),
    /// The person asked not to be contacted again.
    Unsubscribe,
    /// Out-of-office or other automatic response.
    AutoReply,
    Normal,
}

fn email_re() -> regex::Regex {
    regex::Regex::new(r"(?i)[a-z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-z0-9.-]+\.[a-z]{2,}").expect("valid regex")
}

/// `body` may be the preview; bounces are confirmed against the full text when available.
pub fn classify(subject: &str, from: &str, body: &str) -> Signal {
    let subject_l = subject.to_lowercase();
    let from_l = from.to_lowercase();
    let bounce_sender = ["mailer-daemon", "postmaster", "mail delivery", "mail-daemon"].iter().any(|s| from_l.contains(s));
    let bounce_subject = [
        "undeliverable",
        "undelivered mail",
        "delivery status notification (failure)",
        "delivery has failed",
        "returned mail",
        "mail delivery failed",
        "failure notice",
        // Chinese-language mail servers and senders.
        "退信",
        "无法投递",
        "投递失败",
        "邮件被退回",
    ]
    .iter()
    .any(|s| subject_l.contains(s));
    if bounce_sender || bounce_subject {
        let failed = failed_recipients(body);
        if bounce_sender || !failed.is_empty() {
            return Signal::Bounce(failed);
        }
    }
    let auto = [
        "automatic reply",
        "auto-reply",
        "autoreply",
        "auto reply",
        "out of office",
        "out of the office",
        "on vacation",
        "away from the office",
        "自动回复",
        "自动答复",
        "休假",
        "不在办公室",
    ];
    if auto.iter().any(|s| subject_l.contains(s)) {
        return Signal::AutoReply;
    }
    // Opt-outs are short and say so near the start; quoted earlier mail is ignored.
    let head: String = body
        .lines()
        .filter(|l| !l.trim_start().starts_with('>'))
        .take_while(|l| !l.starts_with("On ") && !l.contains("写道") && !l.starts_with("-----"))
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(300)
        .collect::<String>()
        .to_lowercase();
    let opt_out = regex::Regex::new(
        r"(?i)(^|[\s,.!，。！])(stop|unsubscribe|remove me|opt[ -]?out)([\s,.!，。！]|$)|退订|取消订阅|不要再(联系|发)|请勿再(联系|发)|别再(联系|发)|不再需要.*(邮件|联系)",
    )
    .expect("valid regex");
    if opt_out.is_match(&head) || opt_out.is_match(&subject_l) && subject_l.chars().count() < 40 {
        return Signal::Unsubscribe;
    }
    Signal::Normal
}

/// The return date an out-of-office reply mentions (2026-11-10, 11月10日, November 10, 10 Nov),
/// relative to `today`. Dates already past roll to next year.
pub fn return_date(text: &str, today: chrono::NaiveDate) -> Option<chrono::NaiveDate> {
    use chrono::{Datelike, NaiveDate};
    let iso = regex::Regex::new(r"(20\d\d)[-/.](\d{1,2})[-/.](\d{1,2})").expect("valid regex");
    if let Some(c) = iso.captures(text) {
        return NaiveDate::from_ymd_opt(c[1].parse().ok()?, c[2].parse().ok()?, c[3].parse().ok()?);
    }
    let months = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let lower = text.to_lowercase();
    let month_day = |m: u32, d: u32| {
        let mut date = NaiveDate::from_ymd_opt(today.year(), m, d)?;
        if date < today {
            date = NaiveDate::from_ymd_opt(today.year() + 1, m, d)?;
        }
        Some(date)
    };
    let cn = regex::Regex::new(r"(\d{1,2})\s*月\s*(\d{1,2})\s*[日号]").expect("valid regex");
    if let Some(c) = cn.captures(text) {
        return month_day(c[1].parse().ok()?, c[2].parse().ok()?);
    }
    let en = regex::Regex::new(r"\b(jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)[a-z]*\.?\s+(\d{1,2})\b|\b(\d{1,2})\s+(jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)[a-z]*\b").expect("valid regex");
    if let Some(c) = en.captures(&lower) {
        let (m, d) = match (c.get(1), c.get(2), c.get(3), c.get(4)) {
            (Some(m), Some(d), _, _) => (m.as_str(), d.as_str()),
            (_, _, Some(d), Some(m)) => (m.as_str(), d.as_str()),
            _ => return None,
        };
        let month = months.iter().position(|x| *x == m)? as u32 + 1;
        return month_day(month, d.parse().ok()?);
    }
    None
}

/// Addresses named as failed in a delivery status notification.
pub fn failed_recipients(text: &str) -> Vec<String> {
    let re = email_re();
    let mut found: Vec<String> = Vec::new();
    for line in text.lines() {
        let lower = line.to_lowercase();
        if lower.starts_with("final-recipient:") || lower.starts_with("original-recipient:") || lower.starts_with("x-failed-recipients:") {
            for m in re.find_iter(line) {
                let address = m.as_str().to_lowercase();
                if !found.contains(&address) {
                    found.push(address);
                }
            }
        }
    }
    found
}

impl Service {
    /// Discards unsent drafts to `address` (or for `slug`), so nothing queued reaches them.
    pub async fn discard_pending_drafts(&self, slug: Option<&str>, address: Option<&str>) -> Result<usize, String> {
        let mut discarded = 0;
        for draft in self.drafts(slug).await? {
            let meta = &draft["metadata"];
            if !matches!(meta["status"].as_str(), Some("draft" | "approved")) {
                continue;
            }
            if let Some(address) = address
                && !meta["to"].as_str().unwrap_or_default().to_lowercase().contains(address)
            {
                continue;
            }
            let Some(path) = draft["path"].as_str() else { continue };
            let current = metadata(&self.read_note(path).await?);
            if matches!(current["status"].as_str(), Some("draft" | "approved")) {
                self.docs.execute(path, "property.set", json!({"key":"status","value":"discarded"}), "system:signals").await?;
                discarded += 1;
            }
        }
        Ok(discarded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounces() {
        let dsn = "This is the mail system.\nFinal-Recipient: rfc822; gone@example.com\nAction: failed\nStatus: 5.1.1";
        assert_eq!(classify("Undelivered Mail Returned to Sender", "MAILER-DAEMON@mx.example.com", dsn), Signal::Bounce(vec!["gone@example.com".into()]));
        assert_eq!(failed_recipients("X-Failed-Recipients: a@x.io, b@y.io"), vec!["a@x.io", "b@y.io"]);
        // A person writing about a "returned mail" problem is not a bounce without a DSN.
        assert_eq!(classify("Returned mail question", "lin@example.com", "我的邮件被退回了怎么办"), Signal::Normal);
    }

    #[test]
    fn return_dates() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 11, 3).unwrap();
        let d = |y, m, d| chrono::NaiveDate::from_ymd_opt(y, m, d);
        assert_eq!(return_date("Back on 2026-11-10", today), d(2026, 11, 10));
        assert_eq!(return_date("我 11月10日 回来", today), d(2026, 11, 10));
        assert_eq!(return_date("Out until November 12.", today), d(2026, 11, 12));
        assert_eq!(return_date("back 5 Jan", today), d(2027, 1, 5));
        assert_eq!(return_date("I am away this week", today), None);
    }

    #[test]
    fn opt_outs_and_auto_replies() {
        assert_eq!(classify("Re: 问候", "lin@example.com", "请不要再联系我，谢谢"), Signal::Unsubscribe);
        assert_eq!(classify("Re: hello", "lin@example.com", "STOP"), Signal::Unsubscribe);
        assert_eq!(classify("Re: hello", "lin@example.com", "Please unsubscribe me."), Signal::Unsubscribe);
        assert_eq!(classify("Automatic reply: hello", "lin@example.com", "I am out until Monday"), Signal::AutoReply);
        assert_eq!(classify("自动回复：问候", "lin@example.com", "休假中"), Signal::AutoReply);
        // Words inside normal sentences or quoted text do not opt out.
        assert_eq!(classify("Re: hello", "lin@example.com", "Thanks! The bus stopped near the office.\n> unsubscribe"), Signal::Normal);
        assert_eq!(classify("Re: 合作", "lin@example.com", "很感兴趣，下周聊聊？"), Signal::Normal);
    }
}

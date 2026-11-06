//! Channel kinds, in one place. Email is the only channel the app sends (SMTP); every other
//! messaging channel is "copy + mark sent" by a person. Adding a kind here also adds it to the
//! select fields of existing workspaces when they are opened (see `schema::channel_choices`).

/// Channels a message can go through: drafts, lists and interactions.
pub const MESSAGE: &[&str] = &[
    "email", "x", "instagram", "wechat", "whatsapp", "tiktok", "facebook", "linkedin", "other",
];

/// Every kind a contact channel row may have: messaging channels plus profile links.
pub const KINDS: &[&str] = &[
    "email", "x", "instagram", "homepage", "scholar", "github", "wechat", "whatsapp", "tiktok",
    "facebook", "linkedin", "other",
];

/// Choices of the contacts table's "Primary channel" column.
pub const PRIMARY: &[&str] = &[
    "Email", "X", "Instagram", "WeChat", "WhatsApp", "TikTok", "Facebook", "LinkedIn", "Other",
];

/// Whether drafts, lists and interactions may use `kind`.
pub fn is_message(kind: &str) -> bool {
    MESSAGE.contains(&kind)
}

/// Whether a contact channel row may have `kind`.
pub fn is_kind(kind: &str) -> bool {
    KINDS.contains(&kind)
}

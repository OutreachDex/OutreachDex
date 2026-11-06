pub mod channels;
pub mod config;
pub mod lists;
pub mod schema;
pub mod workspace;

pub use workspace::{Error, FileRead, Workspace};
pub mod coerce;
mod common;
pub mod fs;
pub use common::*;
pub mod time {
    pub fn to_rfc3339(millis: i64) -> String {
        chrono::DateTime::from_timestamp_millis(millis)
            .unwrap_or_default()
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }
}

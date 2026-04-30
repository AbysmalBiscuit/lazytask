// Date/text formatting utilities

use chrono::{DateTime, Local, Utc};

pub fn format_date(date: &DateTime<Utc>) -> String {
    let local_date = date.with_timezone(&Local);
    local_date.format("%Y-%m-%d").to_string()
}

pub fn format_datetime(datetime: &DateTime<Utc>) -> String {
    let local_datetime = datetime.with_timezone(&Local);
    local_datetime.format("%Y-%m-%d %H:%M").to_string()
}

pub fn format_relative_date(date: &DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(*date);
    
    if duration.num_days() > 0 {
        format!("{} days ago", duration.num_days())
    } else if duration.num_hours() > 0 {
        format!("{}h ago", duration.num_hours())
    } else if duration.num_minutes() > 0 {
        format!("{}m ago", duration.num_minutes())
    } else {
        "Just now".to_string()
    }
}

/// Truncate a string to at most `max_chars` USER-PERCEIVED characters.
/// Operates on `chars()` to avoid panicking on UTF-8 byte boundaries.
/// If truncated, appends `…` (single char) so the visible width stays predictable.
pub fn truncate_chars(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    let mut s: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    s.push('…');
    s
}

/// Backwards-compatible alias that operates on chars rather than bytes.
pub fn truncate_text(text: &str, max_length: usize) -> String {
    truncate_chars(text, max_length)
}


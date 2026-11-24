//! Local transfer history: JSONL log plus the `hayate history` subcommand.
//!
//! Every completed send/receive appends one JSON object per line to
//! `<data dir>/hayate/history.jsonl`. The log is local-only metadata — no
//! payload content, no secrets.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::cli::HistoryArgs;
use crate::output;

/// One recorded transfer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Unix timestamp (seconds) of completion.
    pub ts: u64,
    /// `"send"` or `"receive"`.
    pub direction: String,
    /// Display name of the payload.
    pub filename: String,
    /// Payload size in bytes.
    pub size: u64,
    /// Wall-clock transfer time in seconds.
    pub elapsed_secs: f64,
    /// Average throughput in bytes/sec.
    pub speed_bps: u64,
    /// Integrity checksum (`algo$hex`).
    pub checksum: String,
    /// Negotiated cipher name.
    pub cipher: String,
    /// Remote peer address.
    pub peer: String,
    /// Destination path (receive only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// Resolves the history log path.
fn log_path() -> Result<PathBuf> {
    let dir = dirs::data_dir().context("could not locate the user data directory")?;
    Ok(dir.join("hayate").join("history.jsonl"))
}

/// Appends an entry to the history log. Failures are non-fatal: history must
/// never break a transfer, so callers should log-and-continue on error.
pub fn record(mut entry: HistoryEntry) -> Result<()> {
    entry.ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let path = log_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(file, "{}", serde_json::to_string(&entry)?)?;
    Ok(())
}

/// Builds and records a history entry for a completed transfer (best-effort;
/// failures surface as a warning, never as a transfer error).
#[allow(clippy::too_many_arguments)]
pub fn record_transfer(
    direction: &str,
    filename: &str,
    size: u64,
    elapsed_secs: f64,
    checksum: &str,
    cipher: &str,
    peer: &str,
    path: Option<String>,
) {
    let speed_bps =
        if elapsed_secs > f64::EPSILON { (size as f64 / elapsed_secs) as u64 } else { size };
    let result = record(HistoryEntry {
        ts: 0,
        direction: direction.to_owned(),
        filename: filename.to_owned(),
        size,
        elapsed_secs,
        speed_bps,
        checksum: checksum.to_owned(),
        cipher: cipher.to_owned(),
        peer: peer.to_owned(),
        path,
    });
    if let Err(e) = result {
        crate::output::warn(&format!("could not record history: {e}"));
    }
}

/// Reads all entries, oldest first. Unparseable lines are skipped.
fn read_all() -> Result<Vec<HistoryEntry>> {
    let path = log_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path)?;
    Ok(text.lines().filter_map(|line| serde_json::from_str(line).ok()).collect())
}

/// Returns the most recent entries, newest first, capped at `limit`.
///
/// `limit == 0` returns everything. Unreadable logs yield an empty list so
/// callers such as the dashboard can stay up.
pub fn recent(limit: usize) -> Result<Vec<HistoryEntry>> {
    let mut entries = read_all()?;
    entries.reverse();
    if limit > 0 {
        entries.truncate(limit);
    }
    Ok(entries)
}

/// `hayate history` subcommand entry point.
pub fn run(args: HistoryArgs) -> Result<()> {
    if args.clear {
        if !args.yes {
            if !output::stdin_is_tty() {
                anyhow::bail!("refusing to clear history without --yes (no TTY to confirm)");
            }
            let confirmed = output::suspend_for_prompt(|| {
                inquire::Confirm::new("Delete the transfer history?").with_default(false).prompt()
            })?;
            if !confirmed {
                output::info("History unchanged.");
                return Ok(());
            }
        }
        let path = log_path()?;
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        output::ok("History cleared.");
        return Ok(());
    }

    let entries = read_all()?;
    if entries.is_empty() {
        output::info("No transfers recorded yet.");
        return Ok(());
    }

    let mut recent: Vec<&HistoryEntry> = entries.iter().rev().collect();
    if args.limit > 0 {
        recent.truncate(args.limit);
    }

    if crate::policy::get().is_json() {
        for entry in &recent {
            println!("{}", serde_json::to_string(entry)?);
        }
        return Ok(());
    }

    println!();
    output::ok(&format!("{} recorded transfer(s)", entries.len()));
    println!();
    for entry in recent {
        let arrow = if entry.direction == "send" { "↑" } else { "↓" };
        let when = format_timestamp(entry.ts);
        let line = format!(
            "   {arrow} {:<20} {:>10}  {:>11}/s  {:<20} {}",
            truncate(&entry.filename, 20),
            output::format_bytes(entry.size),
            output::format_bytes(entry.speed_bps),
            entry.peer,
            when,
        );
        println!("{line}");
    }
    println!();
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    } else {
        s.to_owned()
    }
}

/// Formats a Unix timestamp as `YYYY-MM-DD HH:MM` (UTC) without a date crate.
fn format_timestamp(ts: u64) -> String {
    let days = (ts / 86_400) as i64;
    let secs = ts % 86_400;
    let (hour, min) = (secs / 3600, (secs % 3600) / 60);

    // Howard Hinnant's civil-from-days algorithm.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02} {hour:02}:{min:02}")
}

#[cfg(test)]
mod tests {
    use super::format_timestamp;

    #[test]
    fn timestamp_epoch_zero() {
        assert_eq!(format_timestamp(0), "1970-01-01 00:00");
    }

    #[test]
    fn timestamp_known_date() {
        // 2026-07-25 14:46:00 UTC = 1_784_990_760 (verified against date -u).
        assert_eq!(format_timestamp(1_784_990_760), "2026-07-25 14:46");
    }

    #[test]
    fn timestamp_leap_day() {
        // 2000-02-29 12:00 UTC = 951825600.
        assert_eq!(format_timestamp(951_825_600), "2000-02-29 12:00");
    }
}

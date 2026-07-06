//! `hayate man` subcommand — prints mandoc-compatible man pages to stdout.
//!
//! Man pages are generated at compile time by `build.rs` and written to the
//! `man/` directory. This subcommand renders the same pages on demand so users
//! can inspect them without installing system man pages.

use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::CommandFactory;

use crate::cli::{Cli, ManArgs};

/// Prints the requested man page to stdout.
///
/// # Errors
///
/// Returns an error if the requested page is unknown or if stdout cannot be
/// written.
pub fn run(args: ManArgs) -> Result<()> {
    let cmd = Cli::command();
    let name = args.page.to_lowercase();

    let page_cmd = if name == "hayate" || name.is_empty() {
        cmd.clone()
    } else {
        let subcmd = cmd
            .get_subcommands()
            .find(|s| s.get_name() == name)
            .cloned()
            .with_context(|| format!("unknown man page: {}", args.page))?;
        if subcmd.is_hide_set() {
            bail!("no man page for hidden subcommand: {}", args.page);
        }
        subcmd
    };

    let mut buffer = Vec::new();
    clap_mangen::Man::new(page_cmd).render(&mut buffer)?;

    let normalized = normalize_man_page(&buffer, &args.page, env!("CARGO_PKG_VERSION"));
    std::io::stdout()
        .write_all(&normalized)
        .context("failed to write man page to stdout")?;
    Ok(())
}

/// Normalizes clap_mangen output for maximum compatibility with mandoc.
///
/// This mirrors the logic in `build.rs` so the runtime man page matches the
/// shipped man page format.
fn normalize_man_page(roff: &[u8], title: &str, version: &str) -> Vec<u8> {
    const MAN_DATE: &str = "2026-07-04";
    const WRAP_WIDTH: usize = 80;

    let title = title.to_uppercase();
    let source = format!("hayate {version}");

    let lines: Vec<String> = String::from_utf8_lossy(roff)
        .lines()
        .map(|l| l.trim_end().to_owned())
        .collect();

    let mut out = Vec::with_capacity(lines.len());

    for (i, line) in lines.iter().enumerate() {
        if line.starts_with(".TH ") {
            let section = line.split_whitespace().nth(2).unwrap_or("1");
            out.push(format!(
                ".TH {} {} \"{MAN_DATE}\" \"{source}\" \"Hayate Manual\"",
                title, section
            ));
            continue;
        }

        if !line.starts_with('.') && line.len() > WRAP_WIDTH {
            out.extend(wrap_line(line, WRAP_WIDTH));
            continue;
        }

        if line.trim() == ".br" {
            let prev_blank = i
                .checked_sub(1)
                .and_then(|i| lines.get(i))
                .is_none_or(|l| l.trim().is_empty());
            let next_blank = lines.get(i + 1).is_none_or(|l| l.trim().is_empty());
            if prev_blank || next_blank {
                continue;
            }
        }

        out.push(line.clone());
    }

    let mut result = out.join("\n");
    result.push('\n');
    result.into_bytes()
}

fn wrap_line(line: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut remaining = line;

    while remaining.len() > width {
        let split = remaining[..width].rfind(' ').unwrap_or(width);
        out.push(remaining[..split].to_owned());
        remaining = remaining[split..].trim_start();
    }

    out.push(remaining.to_owned());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_rewrites_th_header() {
        let input = b".TH hayate 1 \"DATE\" \"SOURCE\" \"MANUAL\"\n";
        let out = normalize_man_page(input, "send", "1.2.3");
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains(".TH SEND 1 \"2026-07-04\" \"hayate 1.2.3\" \"Hayate Manual\""));
    }

    #[test]
    fn normalize_wraps_long_text() {
        let long = "a ".repeat(60);
        let input = format!("{long}\n").into_bytes();
        let out = normalize_man_page(&input, "hayate", "1.2.3");
        let s = String::from_utf8(out).unwrap();
        // The wrapped output should contain at least one line break inside the
        // long sentence (more than one line in total).
        assert!(s.lines().count() > 1);
    }
}

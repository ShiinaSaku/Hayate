//! Build script for hayate-cli.
//!
//! - Injects git-derived version metadata into the binary.
//! - Generates mandoc-compatible man pages from the clap CLI definition.

#![allow(missing_docs)]

use std::{fs, path::Path, process::Command as StdCommand};

use clap::CommandFactory;

include!("src/cli.rs");

const MAN_DIR: &str = "../man";
const MAN_DATE: &str = "2026-07-04";
const WRAP_WIDTH: usize = 80;

fn main() -> std::io::Result<()> {
    println!("cargo:rerun-if-changed=src/cli.rs");

    emit_build_metadata();

    let out_dir = Path::new(MAN_DIR);
    fs::create_dir_all(out_dir)?;

    generate_man_pages(out_dir)
}

fn emit_build_metadata() {
    let version = env!("CARGO_PKG_VERSION");

    println!("cargo:rustc-env=GIT_VERSION={version}");
    println!("cargo:rustc-env=GIT_COMMIT_HASH={}", git_short_hash());
}

fn generate_man_pages(out_dir: &Path) -> std::io::Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    let cmd = Cli::command();

    write_man_page(cmd.clone(), "hayate", version, out_dir)?;

    for subcmd in cmd.get_subcommands().filter(|s| !s.is_hide_set()) {
        if subcmd.get_name() == "help" {
            // clap provides this automatically.
            continue;
        }

        let title = format!("hayate-{}", subcmd.get_name());
        write_man_page(subcmd.clone(), &title, version, out_dir)?;
    }

    Ok(())
}

fn write_man_page(
    cmd: clap::Command,
    title: &str,
    version: &str,
    out_dir: &Path,
) -> std::io::Result<()> {
    let mut buffer = Vec::new();

    clap_mangen::Man::new(cmd).render(&mut buffer)?;

    fs::write(
        out_dir.join(format!("{title}.1")),
        normalize_man_page(&buffer, title, version),
    )
}

fn git_short_hash() -> String {
    StdCommand::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".into())
}

/// Normalizes clap_mangen output for maximum compatibility with mandoc.
///
/// This function:
///
/// - rewrites the `.TH` header
/// - removes trailing whitespace
/// - wraps long text lines
/// - removes stray `.br` directives around blank paragraphs
fn normalize_man_page(roff: &[u8], title: &str, version: &str) -> Vec<u8> {
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

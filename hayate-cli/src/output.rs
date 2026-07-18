//! Terminal output helpers: banner, status lines, progress bars, cards, and
//! summaries.
//!
//! All visual output flows through this module so the rest of the CLI never
//! constructs raw ANSI escapes or guesses column widths.
//!
//! On terminals that do not support Unicode (e.g. legacy Windows CMD,
//! some Termux configurations), box-drawing glyphs fall back to plain ASCII.

use std::io::IsTerminal;
use std::sync::OnceLock;

use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

const VERSION: &str = env!("GIT_VERSION");

// Global MultiProgress manager. Using a single MultiProgress even for one bar
// lets us safely println during active bars and suspend for prompts without
// corrupting the terminal. MultiProgress::new is cheap and safe to create even
// when not a TTY.
static MULTI: OnceLock<MultiProgress> = OnceLock::new();

/// Returns the shared `MultiProgress` instance.
///
/// All spinners and progress bars should be created through this manager so
/// plain status messages can be printed safely while bars are active.
#[inline]
pub fn multi() -> &'static MultiProgress {
    MULTI.get_or_init(MultiProgress::new)
}

// ─────────────────────────────────────────────────────────────────────────────
// Machine-readable (JSON) output
// ─────────────────────────────────────────────────────────────────────────────

use serde::Serialize;

/// A single structured event emitted on `--format json`.
///
/// Every meaningful status change (stages, peer discovery, transfer offers and
/// completion, errors) is mirrored here so automation can consume Hayate's
/// output without parsing ANSI-decorated text. Events are newline-delimited
/// JSON objects on stdout (errors on stderr), each tagged with an `event`
/// field.
#[derive(Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum JsonEvent {
    Status {
        level: String,
        message: String,
    },
    Stage {
        stage: String,
        detail: String,
    },
    Field {
        key: String,
        value: String,
    },
    PairingCode {
        code: String,
        command: String,
    },
    Peer {
        name: String,
        address: String,
        os: String,
        rtt_ms: Option<f64>,
    },
    DiscoverSummary {
        count: usize,
    },
    TransferOffer {
        filename: String,
        size: u64,
        kind: String,
        from: String,
        cipher: String,
        hash: String,
    },
    TransferInfo {
        title: String,
        fields: std::collections::HashMap<String, String>,
    },
    TransferComplete {
        filename: String,
        size: u64,
        elapsed_secs: f64,
        speed_bps: u64,
        checksum: String,
        cipher: String,
        compressed: bool,
    },
    Bound {
        address: String,
        backend: String,
    },
    BoundAddresses {
        addresses: Vec<String>,
    },
    Error {
        message: String,
    },
}

/// Emits a JSON event to stdout (flushed) when the active policy is JSON mode.
/// In any other mode this is a no-op.
fn json_stdout(event: JsonEvent) {
    if !crate::policy::get().is_json() {
        return;
    }
    if let Ok(line) = serde_json::to_string(&event) {
        use std::io::Write;
        let mut lock = std::io::stdout().lock();
        let _ = writeln!(lock, "{line}");
        let _ = lock.flush();
    }
}

/// Emits a JSON error event to stderr (flushed) when the active policy is JSON
/// mode. In any other mode this is a no-op.
fn json_stderr(event: JsonEvent) {
    if !crate::policy::get().is_json() {
        return;
    }
    if let Ok(line) = serde_json::to_string(&event) {
        eprintln!("{line}");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Terminal capability detection (width, TTY, Unicode)
// ─────────────────────────────────────────────────────────────────────────────

thread_local! {
    /// Whether the output terminal supports Unicode box-drawing glyphs.
    static UNICODE_CAPABLE: bool = detect_unicode_capable();

    /// Whether stderr is a true terminal (not piped/redirected).
    ///
    /// MultiProgress defaults to stderr, so we use stderr TTY status as the gate.
    static IS_TTY: bool = std::io::stderr().is_terminal();
}

fn detect_unicode_capable() -> bool {
    if std::env::var_os("HAYATE_ASCII").is_some_and(|v| !v.is_empty()) {
        return false;
    }
    if std::env::var_os("TERM").is_some_and(|t| t == "dumb") {
        return false;
    }
    if cfg!(windows) {
        std::env::var_os("WT_SESSION").is_some()
            || std::env::var_os("TERM_PROGRAM").is_some()
            || std::env::var_os("WT_PROFILE_ID").is_some()
    } else {
        true
    }
}

/// Returns `true` when stderr is a real terminal (not piped or redirected).
#[inline]
pub fn is_tty() -> bool {
    IS_TTY.with(|v| *v)
}

/// Returns `true` when the terminal supports Unicode box-drawing glyphs.
#[inline]
pub fn unicode_capable() -> bool {
    UNICODE_CAPABLE.with(|v| *v)
}

/// Detect terminal width: ioctl, then `$COLUMNS`, then 80.
///
/// Clamped so status cards stay readable on both tiny and huge terminals.
#[must_use]
pub fn terminal_width() -> usize {
    let raw = platform_terminal_width()
        .or_else(|| {
            std::env::var("COLUMNS").ok().and_then(|c| c.parse::<u16>().ok()).filter(|w| *w > 0)
        })
        .unwrap_or(80) as usize;
    raw.clamp(40, 120)
}

/// Inner content width for framed cards (leaves room for margins and borders).
#[must_use]
pub fn card_inner_width() -> usize {
    terminal_width().saturating_sub(8).clamp(36, 72)
}

#[cfg(unix)]
fn platform_terminal_width() -> Option<u16> {
    // SAFETY: zeroed winsize + ioctl TIOCGWINSZ on stderr is the standard
    // terminal-size query; non-zero return means ioctl failed.
    unsafe {
        let mut ws: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDERR_FILENO, libc::TIOCGWINSZ, &mut ws) == 0 && ws.ws_col > 0 {
            Some(ws.ws_col)
        } else {
            None
        }
    }
}

#[cfg(not(unix))]
fn platform_terminal_width() -> Option<u16> {
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// Status icons — Unicode when available, ASCII fallbacks otherwise
// ─────────────────────────────────────────────────────────────────────────────

fn icon_info() -> &'static str {
    if unicode_capable() { "ℹ" } else { "i" }
}
fn icon_ok() -> &'static str {
    if unicode_capable() { "✓" } else { "OK" }
}
fn icon_warn() -> &'static str {
    if unicode_capable() { "⚠" } else { "!" }
}
fn icon_err() -> &'static str {
    if unicode_capable() { "✗" } else { "X" }
}
fn icon_arrow() -> &'static str {
    if unicode_capable() { "▶" } else { ">" }
}
fn icon_dot() -> &'static str {
    if unicode_capable() { "●" } else { "*" }
}
fn icon_lock() -> &'static str {
    // No emoji — geometric mark only.
    "#"
}

// ─────────────────────────────────────────────────────────────────────────────
// Box-drawing primitives — Unicode or ASCII
// ─────────────────────────────────────────────────────────────────────────────

fn box_tl() -> &'static str {
    if unicode_capable() { "╭" } else { "+" }
}
fn box_tr() -> &'static str {
    if unicode_capable() { "╮" } else { "+" }
}
fn box_bl() -> &'static str {
    if unicode_capable() { "╰" } else { "+" }
}
fn box_br() -> &'static str {
    if unicode_capable() { "╯" } else { "+" }
}
pub(crate) fn box_h() -> &'static str {
    if unicode_capable() { "─" } else { "-" }
}
pub(crate) fn box_v() -> &'static str {
    if unicode_capable() { "│" } else { "|" }
}

fn box_line(width: usize) -> String {
    box_h().repeat(width)
}

// ─────────────────────────────────────────────────────────────────────────────
// Status lines — route through MultiProgress when a bar may be active
// ─────────────────────────────────────────────────────────────────────────────

pub fn info(msg: &str) {
    json_stdout(JsonEvent::Status { level: "info".into(), message: msg.to_owned() });
    if crate::policy::get().is_json() {
        return;
    }
    let text = format!("   {}  {}", style(icon_info()).bold().blue(), style(msg).white());
    if is_tty() {
        let _ = multi().println(text);
    } else {
        println!("{text}");
    }
}

pub fn ok(msg: &str) {
    json_stdout(JsonEvent::Status { level: "ok".into(), message: msg.to_owned() });
    if crate::policy::get().is_json() {
        return;
    }
    let text = format!("   {}  {}", style(icon_ok()).bold().green(), msg);
    if is_tty() {
        let _ = multi().println(text);
    } else {
        println!("{text}");
    }
}

pub fn warn(msg: &str) {
    json_stdout(JsonEvent::Status { level: "warn".into(), message: msg.to_owned() });
    if crate::policy::get().is_json() {
        return;
    }
    let text = format!("   {}  {}", style(icon_warn()).bold().yellow(), style(msg).yellow());
    if is_tty() {
        let _ = multi().println(text);
    } else {
        println!("{text}");
    }
}

pub fn err(msg: &str) {
    json_stderr(JsonEvent::Error { message: msg.to_owned() });
    if crate::policy::get().is_json() {
        return;
    }
    let text = format!("   {}  {}", style(icon_err()).bold().red(), style(msg).red());
    if is_tty() {
        let _ = multi().println(text);
    } else {
        eprintln!("{text}");
    }
}

pub fn print_error(err: &anyhow::Error) {
    json_stderr(JsonEvent::Error { message: err.to_string() });
    if crate::policy::get().is_json() {
        return;
    }
    let mut lines = Vec::new();
    let mut chain = err.chain();
    let branch = if unicode_capable() { "└─" } else { "`-" };
    if let Some(top_err) = chain.next() {
        lines.push(format!(
            "   {}  {}",
            style(icon_err()).bold().red(),
            style(top_err.to_string()).bold().red()
        ));
    }
    for cause in chain {
        lines.push(format!("      {} {}", style(branch).dim(), style(cause.to_string()).dim()));
    }
    if is_tty() {
        for line in lines {
            let _ = multi().println(line);
        }
    } else {
        for line in lines {
            eprintln!("{line}");
        }
    }
}

pub fn stage(name: &str, detail: impl std::fmt::Display) {
    let detail = detail.to_string();
    json_stdout(JsonEvent::Stage { stage: name.to_owned(), detail: detail.clone() });
    if crate::policy::get().is_json() {
        return;
    }
    let text = format!(
        "   {}  {:<11} {}",
        style(icon_arrow()).bold().cyan(),
        style(name).bold(),
        style(detail).white()
    );
    if is_tty() {
        let _ = multi().println(text);
    } else {
        println!("{text}");
    }
}

pub fn key_value(key: &str, value: impl std::fmt::Display) {
    let value = value.to_string();
    json_stdout(JsonEvent::Field { key: key.to_owned(), value: value.clone() });
    if crate::policy::get().is_json() {
        return;
    }
    let text =
        format!("      {} {}", style(format!("{key:<10}")).dim(), style(value).white().bold());
    if is_tty() {
        let _ = multi().println(text);
    } else {
        println!("{text}");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Banner
// ─────────────────────────────────────────────────────────────────────────────

pub fn print_banner() {
    let width = terminal_width();

    if unicode_capable() && width >= 45 {
        let logo = r#"
    __  _______  _____  ____________
   / / / /   \ \/ /   |/_  __/ ____/
  / /_/ / /| |\  / /| | / / / __/
 / __  / ___ |/ / ___ |/ / / /___
/_/ /_/_/  |_/_/_/  |_/_/ /_____/
"#;
        println!("{}", style(logo).bold().cyan());
    } else {
        let logo = r#"
  H A Y A T E
  encrypted LAN transfer
"#;
        println!("{}", style(logo).bold().cyan());
    }

    let separator = if unicode_capable() { "━" } else { "=" };
    let bar_w = width.saturating_sub(6).clamp(32, 72);
    println!(
        "   {} {} {} {} {}",
        style("Hayate").bold().green(),
        style(box_v()).dim(),
        style("encrypted LAN transfer").white(),
        style(box_v()).dim(),
        style(format!("v{VERSION}")).cyan().bold()
    );
    println!("   {}", style(separator.repeat(bar_w)).dim());
    println!();
}

// ─────────────────────────────────────────────────────────────────────────────
// Pairing code display
// ─────────────────────────────────────────────────────────────────────────────

pub fn pairing_code(code: &str, command: &str) {
    json_stdout(JsonEvent::PairingCode { code: code.to_owned(), command: command.to_owned() });
    if crate::policy::get().is_json() {
        return;
    }
    let inner_width = card_inner_width().min(56);
    let v = box_v();
    println!();
    println!(
        "   {}{}{}",
        style(box_tl()).dim(),
        style(box_line(inner_width)).dim(),
        style(box_tr()).dim()
    );
    println!(
        "   {}  {} {}{}",
        style(v).dim(),
        style(icon_lock()).bold(),
        style(" Pairing Code").bold().cyan(),
        pad_right("", inner_width - 18, style(v).dim()),
    );
    println!(
        "   {}{}{}",
        style(v).dim(),
        style(format!("  {}", box_line(inner_width - 2))).dim(),
        style(v).dim()
    );
    println!(
        "   {}  {}{}",
        style(v).dim(),
        style(code).bold().yellow(),
        pad_right(code, inner_width - 2, style(v).dim()),
    );
    println!(
        "   {}{}{}",
        style(v).dim(),
        style(format!("  {}", box_line(inner_width - 2))).dim(),
        style(v).dim()
    );
    println!(
        "   {}  {} {}{}",
        style(v).dim(),
        style(icon_dot()).dim(),
        style("Run on receiver:").dim(),
        pad_right("● Run on receiver:", inner_width - 2, style(v).dim()),
    );
    println!(
        "   {}  {}{}",
        style(v).dim(),
        style(command).green().bold(),
        pad_right(command, inner_width - 2, style(v).dim()),
    );
    println!(
        "   {}{}{}",
        style(box_bl()).dim(),
        style(box_line(inner_width)).dim(),
        style(box_br()).dim()
    );
    println!();
}

/// Returns padding + border. Caller applies the style to `border`.
fn pad_right(content: &str, total_width: usize, border: impl std::fmt::Display) -> String {
    let content_len = console::measure_text_width(content);
    let pad = if total_width > content_len { total_width - content_len } else { 1 };
    format!("{}{}", " ".repeat(pad), border)
}

#[must_use]
pub fn cipher_name(cipher_id: u8) -> &'static str {
    match cipher_id {
        hayate::crypto::CIPHER_AES256_GCM => "AES-256-GCM",
        hayate::crypto::CIPHER_CHACHA20 => "ChaCha20-Poly1305",
        _ => "unknown",
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Transfer info card
// ─────────────────────────────────────────────────────────────────────────────

/// Displays a compact info card with key-value pairs inside a box.
pub fn print_info_card(title: &str, rows: &[(&str, String)]) {
    json_stdout(JsonEvent::TransferInfo {
        title: title.to_owned(),
        fields: rows.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect(),
    });
    if crate::policy::get().is_json() {
        return;
    }
    let inner_width = card_inner_width();
    let v = box_v();
    println!();
    // Top border
    println!(
        "   {}{}{}",
        style(box_tl()).cyan(),
        style(box_line(inner_width)).cyan(),
        style(box_tr()).cyan()
    );
    // Title row
    let title_display = format!("  {} {}", icon_arrow(), title);
    println!(
        "   {} {}{}",
        style(v).cyan(),
        style(&title_display).bold().cyan(),
        pad_right(&title_display, inner_width - 1, style(v).cyan()),
    );
    // Separator
    println!(
        "   {}{}{}",
        style(v).cyan(),
        style(format!("  {}", box_line(inner_width - 2))).dim(),
        style(v).cyan()
    );
    // Key-value rows
    for (key, value) in rows {
        let row_text = format!("  {:<12} {}", key, value);
        println!(
            "   {}    {} {}{}",
            style(v).cyan(),
            style(format!("{key:<12}")).dim(),
            style(value).white().bold(),
            pad_right(&row_text, inner_width - 1, style(v).cyan()),
        );
    }
    // Bottom border
    println!(
        "   {}{}{}",
        style(box_bl()).cyan(),
        style(box_line(inner_width)).cyan(),
        style(box_br()).cyan()
    );
    println!();
}

// ─────────────────────────────────────────────────────────────────────────────
// Transfer offer card (for receiver prompts)
// ─────────────────────────────────────────────────────────────────────────────

/// Displays a transfer offer card before the accept/reject prompt.
pub fn print_transfer_offer(
    filename: &str,
    size: u64,
    kind: &str,
    peer: std::net::SocketAddr,
    cipher: &str,
    hash_algo: &str,
) {
    json_stdout(JsonEvent::TransferOffer {
        filename: filename.to_owned(),
        size,
        kind: kind.to_owned(),
        from: peer.to_string(),
        cipher: cipher.to_owned(),
        hash: hash_algo.to_owned(),
    });
    if crate::policy::get().is_json() {
        return;
    }
    let rows = [
        ("filename", filename.to_owned()),
        ("type", kind.to_owned()),
        ("size", format_bytes(size)),
        ("from", peer.to_string()),
        ("cipher", cipher.to_owned()),
        ("hash", hash_algo.to_owned()),
    ];
    // Pretty path only: skip the JSON re-emit from `print_info_card` by
    // temporarily relying on pretty mode (already checked).
    print_info_card("Incoming Transfer", &rows);
}

// ─────────────────────────────────────────────────────────────────────────────
// Progress bar
// ─────────────────────────────────────────────────────────────────────────────

/// Returns progress characters that render on the current terminal.
fn progress_chars() -> &'static str {
    if unicode_capable() { "━━╸ " } else { "==> " }
}

fn spinner_tick_chars() -> &'static str {
    if unicode_capable() { "⣾⣽⣻⢿⡿⣟⣯⣷⠿" } else { "/-\\|" }
}

/// Creates a labelled transfer progress bar attached to the shared
/// MultiProgress.
pub fn transfer_progress_bar(label: &str, total_bytes: u64) -> ProgressBar {
    let template = if unicode_capable() {
        "   {prefix:.bold.cyan} {spinner} {wide_bar:.cyan/blue} {bytes:>10}/{total_bytes:10}  {bytes_per_sec:>11.green}  {eta:>6.dim}"
    } else {
        "   {prefix:.bold} {spinner} {wide_bar} {bytes:>10}/{total_bytes:10}  {bytes_per_sec:>11}  {eta:>6}"
    };
    let style = ProgressStyle::with_template(template)
        .expect("valid template")
        .progress_chars(progress_chars());
    let pb = multi().add(ProgressBar::new(total_bytes));
    pb.set_style(style);
    pb.set_prefix(format!("{label:>8}"));
    // Seed the initial draw so the bar is visible before the first chunk
    // lands, and speed/ETA are calculated from actual progress deltas rather
    // than being polluted by zero-progress steady-tick samples.
    pb.set_position(0);
    pb
}

pub fn set_transfer_position(pb: &ProgressBar, bytes: u64) {
    if let Some(len) = pb.length()
        && bytes > len
    {
        pb.set_length(bytes);
    }
    pb.set_position(bytes);
}

pub fn finish_transfer_progress(pb: &ProgressBar, total_bytes: u64) {
    set_transfer_position(pb, total_bytes.max(pb.position()));
    pb.finish_and_clear();
}

/// Creates a spinner for indeterminate progress attached to the shared
/// MultiProgress.
pub fn spinner(label: &str, detail: &str) -> ProgressBar {
    let template = if unicode_capable() {
        "   {spinner:.cyan.bold}  {prefix:.bold}"
    } else {
        "   {spinner}  {prefix}"
    };
    let style = ProgressStyle::with_template(template)
        .expect("valid template")
        .tick_chars(spinner_tick_chars());
    let pb = multi().add(ProgressBar::new_spinner());
    pb.set_style(style);
    pb.set_prefix(format!("{label}  {detail}"));
    if is_tty() {
        pb.enable_steady_tick(std::time::Duration::from_millis(120));
    }
    pb
}

/// Updates the detail portion of an existing spinner.
/// Updates an existing spinner's message. Kept for call sites that keep a bar
/// across multi-step waits without recreating it.
#[allow(dead_code)]
pub fn spinner_update(pb: &ProgressBar, label: &str, detail: &str) {
    pb.set_prefix(format!("{label}  {detail}"));
}

/// Creates a progress bar for network scanning attached to the shared
/// MultiProgress.
pub fn scan_progress_bar(total_hosts: u64) -> ProgressBar {
    let template = if unicode_capable() {
        "   {spinner:.cyan.bold}  Scanning {wide_bar:.cyan/blue} {pos:>4}/{len:<4} hosts  {msg:.dim.white}"
    } else {
        "   {spinner}  Scanning {wide_bar} {pos:>4}/{len:<4} hosts  {msg}"
    };
    let style = ProgressStyle::with_template(template)
        .expect("valid template")
        .progress_chars(progress_chars())
        .tick_chars(spinner_tick_chars());
    let pb = multi().add(ProgressBar::new(total_hosts));
    pb.set_style(style);
    if is_tty() {
        pb.enable_steady_tick(std::time::Duration::from_millis(120));
    }
    pb
}

// ─────────────────────────────────────────────────────────────────────────────
// Prompt suspend helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Run a closure while the shared MultiProgress is suspended (all bars hidden).
/// Use this around interactive prompts so the prompt can draw normally.
pub fn suspend_for_prompt<R>(f: impl FnOnce() -> R) -> R {
    multi().suspend(f)
}

// ─────────────────────────────────────────────────────────────────────────────
// Peer discovery table
// ─────────────────────────────────────────────────────────────────────────────

/// Prints a single-line live notification when a peer is discovered.
pub fn peer_found_live(
    name: &str,
    addr: &std::net::SocketAddr,
    os: &str,
    rtt: &str,
    quality: &str,
) {
    let rtt = rtt.trim();
    let rtt_ms =
        rtt.strip_suffix("ms").and_then(|value| value.trim().parse::<f64>().ok()).or_else(|| {
            rtt.strip_suffix("µs")
                .and_then(|value| value.trim().parse::<f64>().ok())
                .map(|value| value / 1000.0)
        });
    json_stdout(JsonEvent::Peer {
        name: name.to_owned(),
        address: addr.to_string(),
        os: os.to_owned(),
        rtt_ms,
    });
    if crate::policy::get().is_json() {
        return;
    }
    let text = format!(
        "   {} {}  {} {}  {}",
        style(quality).green(),
        style(name).white().bold(),
        style(addr).green(),
        style(os).dim(),
        style(rtt).dim(),
    );
    if is_tty() {
        let _ = multi().println(text);
    } else {
        println!("{text}");
    }
}

pub fn print_peer_table(peers: &[(String, std::net::SocketAddr, String)]) {
    json_stdout(JsonEvent::DiscoverSummary { count: peers.len() });
    if peers.is_empty() {
        warn("No peers found on the network.");
        return;
    }

    if crate::policy::get().is_json() {
        return;
    }

    let inner_width = card_inner_width().max(52);
    let v = box_v();
    println!();
    ok(&format!("Discovered {} peer(s)", peers.len()));
    println!();

    // Header
    println!(
        "   {}{}{}",
        style(box_tl()).dim(),
        style(box_line(inner_width)).dim(),
        style(box_tr()).dim()
    );
    println!(
        "   {}  {:<4} {:<22} {:<24} {}  {}",
        style(v).dim(),
        style("#").bold().dim(),
        style("NAME").bold().dim(),
        style("ADDRESS").bold().dim(),
        style("OS").bold().dim(),
        style(v).dim()
    );
    println!(
        "   {}{}{}",
        style(v).dim(),
        style(format!("  {}", box_line(inner_width - 2))).dim(),
        style(v).dim()
    );

    // Rows
    for (idx, (name, addr, os)) in peers.iter().enumerate() {
        let num = format!("{}", idx + 1);
        let name_len = name.chars().count();
        let name_display = if name_len > 20 {
            format!("{}…", name.chars().take(19).collect::<String>())
        } else {
            name.clone()
        };
        let addr_str = addr.to_string();
        println!(
            "   {}  {:<4} {:<22} {:<24} {}{}",
            style(v).dim(),
            style(&num).cyan().bold(),
            style(&name_display).white(),
            style(&addr_str).green(),
            style(os).dim(),
            pad_right(
                &format!("  {num:<4} {name_display:<22} {addr_str:<24} {os}"),
                inner_width,
                style(v).dim(),
            )
        );
    }

    // Bottom border
    println!(
        "   {}{}{}",
        style(box_bl()).dim(),
        style(box_line(inner_width)).dim(),
        style(box_br()).dim()
    );
    println!();
}

// ─────────────────────────────────────────────────────────────────────────────
// Transfer summary card
// ─────────────────────────────────────────────────────────────────────────────

pub fn print_transfer_summary(
    filename: &str,
    bytes: u64,
    elapsed_secs: f64,
    checksum: &str,
    compressed: bool,
    cipher: &str,
) {
    let speed_val = speed(bytes, elapsed_secs);
    json_stdout(JsonEvent::TransferComplete {
        filename: filename.to_owned(),
        size: bytes,
        elapsed_secs,
        speed_bps: speed_val,
        checksum: checksum.to_owned(),
        cipher: cipher.to_owned(),
        compressed,
    });
    if crate::policy::get().is_json() {
        return;
    }
    let speed_str = format!("{}/s", format_bytes(speed_val));
    let speed_styled = color_speed(speed_val, &speed_str);

    let rows = [
        ("file", filename.to_owned()),
        ("size", format_bytes(bytes)),
        ("time", format!("{elapsed_secs:.2}s")),
        ("speed", speed_str.clone()),
        ("cipher", cipher.to_owned()),
        ("compress", if compressed { "zstd".to_owned() } else { "off".to_owned() }),
        ("checksum", truncate_checksum(checksum)),
    ];

    let inner_width = card_inner_width();
    let v = box_v();
    println!();
    // Top border
    println!(
        "   {}{}{}",
        style(box_tl()).green(),
        style(box_line(inner_width)).green(),
        style(box_tr()).green()
    );
    // Title
    let title_text = format!("  {} Transfer Complete", icon_ok());
    println!(
        "   {} {}{}",
        style(v).green(),
        style(&title_text).bold().green(),
        pad_right(&title_text, inner_width - 1, style(v).green()),
    );
    // Separator
    println!(
        "   {}{}{}",
        style(v).green(),
        style(format!("  {}", box_line(inner_width - 2))).dim(),
        style(v).green()
    );
    // Rows
    for (key, value) in &rows {
        let row_text = format!("  {:<12} {}", key, value);
        if *key == "speed" {
            println!(
                "   {}    {} {}{}",
                style(v).green(),
                style(format!("{key:<12}")).dim(),
                speed_styled,
                pad_right(&row_text, inner_width - 1, style(v).green()),
            );
        } else {
            println!(
                "   {}    {} {}{}",
                style(v).green(),
                style(format!("{key:<12}")).dim(),
                style(value).white().bold(),
                pad_right(&row_text, inner_width - 1, style(v).green()),
            );
        }
    }
    // Bottom border
    println!(
        "   {}{}{}",
        style(box_bl()).green(),
        style(box_line(inner_width)).green(),
        style(box_br()).green()
    );
    println!();
}

/// Color-code speed based on performance tiers.
fn color_speed(bytes_per_sec: u64, display: &str) -> String {
    const MB: u64 = 1024 * 1024;
    if bytes_per_sec >= 100 * MB {
        format!("{}", style(display).green().bold())
    } else if bytes_per_sec >= 10 * MB {
        format!("{}", style(display).yellow().bold())
    } else {
        format!("{}", style(display).red().bold())
    }
}

fn truncate_checksum(checksum: &str) -> String {
    let chars: Vec<char> = checksum.chars().collect();
    if chars.len() > 16 {
        let first_part: String = chars[..8].iter().collect();
        let last_part: String = chars[chars.len() - 8..].iter().collect();
        format!("{}…{}", first_part, last_part)
    } else {
        checksum.to_owned()
    }
}

#[must_use]
pub fn format_bytes(b: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = b as f64;
    let mut unit = UNITS[0];
    for &u in &UNITS[1..] {
        if v < 1024.0 {
            break;
        }
        v /= 1024.0;
        unit = u;
    }
    if v < 10.0 {
        format!("{v:.2} {unit}")
    } else if v < 100.0 {
        format!("{v:.1} {unit}")
    } else {
        format!("{v:.0} {unit}")
    }
}

fn speed(bytes: u64, elapsed_secs: f64) -> u64 {
    if elapsed_secs <= f64::EPSILON {
        return bytes;
    }
    (bytes as f64 / elapsed_secs) as u64
}

#[must_use]
pub fn get_backend_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "IOCP"
    }
    #[cfg(target_os = "macos")]
    {
        "kqueue"
    }
    #[cfg(target_os = "android")]
    {
        "epoll"
    }
    #[cfg(target_os = "linux")]
    {
        "io_uring"
    }
    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "android",
        target_os = "linux"
    )))]
    {
        "polling"
    }
}

/// Prints a single "Bound" line showing the socket's bind address.
/// Use [`print_local_addresses`] afterwards to list reachable addresses.
pub fn print_bound(addr: impl std::fmt::Display) {
    let backend = get_backend_name();
    let addr = addr.to_string();
    json_stdout(JsonEvent::Bound { address: addr.clone(), backend: backend.to_owned() });
    if crate::policy::get().is_json() {
        return;
    }
    let dot = icon_dot();
    println!(
        "   {}  {} {} · {} {}",
        style(dot).bold().green(),
        style("Bound").bold().white(),
        style(addr).bold().yellow(),
        style("QUIC").bold().magenta(),
        style(format!("[{backend}]")).bold().cyan()
    );
}

/// Prints the cancellation hint.
pub fn print_cancel_hint() {
    if crate::policy::get().is_json() {
        return;
    }
    println!("      {}  {}", style("ESC / q").bold().dim(), style("to exit").dim());
}

/// Prints a compact table of local addresses a peer can connect to, with
/// the interface name alongside each address.
pub fn print_local_addresses(addrs: &[(std::net::Ipv4Addr, String)]) {
    if addrs.is_empty() {
        return;
    }
    json_stdout(JsonEvent::BoundAddresses {
        addresses: addrs
            .iter()
            .map(
                |(ip, name)| {
                    if name.is_empty() { ip.to_string() } else { format!("{ip} ({name})") }
                },
            )
            .collect(),
    });
    if crate::policy::get().is_json() {
        return;
    }
    // Find the widest IP:port so columns align.
    let max_ip_width = addrs.iter().map(|(ip, _)| ip.to_string().len()).max().unwrap_or(15);
    let max_name_width = addrs.iter().map(|(_, name)| name.len()).max().unwrap_or(8);

    let inner = max_ip_width + max_name_width + 7; // "  ● " + "  " + padding
    let v = box_v();
    let dot = icon_dot();

    // Top border
    println!(
        "   {}{}{}",
        style(box_tl()).dim(),
        style(box_line(inner)).dim(),
        style(box_tr()).dim()
    );
    for (ip, name) in addrs {
        let ip_pad = " ".repeat(max_ip_width.saturating_sub(ip.to_string().len()));
        let name_pad = " ".repeat(max_name_width.saturating_sub(name.len()));
        println!(
            "   {}  {} {}{} {}{}  {}",
            style(v).dim(),
            style(dot).green(),
            style(ip).yellow().bold(),
            style(ip_pad).dim(),
            style(name).dim(),
            style(name_pad).dim(),
            style(v).dim()
        );
    }
    // Bottom border
    println!(
        "   {}{}{}",
        style(box_bl()).dim(),
        style(box_line(inner)).dim(),
        style(box_br()).dim()
    );
}

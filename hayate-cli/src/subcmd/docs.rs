//! `hayate docs` — in-terminal handbook, or open the website.

use std::process::Command;

use anyhow::{Context, Result, bail};
use console::style;

use crate::cli::DocsArgs;
use crate::output::{self, box_h, box_v, card_inner_width, unicode_capable};

/// Official product / docs site.
pub const DOCS_URL: &str = "https://hayate.shiina.xyz";

/// Library API docs.
pub const API_DOCS_URL: &str = "https://docs.rs/hayate";

/// Runs `hayate docs`.
pub fn run(args: DocsArgs) -> Result<()> {
    if args.web {
        return open_url(DOCS_URL);
    }

    let topic =
        args.topic.as_deref().map(str::to_ascii_lowercase).unwrap_or_else(|| "all".to_owned());

    match topic.as_str() {
        "all" | "" => print_all(),
        "start" | "quick" | "quickstart" => section_quick_start(),
        "send" => section_send(),
        "receive" | "recv" => section_receive(),
        "discover" | "scan" => section_discover(),
        "completions" | "complete" => section_completions(),
        "peers" | "peer" => section_peers(),
        "history" | "log" => section_history(),
        "web" | "dashboard" | "ui" => section_web(),
        "site" | "online" => section_online(),
        "exit" | "codes" | "status" => section_exit_codes(),
        "env" | "environment" => section_environment(),
        "security" => section_security(),
        other => {
            bail!(
                "unknown docs topic `{other}`\n\
                 try: all, start, send, receive, discover, web, peers, history, completions, exit, env, security, site"
            );
        },
    }

    Ok(())
}

fn print_all() {
    output::print_banner();
    section_intro();
    section_quick_start();
    section_send();
    section_receive();
    section_discover();
    section_web();
    section_peers();
    section_history();
    section_completions();
    section_exit_codes();
    section_environment();
    section_security();
    section_online();
}

fn section_intro() {
    heading("What is Hayate?");
    body("Encrypted LAN file transfer over QUIC. No cloud, no accounts, no SSH.");
    body("Pair with a short code phrase, or dial a receiver by ip:port.");
    body(&format!("Version  {}", env!("CARGO_PKG_VERSION")));
    println!();
}

fn section_quick_start() {
    heading("Quick start");
    sub("Pairing (no IP)");
    code("hayate send ./photos.zip");
    code("hayate receive --code \"forest-river-mango-silver-orbit\"");
    println!();
    sub("Direct");
    code("hayate receive --port 50001 --output ./downloads");
    code("hayate send ./archive.tar.gz 192.168.1.50:50001");
    println!();
    sub("Folders");
    body("Directories stream as a safe tar archive (no path traversal / symlinks).");
    code("hayate send ./project --code alpha-bravo-charlie-delta");
    println!();
}

fn section_send() {
    heading("hayate send [PATH] [TARGET]");
    body("Omit TARGET to print a pairing code and wait for a receiver.");
    body("Pass ip:port for a direct connection. Optional --passphrase authenticates it.");
    kv("PATH", "file or directory to send (omit for an interactive prompt)");
    kv("TARGET", "receiver ip:port (optional)");
    kv("--to NAME", "dial a saved peer (see hayate peers)");
    kv("--pick", "scan the LAN and choose a receiver interactively");
    kv("--code", "pairing phrase");
    kv("--passphrase", "direct-mode secret");
    kv("-z / --compress", "zstd on (default)");
    kv("--no-compress", "disable compression");
    kv("--hash", "blake3 | sha256");
    kv("--bandwidth-limit", "cap throughput, e.g. 10MiB, 500KiB, 2M");
    kv("--no-progress", "hide progress bar");
    println!();
    sub("Examples");
    code("hayate send ./report.pdf");
    code("hayate send ./blob.bin 10.0.0.5:50001 --no-compress --format json");
    code("hayate send ./movie.mkv --to laptop --bandwidth-limit 50MiB");
    code("hayate send ./photo.jpg --pick");
    code("hayate send ./blob.bin 10.0.0.5:50001 --passphrase \"shared secret\"");
    println!();
}

fn section_receive() {
    heading("hayate receive");
    body("Listen for direct transfers, or join a pairing session with --code.");
    body("Prompts before accepting unless --auto-accept is set.");
    kv("-b / --bind", "bind address (HAYATE_BIND, default 0.0.0.0)");
    kv("-p / --port", "listen port (HAYATE_PORT, default 50001)");
    kv("-o / --output", "save directory (default .)");
    kv("--code", "join pairing session");
    kv("--passphrase", "authenticate direct sender");
    kv("--auto-accept", "skip confirm prompt");
    kv("--resume", "continue interrupted single-file transfers");
    kv("--once", "exit after the first completed transfer");
    kv("--no-progress", "hide progress bar");
    println!();
    sub("Examples");
    code("hayate receive --output ./downloads");
    code("hayate receive --code \"forest-river-mango-silver-orbit\"");
    code("hayate receive --auto-accept --no-progress --format plain");
    code("hayate receive --once --resume --output ./downloads");
    println!();
}

fn section_discover() {
    heading("hayate discover");
    body("Find receivers on the local network. Uses mDNS and a UDP broadcast");
    body("query first (instant, no scanning); falls back to a QUIC subnet sweep.");
    body("Results are unauthenticated hints — the pairing handshake is the proof.");
    kv("-t / --timeout", "seconds (default 15)");
    kv("--scan", "also sweep every host (finds receivers that do not advertise)");
    kv("--cidr", "sweep an explicit network, e.g. 192.168.1.0/24 or 10.0.0.0/16");
    println!();
    code("hayate discover");
    code("hayate discover --scan --timeout 5");
    code("hayate discover --cidr 10.0.0.0/16");
    println!();
}

fn section_web() {
    heading("hayate web");
    body("Loopback dashboard: drag-and-drop send, receive listener, live progress.");
    body("Peers, device info, and transfer history update over SSE.");
    body("The startup URL carries a per-launch token — treat it like a password.");
    kv("--bind", "interface to bind (default 127.0.0.1)");
    kv("--port", "dashboard port (default 8787)");
    kv("--allow-lan", "expose beyond this machine (not recommended)");
    println!();
    code("hayate web");
    code("hayate web --port 9000");
    println!();
}

fn section_peers() {
    heading("hayate peers");
    body("Save receiver addresses under short names, then send with --to NAME.");
    body("Successful direct sends are auto-remembered by peer IP.");
    kv("list", "show saved peers");
    kv("add NAME ip:port", "save a peer");
    kv("remove NAME", "delete a peer");
    println!();
    code("hayate peers add laptop 192.168.1.20:50001");
    code("hayate send ./movie.mkv --to laptop");
    println!();
}

fn section_history() {
    heading("hayate history");
    body("Every completed transfer is appended to a local JSONL log.");
    kv("-n / --limit", "show at most N entries (default 20, 0 = all)");
    kv("--clear", "delete the history log");
    println!();
    code("hayate history");
    code("hayate history -n 5 --format json");
    println!();
}

fn section_completions() {
    heading("Shell completions");
    body("Print a script, or install it to a conventional user path.");
    println!();
    code("hayate completions bash");
    code("hayate completions zsh --install");
    code("hayate completions fish --install");
    println!();
    sub("Bash (session)");
    code("eval \"$(hayate completions bash)\"");
    println!();
    sub("Zsh (after --install)");
    code("fpath=(~/.zsh/completions $fpath)");
    code("autoload -Uz compinit && compinit");
    println!();
    sub("Fish");
    body("Loads ~/.config/fish/completions/ on the next session.");
    println!();
}

fn section_exit_codes() {
    heading("Exit codes");
    kv("0", "success");
    kv("1", "general error");
    kv("2", "usage / argument error");
    kv("3", "transfer rejected");
    kv("4", "protocol version mismatch");
    kv("5", "invalid pairing passphrase");
    kv("6", "timed out");
    kv("7", "cancelled");
    kv("130", "interrupted (Ctrl+C / Esc)");
    println!();
}

fn section_environment() {
    heading("Environment");
    kv("HAYATE_BIND", "default receive bind address");
    kv("HAYATE_PORT", "default receive port");
    kv("HAYATE_FORCE_CHACHA20", "force ChaCha20-Poly1305");
    kv("HAYATE_ASCII", "ASCII-only status glyphs");
    kv("NO_COLOR", "disable color");
    kv("COLUMNS", "terminal width fallback");
    kv("TERM=dumb", "disable fancy Unicode");
    println!();
    body("Global flags:  --color  --format  -q  -v");
    println!();
}

fn section_security() {
    heading("Security (short)");
    body("X25519 ECDH + Argon2id-stretched phrase + HKDF-SHA256 + AEAD frames.");
    body("Phrase stretching slows offline guessing, but is not a full PAKE.");
    body("Tar extract rejects absolute paths, .., and symlinks.");
    body("Discovery is unauthenticated; the handshake is the trust boundary.");
    println!();
}

fn section_online() {
    heading("More");
    body(&format!("Website     {DOCS_URL}"));
    body(&format!("API docs    {API_DOCS_URL}"));
    body("Open site   hayate docs --web");
    body("This guide  hayate docs");
    body("One topic   hayate docs send | receive | completions | exit | env");
    println!();
}

// ── rendering helpers ──────────────────────────────────────────────────────

fn heading(title: &str) {
    let w = card_inner_width();
    let rule = if unicode_capable() { box_h().repeat(w) } else { "-".repeat(w) };
    println!();
    println!(
        "   {} {}",
        style(if unicode_capable() { "▸" } else { ">" }).bold().cyan(),
        style(title).bold().green()
    );
    println!("   {}", style(rule).dim());
}

fn sub(title: &str) {
    println!("   {} {}", style(box_v()).dim(), style(title).bold().white());
}

fn body(text: &str) {
    println!("   {}", style(text).white());
}

fn code(line: &str) {
    println!("      {}", style(line).cyan());
}

fn kv(key: &str, value: &str) {
    println!("      {}  {}", style(format!("{key:<22}")).dim(), style(value).white());
}

fn open_url(url: &str) -> Result<()> {
    println!(
        "   {}  Opening {}",
        style(if unicode_capable() { "→" } else { ">" }).bold().cyan(),
        style(url).underlined().cyan()
    );

    let status = if cfg!(target_os = "macos") {
        Command::new("open").arg(url).status()
    } else if cfg!(target_os = "windows") {
        // `start` is a shell builtin; empty title avoids issues with quoted
        // URLs.
        Command::new("cmd").args(["/C", "start", "", url]).status()
    } else {
        // Linux / Android Termux: try xdg-open, then sensible-browser.
        match Command::new("xdg-open").arg(url).status() {
            Ok(s) if s.success() => Ok(s),
            _ => Command::new("sensible-browser").arg(url).status(),
        }
    }
    .with_context(|| format!("failed to open browser for {url}"))?;

    if !status.success() {
        bail!("could not open a browser (exit {status}).\nVisit: {url}");
    }
    Ok(())
}

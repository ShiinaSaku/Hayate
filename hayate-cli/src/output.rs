//! Terminal output helpers — banner, status lines, progress bars.
//!
//! Rules:
//!  - No emojis.
//!  - Clean ASCII box characters only.
//!  - Consistent margin and column widths.

use console::style;
use indicatif::{ProgressBar, ProgressStyle};

const VERSION: &str = env!("CARGO_PKG_VERSION");

// ---------------------------------------------------------------------------
// Banner
// ---------------------------------------------------------------------------

pub fn print_banner() {
    let term = console::Term::stdout();
    let width = term.size_checked().map(|(_, w)| w).unwrap_or(80);

    if width >= 65 {
        // Vibrant cyan styling for the new high-tech cybersecurity logo
        let logo = r#"
  __   __     _____    __  __    _____    _______     _____  
 /\_\ /_/\   /\___/\ /\  /\  /\ /\___/\ /\_______)\ /\_____\ 
( ( (_) ) ) / / _ \ \\ \ \/ / // / _ \ \\(___  __\/( (_____/ 
 \ \___/ /  \ \(_)/ / \ \__/ / \ \(_)/ /  / / /     \ \__\   
 / / _ \ \  / / _ \ \  \__/ /  / / _ \ \ ( ( (      / /__/_  
( (_( )_) )( (_( )_) ) / / /  ( (_( )_) ) \ \ \    ( (_____\ 
 \/_/ \_\/  \/_/ \_\/  \/_/    \/_/ \_\/  /_/_/     \/_____/ 
"#;
        println!("{}", style(logo).bold().cyan());
    } else {
        // Scaled fallback logo for narrow screen sizes (e.g. mobile/Termux portrait)
        let logo = r#"
  _  _  _  _  _  _ ___ ___ 
 | || |/ _ \| || / _ \ | | 
 | __ | (_) \  / | (_) | | 
 |_||_|\___/ \/   \___/|_| 
"#;
        println!("{}", style(logo).bold().cyan());
    }

    println!(
        "   {} {} {}",
        style("Swift File Transfer").bold().green(),
        style("|").dim(),
        style(format!("v{VERSION}")).cyan().bold()
    );
    println!(
        "   {}\n",
        style("Secure, Encrypted, & Compressed").dim().yellow()
    );
}

// ---------------------------------------------------------------------------
// Status lines
// ---------------------------------------------------------------------------

pub fn info(msg: &str) {
    println!("   {}  {}", style("*").bold().blue(), msg);
}

pub fn ok(msg: &str) {
    println!("   {}  {}", style("+").bold().green(), msg);
}

pub fn warn(msg: &str) {
    println!("   {}  {}", style("!").bold().yellow(), msg);
}

pub fn err(msg: &str) {
    eprintln!("   {}  {}", style("x").bold().red(), msg);
}

// ---------------------------------------------------------------------------
// Progress bar
// ---------------------------------------------------------------------------

/// Creates a download/upload progress bar.
pub fn progress_bar(total_bytes: u64) -> ProgressBar {
    let style = ProgressStyle::with_template(
        "   {spinner:.green} [{elapsed_precise}] ▕{bar:40.cyan/blue}▏ {bytes}/{total_bytes} ({percent}%) {bytes_per_sec} {eta}",
    )
    .expect("valid template")
    .progress_chars("█▉▊▋▌▍▎▏  ");
    let pb = ProgressBar::new(total_bytes);
    pb.set_style(style);
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb
}

/// Creates a spinner for indeterminate progress.
#[allow(dead_code)]
pub fn spinner(prefix: &str) -> ProgressBar {
    let style = ProgressStyle::with_template(&format!("   {{spinner:.cyan}} {prefix}  {{msg}}"))
        .expect("valid template");
    let pb = ProgressBar::new_spinner();
    pb.set_style(style);
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb
}

// ---------------------------------------------------------------------------
// Peer discovery table
// ---------------------------------------------------------------------------

pub fn print_peer_table(peers: &[(String, std::net::SocketAddr, String)]) {
    if peers.is_empty() {
        warn("No peers found.");
        return;
    }
    println!("\n  Found {} peer(s):\n", peers.len());
    println!("  {:<24} {:<24} OS", "NAME", "ADDRESS");
    println!("  {}", "-".repeat(64));
    for (name, addr, os) in peers {
        println!("  {:<24} {:<24} {}", name, addr, os);
    }
    println!();
}

// ---------------------------------------------------------------------------
// Transfer summary
// ---------------------------------------------------------------------------

pub fn print_transfer_summary(
    filename: &str,
    bytes: u64,
    elapsed_secs: f64,
    checksum: &str,
    compressed: bool,
) {
    println!();
    ok(&format!("Transfer complete: {filename}"));
    info(&format!(
        "Size: {}  Time: {:.1}s  Speed: {}/s  Compress: {}",
        format_bytes(bytes),
        elapsed_secs,
        format_bytes((bytes as f64 / elapsed_secs) as u64),
        if compressed { "zstd" } else { "off" },
    ));
    info(&format!("SHA-256: {checksum}"));
    println!();
}

fn format_bytes(b: u64) -> String {
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

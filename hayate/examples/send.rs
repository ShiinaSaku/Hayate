//! Standalone example demonstrating how to send files programmatically using
//! `HayateSender`.
//!
//! Run this example with:
//! ```bash
//! cargo run --example send <file_or_directory_path> <receiver_ip:port>
//! ```

use std::net::SocketAddr;

use hayate::runner::HayateSender;

#[compio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: cargo run --example send <file_or_directory_path> <receiver_ip:port>");
        std::process::exit(1);
    }
    let path = &args[1];
    let target_addr: SocketAddr = args[2].parse()?;

    println!("Connecting and sending '{path}' to {target_addr}...");

    let sender = HayateSender::new().target(target_addr).compress(true);

    let checksum = sender
        .send(path, |bytes| {
            println!("Progress: {bytes} bytes transferred");
            Ok(())
        })
        .await?;

    println!("Transfer complete!");
    println!("SHA-256 Checksum: {checksum}");

    Ok(())
}

//! Standalone example demonstrating how to receive files programmatically using
//! `HayateReceiver`.
//!
//! Run this example with:
//! ```bash
//! cargo run --example receive [port]
//! ```

use std::net::SocketAddr;

use hayate::runner::HayateReceiver;

#[compio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let port = if args.len() > 1 { args[1].parse::<u16>()? } else { 50001 };

    let bind_addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("Listening for incoming Hayate connections on {bind_addr}...");

    let receiver = HayateReceiver::new().bind(bind_addr).auto_accept(false); // Ask for consent

    let (checksum, dest) = receiver
        .receive(
            ".",
            |meta| {
                println!("\nIncoming Transfer Request:");
                println!("  Name: {}", meta.filename);
                println!("  Size: {} bytes", meta.total_size);
                println!("Accepting transfer request automatically...");
                true // Accept
            },
            |bytes| {
                println!("Progress: {bytes} bytes received");
                Ok(())
            },
        )
        .await?;

    println!("\nTransfer complete!");
    println!("Saved to: {}", dest.display());
    println!("SHA-256 Checksum: {checksum}");

    Ok(())
}

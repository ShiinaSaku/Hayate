//! Loopback throughput benchmark for the transfer engine.
//!
//! This is a developer tool, not a correctness test: it sends a file to a
//! receiver in the same process over loopback, timing the full
//! connect/handshake/payload path. Use it to compare pipeline changes on the
//! same machine.
//!
//! ```bash
//! cargo run --release --example throughput -- /path/to/large.bin [rounds]
//! ```

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::PathBuf;
use std::time::Instant;

use hayate::{HayateReceiver, HayateSender};

fn pick_port() -> u16 {
    let listener = std::net::TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
        .expect("bind ephemeral port");
    listener.local_addr().expect("local addr").port()
}

#[compio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(args.next().expect("usage: throughput <file> [rounds]"));
    let rounds: usize = args.next().map_or(3, |value| value.parse().expect("round count"));
    let size = std::fs::metadata(&path)?.len();
    let compress = std::env::var("HAYATE_BENCH_COMPRESS").map_or(true, |value| value != "0");

    for round in 0..rounds {
        let port = pick_port();
        let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
        let dst = std::env::temp_dir().join(format!("hayate-throughput-{round}"));
        std::fs::create_dir_all(&dst)?;

        let receiver_addr: SocketAddr = addr.into();
        let recv_dst = dst.clone();
        let receiver = HayateReceiver::new().bind(receiver_addr).auto_accept(true);
        let recv_handle = compio::runtime::spawn(async move {
            receiver.receive(&recv_dst, |_| true, |_| Ok(())).await
        });
        compio::time::sleep(std::time::Duration::from_millis(50)).await;

        let started = Instant::now();
        let send_checksum = HayateSender::new()
            .target(receiver_addr)
            .compress(compress)
            .send(&path, |_| Ok(()))
            .await?;
        let (recv_checksum, received) =
            recv_handle.await.map_err(|error| std::io::Error::other(error.to_string()))??;
        let elapsed = started.elapsed();

        assert_eq!(send_checksum, recv_checksum, "checksum mismatch");
        let bytes = std::fs::metadata(&received)?.len();
        assert_eq!(bytes, size, "size mismatch");

        let mib = size as f64 / (1024.0 * 1024.0);
        let seconds = elapsed.as_secs_f64();
        println!("round {round}: {mib:.0} MiB in {seconds:.3}s = {:.0} MiB/s", mib / seconds);

        std::fs::remove_dir_all(&dst)?;
    }
    Ok(())
}

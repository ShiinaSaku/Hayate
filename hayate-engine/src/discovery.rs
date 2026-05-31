//! Discovery over UDP multicast/broadcast using compio.

use std::{io, net::SocketAddr, time::Duration};

use sha2::{Digest, Sha256};

/// Computes SHA-256 of the phrase and returns the first 4 bytes as a hex string.
#[must_use]
pub fn derive_channel_id(phrase: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(phrase.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..4])
}

/// Periodically broadcasts a UDP packet with the channel ID and QUIC listening port.
pub async fn start_broadcaster(channel_id: String, port: u16) -> Result<(), io::Error> {
    let socket = compio::net::UdpSocket::bind("0.0.0.0:0").await?;
    socket.set_broadcast(true)?;
    let msg = format!(
        "HAYATE_PEER:{}:{}:{}",
        channel_id,
        std::env::consts::OS,
        port
    );
    let target: SocketAddr = "255.255.255.255:50002".parse().unwrap();
    let loopback: SocketAddr = "127.0.0.1:50002".parse().unwrap();

    loop {
        let compio::BufResult(res, _) = socket.send_to(msg.as_bytes().to_vec(), target).await;
        if let Err(_e) = res {
            // Ignore network send errors silently
        }
        let compio::BufResult(res, _) = socket.send_to(msg.as_bytes().to_vec(), loopback).await;
        if let Err(_e) = res {
            // Ignore
        }
        compio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// Listens for a UDP broadcast. If `target_phrase` is provided, it only yields a peer
/// whose derived `ChannelID` matches. Otherwise, it yields the first peer detected.
/// Returns the peer's resolved IP and port if found within the timeout.
pub async fn listen_for_broadcast(
    target_phrase: Option<String>,
    timeout: Duration,
) -> Result<Option<(String, SocketAddr, String)>, io::Error> {
    let target_channel_id = target_phrase.as_deref().map(derive_channel_id);

    let std_socket = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )?;
    std_socket.set_reuse_address(true)?;
    #[cfg(not(windows))]
    std_socket.set_reuse_port(true)?;

    let listen_addr: SocketAddr = "0.0.0.0:50002".parse().unwrap();
    std_socket.bind(&socket2::SockAddr::from(listen_addr))?;

    let socket = compio::net::UdpSocket::from_std(std_socket.into())?;
    let buf = vec![0u8; 1024];

    // Wrap in a compio timeout
    let res = compio::time::timeout(timeout, async move {
        let mut temp_buf = buf;
        loop {
            let compio::BufResult(recv_res, b) = socket.recv_from(temp_buf).await;
            temp_buf = b;
            match recv_res {
                Ok((n, src_addr)) => {
                    let data = &temp_buf[..n];
                    if let Ok(text) = std::str::from_utf8(data) {
                        let parts: Vec<&str> = text.split(':').collect();
                        // Format: HAYATE_PEER:<ChannelID>:<OS>:<Port>
                        if parts.len() == 4 && parts[0] == "HAYATE_PEER" {
                            let matches = match &target_channel_id {
                                Some(expected_id) => parts[1] == expected_id,
                                None => true,
                            };
                            if matches && let Ok(port) = parts[3].parse::<u16>() {
                                let peer_addr = SocketAddr::new(src_addr.ip(), port);
                                let os = parts[2].to_owned();
                                return Ok(Some(("Hayate Peer".to_owned(), peer_addr, os)));
                            }
                        }
                    }
                }
                Err(e) => return Err(e),
            }
        }
    })
    .await;

    match res {
        Ok(inner_res) => inner_res,
        Err(_) => Ok(None), // timeout expired
    }
}

//! `hayate send` subcommand.

use std::net::{SocketAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use anyhow::{Context, Result, bail};
use hayate::protocol::TransferKind;
use hayate::{HayateSender, TransferStage};

use crate::cli::SendArgs;
use crate::ui::{PathCompleter, TransferUi};
use crate::{history, output, peers, policy};

pub async fn run(args: SendArgs, cancelled: Arc<AtomicBool>) -> Result<()> {
    let path = match &args.path {
        Some(p) => p.clone(),
        None => prompt_path()?,
    };
    if !path.exists() {
        bail!("Path does not exist: {}", path.display());
    }

    // Resolve the effective target: positional addr > --to NAME > --pick.
    let target_str: Option<String> = if let Some(name) = &args.to {
        let addr = peers::resolve(name)?
            .with_context(|| format!("unknown peer \"{name}\" (see `hayate peers list`)"))?;
        output::info(&format!("Sending to saved peer \"{name}\" ({addr})"));
        Some(addr)
    } else if args.pick {
        Some(pick_peer(&cancelled).await?)
    } else {
        args.target.clone()
    };

    if target_str.is_none() && args.passphrase.is_some() {
        bail!("--passphrase requires a target; use --code for pairing");
    }

    let compress = args.compress && !args.no_compress;
    let hash_algo = args.hash.as_str().to_owned();

    let rate_limit = match &args.bandwidth_limit {
        Some(raw) => Some(parse_rate(raw)?),
        None => None,
    };

    let mut builder = HayateSender::new().compress(compress).hash_algo(hash_algo.clone());
    if let Some(rate) = rate_limit {
        builder = builder.bandwidth_limit(rate);
    }

    // Pairing code: explicit `--code`, or auto-generated when no target is
    // given.
    let (phrase, print_instruction) = if let Some(code) = &args.code {
        (Some(crate::subcmd::normalize_code(code)?), false)
    } else if target_str.is_none() {
        let p = crate::words::generate_phrase();
        (Some(p), policy::get().normal())
    } else {
        (None, false)
    };

    if let Some(target_str) = &target_str {
        let target_addr = resolve_target(target_str)?;
        builder = builder.target(target_addr);
        // Optional out-of-band secret on a direct transfer. --passphrase
        // wins over a legacy --code used with a target.
        if let Some(secret) = args.passphrase.clone().or_else(|| phrase.clone()) {
            builder = builder.passphrase(secret);
        }
    } else {
        let phrase = phrase.expect("pairing mode always has a phrase");
        if print_instruction {
            output::pairing_code(&phrase, &format!("hayate receive --code \"{phrase}\""));
        }
        builder = builder.code(phrase);
    }

    let ui = TransferUi::new(cancelled, args.no_progress);

    let result = builder
        .send_with(
            &path,
            {
                let ui = ui.clone();
                move |stage| {
                    ui.check_cancelled()?;
                    match stage {
                        TransferStage::Connecting { peer } => {
                            output::stage("connect", format!("dialing {peer}"));
                            ui.spinner("Connecting", &peer.to_string());
                        },
                        TransferStage::Pairing { code } => {
                            if !print_instruction {
                                output::stage("pairing", format!("waiting with code \"{code}\""));
                            }
                            ui.spinner("Pairing", "waiting for receiver…");
                        },
                        TransferStage::Connected { peer } => {
                            ui.clear_spinner();
                            output::ok(&format!("Connected to {peer}"));
                        },
                        TransferStage::Handshaking => {
                            output::stage("handshake", "negotiating cipher…");
                        },
                        TransferStage::Ready { meta, cipher_id, peer, total_size } => {
                            let kind = if meta.transfer_type == TransferKind::Directory {
                                "directory"
                            } else {
                                "file"
                            };
                            let mut rows = vec![
                                ("file", meta.filename.clone()),
                                ("type", kind.to_owned()),
                                ("size", output::format_bytes(total_size)),
                                (
                                    "compress",
                                    if compress {
                                        "zstd level 1".to_owned()
                                    } else {
                                        "off".to_owned()
                                    },
                                ),
                                ("hash", hash_algo.clone()),
                                ("cipher", output::cipher_name(cipher_id).to_owned()),
                                ("peer", peer.to_string()),
                            ];
                            if let Some(rate) = rate_limit {
                                rows.push(("limit", format!("{}/s", output::format_bytes(rate))));
                            }
                            output::print_info_card("Sending", &rows);
                        },
                        TransferStage::Resuming { offset } => {
                            output::stage(
                                "resume",
                                format!("continuing from {}", output::format_bytes(offset)),
                            );
                            ui.set_resume_offset(offset);
                        },
                        TransferStage::Transferring { total_size, .. } => {
                            ui.start_transfer("send", total_size);
                        },
                        TransferStage::Finishing
                        | TransferStage::WaitingForPeer
                        | TransferStage::Discovering { .. }
                        | TransferStage::Offer { .. } => {},
                        // `TransferStage` is non_exhaustive for future engine
                        // stages.
                        _ => {},
                    }
                    Ok(())
                }
            },
            {
                let ui = ui.clone();
                move |bytes| {
                    ui.check_cancelled()?;
                    ui.set_position(bytes);
                    Ok(())
                }
            },
        )
        .await;

    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => {
            ui.clear_all();
            return Err(error).context("send failed");
        },
    };
    ui.finish_progress(outcome.total_size);

    let elapsed = ui.elapsed();
    output::print_transfer_summary(
        &outcome.meta.filename,
        outcome.total_size,
        elapsed,
        &outcome.checksum,
        compress,
        output::cipher_name(outcome.cipher_id),
    );

    // Remember direct-transfer IPv4 peers for `hayate send --to`. IPv6 literals
    // are not valid peer names, and a failed convenience write must never turn
    // a completed transfer into a warning.
    if target_str.is_some() && args.code.is_none() && outcome.peer.is_ipv4() {
        let _ = peers::record(&outcome.peer.ip().to_string(), &outcome.peer.to_string());
    }
    history::record_transfer(
        "send",
        &outcome.meta.filename,
        outcome.total_size,
        elapsed,
        &outcome.checksum,
        output::cipher_name(outcome.cipher_id),
        &outcome.peer.to_string(),
        None,
    );

    Ok(())
}

/// Interactive LAN scan + receiver selection for `--pick`.
async fn pick_peer(cancelled: &Arc<AtomicBool>) -> Result<String> {
    if !output::stdin_is_tty() {
        bail!("--pick requires an interactive terminal (no TTY detected)");
    }
    let mut found = crate::subcmd::discover::fast_discover(std::time::Duration::from_secs(2)).await;
    if found.is_empty() {
        found =
            crate::subcmd::discover::scan_for_peers(5, None, Arc::clone(cancelled), true, |_| {})
                .await?;
    }
    if found.is_empty() {
        bail!("no receivers found on the network (is `hayate receive` running?)");
    }
    let options: Vec<String> = found.iter().map(|p| format!("{}  ({})", p.addr, p.name)).collect();
    let choice =
        output::suspend_for_prompt(|| inquire::Select::new("Select a receiver", options).prompt());
    match choice {
        Ok(selected) => {
            let idx = found
                .iter()
                .position(|p| selected.starts_with(&p.addr.to_string()))
                .context("selection did not match a discovered peer")?;
            Ok(found[idx].addr.to_string())
        },
        Err(inquire::InquireError::OperationInterrupted) => bail!("cancelled"),
        Err(e) => Err(anyhow::anyhow!(e)),
    }
}

/// Parses a throughput cap like `10MiB`, `500KiB`, `2M`, or a plain number
/// (bytes per second).
fn parse_rate(raw: &str) -> Result<u64> {
    let s = raw.trim();
    let split = s.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(s.len());
    let (num_part, unit_part) = s.split_at(split);
    let value: f64 =
        num_part.trim().parse().with_context(|| format!("invalid bandwidth limit \"{raw}\""))?;
    let mult: f64 = match unit_part.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1.0,
        "k" | "kb" => 1_000.0,
        "m" | "mb" => 1_000_000.0,
        "g" | "gb" => 1_000_000_000.0,
        "kib" => 1024.0,
        "mib" => 1024.0 * 1024.0,
        "gib" => 1024.0 * 1024.0 * 1024.0,
        other => bail!("unknown unit \"{other}\" in bandwidth limit (try 10MiB, 500KiB, 2M)"),
    };
    let rate = value * mult;
    if !(rate.is_finite() && rate > 0.0) {
        bail!("bandwidth limit must be positive");
    }
    Ok(rate as u64)
}

/// Resolves a target address, preferring IPv4.
///
/// A receiver bound to `0.0.0.0` is unreachable over IPv6, and `localhost`
/// resolves to `::1` first on most systems, so taking the first resolver result
/// fails even when the IPv4 loopback would work.
fn resolve_target(raw: &str) -> Result<SocketAddr> {
    let mut candidates: Vec<SocketAddr> = raw
        .to_socket_addrs()
        .with_context(|| format!("invalid target address \"{raw}\""))?
        .collect();
    if candidates.is_empty() {
        bail!("could not resolve target \"{raw}\"");
    }
    candidates.sort_by_key(|addr| !addr.is_ipv4());
    Ok(candidates[0])
}

/// Interactive path prompt for `hayate send` with no positional path.
fn prompt_path() -> Result<PathBuf> {
    if !output::stdin_is_tty() {
        bail!("no path given and no TTY for the interactive picker");
    }
    let answer = output::suspend_for_prompt(|| {
        inquire::Text::new("File or directory to send")
            .with_autocomplete(PathCompleter::files_and_dirs())
            .prompt()
    });
    match answer {
        Ok(text) => {
            let p = PathBuf::from(text.trim());
            if p.as_os_str().is_empty() {
                bail!("no path given");
            }
            Ok(p)
        },
        Err(inquire::InquireError::OperationInterrupted) => bail!("cancelled"),
        Err(e) => Err(anyhow::anyhow!(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_rate, resolve_target};

    #[test]
    fn resolve_target_prefers_ipv4_literal() {
        let addr = resolve_target("127.0.0.1:50001").unwrap();
        assert!(addr.is_ipv4());
        assert_eq!(addr.port(), 50001);
    }

    #[test]
    fn resolve_target_keeps_ipv6_literal() {
        let addr = resolve_target("[::1]:50001").unwrap();
        assert!(addr.is_ipv6());
    }

    #[test]
    fn resolve_target_rejects_garbage() {
        assert!(resolve_target("not an address").is_err());
    }

    #[test]
    fn parse_rate_plain_bytes() {
        assert_eq!(parse_rate("1024").unwrap(), 1024);
    }

    #[test]
    fn parse_rate_binary_units() {
        assert_eq!(parse_rate("10MiB").unwrap(), 10 * 1024 * 1024);
        assert_eq!(parse_rate("500KiB").unwrap(), 500 * 1024);
        assert_eq!(parse_rate("1GiB").unwrap(), 1024 * 1024 * 1024);
    }

    #[test]
    fn parse_rate_decimal_units() {
        assert_eq!(parse_rate("2M").unwrap(), 2_000_000);
        assert_eq!(parse_rate("3kb").unwrap(), 3_000);
    }

    #[test]
    fn parse_rate_fractional() {
        assert_eq!(parse_rate("1.5MiB").unwrap(), 1024 * 1024 * 3 / 2);
    }

    #[test]
    fn parse_rate_rejects_garbage() {
        assert!(parse_rate("").is_err());
        assert!(parse_rate("fast").is_err());
        assert!(parse_rate("10TiB").is_err());
        assert!(parse_rate("0").is_err());
        assert!(parse_rate("-5M").is_err());
    }
}

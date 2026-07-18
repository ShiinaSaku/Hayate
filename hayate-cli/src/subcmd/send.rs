//! `hayate send` subcommand.

use std::net::ToSocketAddrs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use hayate::protocol::TransferKind;
use hayate::{EngineError, HayateSender, TransferStage};
use indicatif::ProgressBar;

use crate::cli::SendArgs;
use crate::{output, policy};

pub async fn run(args: SendArgs, cancelled: Arc<AtomicBool>) -> Result<()> {
    if cancelled.load(Ordering::SeqCst) {
        bail!("cancelled");
    }
    let path = &args.path;
    if !path.exists() {
        bail!("Path does not exist: {}", path.display());
    }

    let compress = args.compress && !args.no_compress;
    let hash_algo = args.hash.as_str().to_owned();

    let mut builder = HayateSender::new().compress(compress).hash_algo(hash_algo.clone());

    // Pairing code: explicit `--code`, or auto-generated when no target is given.
    let (phrase, print_instruction) = if let Some(code) = &args.code {
        (Some(code.clone()), false)
    } else if args.target.is_none() {
        let p = crate::words::generate_phrase();
        (Some(p), policy::get().normal())
    } else {
        (None, false)
    };

    if let Some(target_str) = &args.target {
        let target_addr = target_str
            .to_socket_addrs()
            .context("invalid target address")?
            .next()
            .context("could not resolve target")?;
        builder = builder.target(target_addr);
        // Optional out-of-band secret on a direct transfer.
        if let Some(code) = phrase {
            builder = builder.passphrase(code);
        }
    } else {
        let phrase = phrase.expect("pairing mode always has a phrase");
        if print_instruction {
            output::pairing_code(&phrase, &format!("hayate receive --code \"{phrase}\""));
        }
        builder = builder.code(phrase);
    }

    let spinner: Arc<Mutex<Option<ProgressBar>>> = Arc::new(Mutex::new(None));
    let progress: Arc<Mutex<Option<ProgressBar>>> = Arc::new(Mutex::new(None));
    let no_progress = args.no_progress || policy::get().no_progress();
    let transfer_start = Arc::new(Mutex::new(None));

    let spinner_for_stages = Arc::clone(&spinner);
    let progress_for_stages = Arc::clone(&progress);
    let cancelled_transfer = Arc::clone(&cancelled);
    let transfer_start_stage = Arc::clone(&transfer_start);

    let result = builder
        .send_with(
            path,
            move |stage| {
                if cancelled_transfer.load(Ordering::SeqCst) {
                    return Err(EngineError::Cancelled("transfer cancelled by user".into()));
                }
                match stage {
                    TransferStage::Connecting { peer } => {
                        output::stage("connect", format!("dialing {peer}"));
                        if !no_progress {
                            *spinner_for_stages.lock().unwrap() =
                                Some(output::spinner("Connecting", &peer.to_string()));
                        }
                    },
                    TransferStage::Pairing { code } => {
                        if !print_instruction {
                            output::stage("pairing", format!("waiting with code \"{code}\""));
                        }
                        if !no_progress {
                            *spinner_for_stages.lock().unwrap() =
                                Some(output::spinner("Pairing", "waiting for receiver…"));
                        }
                    },
                    TransferStage::Connected { peer } => {
                        clear_spinner(&spinner_for_stages);
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
                        output::print_info_card(
                            "Sending",
                            &[
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
                            ],
                        );
                        if !no_progress && total_size > 0 {
                            *progress_for_stages.lock().unwrap() =
                                Some(output::transfer_progress_bar("send", total_size));
                        }
                    },
                    TransferStage::Transferring { .. } => {
                        *transfer_start_stage.lock().unwrap_or_else(|e| e.into_inner()) =
                            Some(Instant::now());
                    },
                    TransferStage::Finishing
                    | TransferStage::WaitingForPeer
                    | TransferStage::Discovering { .. }
                    | TransferStage::Offer { .. } => {},
                    // `TransferStage` is non_exhaustive for future engine stages.
                    _ => {},
                }
                Ok(())
            },
            {
                let cancelled = Arc::clone(&cancelled);
                let progress = Arc::clone(&progress);
                move |bytes| {
                    if cancelled.load(Ordering::SeqCst) {
                        return Err(EngineError::Cancelled("transfer cancelled by user".into()));
                    }
                    if let Some(pb) = progress.lock().unwrap().as_ref() {
                        output::set_transfer_position(pb, bytes);
                    }
                    Ok(())
                }
            },
        )
        .await;

    clear_spinner(&spinner);
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => {
            clear_progress(&progress);
            return Err(error).context("send failed");
        },
    };
    if let Some(pb) = progress.lock().unwrap().take() {
        output::finish_transfer_progress(&pb, outcome.total_size);
    }

    let elapsed = transfer_start
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .map_or(0.0, |start| start.elapsed().as_secs_f64());
    output::print_transfer_summary(
        &outcome.meta.filename,
        outcome.total_size,
        elapsed,
        &outcome.checksum,
        compress,
        output::cipher_name(outcome.cipher_id),
    );

    Ok(())
}

fn clear_spinner(spinner: &Arc<Mutex<Option<ProgressBar>>>) {
    if let Some(s) = spinner.lock().unwrap_or_else(|e| e.into_inner()).take() {
        s.finish_and_clear();
    }
}

fn clear_progress(progress: &Arc<Mutex<Option<ProgressBar>>>) {
    if let Some(pb) = progress.lock().unwrap_or_else(|e| e.into_inner()).take() {
        pb.finish_and_clear();
    }
}

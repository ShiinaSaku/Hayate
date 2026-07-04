//! `hayate man` subcommand — generates man pages for the CLI.

use std::fs;

use anyhow::{Context, Result};
use clap::CommandFactory;

use crate::cli::{Cli, ManArgs};

/// Generates a man page for each visible Hayate subcommand and writes them to
/// `out_dir`.
///
/// The top-level `hayate.1` page is always generated. Subcommands that have
/// their own help surface (e.g. `hayate receive`, `hayate send`) get their own
/// man section 1 page as well, named `hayate-<subcommand>.1`.
pub fn run(args: ManArgs) -> Result<()> {
    fs::create_dir_all(&args.out_dir)
        .with_context(|| format!("creating man page directory {}", args.out_dir.display()))?;

    let cmd = Cli::command();
    let man = clap_mangen::Man::new(cmd.clone());
    let mut buffer = Vec::new();
    man.render(&mut buffer)
        .context("rendering hayate man page")?;
    fs::write(args.out_dir.join("hayate.1"), buffer).with_context(|| "writing hayate.1")?;

    for subcmd in cmd.get_subcommands().filter(|s| !s.is_hide_set()) {
        let name = subcmd.get_name();
        let man = clap_mangen::Man::new(subcmd.clone());
        let mut buffer = Vec::new();
        man.render(&mut buffer)
            .with_context(|| format!("rendering man page for {name}"))?;
        fs::write(args.out_dir.join(format!("hayate-{name}.1")), buffer)
            .with_context(|| format!("writing hayate-{name}.1"))?;
    }

    println!("Man pages written to {}", args.out_dir.display());
    Ok(())
}

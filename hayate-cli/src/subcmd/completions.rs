//! `hayate completions` subcommand — prints or installs a shell completion script.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::CommandFactory;

use crate::cli::{Cli, CompletionsArgs};

/// Generates a completion script for the requested shell.
///
/// With `--install`, the script is written to the shell's default configuration
/// directory (e.g. `~/.zshrc` or `~/.bashrc`). Without `--install`, it is
/// printed to stdout.
pub fn run(args: CompletionsArgs) -> Result<()> {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_owned();

    if args.install {
        install(&args.shell, &mut cmd, &name)
    } else {
        clap_complete::generate(args.shell, &mut cmd, name, &mut std::io::stdout());
        Ok(())
    }
}

fn install(shell: &clap_complete::Shell, cmd: &mut clap::Command, name: &str) -> Result<()> {
    let path = completion_path(shell)?;
    let mut buffer = Vec::new();
    clap_complete::generate(*shell, cmd, name, &mut buffer);

    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("completion path has no parent"))?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create directory {}", parent.display()))?;
    std::fs::write(&path, &buffer)
        .with_context(|| format!("failed to write completion script to {}", path.display()))?;

    println!(
        "   Completion script installed to {}\n   Restart your shell or source it to activate.",
        path.display()
    );
    Ok(())
}

fn completion_path(shell: &clap_complete::Shell) -> Result<PathBuf> {
    let home = dirs::home_dir().context("could not determine home directory")?;
    match shell {
        clap_complete::Shell::Bash => Ok(home.join(".bash_completion.d").join("hayate")),
        clap_complete::Shell::Zsh => Ok(home.join(".zsh").join("completions").join("_hayate")),
        clap_complete::Shell::Fish => Ok(home
            .join(".config")
            .join("fish")
            .join("completions")
            .join("hayate.fish")),
        clap_complete::Shell::PowerShell => {
            #[cfg(target_os = "windows")]
            {
                Ok(home
                    .join("Documents")
                    .join("PowerShell")
                    .join("Modules")
                    .join("hayate")
                    .join("hayate.ps1"))
            }
            #[cfg(not(target_os = "windows"))]
            {
                bail!("PowerShell completion install is only supported on Windows");
            }
        }
        clap_complete::Shell::Elvish => bail!("Elvish completion install is not supported"),
        _ => bail!("completion install is not supported for {}", shell),
    }
}

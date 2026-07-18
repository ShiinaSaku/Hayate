//! `hayate completions` subcommand — prints or installs a shell completion
//! script.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::CommandFactory;

use crate::cli::{Cli, CompletionsArgs};

/// Generates a completion script for the requested shell.
///
/// With `--install`, the script is written to the shell's default configuration
/// directory. Without `--install`, it is printed to stdout.
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

    let parent = path.parent().ok_or_else(|| anyhow::anyhow!("completion path has no parent"))?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create directory {}", parent.display()))?;
    std::fs::write(&path, &buffer)
        .with_context(|| format!("failed to write completion script to {}", path.display()))?;

    println!("Installed completions to {}", path.display());
    println!();
    match shell {
        clap_complete::Shell::Bash => {
            println!("Add this to ~/.bashrc (if not already present), then restart the shell:");
            println!();
            println!("  [ -f {} ] && . {}", path.display(), path.display());
            println!();
            println!("Or for this session only:");
            println!();
            println!("  eval \"$(hayate completions bash)\"");
        },
        clap_complete::Shell::Zsh => {
            println!("Add this to ~/.zshrc (if not already present), then run `exec zsh`:");
            println!();
            println!("  fpath=({} $fpath)", parent.display());
            println!("  autoload -Uz compinit && compinit");
            println!();
            println!("Or for this session only:");
            println!();
            println!("  eval \"$(hayate completions zsh)\"");
        },
        clap_complete::Shell::Fish => {
            println!("Fish loads ~/.config/fish/completions/ automatically.");
            println!("Open a new fish session, or run:");
            println!();
            println!("  source {}", path.display());
        },
        clap_complete::Shell::PowerShell => {
            println!("Dot-source the script from your PowerShell profile:");
            println!();
            println!("  . \"{}\"", path.display());
            println!();
            println!("Find your profile path with:  echo $PROFILE");
        },
        _ => {
            println!("Restart your shell to activate completions.");
        },
    }
    println!();
    println!("See also:  hayate docs completions");
    Ok(())
}

fn completion_path(shell: &clap_complete::Shell) -> Result<PathBuf> {
    let home = dirs::home_dir().context("could not determine home directory")?;
    match shell {
        clap_complete::Shell::Bash => Ok(home.join(".bash_completion.d").join("hayate")),
        clap_complete::Shell::Zsh => Ok(home.join(".zsh").join("completions").join("_hayate")),
        clap_complete::Shell::Fish => {
            Ok(home.join(".config").join("fish").join("completions").join("hayate.fish"))
        },
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
        },
        clap_complete::Shell::Elvish => bail!("Elvish completion install is not supported"),
        _ => bail!("completion install is not supported for {shell}"),
    }
}

mod cli;
mod output;
mod subcmd;
mod words;

use anyhow::Result;
use clap::Parser;
use cli::Cli;

fn main() -> Result<()> {
    if std::env::var_os("NO_COLOR").is_none() {
        console::set_colors_enabled(true);
    }

    let cli = Cli::parse();

    // compio thread-per-core runtime: single OS thread, one io_uring /
    // IOCP / kqueue completion queue, no work-stealing scheduler.
    compio::runtime::Runtime::new()?.block_on(subcmd::dispatch(cli))
}

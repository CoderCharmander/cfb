use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
pub struct Cli {
    #[clap(subcommand)]
    pub subcommand: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    Run {
        #[clap(value_parser)]
        source_file: PathBuf,
        #[clap(value_parser, long)]
        stdin: Option<PathBuf>,
    },
    BuildAll,
}

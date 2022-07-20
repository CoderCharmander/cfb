use std::{
    fs::{self, File},
    io::BufReader,
    path::Path,
};

use anyhow::{Context, Result};
use clap::Parser;
use cli::{Cli, Command};
use make::CodeRunner;
mod cli;
mod make;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let command = cli.subcommand.unwrap_or(Command::BuildAll);
    let configs = make::load_config()?;

    fs::create_dir_all(Path::new("cfb-out"))?;
    match command {
        Command::Run { source_file, stdin } => {
            let source_file = source_file.canonicalize()?;
            let output_file = Path::new("cfb-out").join(
                source_file
                    .file_stem()
                    .context("Invalid source file name")?,
            );
            let stdin = if let Some(stdin) = stdin {
                Some(BufReader::new(File::open(stdin)?))
            } else {
                None
            };
            let output = configs.run(&source_file, &output_file, stdin)?;
            print!("{}", output);
        }
        Command::BuildAll => {
            unimplemented!();
        }
    }

    Ok(())
}

use std::{
    borrow::Cow,
    collections::HashMap,
    fs::File,
    io::{self, Write},
    path::Path,
    process::{Command, Stdio},
};

use anyhow::{bail, Context, Error, Result};
use colored::Colorize;
use dynfmt::{Format, SimpleCurlyFormat};
use serde::Deserialize;
use shell_quote::sh;

#[derive(Deserialize)]
pub struct LanguageConfig {
    compile_commands: Vec<String>,
    run_command: String,
}

pub type LanguageConfigs = HashMap<String, LanguageConfig>;

#[derive(Deserialize)]
pub struct Config {
    langs: LanguageConfigs,
    default_stdin: Option<String>,
}

mod sym {
    pub const COMMAND: &str = " $";
    pub const TASK: &str = " %";
    pub const FILE: &str = "::";
}

fn format_command(command: &str, source: &Path, output: &Path) -> Result<String> {
    let mut format_args = HashMap::new();
    let source_quoted = sh::quote(source);
    let output_quoted = sh::quote(output);
    format_args.insert("source", source_quoted.to_string_lossy());
    format_args.insert("output", output_quoted.to_string_lossy());
    format_args.insert("source_unquoted", source.to_string_lossy());
    format_args.insert("output_unquoted", output.to_string_lossy());
    SimpleCurlyFormat
        .format(command, &format_args)
        .map_err(|e| {
            eprintln!("{}", e);
            Error::msg("Invalid format string")
        })
        .map(Cow::into_owned)
}

pub fn load_config() -> Result<Config> {
    let mut configs = HashMap::new();
    let mut default_stdin = None;
    let dir = std::env::current_dir()?.canonicalize()?;

    for p in dir.ancestors() {
        let config_path = p.join("cfb.toml");
        if config_path.exists() {
            let current_config = toml::from_str::<Config>(
                &std::fs::read_to_string(&config_path)?,
            )
            .with_context(|| {
                format!(
                    "Failed to parse config file: {}",
                    config_path.to_string_lossy()
                )
            })?;
            for (language, config) in current_config.langs {
                configs.entry(language).or_insert(config);
            }
            if let Some(stdin) = current_config.default_stdin {
                default_stdin.get_or_insert(stdin);
            }
        }
    }

    Ok(Config {
        langs: configs,
        default_stdin,
    })
}

fn run_command(command: &str, stdin: Option<impl io::Read>) -> Result<String> {
    println!(
        "{} {}",
        sym::COMMAND.bright_white().bold(),
        command.bright_black()
    );
    let output = if let Some(mut stdin) = stdin {
        let mut child = Command::new("/bin/sh")
            .arg("-c")
            .arg(&command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut child_stdin = child.stdin.take().context("Failed to open command stdin")?;
        io::copy(&mut stdin, &mut child_stdin)?;
        drop(child_stdin);
        child.wait_with_output()?
    } else {
        std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(&command)
            .output()
            .context("Command execution failed")?
    };
    if !output.status.success() {
        io::stdout().write_all(&output.stdout)?;
        io::stderr().write_all(&output.stderr)?;
        bail!(
            "Command failed: `{}` (exit code {:?})",
            command,
            output.status.code()
        );
    }
    Ok(
        (String::from_utf8_lossy(&output.stdout) + String::from_utf8_lossy(&output.stderr))
            .to_string(),
    )
}

fn is_output_up_to_date(source: &Path, output: &Path) -> bool {
    if !output.exists() {
        return false;
    }
    source
        .metadata()
        .and_then(|source_meta| Ok(source_meta.modified()? <= output.metadata()?.modified()?))
        .unwrap_or(false)
}

impl LanguageConfig {
    pub fn format_compile_commands(&self, source: &Path, output: &Path) -> Result<Vec<String>> {
        self.compile_commands
            .iter()
            .map(|cmd| format_command(cmd, source, output))
            .collect()
    }

    pub fn format_run_command(&self, source: &Path, output: &Path) -> Result<String> {
        format_command(&self.run_command, source, output)
    }

    pub fn build(&self, source: &Path, output: &Path) -> Result<()> {
        if is_output_up_to_date(source, output) {
            eprintln!("   {}", "skip".yellow().bold());
            return Ok(());
        }
        let commands = self.format_compile_commands(source, output)?;
        for command in commands {
            run_command(&command, Option::<File>::None)?;
        }
        Ok(())
    }

    pub fn run(
        &self,
        source: &Path,
        output: &Path,
        stdin: Option<impl io::Read>,
    ) -> Result<String> {
        let command = self.format_run_command(source, output)?;
        run_command(&command, stdin)
    }
}

pub trait CodeRunner {
    fn build(&self, source: &Path, output: &Path) -> Result<()>;
    fn run(&self, source: &Path, output: &Path, stdin: Option<impl io::Read>) -> Result<String>;
    fn matches(&self, source: &Path) -> bool;
}

impl CodeRunner for Config {
    fn build(&self, source: &Path, output: &Path) -> Result<()> {
        let ext = source.extension().context("No extension on source file")?;
        let lang_config = self
            .langs
            .get(ext.to_str().context("Invalid extension")?)
            .with_context(|| {
                format!("No language config for extension {}", ext.to_string_lossy())
            })?;
        eprintln!(
            "{} {} {}",
            sym::TASK.bright_white().bold(),
            "build".bright_green().bold(),
            source.to_string_lossy().bright_blue().bold(),
        );
        lang_config.build(source, output)?;
        Ok(())
    }
    fn run(&self, source: &Path, output: &Path, stdin: Option<impl io::Read>) -> Result<String> {
        let ext = source.extension().context("No extension on source file")?;
        let lang_config = self
            .langs
            .get(ext.to_str().context("Invalid extension")?)
            .with_context(|| {
                format!("No language config for extension {}", ext.to_string_lossy())
            })?;
        self.build(source, output)?;
        eprintln!(
            "{} {} {}",
            sym::TASK.bright_white().bold(),
            "run".bright_green().bold(),
            source.to_string_lossy().bright_blue().bold(),
        );
        lang_config.run(source, output, stdin)
    }

    fn matches(&self, source: &Path) -> bool {
        source.extension().map_or(false, |ext| {
            self.langs.contains_key(&ext.to_string_lossy().to_string())
        })
    }
}

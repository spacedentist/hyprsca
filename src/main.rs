use std::path::PathBuf;

use clap::{Parser, Subcommand};
use hyprrust::HyprlandConnection;
use hyprrust::commands::Command;
use hyprrust::data::{HyprlandData, HyprlandDataWithArgument, Monitor, Version};
use hyprrust_macros::{HyprlandData, HyprlandDataWithArgument};
use log::{Level, debug, error, log_enabled};
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug)]
#[clap(
    name = "hyprsca",
    version,
    about = "Save and restore monitor configurations in Hyprland"
)]
pub struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Save configuration screen configuration
    Save,

    /// Restore previously save screen configuration
    Restore(RestoreOptions),

    /// Display information on connected monitors
    Info,
}

#[derive(Parser, Debug)]
struct RestoreOptions {
    /// If no saved configuration is found, apply a default configuration as default
    #[clap(long)]
    fallback_to_default: bool,
}

#[derive(Deserialize, Debug, Default)]
struct ConfigFile {
    lid: Vec<LidConfig>,
}

#[derive(Deserialize, Debug, Clone)]
struct LidConfig {
    file: PathBuf,
    head: String,
}

#[derive(Serialize, Deserialize, Debug, HyprlandData, HyprlandDataWithArgument)]
pub struct Monitors(Vec<Monitor>);
impl std::ops::Deref for Monitors {
    type Target = Vec<Monitor>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Head {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    name: Option<String>,
    make: String,
    model: String,
    serial: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    config: Option<HeadConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct HeadConfig {
    width: i32,
    height: i32,
    refresh_rate: f64,
    x: i32,
    y: i32,
    scale: f64,
    transform: i32,
    vrr: bool,
}

impl From<&Monitor> for Head {
    fn from(mon: &Monitor) -> Self {
        Self {
            name: Some(mon.name.clone()),
            make: mon.make.clone(),
            model: mon.model.clone(),
            serial: mon.serial.clone(),
            config: if mon.disabled {
                None
            } else {
                Some(HeadConfig {
                    width: mon.width,
                    height: mon.height,
                    refresh_rate: mon.refresh_rate,
                    x: mon.x,
                    y: mon.y,
                    scale: mon.scale,
                    transform: mon.transform,
                    vrr: mon.vrr,
                })
            },
        }
    }
}

impl Head {
    pub fn cmp_mms(&self, other: &Self) -> std::cmp::Ordering {
        self.make
            .cmp(&other.make)
            .then_with(|| self.model.cmp(&other.model))
            .then_with(|| self.serial.cmp(&other.serial))
    }
}

impl Command for Head {
    fn get_command(&self) -> String {
        if let Some(ref cfg) = self.config {
            format!(
                "keyword monitor {},{}x{}@{},{}x{},{},transform,{},vrr,{}",
                self.name.as_deref().unwrap_or(""),
                cfg.width,
                cfg.height,
                cfg.refresh_rate,
                cfg.x,
                cfg.y,
                cfg.scale,
                cfg.transform,
                if cfg.vrr { 1 } else { 0 }
            )
        } else {
            format!(
                "keyword monitor {},disable",
                self.name.as_deref().unwrap_or(""),
            )
        }
    }
}

#[derive(Debug, Clone)]
struct HyprlandCommand(pub String);

impl Command for HyprlandCommand {
    fn get_command(&self) -> String {
        self.0.clone()
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let cli = Cli::parse();
    let config = read_config_file()?;
    debug!("Config: {:?}", &config);

    let conn = HyprlandConnection::new();
    if log_enabled!(Level::Debug) {
        let version = conn.get_sync::<Version>()?.version;
        debug!("Hyprland version: {version}");
    }

    // Find out which heads should be ignored because of a closed lid
    let ignored_head_names: std::collections::HashSet<String> = config
        .lid
        .iter()
        .filter_map(|LidConfig { file, head }| {
            let closed = std::fs::read(file)
                .ok()
                .map(|contents| contents.trim_ascii().ends_with(b"closed"))
                .unwrap_or(false);
            if closed { Some(head.to_string()) } else { None }
        })
        .collect();

    // Get all monitors from Hyprland, convert to Head structure and sort
    let monitors = conn.get_with_argument_sync::<Monitors>("all".to_string())?;
    let mut ignored_heads = Vec::new();
    let mut heads: Vec<Head> = monitors
        .iter()
        .map(Head::from)
        .filter_map(|h| {
            if h.name
                .as_ref()
                .map(|name| ignored_head_names.contains(name))
                .unwrap_or(false)
            {
                ignored_heads.push(h);
                None
            } else {
                Some(h)
            }
        })
        .collect();
    heads.sort_by(Head::cmp_mms);

    match cli.command {
        Commands::Save => {
            let base_directories = xdg::BaseDirectories::with_prefix("hyprsca")?;
            let path = base_directories
                .place_state_file(format!("{}.json", hex::encode(hash_heads(&heads))))?;
            debug!("Saving screen config to {}", path.display());
            heads.iter_mut().for_each(|h| {
                h.name = None;
            });
            std::fs::write(path, serde_json::to_string_pretty(&heads)?)?;
        }
        Commands::Restore(ref opt) => {
            if let Err(err) = restore_config(&heads, &ignored_heads, &conn) {
                error!("{}", err);

                if opt.fallback_to_default {
                    let commands: Vec<Box<dyn Command>> = heads
                        .iter()
                        .filter_map(|h| {
                            h.name.as_ref().map(|name| -> Box<dyn Command> {
                                Box::new(HyprlandCommand(format!(
                                    "keyword monitor {},preferred,auto,auto",
                                    name
                                )))
                            })
                        })
                        .collect();

                    conn.send_recipe_sync(&commands)
                        .map_err(|mut verr| verr.pop().unwrap())?;
                }
            }
        }
        Commands::Info => {
            let base_directories = xdg::BaseDirectories::with_prefix("hyprsca")?;
            println!("{} connected heads:", heads.len() + ignored_heads.len());
            for head in heads.iter() {
                println!(
                    "* {}\n  Make: {}\n  Model: {}\n  Serial: {}",
                    head.name.as_deref().unwrap_or(""),
                    &head.make,
                    &head.model,
                    &head.serial
                );
            }
            for head in ignored_heads.iter() {
                println!(
                    "* {} [ignored]\n  Make: {}\n  Model: {}\n  Serial: {}",
                    head.name.as_deref().unwrap_or(""),
                    &head.make,
                    &head.model,
                    &head.serial
                );
            }
            let path = base_directories
                .get_state_file(format!("{}.json", hex::encode(hash_heads(&heads))));

            println!("Configuration path: {}", path.display());
        }
    }

    Ok(())
}

fn hash_heads(heads: &[Head]) -> [u8; 32] {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(heads.len().to_le_bytes());

    for head in heads {
        hasher.update(head.make.len().to_le_bytes());
        hasher.update(head.make.as_bytes());
        hasher.update(head.model.len().to_le_bytes());
        hasher.update(head.model.as_bytes());
        hasher.update(head.serial.len().to_le_bytes());
        hasher.update(head.serial.as_bytes());
    }

    hasher.finalize().into()
}

fn restore_config(
    heads: &[Head],
    ignored_heads: &[Head],
    conn: &HyprlandConnection,
) -> anyhow::Result<()> {
    let base_directories = xdg::BaseDirectories::with_prefix("hyprsca")?;
    let path = base_directories.get_state_file(format!("{}.json", hex::encode(hash_heads(heads))));
    debug!("Attempting to load screen config from {}", path.display());
    let mut saved_heads = serde_json::from_slice::<Vec<Head>>(&std::fs::read(&path)?)?;
    if saved_heads.len() != heads.len() {
        return Err(anyhow::anyhow!(
            "Screen config {} does not match connected heads ({}!={})",
            path.display(),
            saved_heads.len(),
            heads.len()
        ));
    }
    saved_heads.sort_by(Head::cmp_mms);

    for (idx, (saved_head, head)) in saved_heads.iter_mut().zip(heads.iter()).enumerate() {
        if (&saved_head.make, &saved_head.model, &saved_head.serial)
            != (&head.make, &head.model, &head.serial)
        {
            return Err(anyhow::anyhow!(
                "Screen config {} does not match connected heads (idx {})",
                path.display(),
                idx,
            ));
        }
        saved_head.name = head.name.clone();
    }

    debug!("Restoring config: {:?}", saved_heads);
    let commands: Vec<Box<dyn Command>> = saved_heads
        .into_iter()
        .chain(ignored_heads.iter().map(|h| {
            let mut h = h.clone();
            h.config = None;
            h
        }))
        .map(|m| -> Box<dyn Command> { Box::new(m) })
        .collect();

    if log_enabled!(Level::Debug) {
        for cmd in commands.iter() {
            debug!("hyprctl {}", cmd.get_command());
        }
    }

    conn.send_recipe_sync(&commands)
        .map_err(|mut verr| verr.pop().unwrap())?;

    Ok(())
}

fn read_config_file() -> anyhow::Result<ConfigFile> {
    let base_directories = xdg::BaseDirectories::new()?;
    let path = base_directories.get_config_file("hyprsca.toml");

    let contents = std::fs::read(path);

    if let Err(ref err) = contents {
        if err.kind() == std::io::ErrorKind::NotFound {
            return Ok(Default::default());
        }
    }

    Ok(toml::from_str(std::str::from_utf8(&contents?)?)?)
}

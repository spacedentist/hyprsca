use std::path::PathBuf;

use clap::{Parser, Subcommand};
use log::{debug, error};
use serde::Deserialize;

use wlscsr::{
    backend::{Backend, HyprctlBackend, WlrRandrBackend},
    types::Head,
};

#[derive(Parser, Debug)]
#[clap(
    name = "wlscsr",
    version,
    about = "Save and restore monitor configurations in Hyprland"
)]
pub struct Cli {
    #[clap(long)]
    #[arg(value_enum)]
    backend: Option<BackendType>,

    #[clap(long)]
    executable: Option<String>,

    #[clap(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum BackendType {
    WlrRandr,
    Hyprctl,
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

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let cli = Cli::parse();
    let backend: Box<dyn Backend> = match cli.backend.unwrap_or(BackendType::WlrRandr) {
        BackendType::WlrRandr => Box::new(WlrRandrBackend::new(
            cli.executable
                .as_deref()
                .or(option_env!("STD_EXECUTABLE_WLR_RANDR"))
                .unwrap_or("wlr-randr")
                .to_string(),
        )),
        BackendType::Hyprctl => Box::new(HyprctlBackend::new(
            cli.executable
                .as_deref()
                .or(option_env!("STD_EXECUTABLE_HYPRCTL"))
                .unwrap_or("hyprctl")
                .to_string(),
        )),
    };

    let config = read_config_file()?;
    debug!("Config: {:?}", &config);

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

    // Get all heads from backend and sort
    let mut ignored_heads = Vec::new();
    let mut heads: Vec<Head> = backend
        .get_all_heads()?
        .into_iter()
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
            let base_directories = xdg::BaseDirectories::with_prefix("wlscsr")?;
            let path = base_directories
                .place_state_file(format!("{}.json", hex::encode(hash_heads(&heads))))?;
            debug!("Saving screen config to {}", path.display());
            heads.iter_mut().for_each(|h| {
                h.name = None;
            });
            std::fs::write(path, serde_json::to_string_pretty(&heads)?)?;
        }
        Commands::Restore(ref opt) => match load_head_config(&heads, &ignored_heads) {
            Ok(saved_heads) => backend.set_head_config(&saved_heads)?,
            Err(err) => {
                if opt.fallback_to_default {
                    error!("{}", err);

                    let active_head_names: Vec<String> =
                        heads.iter().filter_map(|h| h.name.clone()).collect();
                    let inactive_head_names: Vec<String> = ignored_heads
                        .iter()
                        .filter_map(|h| h.name.clone())
                        .collect();
                    backend.fallback_head_config(&active_head_names, &inactive_head_names)?
                } else {
                    Err(err)?;
                }
            }
        },
        Commands::Info => {
            let base_directories = xdg::BaseDirectories::with_prefix("wlscsr")?;
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
        for s in [&head.make, &head.model, &head.serial] {
            let bytes = s.as_bytes();
            hasher.update(bytes.len().to_le_bytes());
            hasher.update(bytes);
        }
    }

    hasher.finalize().into()
}

fn load_head_config(heads: &[Head], ignored_heads: &[Head]) -> anyhow::Result<Vec<Head>> {
    let base_directories = xdg::BaseDirectories::with_prefix("wlscsr")?;
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

    saved_heads.extend(ignored_heads.iter().map(|h| {
        let mut h = h.clone();
        h.config = None;
        h
    }));
    debug!("Restoring config: {:?}", saved_heads);

    Ok(saved_heads)
}

fn read_config_file() -> anyhow::Result<ConfigFile> {
    let base_directories = xdg::BaseDirectories::new()?;
    let path = base_directories.get_config_file("wlscsr.toml");

    let contents = std::fs::read(path);

    if let Err(ref err) = contents {
        if err.kind() == std::io::ErrorKind::NotFound {
            return Ok(Default::default());
        }
    }

    Ok(toml::from_str(std::str::from_utf8(&contents?)?)?)
}

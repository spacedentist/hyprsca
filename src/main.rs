use clap::{Parser, Subcommand};
use hyprrust::HyprlandConnection;
use hyprrust::commands::Command;
use hyprrust::data::{HyprlandData, HyprlandDataWithArgument, Monitor, Version};
use hyprrust_macros::{HyprlandData, HyprlandDataWithArgument};
use log::{Level, debug, log_enabled};
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
    Restore,

    /// Display information on connected monitors
    Info,
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

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let cli = Cli::parse();

    let conn = HyprlandConnection::new();
    if log_enabled!(Level::Debug) {
        let version = conn.get_sync::<Version>()?.version;
        debug!("Hyprland version: {version}");
    }

    // Get all monitors from Hyprland, convert to Head structure and sort
    let monitors = conn.get_with_argument_sync::<Monitors>("all".to_string())?;
    let mut heads: Vec<Head> = monitors.iter().map(Head::from).collect();
    heads.sort_by(Head::cmp_mms);

    // Calculate our hash from the set of connected screens
    let hash = hash_heads(&heads);

    let base_directories = xdg::BaseDirectories::with_prefix("hyprsca")?;

    match cli.command {
        Commands::Save => {
            let path = base_directories.place_state_file(format!("{}.json", hex::encode(hash)))?;
            heads.iter_mut().for_each(|h| {
                h.name = None;
            });
            std::fs::write(path, serde_json::to_string_pretty(&heads)?)?;
        }
        Commands::Restore => {
            let path = base_directories.get_state_file(format!("{}.json", hex::encode(hash)));
            let mut saved_heads: Vec<Head> = serde_json::from_slice(&std::fs::read(path)?)?;
            if saved_heads.len() != heads.len() {
                return Err(anyhow::anyhow!("Heads lengths mismatch"));
            }
            saved_heads.sort_by(Head::cmp_mms);

            for (saved_head, head) in saved_heads.iter_mut().zip(heads.iter()) {
                if (&saved_head.make, &saved_head.model, &saved_head.serial)
                    != (&head.make, &head.model, &head.serial)
                {
                    return Err(anyhow::anyhow!("Mismatch"));
                }
                saved_head.name = head.name.clone();
            }

            println!("Saved heads: {:?}", saved_heads);
            let heads: Vec<Box<dyn Command>> = saved_heads
                .into_iter()
                .map(|m| -> Box<dyn Command> { Box::new(m) })
                .collect();

            conn.send_recipe_sync(&heads)
                .map_err(|mut verr| verr.pop().unwrap())?;
        }
        Commands::Info => {
            println!("{} connected heads:", heads.len());
            for head in heads.iter() {
                println!(
                    "* {}\n  Make: {}\n  Model: {}\n  Serial: {}",
                    head.name.as_deref().unwrap_or(""),
                    &head.make,
                    &head.model,
                    &head.serial
                );
            }
            let path = base_directories.get_state_file(format!("{}.json", hex::encode(hash)));
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

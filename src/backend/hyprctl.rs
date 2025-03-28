use log::debug;
use serde::Deserialize;

use crate::backend::Backend;
use crate::types::{Head, HeadConfig};

pub struct HyprctlBackend {
    executable: String,
}

impl HyprctlBackend {
    pub fn new(executable: String) -> Self {
        Self { executable }
    }
}

impl Backend for HyprctlBackend {
    fn get_all_heads(&self) -> anyhow::Result<Vec<Head>> {
        let output = std::process::Command::new(&self.executable)
            .arg("-j")
            .arg("monitors")
            .arg("all")
            .output()?;
        if !output.status.success() {
            return Err(anyhow::anyhow!("hyprctl failed"));
        }
        let heads: Vec<HyprctlHead> = serde_json::from_slice(&output.stdout)?;
        Ok(heads.into_iter().map(HyprctlHead::make_head).collect())
    }

    fn set_head_config(&self, heads: &[Head]) -> anyhow::Result<()> {
        let mut cmd = std::process::Command::new(&self.executable);
        cmd.stdout(std::process::Stdio::null());
        cmd.arg("--batch");

        for head in heads {
            let Some(ref name) = head.name else {
                continue;
            };

            cmd.arg(if let Some(ref cfg) = head.config {
                format!(
                    "keyword monitor {},{}x{}@{},{}x{},{},transform,{},vrr,{};",
                    name,
                    cfg.width,
                    cfg.height,
                    cfg.refresh_rate,
                    cfg.x,
                    cfg.y,
                    cfg.scale,
                    cfg.transform,
                    if cfg.vrr { 1 } else { 0 },
                )
            } else {
                format!("keyword monitor {},disable;", name)
            });
        }

        debug!("Executing {:?}", cmd);
        if !cmd.status()?.success() {
            return Err(anyhow::anyhow!("wlr-randr failed"));
        }

        Ok(())
    }

    fn fallback_head_config(
        &self,
        active_head_names: &[String],
        inactive_head_names: &[String],
    ) -> anyhow::Result<()> {
        let mut cmd = std::process::Command::new(&self.executable);
        cmd.stdout(std::process::Stdio::null());
        cmd.arg("--batch");

        for head in active_head_names {
            cmd.arg(format!("keyword monitor {},preferred,auto,1;", head));
        }

        for head in inactive_head_names {
            cmd.arg(format!("keyword monitor {},disable;", head));
        }

        debug!("Executing {:?}", cmd);
        if !cmd.status()?.success() {
            return Err(anyhow::anyhow!("wlr-randr failed"));
        }

        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct HyprctlHead {
    name: String,
    make: String,
    model: String,
    serial: String,
    disabled: bool,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    #[serde(rename = "refreshRate")]
    refresh_rate: f64,
    transform: i32,
    scale: f64,
    vrr: bool,
}

impl HyprctlHead {
    fn make_head(self) -> Head {
        Head {
            name: Some(self.name),
            make: self.make,
            model: self.model,
            serial: self.serial,
            config: if !self.disabled {
                Some(HeadConfig {
                    x: self.x,
                    y: self.y,
                    width: self.width,
                    height: self.height,
                    refresh_rate: self.refresh_rate,
                    scale: self.scale,
                    vrr: self.vrr,
                    transform: self.transform,
                })
            } else {
                None
            },
        }
    }
}

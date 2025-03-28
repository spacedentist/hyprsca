use crate::backend::Backend;
use crate::types::{Head, HeadConfig};
use log::debug;
use serde::Deserialize;

pub struct WlrRandrBackend {
    executable: String,
}

impl WlrRandrBackend {
    pub fn new(executable: String) -> Self {
        Self { executable }
    }
}

impl Backend for WlrRandrBackend {
    fn get_all_heads(&self) -> anyhow::Result<Vec<Head>> {
        let output = std::process::Command::new(&self.executable)
            .arg("--json")
            .output()?;
        if !output.status.success() {
            return Err(anyhow::anyhow!("wlr-randr failed"));
        }
        let heads: Vec<WlrRandrHead> = serde_json::from_slice(&output.stdout)?;
        Ok(heads.into_iter().map(WlrRandrHead::make_head).collect())
    }

    fn set_head_config(&self, heads: &[Head]) -> anyhow::Result<()> {
        let mut cmd = std::process::Command::new(&self.executable);

        for head in heads {
            let Some(ref name) = head.name else {
                continue;
            };
            cmd.arg("--output");
            cmd.arg(name);

            if let Some(ref config) = head.config {
                cmd.arg("--on");

                cmd.arg("--mode");
                cmd.arg(format!(
                    "{}x{}@{}Hz",
                    config.width, config.height, config.refresh_rate
                ));

                cmd.arg("--pos");
                cmd.arg(format!("{},{}", config.x, config.y));

                cmd.arg("--scale");
                cmd.arg(format!("{}", config.scale));

                cmd.arg("--transform");
                cmd.arg(match config.transform {
                    1 => "90",
                    2 => "180",
                    3 => "270",
                    4 => "flipped",
                    5 => "flipped-90",
                    6 => "flipped-180",
                    7 => "flipped-270",
                    _ => "normal",
                });

                cmd.arg("--adaptive-sync");
                cmd.arg(if config.vrr { "enabled" } else { "disabled" });
            } else {
                cmd.arg("--off");
            }
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

        let mut previous_head = None;
        for head in active_head_names {
            cmd.arg("--output");
            cmd.arg(head);

            cmd.arg("--on");
            cmd.arg("--preferred");

            if let Some(previous_head) = previous_head {
                cmd.arg("--right-of");
                cmd.arg(previous_head);
            } else {
                cmd.arg("--pos");
                cmd.arg("0,0");
            }

            cmd.arg("--scale");
            cmd.arg("1");

            cmd.arg("--transform");
            cmd.arg("normal");

            previous_head = Some(head);
        }

        for head in inactive_head_names {
            cmd.arg("--output");
            cmd.arg(head);

            cmd.arg("--off");
        }

        debug!("Executing {:?}", cmd);
        if !cmd.status()?.success() {
            return Err(anyhow::anyhow!("wlr-randr failed"));
        }

        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct WlrRandrHead {
    name: String,
    make: Option<String>,
    model: Option<String>,
    serial: Option<String>,
    enabled: bool,
    position: Option<WlrRandrHeadPosition>,
    modes: Vec<WlrRandrHeadMode>,
    transform: Option<String>,
    scale: Option<f64>,
    adaptive_sync: Option<bool>,
}
#[derive(Debug, Deserialize)]
struct WlrRandrHeadMode {
    width: i32,
    height: i32,
    refresh: f64,
    // preferred: bool,
    current: bool,
}
#[derive(Debug, Deserialize)]
struct WlrRandrHeadPosition {
    x: i32,
    y: i32,
}
impl WlrRandrHead {
    fn make_head(self) -> Head {
        Head {
            name: Some(self.name),
            make: self.make.unwrap_or_default(),
            model: self.model.unwrap_or_default(),
            serial: self.serial.unwrap_or_default(),
            config: if self.enabled && !self.modes.is_empty() {
                let mode_idx = self
                    .modes
                    .iter()
                    .position(|m| m.current)
                    .unwrap_or_default();
                let mode = self.modes.get(mode_idx).unwrap();
                Some(HeadConfig {
                    x: self.position.as_ref().map(|p| p.x).unwrap_or(0),
                    y: self.position.as_ref().map(|p| p.y).unwrap_or(0),
                    width: mode.width,
                    height: mode.height,
                    refresh_rate: mode.refresh,
                    scale: self.scale.unwrap_or(1.0),
                    vrr: self.adaptive_sync.unwrap_or(false),
                    transform: match self.transform.as_deref().unwrap_or_default() {
                        "90" => 1,
                        "180" => 2,
                        "270" => 3,
                        "flipped" => 4,
                        "flipped-90" => 5,
                        "flipped-180" => 6,
                        "flipped-270" => 7,
                        _ => 0,
                    },
                })
            } else {
                None
            },
        }
    }
}

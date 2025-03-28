use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Head {
    #[serde(skip)]
    pub name: Option<String>,
    pub make: String,
    pub model: String,
    pub serial: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub config: Option<HeadConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct HeadConfig {
    pub width: i32,
    pub height: i32,
    pub refresh_rate: f64,
    pub x: i32,
    pub y: i32,
    pub scale: f64,
    pub transform: i32,
    pub vrr: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HeadMode {
    pub width: i32,
    pub height: i32,
    pub refresh_rate: f64,
}

impl Head {
    pub fn cmp_mms(&self, other: &Self) -> std::cmp::Ordering {
        self.make
            .cmp(&other.make)
            .then_with(|| self.model.cmp(&other.model))
            .then_with(|| self.serial.cmp(&other.serial))
    }
}

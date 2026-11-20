use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub distro: String,
    pub poll_interval_secs: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            distro: String::new(),
            poll_interval_secs: 60,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if !(30..=3600).contains(&self.poll_interval_secs) {
            return Err("刷新间隔必须在 30～3600 秒之间。".into());
        }
        if self.distro.starts_with('-')
            || self.distro.len() > 128
            || self.distro.chars().any(char::is_control)
        {
            return Err("WSL 发行版名称无效。".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OverlayPosition {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    pub settings: Settings,
    pub locked: bool,
    pub overlay_visible: bool,
    pub overlay_position: Option<OverlayPosition>,
    pub completion_sound: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            settings: Settings::default(),
            locked: false,
            overlay_visible: true,
            overlay_position: None,
            completion_sound: true,
        }
    }
}

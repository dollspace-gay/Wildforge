//! Persistent game settings (config.txt, `key=value` lines).

use std::path::PathBuf;

#[derive(Clone, PartialEq, Debug)]
pub struct Config {
    /// Master volume 0..1.
    pub volume: f32,
    /// Mouse sensitivity multiplier.
    pub sensitivity: f32,
    /// View distance in chunks.
    pub view_dist: i32,
    /// Field of view in degrees.
    pub fov: f32,
    /// Active texture pack id (`packs/<id>/` or built-in); empty = none
    /// (procedural). Fresh installs default to the bundled gemini pack.
    pub pack: String,
    /// Point lights: 0 = off, 1 = shadowless, 2 = hard shadows.
    pub lights: u8,
    /// Ambient floor: true = stark (dark truly dark), false = soft.
    pub stark: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            volume: 0.7,
            sensitivity: 1.0,
            view_dist: 7,
            fov: 75.0,
            pack: "gemini".into(),
            lights: 2,
            stark: true,
        }
    }
}

fn path() -> PathBuf {
    PathBuf::from("config.txt")
}

impl Config {
    pub fn from_text(text: &str) -> Config {
        let mut c = Config::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let (k, v) = (k.trim(), v.trim());
            match k {
                "volume" => {
                    if let Ok(x) = v.parse::<f32>() {
                        c.volume = x.clamp(0.0, 1.0);
                    }
                }
                "sensitivity" => {
                    if let Ok(x) = v.parse::<f32>() {
                        c.sensitivity = x.clamp(0.1, 3.0);
                    }
                }
                "view_dist" => {
                    if let Ok(x) = v.parse::<i32>() {
                        c.view_dist = x.clamp(4, 12);
                    }
                }
                "fov" => {
                    if let Ok(x) = v.parse::<f32>() {
                        c.fov = x.clamp(50.0, 110.0);
                    }
                }
                "pack" => c.pack = v.to_string(),
                "lights" => {
                    c.lights = match v {
                        "off" => 0,
                        "on" => 1,
                        _ => 2,
                    }
                }
                "darkness" => c.stark = v != "soft",
                _ => {}
            }
        }
        c
    }

    pub fn to_text(&self) -> String {
        format!(
            "volume={:.2}\nsensitivity={:.2}\nview_dist={}\nfov={:.0}\npack={}\nlights={}\ndarkness={}\n",
            self.volume,
            self.sensitivity,
            self.view_dist,
            self.fov,
            self.pack,
            ["off", "on", "shadows"][self.lights.min(2) as usize],
            if self.stark { "stark" } else { "soft" },
        )
    }

    pub fn load() -> Config {
        match std::fs::read_to_string(path()) {
            Ok(text) => Config::from_text(&text),
            Err(_) => Config::default(),
        }
    }

    pub fn save(&self) {
        if let Err(e) = std::fs::write(path(), self.to_text()) {
            eprintln!("config: save failed: {e}");
        }
    }
}

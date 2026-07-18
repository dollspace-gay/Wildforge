//! Persistent game settings (config.txt, `key=value` lines).

use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Config {
    /// Master volume 0..1.
    pub volume: f32,
    /// Mouse sensitivity multiplier.
    pub sensitivity: f32,
    /// View distance in chunks.
    pub view_dist: i32,
    /// Field of view in degrees.
    pub fov: f32,
}

impl Default for Config {
    fn default() -> Config {
        Config { volume: 0.7, sensitivity: 1.0, view_dist: 7, fov: 75.0 }
    }
}

fn path() -> PathBuf {
    PathBuf::from("config.txt")
}

impl Config {
    pub fn load() -> Config {
        let mut c = Config::default();
        let Ok(text) = std::fs::read_to_string(path()) else {
            return c;
        };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
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
                _ => {}
            }
        }
        c
    }

    pub fn save(&self) {
        let text = format!(
            "volume={:.2}\nsensitivity={:.2}\nview_dist={}\nfov={:.0}\n",
            self.volume, self.sensitivity, self.view_dist, self.fov
        );
        if let Err(e) = std::fs::write(path(), text) {
            eprintln!("config: save failed: {e}");
        }
    }
}

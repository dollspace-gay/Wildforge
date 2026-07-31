//! Persistent game settings (config.txt, `key=value` lines).

use std::path::PathBuf;

/// How far the world may be loaded, in chunks.
///
/// 64 is 1024 blocks, which is past the 900 that separates one country
/// from the next — so at the top of the slider a heart's edifice is
/// always somewhere on the horizon, which is the entire point.
///
/// The costs, in order of who cares:
/// - RAM: the loaded set is (2n+1)^2 chunks. At 64 that is ~16600 of
///   them, and at the measured cost below that is over 4 GB — which is
///   why the slider is bounded by [`max_view_dist_for_memory`] rather
///   than by this constant alone. The old note here said "~190 KB" and
///   "about 3 GB": both undercounted, because they left out the two
///   light planes entirely.
/// - Draw: the opaque pass is frustum-culled and shadows are
///   range-culled, so what you pay for is what is in front of you, not
///   what is loaded.
/// - Filling it: the real bottleneck, and why the generator pool and
///   the streaming budgets below scale with this number instead of
///   sitting at the constants that suited a 7-chunk view.
pub const MAX_VIEW_DIST: i32 = 64;

/// What one resident chunk costs, measured on real generated terrain.
///
/// Blocks and sky light are genuinely per-cell; block light and (on
/// untouched ground) metadata compact away. A chunk with a torch in it and
/// worked soil under it pays the full 448 KB, so this is a floor, not a
/// promise — but it is the honest number to size a view distance against.
pub const CHUNK_RESIDENT_BYTES: u64 = 262 * 1024;

/// Share of available memory the loaded world may claim.
const WORLD_MEMORY_SHARE: f64 = 0.55;

/// Memory this machine can give us right now, if it will say.
fn available_memory() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        let line = text.lines().find(|l| l.starts_with("MemAvailable:"))?;
        let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
        Some(kb * 1024)
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
        status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if unsafe { GlobalMemoryStatusEx(&mut status) } != 0 {
            return Some(status.ullAvailPhys);
        }
        return None;
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    None
}

/// The largest view distance this machine can actually back with memory.
///
/// The slider used to run to [`MAX_VIEW_DIST`] on every machine, which asks
/// for over 4 GB of resident chunks — a setting that does not render a
/// distant edifice so much as end the process. A setting that cannot be
/// honoured is worse than one that is not offered.
pub fn max_view_dist_for_memory() -> i32 {
    let Some(available) = available_memory() else {
        // No answer: trust the player, cap at the old maximum.
        return MAX_VIEW_DIST;
    };
    let budget = (available as f64 * WORLD_MEMORY_SHARE) as u64;
    let affordable = budget / CHUNK_RESIDENT_BYTES;
    // chunks = (2r+1)^2  =>  r = (sqrt(chunks) - 1) / 2
    let radius = ((affordable as f64).sqrt() - 1.0) / 2.0;
    (radius as i32).clamp(MIN_VIEW_DIST, MAX_VIEW_DIST)
}

/// Below this the world is unplayable, so it is never clamped away.
pub const MIN_VIEW_DIST: i32 = 4;

#[derive(Clone, PartialEq, Debug)]
pub struct Config {
    /// Local Wildforge display name. It is presentation, never an account key.
    pub display_name: String,
    /// Set only after the player explicitly accepts an editable local name.
    pub profile_complete: bool,
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
    /// How point-light shadows are computed: true = exact voxel-grid march
    /// (no self-shadow acne), false = the legacy distance cube map.
    pub point_grid: bool,
    /// Packed player Style (style.rs) — how others see you.
    pub appearance: u32,
    /// Ambient floor: true = stark (dark truly dark), false = soft.
    pub stark: bool,
    /// Draw the thin wireframe on the targeted block.
    pub outline: bool,
    /// Bloom: overbright emitters and highlights bleed a soft glow.
    pub bloom: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            display_name: "PLAYER".into(),
            profile_complete: false,
            volume: 0.7,
            sensitivity: 1.0,
            // Was 7 (112 blocks), which put a fog wall closer than any
            // landmark in the game. 12 is 192 blocks and 625 chunks — about
            // 160 MB at the measured cost, where the old note here said 120
            // MB by leaving both light planes out of the sum. The slider
            // goes as high as this machine can hold, for anyone who wants to
            // see a country's edifice from the next valley.
            view_dist: 12,
            fov: 75.0,
            pack: "gemini".into(),
            lights: 2,
            point_grid: true,
            stark: true,
            outline: true,
            bloom: true,
            appearance: crate::style::Style::default().pack(),
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
                "display_name" => {
                    if let Ok(name) = crate::identity::DisplayName::parse(v) {
                        c.display_name = name.to_string();
                    }
                }
                "profile_complete" => c.profile_complete = v == "yes",
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
                        // Bounded by what this machine can hold, not just by
                        // what the slider can express.
                        c.view_dist = x.clamp(MIN_VIEW_DIST, max_view_dist_for_memory());
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
                "point_shadows" => c.point_grid = v != "cube",
                "darkness" => c.stark = v != "soft",
                "outline" => c.outline = v != "off",
                "bloom" => c.bloom = v != "off",
                "appearance" => {
                    if let Ok(x) = v.parse::<u32>() {
                        c.appearance = x;
                    }
                }
                _ => {}
            }
        }
        c
    }

    pub fn to_text(&self) -> String {
        format!(
            "display_name={}\nprofile_complete={}\nvolume={:.2}\nsensitivity={:.2}\nview_dist={}\nfov={:.0}\npack={}\nlights={}\npoint_shadows={}\ndarkness={}\noutline={}\nbloom={}\nappearance={}\n",
            self.display_name,
            if self.profile_complete { "yes" } else { "no" },
            self.volume,
            self.sensitivity,
            self.view_dist,
            self.fov,
            self.pack,
            ["off", "on", "shadows"][self.lights.min(2) as usize],
            if self.point_grid { "grid" } else { "cube" },
            if self.stark { "stark" } else { "soft" },
            if self.outline { "on" } else { "off" },
            if self.bloom { "on" } else { "off" },
            self.appearance,
        )
    }

    pub fn load() -> Config {
        match std::fs::read_to_string(path()) {
            Ok(text) => Config::from_text(&text),
            Err(_) => Config::default(),
        }
    }

    pub fn save(&self) {
        if let Err(e) = crate::persist::atomic_write(&path(), self.to_text().as_bytes(), false) {
            eprintln!("config: save failed: {e}");
        }
    }
}

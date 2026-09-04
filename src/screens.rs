//! Data-driven mod screens (capability E11): the pure model.
//!
//! A mod declares screens in `screens.toml` — a title and rows of
//! widgets. The engine renders the rows and routes the clicks; the mod's
//! script supplies the behavior through one hook, `on_screen_click`, and
//! its existing command queue (`give`, `set_block`, `hud_message`, ...).
//!
//! ## Widget kinds
//!
//! - `label` — static text.
//! - `kv_label` — a live readout of one per-player KV key, so scripts and
//!   quests can surface state without any new plumbing.
//! - `toggle { key }` — flips a per-player KV key between "1" and "0"
//!   client-side; state display comes free via `kv_label`.
//! - `button { action }` — dispatches `on_screen_click(screen_id,
//!   action)`. On a guest the click rides [`crate::net::C2S::ScreenClick`]
//!   and the host validates both ids against its own registry before
//!   dispatching, so a tampered client can only ever name buttons that
//!   exist.
//!
//! Rows render grouped in declaration order within each kind: labels,
//! then kv labels, then toggles, then buttons.

/// Schema version of `screens.toml`. Bump when the field grammar changes
/// incompatibly; older files are refused rather than half-read.
pub const SCREENS_SCHEMA_VERSION: u32 = 1;

use crate::registry::Registry;

/// One declared mod screen: its qualified identity, title, and widget rows.
#[derive(Clone, Debug)]
pub struct ScreenDef {
    pub id: String,
    pub title: String,
    pub widgets: Vec<ScreenWidget>,
}

/// One row on a mod screen.
#[derive(Clone, Debug, PartialEq)]
pub enum ScreenWidget {
    /// Static text.
    Label(String),
    /// A live readout: `<prefix><value-or-dash>` of a player-KV key.
    KvLabel { key: String, prefix: String },
    /// Click flips a player-KV key between "1" and "0" (client-local).
    Toggle { label: String, key: String },
    /// Click dispatches `on_screen_click(screen_id, action)` —
    /// host-authoritative for guests.
    Button { label: String, action: String },
}

impl ScreenDef {
    /// All widget rows in render order.
    pub fn rows(&self) -> &[ScreenWidget] {
        &self.widgets
    }
}

#[derive(serde::Deserialize, Clone, Default)]
pub struct RawScreensToml {
    #[serde(default)]
    pub schema_version: Option<u32>,
    #[serde(default)]
    pub screen: Vec<RawScreen>,
}

#[derive(serde::Deserialize, Clone, Default)]
pub struct RawScreen {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub label: Vec<RawText>,
    #[serde(default)]
    pub kv_label: Vec<RawKvLabel>,
    #[serde(default)]
    pub toggle: Vec<RawToggle>,
    #[serde(default)]
    pub button: Vec<RawButton>,
}

#[derive(serde::Deserialize, Clone, Default)]
pub struct RawText {
    pub text: String,
}

#[derive(serde::Deserialize, Clone, Default)]
pub struct RawKvLabel {
    pub key: String,
    #[serde(default)]
    pub prefix: Option<String>,
}

#[derive(serde::Deserialize, Clone, Default)]
pub struct RawToggle {
    pub label: String,
    pub key: String,
}

#[derive(serde::Deserialize, Clone, Default)]
pub struct RawButton {
    pub label: String,
    pub action: String,
}

fn qualify(modid: &str, name: &str) -> String {
    if name.contains(':') {
        name.to_string()
    } else {
        format!("{modid}:{name}")
    }
}

/// Resolve every mod's raw screens into registry order (base first).
/// Duplicate identities and empty titles/actions fail the pack.
pub fn resolve(raws: &[(String, RawScreensToml)]) -> Result<Vec<ScreenDef>, Vec<String>> {
    let mut out: Vec<ScreenDef> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (modid, file) in raws {
        for raw in &file.screen {
            let full = qualify(modid, &raw.id);
            if !seen.insert(full.clone()) {
                errors.push(format!("screen {}: duplicate id", full));
                continue;
            }
            if raw.id.trim().is_empty() {
                errors.push(format!("screen {full}: empty id"));
                continue;
            }
            let title = raw.title.clone().unwrap_or_else(|| raw.id.clone());
            if title.trim().is_empty() {
                errors.push(format!("screen {full}: empty title"));
                continue;
            }
            let mut widgets: Vec<ScreenWidget> = Vec::new();
            for label in &raw.label {
                if label.text.trim().is_empty() {
                    errors.push(format!("screen {full}: empty label text"));
                    continue;
                }
                widgets.push(ScreenWidget::Label(label.text.clone()));
            }
            for kv in &raw.kv_label {
                if kv.key.trim().is_empty() {
                    errors.push(format!("screen {full}: empty kv_label key"));
                    continue;
                }
                widgets.push(ScreenWidget::KvLabel {
                    key: kv.key.clone(),
                    prefix: kv.prefix.clone().unwrap_or_default(),
                });
            }
            for toggle in &raw.toggle {
                if toggle.key.trim().is_empty() {
                    errors.push(format!("screen {full}: toggle with an empty key"));
                    continue;
                }
                widgets.push(ScreenWidget::Toggle {
                    label: toggle.label.clone(),
                    key: toggle.key.clone(),
                });
            }
            for button in &raw.button {
                if button.action.trim().is_empty() || button.label.trim().is_empty() {
                    errors.push(format!(
                        "screen {full}: button {:?} needs a label and an action",
                        button.action
                    ));
                    continue;
                }
                widgets.push(ScreenWidget::Button {
                    label: button.label.clone(),
                    action: button.action.clone(),
                });
            }
            out.push(ScreenDef {
                id: full,
                title,
                widgets,
            });
        }
    }
    if errors.is_empty() {
        Ok(out)
    } else {
        Err(errors)
    }
}

impl Registry {
    /// Resolve a qualified screen id to its registry index (capability E11).
    pub fn screen_by_name(&self, name: &str) -> Option<usize> {
        self.screens.iter().position(|s| s.id == name)
    }

    /// The screen index a block interaction opens: interactions shaped
    /// `screen:<qualified screen id>` (capability E11).
    pub fn screen_by_interaction(&self, interaction: &str) -> Option<usize> {
        let name = interaction.strip_prefix("screen:")?;
        self.screen_by_name(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(id: &str, title: Option<&str>) -> RawScreen {
        RawScreen {
            id: id.to_string(),
            title: title.map(|t| t.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn resolve_qualifies_and_orders() {
        let raws = vec![
            (
                "base".to_string(),
                RawScreensToml {
                    schema_version: None,
                    screen: vec![file("notice", Some("NOTICE"))],
                },
            ),
            (
                "gems".to_string(),
                RawScreensToml {
                    schema_version: None,
                    screen: vec![RawScreen {
                        id: "altar".into(),
                        title: Some("The Altar".into()),
                        label: vec![RawText {
                            text: "Ruby offerings only.".into(),
                        }],
                        kv_label: vec![RawKvLabel {
                            key: "quest_x".into(),
                            prefix: None,
                        }],
                        toggle: vec![RawToggle {
                            label: "Track".into(),
                            key: "tracked".into(),
                        }],
                        button: vec![RawButton {
                            label: "Pray".into(),
                            action: "pray".into(),
                        }],
                    }],
                },
            ),
        ];
        let screens = resolve(&raws).expect("resolves");
        assert_eq!(screens.len(), 2);
        assert_eq!(screens[0].id, "base:notice");
        assert_eq!(screens[1].id, "gems:altar");
        let w = &screens[1].widgets;
        assert_eq!(w.len(), 4);
        assert_eq!(w[0], ScreenWidget::Label("Ruby offerings only.".into()));
        assert!(matches!(&w[2], ScreenWidget::Toggle { key, .. } if key == "tracked"));
        assert!(matches!(&w[3], ScreenWidget::Button { action, .. } if action == "pray"));
    }

    #[test]
    fn empty_action_or_title_fails() {
        let raws = vec![(
            "gems".to_string(),
            RawScreensToml {
                schema_version: None,
                screen: vec![RawScreen {
                    id: "broken".into(),
                    title: None,
                    button: vec![RawButton {
                        label: "Go".into(),
                        action: "".into(),
                    }],
                    ..Default::default()
                }],
            },
        )];
        let errors = resolve(&raws).expect_err("empty action must fail");
        assert!(
            errors
                .iter()
                .any(|e| e.contains("needs a label and an action")),
            "{errors:?}"
        );
    }

    #[test]
    fn duplicate_ids_fail() {
        let raws = vec![
            (
                "base".to_string(),
                RawScreensToml {
                    schema_version: None,
                    screen: vec![file("board", None)],
                },
            ),
            (
                "gems".to_string(),
                RawScreensToml {
                    schema_version: None,
                    screen: vec![file("base:board", None)],
                },
            ),
        ];
        let errors = resolve(&raws).expect_err("duplicate must fail");
        assert!(errors.iter().any(|e| e.contains("duplicate id")));
    }
}

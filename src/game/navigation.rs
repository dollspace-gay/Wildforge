//! Screen state and ordered cross-domain screen transitions.

use super::Game;
use crate::{identity, net};
use crate::inventory::ItemStack;
use crate::registry::ItemId;

#[derive(Clone, PartialEq)]
pub(super) enum Screen {
    Title,
    NewWorld,
    CreatingWorld,
    Accounts,
    Moderation(u32),
    Mods,
    Packs,
    Settings,
    Appearance,
    ConfirmDelete,
    Playing,
    Inventory,
    Furnace(crate::planet::BlockPos),
    Chest(crate::planet::BlockPos),
    Offering(crate::planet::BlockPos),
    Bloomery(crate::planet::BlockPos),
    Kiln(crate::planet::BlockPos),
    /// A recipe-list station machine (capability E7): lists the machine's
    /// `station` recipes and crafts them from the inventory. Temporary
    /// hardcoded screen; E11 generalizes it into mod-extensible screens.
    Workbench(crate::planet::BlockPos),
    /// A tamed carrier's saddlebags, keyed by mob id.
    MobCargo(u32),
    /// Writing a placed sign or waystone.
    SignEdit(crate::planet::BlockPos),
    /// A market stall: the owner manages, everyone else shops.
    Stall(crate::planet::BlockPos),
    /// Talking to a friendly NPC (spec 3.2): the dialogue tree in
    /// `reg.dialogues` selected by the NPC's def. Holds the NPC mob id,
    /// the current node id, and the highlighted choice row.
    Dialog {
        npc: u32,
        node_id: String,
        choice_sel: usize,
    },
    /// The quest journal (spec 3.3): accepted quests and their progress.
    Journal,
    /// The skill tree (capability E5): allocate learned nodes in the
    /// active world's mode-gated tree. Temporary hardcoded screen; E11
    /// generalizes this into mod-extensible screens.
    Skills,
    /// The loadout (capability E6): slot components into worn frames,
    /// repair disabled frames, and save/apply loadout presets. Temporary
    /// hardcoded screen; E11 generalizes this into mod-extensible screens.
    Loadout,
    /// A data-driven mod screen (capability E11): the index into
    /// `Registry::screens`. Rows render from the def; buttons dispatch the
    /// mod's `on_screen_click` hook host-authoritatively.
    Mod(usize),
    Join,
    Paused,
    Dead,
}

/// Screen navigation, focus, browser history, and cursor-held inventory state.
pub(super) struct UiState {
    pub(super) screen: Screen,
    pub(super) held_stack: Option<ItemStack>,
    pub(super) settings_from_pause: bool,
    pub(super) pending_delete: Option<usize>,
    pub(super) dragging_slider: Option<usize>,
    pub(super) search: String,
    pub(super) search_focus: bool,
    /// Sign editor buffer (three short lines) and the active line.
    pub(super) sign_lines: [String; 3],
    pub(super) sign_line: usize,
    pub(super) browse_page: usize,
    pub(super) browse_view: Option<(ItemId, bool)>,
    pub(super) browse_back: Vec<(ItemId, bool)>,
    pub(super) inventory_status_open: bool,
    pub(super) inventory_browser_open: bool,
    pub(super) inventory_discovery_open: bool,
    pub(super) discovery_holder: Option<net::RecordHolderSnap>,
    pub(super) discovery_copy_target: Option<net::RecordHolderSnap>,
    pub(super) discovery_writing_pos: Option<crate::planet::BlockPos>,
    pub(super) discovery_records: Vec<crate::discovery::ObservationSummary>,
    pub(super) discovery_capacity: u16,
    pub(super) discovery_page: usize,
    pub(super) discovery_sort: u8,
    pub(super) discovery_selected: [Option<u64>; 2],
    pub(super) discovery_include_location: bool,
    pub(super) discovery_label: String,
    pub(super) discovery_label_focus: bool,
    pub(super) appearance_from_pause: bool,
    pub(super) account_name: String,
    pub(super) account_handle: String,
    pub(super) account_focus: u8,
    pub(super) account_status: String,
    pub(super) account_task: Option<std::sync::mpsc::Receiver<AccountTaskResult>>,
    pub(super) new_world_mode: String,
    pub(super) new_world_seed: String,
    pub(super) new_world_status: String,
    pub(super) moderation_confirm: Option<u8>,
    pub(super) creation_status: String,
    pub(super) creation_progress: (usize, usize),
    /// Index into `reg.skills.branches` shown on the skill screen.
    pub(super) skills_branch: usize,
    /// Which armor slot (0..=3) the loadout screen acts on.
    pub(super) loadout_select: usize,
    /// Which numbered preset the loadout screen saves into / applies from.
    pub(super) loadout_preset_sel: usize,
}

pub(super) enum AccountTaskResult {
    Linked(Result<identity::atproto::AtprotoAccount, String>),
    Revoked(Result<(), String>),
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            screen: Screen::Title,
            held_stack: None,
            settings_from_pause: false,
            pending_delete: None,
            dragging_slider: None,
            search: String::new(),
            search_focus: false,
            sign_lines: Default::default(),
            sign_line: 0,
            browse_page: 0,
            browse_view: None,
            browse_back: Vec::new(),
            inventory_status_open: false,
            inventory_browser_open: false,
            inventory_discovery_open: false,
            discovery_holder: None,
            discovery_copy_target: None,
            discovery_writing_pos: None,
            discovery_records: Vec::new(),
            discovery_capacity: 0,
            discovery_page: 0,
            discovery_sort: 0,
            discovery_selected: [None; 2],
            discovery_include_location: false,
            discovery_label: String::new(),
            discovery_label_focus: false,
            appearance_from_pause: false,
            account_name: String::new(),
            account_handle: String::new(),
            account_focus: 0,
            account_status: String::new(),
            account_task: None,
            new_world_mode: "survival".into(),
            new_world_seed: String::new(),
            new_world_status: String::new(),
            moderation_confirm: None,
            creation_status: String::new(),
            creation_progress: (0, crate::planet_atlas::AtlasStage::ALL.len()),
            skills_branch: 0,
            loadout_select: 0,
            loadout_preset_sel: 0,
        }
    }
}

impl Game {
    /// All game modes available for new-world creation (capability E1):
    /// the two built-ins plus every mode declared by loaded mods.
    pub(super) fn available_new_world_modes(&self) -> Vec<String> {
        let mut modes = vec!["survival".to_string(), "creative".to_string()];
        for m in &self.content.reg.modes {
            if !modes.contains(&m.id) {
                modes.push(m.id.clone());
            }
        }
        modes
    }

    pub(super) fn set_screen(&mut self, screen: Screen) {
        if self.ui_state.screen == screen {
            return;
        }
        self.presentation.screen_age = 0.0;
        self.interaction.bow_draw = 0.0; // opening any screen relaxes the draw

        // Leaving a container tells the host to stop streaming it.
        if matches!(
            self.ui_state.screen,
            Screen::Furnace(_)
                | Screen::Chest(_)
                | Screen::Offering(_)
                | Screen::Bloomery(_)
                | Screen::Kiln(_)
                | Screen::Workbench(_)
                | Screen::MobCargo(_)
                | Screen::Stall(_)
        ) && let Some(r) = &self.multiplayer.remote
        {
            r.session.send(&net::C2S::CloseContainer);
        }
        // Leaving the inventory returns the cursor-held stack and craft grid.
        if self.ui_state.screen == Screen::Inventory
            || matches!(
                self.ui_state.screen,
                Screen::Furnace(_)
                    | Screen::Chest(_)
                    | Screen::Offering(_)
                    | Screen::Bloomery(_)
                    | Screen::Kiln(_)
                    | Screen::Workbench(_)
                    | Screen::Mod(_)
                    | Screen::MobCargo(_)
                    | Screen::Stall(_)
            )
        {
            let mut back: Vec<ItemStack> = self.ui_state.held_stack.take().into_iter().collect();
            for slot in self.interaction.craft_grid.iter_mut() {
                if let Some(s) = slot.take() {
                    back.push(s);
                }
            }
            let reg = self.content.reg.clone();
            for s in back {
                let left = self.inventory.add_stack(&reg, s);
                if left > 0 {
                    self.drop_stack(ItemStack { count: left, ..s });
                }
            }
        }
        if screen == Screen::Inventory {
            self.ui_state.inventory_status_open = false;
            self.ui_state.inventory_discovery_open = false;
            self.ui_state.discovery_label_focus = false;
            // Creative mode uses the browser as its item source. In survival
            // it is secondary help, so keep it tucked away until requested.
            self.ui_state.inventory_browser_open = self.creative;
        } else {
            self.ui_state.search_focus = false;
            self.ui_state.browse_view = None;
        }
        let playing = screen == Screen::Playing;
        self.ui_state.screen = screen;
        if playing {
            self.input.capture(&self.window, true);
        } else {
            self.input.capture(&self.window, false);
            self.input.clear_held();
            self.interaction.breaking = None;
        }
    }
}

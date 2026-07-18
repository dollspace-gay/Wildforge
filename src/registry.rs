//! Dynamic block/item/recipe registries — the foundation of the mod system.
//! Vanilla content is the built-in `base` mod, registered through the same
//! TOML path external mods use.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct BlockId(pub u16);
pub const AIR: BlockId = BlockId(0);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ItemId(pub u16);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolKind {
    Pickaxe,
    Axe,
    Shovel,
    Hoe,
}

#[derive(Clone, Debug)]
pub struct BlockDef {
    pub name: String,  // "base:stone"
    pub label: String, // "Stone"
    /// Atlas slots per face: +X -X +Y -Y +Z -Z.
    pub tiles: [u16; 6],
    pub hardness: Option<f32>,
    pub tool: Option<ToolKind>,
    pub requires_tool: bool,
    /// Resolved drop (item, count); None = drops nothing.
    pub drops: Option<(ItemId, u32)>,
    pub solid: bool,
    pub opaque: bool,
    /// Right-click behavior: "crafting", "furnace", ...
    pub interaction: Option<String>,
    /// Minimum tool tier for drops when requires_tool is set.
    pub min_tier: u8,
    /// 0 = fluid source, 1..=7 flowing levels. None = not a fluid.
    pub water_level: Option<u8>,
    /// Render as two crossed quads instead of a cube (plants).
    pub cross: bool,
    /// Crop: (final stage block advances no further). tick advances stages.
    pub crop_next: Option<BlockId>,
    pub crop_chance: f32,
    pub crop_any_soil: bool,
    /// Right-click harvest: (item, count, block it becomes).
    pub harvest: Option<(ItemId, u32, BlockId)>,
}

pub const NUTRIENTS: [&str; 5] = ["grain", "vegetable", "fruit", "fungi", "protein"];

#[derive(Clone, Debug)]
pub struct FoodDef {
    pub hunger: f32,
    pub eat_time: f32,
    pub nutrition: [f32; 5],
}

#[derive(Clone, Debug)]
pub struct ItemDef {
    pub name: String,
    pub label: String,
    pub icon: u16,
    pub max_stack: u32,
    /// (kind, speed multiplier on matching blocks, tier)
    pub tool: Option<(ToolKind, f32, u8)>,
    pub durability: u32,
    /// Placing this item puts down this block.
    pub places: Option<BlockId>,
    pub food: Option<FoodDef>,
    /// Attack damage in half-hearts (swords set it high; tools get a
    /// modest implicit value, bare items 1).
    pub damage: f32,
}

/// One box of an animal's model. Sizes/offsets in px (16 px = 1 block);
/// `at` is (center x, bottom y, center z). A box named "leg" is mirrored
/// into four legs at (±x, y, ±z) by the renderer.
#[derive(Clone, Debug)]
pub struct ModelBox {
    pub name: String,
    pub size: [f32; 3],
    pub at: [f32; 3],
    /// Explicit texture for this box (e.g. bone antlers); None = fur.
    pub tile: Option<u16>,
}

#[derive(Clone, Debug)]
pub struct AnimalDef {
    pub name: String,  // "base:deer"
    pub label: String,
    /// Lowercase biome names this species spawns in.
    pub biomes: Vec<String>,
    pub health: f32,
    pub speed: f32,
    /// Player distance that spooks it (0 = bold, only flees when hurt).
    pub flee_range: f32,
    pub group: [u32; 2],
    /// 1-in-N eligible fresh chunks spawn a group.
    pub rarity: u32,
    pub tile: u16,
    pub head_tile: u16,
    pub sound_pitch: f32,
    /// (item, min, max) rolled independently on death.
    pub drops: Vec<(ItemId, u32, u32)>,
    pub model: Vec<ModelBox>,
    /// Collision half-width / height derived from the model.
    pub half_w: f32,
    pub height: f32,
}

/// A recipe slot requirement: one exact item, or any member of a tag.
#[derive(Clone, Debug)]
pub enum Ingredient {
    One(ItemId),
    Any(Vec<ItemId>),
}

impl Ingredient {
    pub fn matches(&self, item: ItemId) -> bool {
        match self {
            Ingredient::One(i) => *i == item,
            Ingredient::Any(list) => list.contains(&item),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RecipeDef {
    pub w: usize,
    pub h: usize,
    pub pattern: Vec<Option<Ingredient>>,
    pub output: ItemId,
    pub count: u32,
}

#[derive(Clone, Debug)]
pub struct SmeltDef {
    pub input: Ingredient,
    pub output: ItemId,
    pub time: f32,
}

#[derive(Clone, Debug)]
pub struct OreFeature {
    pub block: BlockId,
    pub replaces: BlockId,
    pub vein_size: u32,
    pub per_chunk: u32,
    pub y_min: i32,
    pub y_max: i32,
}

#[derive(Clone, Debug)]
pub struct ModInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub path: Option<PathBuf>,
    pub has_script: bool,
    pub error: Option<String>,
}

pub struct Registry {
    pub blocks: Vec<BlockDef>,
    pub items: Vec<ItemDef>,
    pub recipes: Vec<RecipeDef>,
    pub ores: Vec<OreFeature>,
    pub block_by_name: HashMap<String, BlockId>,
    pub item_by_name: HashMap<String, ItemId>,
    /// water_ids[level] — source at 0, flows 1..=7.
    pub water_ids: [BlockId; 8],
    pub unknown_block: BlockId,
    pub mods: Vec<ModInfo>,
    pub smelts: Vec<SmeltDef>,
    /// (fuel ingredient, burn seconds)
    pub fuels: Vec<(Ingredient, f32)>,
    /// Item groups usable as `#tag` recipe ingredients; mods can extend them.
    pub tags: HashMap<String, Vec<ItemId>>,
    /// Mod textures to pack: (slot, png path).
    pub tex_files: Vec<(u16, PathBuf)>,
    /// Pack-addressable names for mod textures: ("<mod_id>/<file stem>", slot).
    pub tex_names: Vec<(String, u16)>,
    pub animals: Vec<AnimalDef>,
}

impl Registry {
    #[inline]
    pub fn block(&self, id: BlockId) -> &BlockDef {
        &self.blocks[id.0 as usize]
    }

    #[inline]
    pub fn item(&self, id: ItemId) -> &ItemDef {
        &self.items[id.0 as usize]
    }

    pub fn block_id(&self, name: &str) -> Option<BlockId> {
        self.block_by_name.get(name).copied()
    }

    pub fn animal_id(&self, name: &str) -> Option<usize> {
        self.animals.iter().position(|a| a.name == name)
    }

    pub fn item_id(&self, name: &str) -> Option<ItemId> {
        self.item_by_name.get(name).copied()
    }

    #[inline]
    pub fn is_air(&self, id: BlockId) -> bool {
        id == AIR
    }

    #[inline]
    pub fn is_solid(&self, id: BlockId) -> bool {
        self.block(id).solid
    }

    #[inline]
    pub fn is_opaque(&self, id: BlockId) -> bool {
        self.block(id).opaque
    }

    #[inline]
    pub fn is_water(&self, id: BlockId) -> bool {
        self.block(id).water_level.is_some()
    }

    #[inline]
    pub fn water_level(&self, id: BlockId) -> Option<u8> {
        self.block(id).water_level
    }

    pub fn water_block(&self, level: u8) -> BlockId {
        self.water_ids[(level as usize).min(7)]
    }

    pub fn water_height(&self, id: BlockId) -> f32 {
        match self.block(id).water_level {
            Some(l) => (8 - l) as f32 / 9.0,
            None => 1.0,
        }
    }

    /// Seconds to break `block` holding `held`.
    pub fn effective_hardness(&self, block: BlockId, held: Option<ItemId>) -> Option<f32> {
        let d = self.block(block);
        let base = d.hardness?;
        let mult = match (held.and_then(|i| self.item(i).tool), d.tool) {
            (Some((kind, speed, _)), Some(class)) if kind == class => speed,
            _ => 1.0,
        };
        Some(base / mult)
    }

    pub fn recipes_for(&self, item: ItemId) -> Vec<&RecipeDef> {
        self.recipes.iter().filter(|r| r.output == item).collect()
    }

    pub fn smelts_for(&self, item: ItemId) -> Vec<&SmeltDef> {
        self.smelts.iter().filter(|s| s.output == item).collect()
    }

    /// (recipes using it, smelts using it, is-a-fuel)
    pub fn uses_of(&self, item: ItemId) -> (Vec<&RecipeDef>, Vec<&SmeltDef>, bool) {
        let r = self
            .recipes
            .iter()
            .filter(|r| r.pattern.iter().flatten().any(|i| i.matches(item)))
            .collect();
        let s = self.smelts.iter().filter(|s| s.input.matches(item)).collect();
        let f = self.fuels.iter().any(|(i, _)| i.matches(item));
        (r, s, f)
    }

    pub fn smelt_for(&self, item: ItemId) -> Option<&SmeltDef> {
        self.smelts.iter().find(|s| s.input.matches(item))
    }

    pub fn fuel_value(&self, item: ItemId) -> Option<f32> {
        self.fuels.iter().find(|(f, _)| f.matches(item)).map(|(_, b)| *b)
    }

    /// Drop for breaking `block` with `held` (requires_tool gating).
    pub fn drops_for(&self, block: BlockId, held: Option<ItemId>) -> Option<(ItemId, u32)> {
        let d = self.block(block);
        if d.requires_tool {
            let ok = match (held.and_then(|i| self.item(i).tool), d.tool) {
                (Some((kind, _, tier)), Some(class)) => kind == class && tier >= d.min_tier,
                _ => false,
            };
            if !ok {
                return None;
            }
        }
        d.drops
    }
}

// ---------------- TOML schema ----------------

#[derive(Deserialize)]
struct ModToml {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    depends: Vec<String>,
}

#[derive(Deserialize, Clone)]
#[serde(untagged)]
enum TexSpec {
    One(String),
    Faces { top: String, side: String, #[serde(default)] bottom: Option<String> },
}

#[derive(Deserialize, Clone)]
struct BlockToml {
    id: String,
    name: Option<String>,
    texture: TexSpec,
    #[serde(default)]
    hardness: Option<f32>,
    #[serde(default)]
    unbreakable: bool,
    #[serde(default)]
    tool: Option<ToolKind>,
    #[serde(default)]
    requires_tool: bool,
    /// "self" (default), "none", or an item id.
    #[serde(default)]
    drops: Option<String>,
    #[serde(default)]
    drop_count: Option<u32>,
    #[serde(default = "yes")]
    solid: bool,
    #[serde(default = "yes")]
    opaque: bool,
    #[serde(default)]
    interaction: Option<String>,
    #[serde(default)]
    min_tier: u8,
    #[serde(default)]
    water: Option<u8>,
    #[serde(default)]
    cross: bool,
    #[serde(default)]
    crop: Option<CropToml>,
    #[serde(default)]
    harvest: Option<HarvestToml>,
    #[serde(default)]
    icon: Option<String>,
    /// Register an item form for placing (default true).
    #[serde(default = "yes")]
    item: bool,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize, Clone)]
struct CropToml {
    stages: u8,
    #[serde(default)]
    next_chance: Option<f32>,
    /// Texture per stage (else the block texture is reused).
    #[serde(default)]
    stage_textures: Vec<String>,
    #[serde(default)]
    any_soil: bool,
}

#[derive(Deserialize, Clone)]
struct HarvestToml {
    item: String,
    #[serde(default)]
    count: Option<u32>,
    becomes: String,
}

#[derive(Deserialize, Clone)]
struct FoodToml {
    hunger: f32,
    #[serde(default)]
    eat_time: Option<f32>,
    #[serde(default)]
    nutrition: HashMap<String, f32>,
}

#[derive(Deserialize, Clone)]
struct ItemToml {
    id: String,
    name: Option<String>,
    texture: String,
    #[serde(default)]
    max_stack: Option<u32>,
    #[serde(default)]
    tool: Option<ToolKind>,
    #[serde(default)]
    tool_speed: Option<f32>,
    #[serde(default)]
    tool_tier: Option<u8>,
    #[serde(default)]
    durability: Option<u32>,
    #[serde(default)]
    food: Option<FoodToml>,
    #[serde(default)]
    places: Option<String>,
    #[serde(default)]
    damage: Option<f32>,
}

#[derive(Deserialize, Clone)]
struct BoxToml {
    size: [f32; 3],
    at: [f32; 3],
    #[serde(default)]
    tex: Option<String>,
}

#[derive(Deserialize, Clone)]
struct AnimalDropToml {
    item: String,
    #[serde(default)]
    min: Option<u32>,
    #[serde(default)]
    max: Option<u32>,
}

#[derive(Deserialize, Clone)]
struct AnimalToml {
    id: String,
    #[serde(default)]
    name: Option<String>,
    biomes: Vec<String>,
    #[serde(default)]
    health: Option<f32>,
    #[serde(default)]
    speed: Option<f32>,
    #[serde(default)]
    flee_range: Option<f32>,
    #[serde(default)]
    group: Option<[u32; 2]>,
    #[serde(default)]
    rarity: Option<u32>,
    tex: String,
    #[serde(default)]
    head_tex: Option<String>,
    #[serde(default)]
    sound_pitch: Option<f32>,
    #[serde(default)]
    drops: Vec<AnimalDropToml>,
    #[serde(default)]
    model: HashMap<String, BoxToml>,
}

#[derive(Deserialize, Clone)]
struct RecipeToml {
    pattern: Vec<String>,
    #[serde(default)]
    keys: HashMap<String, String>,
    output: String,
    #[serde(default)]
    count: Option<u32>,
}

#[derive(Deserialize, Clone)]
struct SmeltToml {
    input: String,
    output: String,
    #[serde(default)]
    time: Option<f32>,
}

#[derive(Deserialize, Clone)]
struct FuelToml {
    item: String,
    burn: f32,
}

#[derive(Deserialize, Clone)]
struct AliasToml {
    old: String,
    new: String,
}

#[derive(Deserialize, Clone)]
struct TagToml {
    id: String,
    items: Vec<String>,
}

#[derive(Deserialize, Clone)]
struct FeatureToml {
    r#type: String,
    block: String,
    #[serde(default)]
    replaces: Option<String>,
    #[serde(default)]
    vein_size: Option<u32>,
    #[serde(default)]
    per_chunk: Option<u32>,
    #[serde(default)]
    y_range: Option<[i32; 2]>,
}

#[derive(Deserialize, Default)]
struct BlocksFile {
    #[serde(default)]
    block: Vec<BlockToml>,
}
#[derive(Deserialize, Default)]
struct ItemsFile {
    #[serde(default)]
    item: Vec<ItemToml>,
}
#[derive(Deserialize, Default)]
struct RecipesFile {
    #[serde(default)]
    recipe: Vec<RecipeToml>,
    #[serde(default)]
    smelt: Vec<SmeltToml>,
    #[serde(default)]
    fuel: Vec<FuelToml>,
}
#[derive(Deserialize, Default)]
struct AliasesFile {
    #[serde(default)]
    alias: Vec<AliasToml>,
}
#[derive(Deserialize, Default)]
struct FeaturesFile {
    #[serde(default)]
    feature: Vec<FeatureToml>,
}
#[derive(Deserialize, Default)]
struct TagsFile {
    #[serde(default)]
    tag: Vec<TagToml>,
}
#[derive(Deserialize, Default)]
struct AnimalsFile {
    #[serde(default)]
    animal: Vec<AnimalToml>,
}

struct RawMod {
    info: ModInfo,
    depends: Vec<String>,
    blocks: Vec<BlockToml>,
    items: Vec<ItemToml>,
    recipes: Vec<RecipeToml>,
    smelts: Vec<SmeltToml>,
    fuels: Vec<FuelToml>,
    features: Vec<FeatureToml>,
    tags: Vec<TagToml>,
    aliases: Vec<AliasToml>,
    animals: Vec<AnimalToml>,
}

// ---------------- loading ----------------

const BASE_BLOCKS: &str = include_str!("../base/blocks.toml");
const BASE_ITEMS: &str = include_str!("../base/items.toml");
const BASE_RECIPES: &str = include_str!("../base/recipes.toml");
const BASE_TAGS: &str = include_str!("../base/tags.toml");
const BASE_FEATURES: &str = include_str!("../base/features.toml");
const BASE_ALIASES: &str = include_str!("../base/aliases.toml");
const BASE_ANIMALS: &str = include_str!("../base/animals.toml");

fn parse_mod_dir(dir: &Path) -> Result<RawMod, String> {
    let manifest = std::fs::read_to_string(dir.join("mod.toml"))
        .map_err(|e| format!("mod.toml: {e}"))?;
    let m: ModToml = toml::from_str(&manifest).map_err(|e| format!("mod.toml: {e}"))?;
    let read = |f: &str| std::fs::read_to_string(dir.join(f)).unwrap_or_default();
    let blocks: BlocksFile =
        toml::from_str(&read("blocks.toml")).map_err(|e| format!("blocks.toml: {e}"))?;
    let items: ItemsFile =
        toml::from_str(&read("items.toml")).map_err(|e| format!("items.toml: {e}"))?;
    let recipes: RecipesFile =
        toml::from_str(&read("recipes.toml")).map_err(|e| format!("recipes.toml: {e}"))?;
    let features: FeaturesFile =
        toml::from_str(&read("features.toml")).map_err(|e| format!("features.toml: {e}"))?;
    let tags: TagsFile =
        toml::from_str(&read("tags.toml")).map_err(|e| format!("tags.toml: {e}"))?;
    let aliases: AliasesFile =
        toml::from_str(&read("aliases.toml")).map_err(|e| format!("aliases.toml: {e}"))?;
    let animals: AnimalsFile =
        toml::from_str(&read("animals.toml")).map_err(|e| format!("animals.toml: {e}"))?;
    let has_script = dir.join("main.rhai").exists();
    Ok(RawMod {
        info: ModInfo {
            id: m.id.clone(),
            name: m.name.unwrap_or(m.id),
            version: m.version.unwrap_or_else(|| "0.0.0".into()),
            path: Some(dir.to_path_buf()),
            has_script,
            error: None,
        },
        depends: m.depends,
        blocks: blocks.block,
        items: items.item,
        smelts: recipes.smelt.clone(),
        fuels: recipes.fuel.clone(),
        recipes: recipes.recipe,
        features: features.feature,
        tags: tags.tag,
        aliases: aliases.alias,
        animals: animals.animal,
    })
}

fn base_mod() -> RawMod {
    let blocks: BlocksFile = toml::from_str(BASE_BLOCKS).expect("base blocks.toml");
    let items: ItemsFile = toml::from_str(BASE_ITEMS).expect("base items.toml");
    let recipes: RecipesFile = toml::from_str(BASE_RECIPES).expect("base recipes.toml");
    let tags: TagsFile = toml::from_str(BASE_TAGS).expect("base tags.toml");
    let features: FeaturesFile = toml::from_str(BASE_FEATURES).expect("base features.toml");
    let aliases: AliasesFile = toml::from_str(BASE_ALIASES).expect("base aliases.toml");
    let animals: AnimalsFile = toml::from_str(BASE_ANIMALS).expect("base animals.toml");
    RawMod {
        info: ModInfo {
            id: "base".into(),
            name: "Wildforge".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            path: None,
            has_script: false,
            error: None,
        },
        depends: vec![],
        blocks: blocks.block,
        items: items.item,
        smelts: recipes.smelt.clone(),
        fuels: recipes.fuel.clone(),
        recipes: recipes.recipe,
        features: features.feature,
        tags: tags.tag,
        aliases: aliases.alias,
        animals: animals.animal,
    }
}

/// Load base + all mods under `mods_dir` into a fresh registry.
/// Individual bad mods are skipped with their error recorded.
pub fn load(mods_dir: &Path) -> Registry {
    let mut raws = vec![base_mod()];
    let mut failed: Vec<ModInfo> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(mods_dir) {
        let mut dirs: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir() && p.join("mod.toml").exists())
            .collect();
        dirs.sort();
        for dir in dirs {
            match parse_mod_dir(&dir) {
                Ok(r) => raws.push(r),
                Err(e) => failed.push(ModInfo {
                    id: dir.file_name().unwrap_or_default().to_string_lossy().into(),
                    name: String::new(),
                    version: String::new(),
                    path: Some(dir),
                    has_script: false,
                    error: Some(e),
                }),
            }
        }
    }

    // Topological order by depends (base first; unknown deps = load error).
    let ids: Vec<String> = raws.iter().map(|r| r.info.id.clone()).collect();
    let mut order: Vec<usize> = Vec::new();
    let mut placed = vec![false; raws.len()];
    for _ in 0..raws.len() {
        let mut progressed = false;
        for i in 0..raws.len() {
            if placed[i] {
                continue;
            }
            let ok = raws[i].depends.iter().all(|d| {
                ids.iter()
                    .enumerate()
                    .any(|(j, id)| id == d && placed[j])
                    || d == &raws[i].info.id
            });
            if ok {
                placed[i] = true;
                order.push(i);
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    for i in 0..raws.len() {
        if !placed[i] {
            let mut info = raws[i].info.clone();
            info.error = Some(format!(
                "unresolved or cyclic dependencies: {:?}",
                raws[i].depends
            ));
            failed.push(info);
        }
    }

    build(order.into_iter().map(|i| raws.remove_stable(i)).collect(), failed)
}

trait RemoveStable {
    fn remove_stable(&mut self, idx: usize) -> RawMod;
}
impl RemoveStable for Vec<RawMod> {
    fn remove_stable(&mut self, idx: usize) -> RawMod {
        // Order indices refer to the original vec; replace with tombstones.
        let dummy = RawMod {
            info: ModInfo {
                id: String::new(),
                name: String::new(),
                version: String::new(),
                path: None,
                has_script: false,
                error: None,
            },
            depends: vec![],
            blocks: vec![],
            items: vec![],
            animals: vec![],
            recipes: vec![],
            smelts: vec![],
            fuels: vec![],
            features: vec![],
            tags: vec![],
            aliases: vec![],
        };
        std::mem::replace(&mut self[idx], dummy)
    }
}

fn build(raws: Vec<RawMod>, mut failed: Vec<ModInfo>) -> Registry {
    let mut reg = Registry {
        blocks: Vec::new(),
        items: Vec::new(),
        recipes: Vec::new(),
        ores: Vec::new(),
        block_by_name: HashMap::new(),
        item_by_name: HashMap::new(),
        water_ids: [AIR; 8],
        unknown_block: AIR,
        mods: Vec::new(),
        smelts: Vec::new(),
        fuels: Vec::new(),
        tags: HashMap::new(),
        tex_files: Vec::new(),
        tex_names: Vec::new(),
        animals: Vec::new(),
    };
    let mut tex_slots: HashMap<String, u16> = crate::atlas::builtin_slots();
    let mut next_slot: u16 = crate::atlas::FIRST_FREE_SLOT;

    // Air (id 0) and the unknown-block placeholder are engine-registered.
    let air = BlockDef {
        name: "base:air".into(),
        label: "Air".into(),
        tiles: [0; 6],
        hardness: None,
        tool: None,
        requires_tool: false,
        drops: None,
        solid: false,
        opaque: false,
        interaction: None,
        min_tier: 0,
        water_level: None,
        cross: false,
        crop_next: None,
        crop_chance: 0.0,
        crop_any_soil: false,
        harvest: None,
    };
    reg.block_by_name.insert(air.name.clone(), BlockId(0));
    reg.blocks.push(air);

    let mut resolve_tex = |spec: &str, mod_path: &Option<PathBuf>, errs: &mut Vec<String>| -> u16 {
        if let Some(name) = spec.strip_prefix('@') {
            return *tex_slots.get(name).unwrap_or_else(|| {
                errs.push(format!("unknown builtin texture @{name}"));
                &crate::atlas::UNKNOWN_SLOT
            });
        }
        let key = format!(
            "{}/{}",
            mod_path.as_deref().map(|p| p.display().to_string()).unwrap_or_default(),
            spec
        );
        if let Some(s) = tex_slots.get(&key) {
            return *s;
        }
        let Some(dir) = mod_path else {
            errs.push(format!("texture {spec} needs a mod directory"));
            return crate::atlas::UNKNOWN_SLOT;
        };
        let path = dir.join("textures").join(spec);
        if !path.exists() {
            errs.push(format!("missing texture {spec}"));
            return crate::atlas::UNKNOWN_SLOT;
        }
        if next_slot >= 256 {
            errs.push("texture atlas full (256 tiles)".into());
            return crate::atlas::UNKNOWN_SLOT;
        }
        let slot = next_slot;
        next_slot += 1;
        tex_slots.insert(key, slot);
        let mod_id = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let stem = spec.strip_suffix(".png").unwrap_or(spec);
        reg.tex_names.push((format!("{mod_id}/{stem}"), slot));
        reg.tex_files.push((slot, path));
        slot
    };

    // Pass 1: register blocks and items (unresolved drops/recipes yet).
    struct PendingDrop {
        block: usize,
        rule: String,
        count: u32,
    }
    let mut pending_drops: Vec<PendingDrop> = Vec::new();
    let mut pending_recipes: Vec<(String, RecipeToml)> = Vec::new();
    let mut pending_features: Vec<(String, FeatureToml)> = Vec::new();
    let mut pending_tags: Vec<(String, TagToml)> = Vec::new();
    let mut pending_smelts: Vec<(String, SmeltToml)> = Vec::new();
    let mut pending_fuels: Vec<(String, FuelToml)> = Vec::new();
    let mut pending_aliases: Vec<(String, AliasToml)> = Vec::new();
    let mut pending_harvests: Vec<(String, BlockId, HarvestToml)> = Vec::new();
    let mut pending_places: Vec<(String, (String, String))> = Vec::new();
    // (mod id, toml, body tile, head tile, per-box tiles) — resolve in pass 1.
    let mut pending_animals: Vec<(String, AnimalToml, u16, u16, HashMap<String, u16>)> =
        Vec::new();

    for raw in &raws {
        if raw.info.id.is_empty() {
            continue; // tombstone
        }
        let mut errs: Vec<String> = Vec::new();
        for b in &raw.blocks {
            let full = qualify(&raw.info.id, &b.id);
            if reg.block_by_name.contains_key(&full) {
                errs.push(format!("duplicate block {full}"));
                continue;
            }
            let tiles = match &b.texture {
                TexSpec::One(t) => [resolve_tex(t, &raw.info.path, &mut errs); 6],
                TexSpec::Faces { top, side, bottom } => {
                    let t = resolve_tex(top, &raw.info.path, &mut errs);
                    let s = resolve_tex(side, &raw.info.path, &mut errs);
                    let bo = bottom
                        .as_ref()
                        .map(|x| resolve_tex(x, &raw.info.path, &mut errs))
                        .unwrap_or(t);
                    [s, s, t, bo, s, s]
                }
            };
            let id = BlockId(reg.blocks.len() as u16);
            let is_fluid = b.water.is_some();
            reg.blocks.push(BlockDef {
                name: full.clone(),
                label: b.name.clone().unwrap_or_else(|| b.id.clone()),
                tiles,
                hardness: if b.unbreakable || is_fluid { None } else { b.hardness.or(Some(1.0)) },
                tool: b.tool,
                requires_tool: b.requires_tool,
                drops: None,
                solid: b.solid && !is_fluid,
                opaque: b.opaque && !is_fluid,
                interaction: b.interaction.clone(),
                min_tier: b.min_tier,
                water_level: b.water,
                cross: b.cross,
                crop_next: None,
                crop_chance: 0.0,
                crop_any_soil: b.crop.as_ref().is_some_and(|c| c.any_soil),
                harvest: None,
            });
            reg.block_by_name.insert(full.clone(), id);
            pending_drops.push(PendingDrop {
                block: id.0 as usize,
                rule: b.drops.clone().unwrap_or_else(|| {
                    if is_fluid { "none".into() } else { "self".into() }
                }),
                count: b.drop_count.unwrap_or(1),
            });
            if let Some(crop) = &b.crop {
                // Auto-register growth stages; each links to the next.
                let mut prev = id;
                for st in 1..crop.stages {
                    let sid = BlockId(reg.blocks.len() as u16);
                    let mut def = reg.blocks[id.0 as usize].clone();
                    def.name = format!("{full}/stage{st}");
                    if let Some(t) = crop.stage_textures.get(st as usize - 1) {
                        let s = resolve_tex(t, &raw.info.path, &mut errs);
                        def.tiles = [s; 6];
                    }
                    reg.block_by_name.insert(def.name.clone(), sid);
                    reg.blocks.push(def);
                    reg.blocks[prev.0 as usize].crop_next = Some(sid);
                    reg.blocks[prev.0 as usize].crop_chance =
                        crop.next_chance.unwrap_or(0.2);
                    prev = sid;
                }
                // The final stage grows no further (clones inherit the
                // base's link otherwise).
                reg.blocks[prev.0 as usize].crop_next = None;
                reg.blocks[prev.0 as usize].crop_chance = 0.0;
            }
            if let Some(h) = &b.harvest {
                // Harvest applies to the final growth stage (or the block
                // itself when it has no stages).
                let target = BlockId(reg.blocks.len() as u16 - 1);
                let target = if b.crop.is_some() { target } else { id };
                pending_harvests.push((raw.info.id.clone(), target, h.clone()));
            }
            if b.water == Some(0) {
                // Auto-register the 7 flowing variants.
                reg.water_ids[0] = id;
                for l in 1..=7u8 {
                    let fid = BlockId(reg.blocks.len() as u16);
                    let mut def = reg.blocks[id.0 as usize].clone();
                    def.name = format!("{full}/flow{l}");
                    def.water_level = Some(l);
                    reg.block_by_name.insert(def.name.clone(), fid);
                    reg.blocks.push(def);
                    reg.water_ids[l as usize] = fid;
                }
            }
            if b.item && !is_fluid {
                let icon_slot = b
                    .icon
                    .as_ref()
                    .map(|t| resolve_tex(t, &raw.info.path, &mut errs))
                    .unwrap_or(tiles[0]);
                let iid = ItemId(reg.items.len() as u16);
                reg.items.push(ItemDef {
                    name: full.clone(),
                    label: reg.blocks[id.0 as usize].label.clone(),
                    icon: icon_slot,
                    max_stack: 64,
                    tool: None,
                    durability: 0,
                    places: Some(id),
                    food: None,
                    damage: 1.0,
                });
                reg.item_by_name.insert(full, iid);
            }
        }
        for it in &raw.items {
            let full = qualify(&raw.info.id, &it.id);
            if reg.item_by_name.contains_key(&full) {
                errs.push(format!("duplicate item {full}"));
                continue;
            }
            let icon = resolve_tex(&it.texture, &raw.info.path, &mut errs);
            let tool = it.tool.map(|k| (k, it.tool_speed.unwrap_or(4.0), it.tool_tier.unwrap_or(1)));
            let iid = ItemId(reg.items.len() as u16);
            let food = it.food.as_ref().map(|f| {
                let mut n = [0.0f32; 5];
                for (k, v) in &f.nutrition {
                    if let Some(i) = NUTRIENTS.iter().position(|x| x == k) {
                        n[i] = *v;
                    }
                }
                FoodDef { hunger: f.hunger, eat_time: f.eat_time.unwrap_or(1.5), nutrition: n }
            });
            let damage = it.damage.unwrap_or(match tool {
                Some((ToolKind::Axe, _, _)) => 3.0,
                Some(_) => 2.0,
                None => 1.0,
            });
            reg.items.push(ItemDef {
                name: full.clone(),
                label: it.name.clone().unwrap_or_else(|| it.id.clone()),
                icon,
                max_stack: if tool.is_some() { 1 } else { it.max_stack.unwrap_or(64) },
                tool,
                durability: it.durability.unwrap_or(if tool.is_some() { 59 } else { 0 }),
                places: None,
                food,
                damage,
            });
            reg.item_by_name.insert(full, iid);
        }
        for r in &raw.recipes {
            pending_recipes.push((raw.info.id.clone(), r.clone()));
        }
        for f in &raw.features {
            pending_features.push((raw.info.id.clone(), f.clone()));
        }
        for it in &raw.items {
            if let Some(p) = &it.places {
                pending_places.push((raw.info.id.clone(), (it.id.clone(), p.clone())));
            }
        }
        for t in &raw.tags {
            pending_tags.push((raw.info.id.clone(), t.clone()));
        }
        for s in &raw.smelts {
            pending_smelts.push((raw.info.id.clone(), s.clone()));
        }
        for a in &raw.animals {
            let tile = resolve_tex(&a.tex, &raw.info.path, &mut errs);
            let head = a
                .head_tex
                .as_ref()
                .map(|t| resolve_tex(t, &raw.info.path, &mut errs))
                .unwrap_or(tile);
            let box_tiles: HashMap<String, u16> = a
                .model
                .iter()
                .filter_map(|(n, b)| {
                    b.tex.as_ref().map(|t| (n.clone(), resolve_tex(t, &raw.info.path, &mut errs)))
                })
                .collect();
            pending_animals.push((raw.info.id.clone(), a.clone(), tile, head, box_tiles));
        }
        for f in &raw.fuels {
            pending_fuels.push((raw.info.id.clone(), f.clone()));
        }
        for a in &raw.aliases {
            pending_aliases.push((raw.info.id.clone(), a.clone()));
        }
        let mut info = raw.info.clone();
        if !errs.is_empty() {
            info.error = Some(errs.join("; "));
        }
        reg.mods.push(info);
    }

    // The unknown-block placeholder.
    let unk = BlockId(reg.blocks.len() as u16);
    reg.blocks.push(BlockDef {
        name: "base:unknown".into(),
        label: "Unknown".into(),
        tiles: [crate::atlas::UNKNOWN_SLOT; 6],
        hardness: Some(0.5),
        tool: None,
        requires_tool: false,
        drops: None,
        solid: true,
        opaque: true,
        interaction: None,
        min_tier: 0,
        water_level: None,
        cross: false,
        crop_next: None,
        crop_chance: 0.0,
        crop_any_soil: false,
        harvest: None,
    });
    reg.block_by_name.insert("base:unknown".into(), unk);
    reg.unknown_block = unk;

    // Pass 2: resolve drops, recipes, features by name.
    let lookup_item = |reg: &Registry, modid: &str, name: &str| -> Option<ItemId> {
        reg.item_id(&qualify(modid, name)).or_else(|| reg.item_id(name))
    };
    // Tags first (recipes reference them). Multiple mods extend the same tag.
    for (modid, t) in pending_tags {
        let tag_name = qualify(&modid, &t.id);
        for item in &t.items {
            if let Some(id) = lookup_item(&reg, &modid, item) {
                let entry = reg.tags.entry(tag_name.clone()).or_default();
                if !entry.contains(&id) {
                    entry.push(id);
                }
            }
        }
    }
    for pd in pending_drops {
        let d = match pd.rule.as_str() {
            "none" => None,
            "self" => {
                let name = reg.blocks[pd.block].name.clone();
                reg.item_id(&name).map(|i| (i, pd.count))
            }
            other => reg.item_id(other).map(|i| (i, pd.count)),
        };
        reg.blocks[pd.block].drops = d;
    }
    // Ingredient helper shared by recipes/smelts/fuels.
    let resolve_ing = |reg: &Registry, modid: &str, name: &str| -> Option<Ingredient> {
        if let Some(tag) = name.strip_prefix('#') {
            reg.tags.get(&qualify(modid, tag)).filter(|l| !l.is_empty()).map(|l| Ingredient::Any(l.clone()))
        } else {
            lookup_item(reg, modid, name).map(Ingredient::One)
        }
    };
    for (modid, s) in pending_smelts {
        if let (Some(input), Some(output)) =
            (resolve_ing(&reg, &modid, &s.input), lookup_item(&reg, &modid, &s.output))
        {
            reg.smelts.push(SmeltDef { input, output, time: s.time.unwrap_or(8.0) });
        }
    }
    for (modid, f) in pending_fuels {
        if let Some(ing) = resolve_ing(&reg, &modid, &f.item) {
            reg.fuels.push((ing, f.burn));
        }
    }
    for (modid, a, tile, head_tile, box_tiles) in pending_animals {
        let full = qualify(&modid, &a.id);
        if reg.animals.iter().any(|x| x.name == full) {
            continue; // duplicate id — first wins, like blocks/items
        }
        let drops = a
            .drops
            .iter()
            .filter_map(|d| {
                lookup_item(&reg, &modid, &d.item)
                    .map(|i| (i, d.min.unwrap_or(1), d.max.unwrap_or(1)))
            })
            .collect();
        let mut model: Vec<ModelBox> = a
            .model
            .iter()
            .map(|(name, b)| ModelBox {
                name: name.clone(),
                size: b.size,
                at: b.at,
                tile: box_tiles.get(name).copied(),
            })
            .collect();
        if model.is_empty() {
            model = vec![
                ModelBox { name: "body".into(), size: [6.0, 6.0, 10.0], at: [0.0, 7.0, 0.0], tile: None },
                ModelBox { name: "head".into(), size: [4.0, 4.0, 4.0], at: [0.0, 11.0, -6.0], tile: None },
                ModelBox { name: "leg".into(), size: [2.0, 7.0, 2.0], at: [2.0, 0.0, 3.0], tile: None },
            ];
        }
        model.sort_by(|a, b| a.name.cmp(&b.name));
        let mut half_w = 0.2f32;
        let mut height = 0.4f32;
        for b in &model {
            half_w = half_w
                .max((b.at[0].abs() + b.size[0] / 2.0) / 16.0)
                .max((b.at[2].abs() + b.size[2] / 2.0) / 16.0);
            height = height.max((b.at[1] + b.size[1]) / 16.0);
        }
        reg.animals.push(AnimalDef {
            name: full,
            label: a.name.clone().unwrap_or_else(|| a.id.clone()),
            biomes: a.biomes.iter().map(|b| b.to_lowercase()).collect(),
            health: a.health.unwrap_or(8.0),
            speed: a.speed.unwrap_or(2.0),
            flee_range: a.flee_range.unwrap_or(6.0),
            group: a.group.unwrap_or([1, 2]),
            rarity: a.rarity.unwrap_or(6).max(1),
            tile,
            head_tile,
            sound_pitch: a.sound_pitch.unwrap_or(1.0),
            drops,
            model,
            half_w: half_w.min(0.45),
            height,
        });
    }
    for (modid, block, h) in pending_harvests {
        let becomes = reg
            .block_id(&qualify(&modid, &h.becomes))
            .or_else(|| reg.block_id(&h.becomes));
        let item = lookup_item(&reg, &modid, &h.item);
        if let (Some(item), Some(becomes)) = (item, becomes) {
            reg.blocks[block.0 as usize].harvest = Some((item, h.count.unwrap_or(2), becomes));
        }
    }
    // Aliases: old name -> already-registered new id (lossless renames).
    for (modid, a) in pending_aliases {
        let new = qualify(&modid, &a.new);
        if let Some(id) = reg.block_by_name.get(&new).copied() {
            reg.block_by_name.entry(a.old.clone()).or_insert(id);
        }
        if let Some(id) = reg.item_by_name.get(&new).copied() {
            reg.item_by_name.entry(a.old.clone()).or_insert(id);
        }
    }
    // Item `places` links (food items that plant crops).
    for (modid, it_toml) in &pending_places {
        if let (Some(item), Some(block)) = (
            reg.item_id(&qualify(modid, &it_toml.0)),
            reg.block_id(&qualify(modid, &it_toml.1)).or_else(|| reg.block_id(&it_toml.1)),
        ) {
            reg.items[item.0 as usize].places = Some(block);
        }
    }
    for (modid, r) in pending_recipes {
        let h = r.pattern.len();
        let w = r.pattern.iter().map(|s| s.chars().count()).max().unwrap_or(0);
        if h == 0 || w == 0 || h > 3 || w > 3 {
            continue;
        }
        let mut pattern = vec![None; w * h];
        let mut ok = true;
        for (y, row) in r.pattern.iter().enumerate() {
            for (x, ch) in row.chars().enumerate() {
                if ch == '.' || ch == ' ' {
                    continue;
                }
                let key = ch.to_string();
                let Some(name) = r.keys.get(&key) else {
                    ok = false;
                    continue;
                };
                if let Some(tag) = name.strip_prefix('#') {
                    let tag_name = qualify(&modid, tag);
                    match reg.tags.get(&tag_name) {
                        Some(list) if !list.is_empty() => {
                            pattern[y * w + x] = Some(Ingredient::Any(list.clone()))
                        }
                        _ => ok = false,
                    }
                } else {
                    match lookup_item(&reg, &modid, name) {
                        Some(i) => pattern[y * w + x] = Some(Ingredient::One(i)),
                        None => ok = false,
                    }
                }
            }
        }
        let Some(out) = lookup_item(&reg, &modid, &r.output) else { continue };
        if ok {
            reg.recipes.push(RecipeDef { w, h, pattern, output: out, count: r.count.unwrap_or(1) });
        }
    }
    // Crop stages inherit their parent's drops (after drop resolution).
    for i in 0..reg.blocks.len() {
        if reg.blocks[i].name.contains("/stage") {
            let base = reg.blocks[i].name.split("/stage").next().unwrap().to_string();
            if let Some(pid) = reg.block_by_name.get(&base).copied() {
                reg.blocks[i].drops = reg.blocks[pid.0 as usize].drops;
            }
        }
    }
    for (modid, f) in pending_features {
        if f.r#type != "ore" {
            continue;
        }
        let lookup_block = |name: &str| {
            reg.block_id(&qualify(&modid, name)).or_else(|| reg.block_id(name))
        };
        let (Some(block), Some(replaces)) = (
            lookup_block(&f.block),
            lookup_block(f.replaces.as_deref().unwrap_or("base:stone")),
        ) else {
            continue;
        };
        let [y0, y1] = f.y_range.unwrap_or([4, 60]);
        reg.ores.push(OreFeature {
            block,
            replaces,
            vein_size: f.vein_size.unwrap_or(5).clamp(1, 32),
            per_chunk: f.per_chunk.unwrap_or(6).clamp(0, 64),
            y_min: y0,
            y_max: y1,
        });
    }

    reg.mods.extend(failed.drain(..));
    reg
}

fn qualify(modid: &str, name: &str) -> String {
    if name.contains(':') {
        name.to_string()
    } else {
        format!("{modid}:{name}")
    }
}

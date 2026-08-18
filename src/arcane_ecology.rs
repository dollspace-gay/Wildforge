//! Deterministic living Current layered over the finite planetary atlas.
//!
//! Site records are persisted inside `ArcaneGeography`'s dynamic container.
//! Their charge is therefore part of the geography custody account exactly
//! once: uptake moves cell Ambient/Dross into a site, release moves it back,
//! and harvest is the only operation which moves it out to an item owner.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::arcane::{BASE_RESONANCES, Current};
use crate::arcane_geography::{ArcaneControlCell, ArcaneDynamicCell, ArcaneGeography};
use crate::planet::{BlockPos, SurfacePos};
use crate::planet_atlas::{
    AtlasPos, BIOME_ARCTIC, BIOME_BADLANDS, BIOME_DESERT, BIOME_FOREST, BIOME_MOUNTAINS,
    BIOME_OCEAN, BIOME_SWAMP, BIOME_TAIGA, BIOME_TUNDRA, HABITAT_ALPINE, HABITAT_AQUATIC_BRACKISH,
    HABITAT_AQUATIC_FRESH, HABITAT_AQUATIC_SALT, HABITAT_BEACH_DUNE, HABITAT_CAVE_OUTLET,
    HABITAT_LAKESHORE, HABITAT_RIPARIAN, HABITAT_SALT_MARSH, HABITAT_SPRING, HABITAT_VOLCANIC_SOIL,
    HABITAT_WETLAND, PlanetAtlas,
};
use crate::registry::{
    ArcaneDisposition, ArcaneEcologyDef, ArcaneEcologyKind, EcologyHarvestClass, EcologyRole,
    EcologySource, Registry,
};

pub const ARCANE_ECOLOGY_VERSION: u32 = 1;
pub const ECOLOGY_MAX_SITES: usize = 65_536;

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ConfluenceHabitat {
    Mangrove,
    #[default]
    OldGrowth,
    GlassHeath,
    EmberGarden,
    NightOasis,
    AuroralLichen,
    SpringMarsh,
    CurrentReef,
    EchoGarden,
}

impl ConfluenceHabitat {
    pub const ALL: [Self; 9] = [
        Self::Mangrove,
        Self::OldGrowth,
        Self::GlassHeath,
        Self::EmberGarden,
        Self::NightOasis,
        Self::AuroralLichen,
        Self::SpringMarsh,
        Self::CurrentReef,
        Self::EchoGarden,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Mangrove => "tropical mangrove confluence",
            Self::OldGrowth => "temperate old-growth confluence",
            Self::GlassHeath => "boreal or alpine glass heath",
            Self::EmberGarden => "volcanic ember garden",
            Self::NightOasis => "desert night oasis",
            Self::AuroralLichen => "polar auroral lichen field",
            Self::SpringMarsh => "freshwater spring marsh",
            Self::CurrentReef => "marine current reef",
            Self::EchoGarden => "deep cave echo garden",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EcologyStage {
    Dormant,
    Establishing,
    #[default]
    Mature,
    Recovering,
    Collapsed,
    Harvested,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EcologyOwnership {
    #[default]
    Genesis,
    Retrogen,
    Cultivated,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EcologySite {
    pub id: u64,
    pub content_id: String,
    pub atlas_pos: AtlasPos,
    pub surface_u: u16,
    pub surface_v: u16,
    /// Zero until the authoritative chunk chooses the exact surface/cave Y.
    pub materialized_y: u8,
    pub variant: ConfluenceHabitat,
    pub ownership: EcologyOwnership,
    pub stage: EcologyStage,
    pub population: u16,
    pub carrying_capacity: u16,
    pub seed_bank: u16,
    pub crystal_stage: u8,
    pub charge: [u32; 6],
    /// Sequestered material remains dross even while tissue holds it.
    pub dross: [u32; 6],
    /// Nutrient mass moves among these three pools and never appears freely.
    pub soil_nutrients: u32,
    pub biomass_nutrients: u32,
    pub detritus_nutrients: u32,
    pub cumulative_water_hu: u64,
    pub last_harvest_day: u64,
    pub harvests: u32,
    pub collapses: u16,
    pub fire_history: u16,
    pub protected: bool,
}

impl EcologySite {
    pub fn charge_total(&self) -> u64 {
        self.charge.into_iter().map(u64::from).sum()
    }

    pub fn dross_total(&self) -> u64 {
        self.dross.into_iter().map(u64::from).sum()
    }

    pub fn nutrient_total(&self) -> u64 {
        u64::from(self.soil_nutrients)
            + u64::from(self.biomass_nutrients)
            + u64::from(self.detritus_nutrients)
    }

    pub fn surface(&self) -> Option<SurfacePos> {
        SurfacePos::new(self.atlas_pos.face, self.surface_u, self.surface_v).ok()
    }

    pub fn block_pos(&self) -> Option<BlockPos> {
        (self.materialized_y != 0).then(|| {
            BlockPos::new(
                self.atlas_pos.face,
                self.surface_u,
                self.materialized_y,
                self.surface_v,
            )
            .expect("persisted ecology site surface is canonical")
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArcaneEcologyState {
    pub version: u32,
    pub content_hash: u64,
    pub completed_days: u64,
    pub in_progress_day: u64,
    pub cursor: u32,
    pub next_cultivated_id: u64,
    pub sites: Vec<EcologySite>,
    pub retrogen_history: Vec<String>,
    pub event_sequence: u64,
    #[serde(default)]
    pub exported: [u64; 6],
}

impl Default for ArcaneEcologyState {
    fn default() -> Self {
        Self {
            version: ARCANE_ECOLOGY_VERSION,
            content_hash: 0,
            completed_days: 0,
            in_progress_day: 0,
            cursor: 0,
            next_cultivated_id: 1u64 << 63,
            sites: Vec::new(),
            retrogen_history: Vec::new(),
            event_sequence: 0,
            exported: [0; 6],
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EcologyAudit {
    pub sites: usize,
    pub natural_sites: usize,
    pub cultivated_sites: usize,
    pub collapsed_sites: usize,
    pub charge: [u64; 6],
    pub dross: [u64; 6],
    pub exported: [u64; 6],
    pub nutrients: u64,
    pub cumulative_water_hu: u64,
    pub roles: BTreeMap<EcologyRole, usize>,
    pub variants: BTreeMap<ConfluenceHabitat, usize>,
    pub checksum: u64,
}

impl EcologyAudit {
    pub fn render(&self) -> String {
        let mut text = format!(
            "Arcane ecology audit\nSites: {} (natural {}, cultivated {}, collapsed {})\nCharge: {}\nDross: {}\nNutrients: {}\nTranspired water: {} HU\nChecksum: {:016x}\n",
            self.sites,
            self.natural_sites,
            self.cultivated_sites,
            self.collapsed_sites,
            self.charge.iter().sum::<u64>(),
            self.dross.iter().sum::<u64>(),
            self.nutrients,
            self.cumulative_water_hu,
            self.checksum
        );
        for habitat in ConfluenceHabitat::ALL {
            text.push_str(&format!(
                "  {}: {}\n",
                habitat.label(),
                self.variants.get(&habitat).copied().unwrap_or(0)
            ));
        }
        text
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EcologyAdvance {
    pub processed: usize,
    pub completed_days: u64,
    pub transpiration: BTreeMap<AtlasPos, u64>,
    pub growth_events: u32,
    pub collapse_events: u32,
    /// Population lost specifically because environmental or internally
    /// sequestered dross exceeded the organism's declared tolerance. The
    /// world uses only these actual habitat injuries for Ire; a dross number
    /// changing by itself is never a grievance.
    pub dross_harm: BTreeMap<AtlasPos, u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EcologyHarvestPlan {
    pub site_id: u64,
    pub current: Current,
    pub charge: [u32; 6],
    pub dross_current: Current,
    pub dross: [u32; 6],
    pub protected: bool,
    pub destructive: bool,
    pub leaves_bud: bool,
    pub item_content: String,
    pub water_hu: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EcologyObservation {
    pub text: String,
    /// Nearby stabilizers damp the harmonic Current bed instead of merely
    /// making it quieter by an arbitrary biome flag.
    pub damped: bool,
}

fn wellglass_structure(charge: [u32; 6]) -> &'static str {
    let slot = charge
        .into_iter()
        .enumerate()
        .max_by_key(|(slot, units)| (*units, std::cmp::Reverse(*slot)))
        .map_or(3, |(slot, _)| slot);
    match BASE_RESONANCES[slot] {
        "base:root" => "carries dark veinlike roots that grip the host rock",
        "base:tide" => "holds clear inclusions that circulate when the air stills",
        "base:ember" => "opens fine warm cracks and closes them in a slow pulse",
        "base:stone" => "builds broad stepped laminae that sit almost unnaturally quiet",
        "base:gale" => "branches into needles that lean against the ordinary wind",
        "base:echo" => "answers small knocks through delayed concentric rings",
        _ => "has grown into a structure with no settled character",
    }
}

/// What an unaided observer can notice in the nearby causal ecology. The
/// wording is intentionally categorical: exact Current and dross quantities
/// remain operator/ledger data until the later tuning-lens goal.
pub fn observation_at(
    geography: &ArcaneGeography,
    registry: &Registry,
    surface: SurfacePos,
    radius: f32,
) -> Option<EcologyObservation> {
    let (distance, site) = geography
        .dynamic
        .ecology
        .sites
        .iter()
        .filter(|site| {
            !matches!(site.stage, EcologyStage::Harvested | EcologyStage::Dormant)
                && registry.arcane_ecology.contains_key(&site.content_id)
        })
        .filter_map(|site| {
            let at = site.surface()?;
            Some((
                crate::planet::geodesic_distance(surface.center(), at.center()),
                site,
            ))
        })
        .filter(|(distance, _)| *distance <= radius as f64)
        .min_by(|(a, site_a), (b, site_b)| {
            a.total_cmp(b).then_with(|| site_a.id.cmp(&site_b.id))
        })?;
    let definition = registry.arcane_ecology.get(&site.content_id)?;
    let charge_band =
        crate::arcane::qualitative_current(site.charge_total(), definition.charge_capacity.max(1));
    let burdened = site.dross_total() > u64::from(definition.dross_tolerance.max(1)) / 2;
    let nearby = if distance <= 18.0 {
        "Nearby"
    } else {
        "In the distance"
    };
    let wake = geography.dynamic.cells[site.atlas_pos.index(geography.manifest.side)].wake_id != 0;
    let mut text = match site.content_id.as_str() {
        "base:rainbell" if charge_band == "dormant" || charge_band == "faint" => {
            format!("{nearby}, rainbells fold shut; their dew has gone dull.")
        }
        "base:rainbell" => format!("{nearby}, rainbells cup a restrained, oil-bright dew."),
        "base:hushwood" => format!(
            "{nearby}, hushwood branches move out of rhythm with the wind, and sound falls away."
        ),
        "base:stormvine" if charge_band == "saturated" => {
            format!("{nearby}, stormvine tendrils answer one another with brief blue-white arcs.")
        }
        "base:stormvine" => format!("{nearby}, stormvine tendrils twitch toward the weather."),
        "base:cairnbloom" if wake => {
            format!("{nearby}, cairnblooms lean after a passage no ordinary wind has made.")
        }
        "base:cairnbloom" => format!(
            "{nearby}, cairnbloom faces hold one shared bearing instead of following the sun."
        ),
        "base:ashlace" if burdened => {
            format!("{nearby}, ashlace tissue has darkened under the dross it is binding.")
        }
        "base:ashlace" => format!("{nearby}, pale ashlace gathers along an old margin."),
        "base:pilgrim_root" if matches!(site.stage, EcologyStage::Collapsed) => {
            format!("{nearby}, pilgrim-root runners have withdrawn and left the old route bare.")
        }
        "base:pilgrim_root" => format!("{nearby}, pilgrim-root runners point along an old route."),
        "base:lantern_reed" if matches!(charge_band, "dormant" | "faint") => {
            format!("{nearby}, lantern reeds barely glow; their stored light is nearly spent.")
        }
        "base:lantern_reed" => format!("{nearby}, lantern reeds spend a low amber light."),
        "base:nightglass" if matches!(charge_band, "dormant" | "faint") => {
            format!("{nearby}, nightglass pods are sealed tight against the heat.")
        }
        "base:nightglass" => format!("{nearby}, nightglass pods show a cold sheen at their seams."),
        "base:frostlace" => {
            format!("{nearby}, frostlace holds an unnaturally still rim of rime against the stone.")
        }
        "base:tidekelp" => format!(
            "{nearby}, tidekelp fronds share an alignment the water alone does not explain."
        ),
        "base:echo_cap" if wake => {
            format!("{nearby}, new rings travel across echo caps after something passed here.")
        }
        "base:echo_cap" => format!("{nearby}, echo-cap rings pulse in a slow, uneven cadence."),
        "base:ember_poppy" if site.fire_history != 0 => {
            format!("{nearby}, ember poppies have opened in the exact reach of an old burn.")
        }
        "base:ember_poppy" => format!("{nearby}, ember poppies wait closed in fertile ash."),
        "base:wellglass_bud" if site.crystal_stage <= 1 => {
            format!("{nearby}, a wellglass bud has stopped at a cloudy, seedlike nub.")
        }
        "base:wellglass_bud" => {
            format!(
                "{nearby}, grown wellglass {}.",
                wellglass_structure(site.charge)
            )
        }
        _ => format!("{nearby}, the local growth holds a {charge_band} harmonic tension."),
    };
    let environmental_band =
        geography.dynamic.dross_state.cells[site.atlas_pos.index(geography.manifest.side)].band;
    if definition.source != EcologySource::Dross {
        match environmental_band {
            crate::dross::DrossBand::Strained => text.push_str(
                " Its leaves and stems repeat a small closing motion before settling again.",
            ),
            crate::dross::DrossBand::Seep => text.push_str(
                " Growth has stalled; the living edges lean away from a repeated dry pulse.",
            ),
            crate::dross::DrossBand::Scar | crate::dross::DrossBand::BreachRisk => text.push_str(
                " The growth is visibly malformed and dormant around broken-symmetry traces.",
            ),
            crate::dross::DrossBand::Clear | crate::dross::DrossBand::Trace => {}
        }
    }
    Some(EcologyObservation {
        text,
        damped: definition.roles.contains(&EcologyRole::Stabilizer) && site.charge_total() != 0,
    })
}

pub fn initialize_genesis(
    atlas: &PlanetAtlas,
    registry: &Registry,
    geography: &mut ArcaneGeography,
) -> Result<(), String> {
    if !geography.dynamic.ecology.sites.is_empty() {
        return validate(atlas, registry, geography);
    }
    let mut definitions = registry
        .blocks
        .iter()
        .filter_map(|block| {
            block
                .arcane_ecology
                .as_ref()
                .filter(|def| def.kind != ArcaneEcologyKind::FiniteMineral)
                .map(|def| (block.name.clone(), def.clone()))
        })
        .collect::<Vec<_>>();
    definitions.sort_by(|a, b| a.0.cmp(&b.0));
    let mut sites = Vec::new();
    let mut ids = BTreeSet::new();
    for (content_id, definition) in definitions {
        let mut candidates = (0..atlas.genesis.geometry.len())
            .filter_map(|index| {
                let pos = AtlasPos::from_index(index, atlas.side())?;
                habitat_suitable(atlas, geography, pos, &definition)
                    .then(|| (candidate_score(atlas, geography, pos, &definition), pos))
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|(score_a, pos_a), (score_b, pos_b)| {
            score_b.cmp(score_a).then_with(|| pos_a.cmp(pos_b))
        });
        let baseline_wanted = (candidates.len() / 768).clamp(4, 144).min(candidates.len());
        if baseline_wanted == 0 {
            continue;
        }
        // Preserve climate diversity before filling the remaining carrying
        // sites by score. With a small finite sample, pure score-stratified
        // selection repeatedly lost temperate old growth to wetter or colder
        // candidates even though valid forest habitat existed. Every chosen
        // position still passed the species' full physical predicates.
        let mut selected = Vec::new();
        let mut selected_positions = BTreeSet::new();
        for variant in ConfluenceHabitat::ALL {
            let matching = candidates
                .iter()
                .filter(|(_, pos)| confluence_variant(atlas, *pos) == variant)
                .collect::<Vec<_>>();
            if matching.is_empty() {
                continue;
            }
            let pick = mix64(
                u64::from(atlas.manifest.seed)
                    ^ hash_bytes(content_id.as_bytes())
                    ^ (variant as u64).rotate_left(29),
            ) as usize
                % matching.len();
            let pos = matching[pick].1;
            if selected_positions.insert(pos) {
                selected.push(pos);
            }
        }
        let wanted = baseline_wanted.max(selected.len()).min(candidates.len());
        // Spread selections throughout the score-ordered list; taking only
        // the top prefix would turn one unusually rich province into the
        // planet's sole source of a progression role.
        for ordinal in 0..candidates.len() {
            if selected.len() >= wanted {
                break;
            }
            let start = ordinal * candidates.len() / wanted;
            let start = start.min(candidates.len() - 1);
            let end = ((ordinal + 1) * candidates.len() / wanted)
                .max(start + 1)
                .min(candidates.len());
            let span = end - start;
            let pick = start
                + (mix64(
                    u64::from(atlas.manifest.seed)
                        ^ hash_bytes(content_id.as_bytes())
                        ^ ordinal as u64,
                ) as usize
                    % span);
            let pos = candidates[pick].1;
            if selected_positions.insert(pos) {
                selected.push(pos);
            }
        }
        if selected.len() < wanted {
            for (_, pos) in &candidates {
                if selected_positions.insert(*pos) {
                    selected.push(*pos);
                    if selected.len() == wanted {
                        break;
                    }
                }
            }
        }
        for pos in selected {
            let mut id = mix64(
                u64::from(atlas.manifest.seed)
                    ^ hash_bytes(content_id.as_bytes()).rotate_left(17)
                    ^ atlas_identity(pos),
            )
            .max(1);
            while !ids.insert(id) {
                id = mix64(id).max(1);
            }
            let surface = site_surface(atlas, pos, id);
            let population = definition.carrying_capacity.div_ceil(2).max(1);
            let nutrient_total = u32::from(
                atlas.genesis.ground.values()[pos.index(atlas.side())].baseline_fertility,
            )
            .saturating_mul(u32::from(definition.carrying_capacity).max(1))
            .saturating_mul(4)
            .max(u32::from(definition.nutrient_per_day).saturating_mul(32));
            let biomass = u32::from(definition.nutrient_per_day)
                .saturating_mul(u32::from(population))
                .saturating_mul(4)
                .min(nutrient_total / 2);
            let mut site = EcologySite {
                id,
                content_id: content_id.clone(),
                atlas_pos: pos,
                surface_u: surface.u(),
                surface_v: surface.v(),
                materialized_y: 0,
                variant: confluence_variant(atlas, pos),
                ownership: EcologyOwnership::Genesis,
                stage: if definition.kind == ArcaneEcologyKind::Crystal {
                    EcologyStage::Establishing
                } else {
                    EcologyStage::Mature
                },
                population,
                carrying_capacity: definition.carrying_capacity,
                seed_bank: population.saturating_mul(2),
                crystal_stage: u8::from(definition.kind == ArcaneEcologyKind::Crystal),
                charge: [0; 6],
                dross: [0; 6],
                soil_nutrients: nutrient_total - biomass,
                biomass_nutrients: biomass,
                detritus_nutrients: 0,
                cumulative_water_hu: 0,
                last_harvest_day: 0,
                harvests: 0,
                collapses: 0,
                fire_history: u16::from(
                    atlas.genesis.tectonics.values()[pos.index(atlas.side())].volcanic_history,
                ),
                protected: false,
            };
            let initial = (definition.charge_capacity / 8)
                .max(1)
                .min(u64::from(u16::MAX));
            let index = pos.index(atlas.side());
            match definition.source {
                EcologySource::Ambient | EcologySource::Heart => {
                    site.charge = take_weighted(
                        &mut geography.dynamic.cells[index].ambient,
                        &definition.resonance,
                        initial,
                    );
                }
                EcologySource::Dross => {
                    site.dross = take_weighted(
                        &mut geography.dynamic.cells[index].dross,
                        &definition.resonance,
                        initial,
                    );
                }
            }
            sites.push(site);
            if sites.len() >= ECOLOGY_MAX_SITES {
                return Err("magical ecology genesis exceeds its site bound".into());
            }
        }
    }
    geography.dynamic.ecology = ArcaneEcologyState {
        version: ARCANE_ECOLOGY_VERSION,
        content_hash: registry.content_hash,
        sites,
        ..ArcaneEcologyState::default()
    };
    validate(atlas, registry, geography)
}

/// Existing-world content reconciliation. Saved sites are never rerolled or
/// deleted. Newly introduced species receive deterministic candidate sites
/// with empty charge; untouched chunks may materialize them, while touched
/// terrain remains authoritative. Removed providers leave dormant named
/// records so reinstalling them restores the same populations.
pub fn reconcile_content(
    atlas: &PlanetAtlas,
    registry: &Registry,
    geography: &mut ArcaneGeography,
) -> Result<usize, String> {
    if geography.dynamic.ecology.sites.is_empty() {
        initialize_genesis(atlas, registry, geography)?;
        return Ok(geography.dynamic.ecology.sites.len());
    }
    if geography.dynamic.ecology.content_hash == registry.content_hash {
        return validate(atlas, registry, geography).map(|()| 0);
    }
    let known = geography
        .dynamic
        .ecology
        .sites
        .iter()
        .map(|site| site.content_id.clone())
        .collect::<BTreeSet<_>>();
    let wanted = registry
        .blocks
        .iter()
        .filter_map(|block| {
            block
                .arcane_ecology
                .as_ref()
                .filter(|definition| definition.kind != ArcaneEcologyKind::FiniteMineral)
                .map(|_| block.name.clone())
        })
        .filter(|content| !known.contains(content))
        .collect::<BTreeSet<_>>();
    if wanted.is_empty() {
        geography.dynamic.ecology.content_hash = registry.content_hash;
        return Ok(0);
    }
    let mut proposal = geography.clone();
    proposal.dynamic.ecology = ArcaneEcologyState::default();
    // Proposal charge stays private; new existing-world sites begin empty.
    initialize_genesis(atlas, registry, &mut proposal)?;
    let mut added = 0usize;
    let existing_ids = geography
        .dynamic
        .ecology
        .sites
        .iter()
        .map(|site| site.id)
        .collect::<BTreeSet<_>>();
    for mut site in proposal
        .dynamic
        .ecology
        .sites
        .into_iter()
        .filter(|site| wanted.contains(&site.content_id))
    {
        if existing_ids.contains(&site.id)
            || geography.dynamic.ecology.sites.len() >= ECOLOGY_MAX_SITES
        {
            continue;
        }
        site.ownership = EcologyOwnership::Retrogen;
        site.charge = [0; 6];
        site.dross = [0; 6];
        site.stage = EcologyStage::Establishing;
        geography.dynamic.ecology.sites.push(site);
        added += 1;
    }
    geography.dynamic.ecology.content_hash = registry.content_hash;
    geography.dynamic.ecology.retrogen_history.push(format!(
        "content {:016x}: {added} deterministic untouched candidates",
        registry.content_hash
    ));
    validate(atlas, registry, geography)?;
    Ok(added)
}

pub fn habitat_suitable(
    atlas: &PlanetAtlas,
    geography: &ArcaneGeography,
    pos: AtlasPos,
    definition: &ArcaneEcologyDef,
) -> bool {
    let index = pos.index(atlas.side());
    let control = geography.controls[index];
    let cell = geography.dynamic.cells[index];
    let stored = geography
        .dynamic
        .ecology
        .sites
        .iter()
        .filter(|site| site.atlas_pos == pos)
        .map(|site| site.charge_total().saturating_add(site.dross_total()))
        .sum::<u64>();
    let local_available = cell.ambient_total().saturating_add(stored);
    let richness = local_available
        .saturating_mul(1_000)
        .checked_div(u64::from(control.capacity).max(1))
        .unwrap_or_default()
        .min(1_000) as u16;
    if richness < definition.min_richness_permille
        || control.stability < definition.min_stability_permille
        || control.stability > definition.max_stability_permille
    {
        return false;
    }
    habitat_tags_suitable(atlas, geography, pos, definition, false)
}

/// Ordinary climate/geology/habitat compatibility. Existing natural sites
/// remain valid records when a changeable prerequisite such as a dross margin
/// disappears; succession can make them dormant or collapsed without turning
/// a save into corruption.
fn habitat_tags_suitable(
    atlas: &PlanetAtlas,
    geography: &ArcaneGeography,
    pos: AtlasPos,
    definition: &ArcaneEcologyDef,
    allow_dynamic_loss: bool,
) -> bool {
    let index = pos.index(atlas.side());
    let climate = atlas.genesis.climate.values()[index];
    let ground = atlas.genesis.ground.values()[index];
    let biome = atlas.genesis.biomes.values()[index];
    let terrain = atlas.genesis.terrain.values()[index];
    let hydro = atlas.genesis.hydrology.values()[index];
    let tectonic = atlas.genesis.tectonics.values()[index];
    let cell = geography.dynamic.cells[index];
    definition.habitat.iter().all(|tag| match tag.as_str() {
        "wetland" => biome.habitat_flags & HABITAT_WETLAND != 0,
        "freshwater_margin" => {
            biome.habitat_flags
                & (HABITAT_AQUATIC_FRESH
                    | HABITAT_RIPARIAN
                    | HABITAT_WETLAND
                    | HABITAT_SPRING
                    | HABITAT_LAKESHORE)
                != 0
        }
        "old_forest" => {
            matches!(biome.baseline_biome, BIOME_FOREST | BIOME_TAIGA)
                && biome.succession_potential >= 120
                && ground.organic >= 28
        }
        "cool_or_temperate" => (-8.0..=25.0).contains(&climate.mean_temperature),
        "warm_wet" => climate.mean_temperature >= 12.0 && climate.mean_precipitation >= 600.0,
        "storm_exposed" => {
            climate
                .seasonal_wind
                .iter()
                .map(|wind| wind[0].hypot(wind[1]))
                .fold(0.0f32, f32::max)
                >= 2.2
                || climate.precipitation_seasonality >= 0.15
                || climate.mean_precipitation >= 850.0
        }
        "exposed" => {
            biome.habitat_flags & HABITAT_ALPINE != 0
                || matches!(biome.baseline_biome, BIOME_BADLANDS | BIOME_MOUNTAINS)
                || biome.vegetation_potential < 100
        }
        "rocky_soil" => ground.soil_depth_decimeters <= 14 || ground.sand >= 120,
        "cave" => {
            biome.habitat_flags & HABITAT_CAVE_OUTLET != 0
                || ground.soil_depth_decimeters <= 5
                || tectonic.fault_intensity >= 420
        }
        "dross_margin" => {
            allow_dynamic_loss
                || cell.dross_total() != 0
                || geography.catalog.sites.iter().any(|site| {
                    site.center == pos
                        && matches!(
                            site.kind,
                            crate::arcane_geography::ArcanePlaceType::Scar
                                | crate::arcane_geography::ArcanePlaceType::Echo
                        )
                })
        }
        "heartshadow" => biome.heart_assignment != 0,
        "temperate_ground" => (-5.0..=28.0).contains(&climate.mean_temperature),
        "swamp" => {
            biome.baseline_biome == BIOME_SWAMP || biome.habitat_flags & HABITAT_WETLAND != 0
        }
        "arid_spring" => {
            climate.aridity >= 0.9
                && biome.habitat_flags & (HABITAT_SPRING | HABITAT_LAKESHORE | HABITAT_RIPARIAN)
                    != 0
        }
        "cool_night" => climate.seasonality >= 5.0 || climate.continentality >= 0.35,
        "permanent_cold" => {
            climate.mean_temperature <= 4.0
                || climate.snow_persistence >= 0.42
                || matches!(biome.baseline_biome, BIOME_ARCTIC | BIOME_TUNDRA)
        }
        "marine" => {
            biome.habitat_flags
                & (HABITAT_AQUATIC_BRACKISH
                    | HABITAT_AQUATIC_SALT
                    | HABITAT_BEACH_DUNE
                    | HABITAT_SALT_MARSH)
                != 0
                || biome.baseline_biome == BIOME_OCEAN
        }
        "nutrient_rich" => ground.baseline_fertility >= 70 || hydro.mean_discharge >= 16.0,
        "moist_cave" => {
            (biome.habitat_flags & HABITAT_CAVE_OUTLET != 0 || tectonic.fault_intensity >= 420)
                && (climate.mean_precipitation >= 380.0 || ground.aquifer_capacity >= 128)
        }
        "old_organic" => ground.organic >= 20 || biome.succession_potential >= 120,
        "fire_disturbed" => {
            tectonic.volcanic_history != 0 || biome.habitat_flags & HABITAT_VOLCANIC_SOIL != 0
        }
        "fertile" => ground.baseline_fertility >= 64,
        "subsurface" => terrain.eroded_elevation > 1.0,
        "host_rock" => tectonic.crust_thickness > 0 && biome.baseline_biome != BIOME_OCEAN,
        // Finite-mineral predicates are validated but do not reserve living
        // sites; the ordinary deposit manifest owns their locations.
        "metamorphic_host" => tectonic.metamorphic_grade >= 2,
        "evaporite_host" => ground.soil_salinity >= 48 || climate.aridity >= 1.0,
        "mafic_host" => tectonic.volcanic_history != 0,
        "sedimentary_host" => tectonic.sediment_basin != crate::planet_atlas::BasinKind::None,
        _ => false,
    })
}

pub fn confluence_variant(atlas: &PlanetAtlas, pos: AtlasPos) -> ConfluenceHabitat {
    let index = pos.index(atlas.side());
    let climate = atlas.genesis.climate.values()[index];
    let ground = atlas.genesis.ground.values()[index];
    let biome = atlas.genesis.biomes.values()[index];
    let tectonic = atlas.genesis.tectonics.values()[index];
    if biome.habitat_flags & (HABITAT_AQUATIC_BRACKISH | HABITAT_AQUATIC_SALT) != 0
        || biome.baseline_biome == BIOME_OCEAN
    {
        ConfluenceHabitat::CurrentReef
    } else if biome.habitat_flags & HABITAT_CAVE_OUTLET != 0
        || (tectonic.fault_intensity >= 520 && ground.soil_depth_decimeters < 5)
    {
        ConfluenceHabitat::EchoGarden
    } else if biome.habitat_flags & (HABITAT_SPRING | HABITAT_WETLAND | HABITAT_RIPARIAN) != 0 {
        if climate.mean_temperature >= 22.0 {
            ConfluenceHabitat::Mangrove
        } else {
            ConfluenceHabitat::SpringMarsh
        }
    } else if biome.habitat_flags & HABITAT_VOLCANIC_SOIL != 0 || tectonic.volcanic_history >= 2 {
        ConfluenceHabitat::EmberGarden
    } else if climate.aridity >= 1.0 || biome.baseline_biome == BIOME_DESERT {
        ConfluenceHabitat::NightOasis
    } else if climate.mean_temperature <= -3.0 || biome.baseline_biome == BIOME_ARCTIC {
        ConfluenceHabitat::AuroralLichen
    } else if biome.habitat_flags & HABITAT_ALPINE != 0
        || matches!(
            biome.baseline_biome,
            BIOME_TAIGA | BIOME_TUNDRA | BIOME_MOUNTAINS
        )
    {
        ConfluenceHabitat::GlassHeath
    } else {
        ConfluenceHabitat::OldGrowth
    }
}

/// Advance unloaded and loaded sites through the same coarse lifecycle.
/// `water_available` is a per-cell spendable snapshot; returned withdrawals
/// must be moved from soil to atmosphere by the authoritative water cycle.
pub struct EcologyConditions<'a> {
    pub water_available: &'a mut BTreeMap<AtlasPos, u64>,
    pub living_hearts: &'a BTreeSet<u16>,
    pub storming: &'a BTreeSet<AtlasPos>,
    pub blocked_crystal_sites: &'a BTreeSet<u64>,
}

impl<'a> EcologyConditions<'a> {
    pub fn new(
        water_available: &'a mut BTreeMap<AtlasPos, u64>,
        living_hearts: &'a BTreeSet<u16>,
        storming: &'a BTreeSet<AtlasPos>,
        blocked_crystal_sites: &'a BTreeSet<u64>,
    ) -> Self {
        Self {
            water_available,
            living_hearts,
            storming,
            blocked_crystal_sites,
        }
    }
}

pub fn advance_toward(
    geography: &mut ArcaneGeography,
    atlas: &PlanetAtlas,
    registry: &Registry,
    target_day: u64,
    budget: usize,
    conditions: EcologyConditions<'_>,
) -> Result<EcologyAdvance, String> {
    let state = &mut geography.dynamic.ecology;
    if state.sites.is_empty() || state.completed_days >= target_day {
        return Ok(EcologyAdvance {
            completed_days: state.completed_days,
            ..EcologyAdvance::default()
        });
    }
    if state.in_progress_day == 0 {
        state.in_progress_day = state.completed_days.saturating_add(1);
        state.cursor = 0;
    }
    let day = state.in_progress_day;
    let start = state.cursor as usize;
    let end = (start + budget.max(1)).min(state.sites.len());
    let mut report = EcologyAdvance::default();
    for site in &mut state.sites[start..end] {
        let Some(definition) = registry.arcane_ecology.get(&site.content_id) else {
            site.stage = EcologyStage::Dormant;
            continue;
        };
        let index = site.atlas_pos.index(atlas.side());
        let environmental_band = geography.dynamic.dross_state.cells[index].band;
        let dross_stalled = definition.source != EcologySource::Dross
            && environmental_band >= crate::dross::DrossBand::Seep;
        let dross_wilting = definition.source != EcologySource::Dross
            && definition.kind != ArcaneEcologyKind::FiniteMineral
            && environmental_band >= crate::dross::DrossBand::Scar;
        let season = crate::planet_atlas::local_season(
            day as u32,
            f64::from(atlas.genesis.geometry.values()[index].latitude_radians),
        );
        let suitable = habitat_suitable_parts(
            atlas,
            &geography.controls,
            &geography.dynamic.cells,
            site.atlas_pos,
            definition,
            site.charge_total(),
            site.dross_total(),
        );
        let heart_alive = atlas.genesis.biomes.values()[index].heart_assignment == 0
            || conditions
                .living_hearts
                .contains(&atlas.genesis.biomes.values()[index].heart_assignment);
        let seasonal = definition.seasons[season];
        let water_need = u64::from(definition.water_per_day_hu)
            .saturating_mul(u64::from(site.population.max(1)));
        let water = conditions
            .water_available
            .entry(site.atlas_pos)
            .or_default();
        let watered = water_need == 0 || *water >= water_need;
        mineralize_detritus(site);
        let attachment_free = definition.kind != ArcaneEcologyKind::Crystal
            || !conditions.blocked_crystal_sites.contains(&site.id);
        if suitable
            && seasonal
            && watered
            && attachment_free
            && site.seed_bank != 0
            && (definition.source != EcologySource::Heart || heart_alive)
            && !dross_stalled
        {
            if water_need != 0 {
                *water -= water_need;
                *report.transpiration.entry(site.atlas_pos).or_default() += water_need;
                site.cumulative_water_hu = site.cumulative_water_hu.saturating_add(water_need);
            }
            let uptake_rate = if site.content_id == "base:stormvine"
                && !conditions.storming.contains(&site.atlas_pos)
            {
                0
            } else {
                definition.uptake_per_day
            };
            let uptake = u64::from(uptake_rate)
                .saturating_mul(u64::from(site.population.max(1)))
                .min(
                    definition
                        .charge_capacity
                        .saturating_sub(site.charge_total()),
                );
            match definition.source {
                EcologySource::Ambient | EcologySource::Heart => {
                    let moved = take_weighted(
                        &mut geography.dynamic.cells[index].ambient,
                        &definition.resonance,
                        uptake,
                    );
                    add_bands(&mut site.charge, moved)?;
                }
                EcologySource::Dross => {
                    let capacity = u64::from(definition.dross_tolerance)
                        .saturating_mul(u64::from(site.population.max(1)));
                    let wanted = uptake.min(capacity.saturating_sub(site.dross_total()));
                    let moved = take_weighted(
                        &mut geography.dynamic.cells[index].dross,
                        &definition.resonance,
                        wanted,
                    );
                    add_bands(&mut site.dross, moved)?;
                }
            }
            release_charge(site, definition, &mut geography.dynamic.cells[index])?;
            if day.is_multiple_of(u64::from(definition.regrowth_days.max(1))) {
                if site.stage == EcologyStage::Collapsed && site.seed_bank != 0 {
                    site.stage = EcologyStage::Recovering;
                }
                if site.population < site.carrying_capacity && site.seed_bank != 0 {
                    let nutrients = u32::from(definition.nutrient_per_day)
                        .saturating_mul(u32::from(definition.regrowth_days.max(1)));
                    if site.soil_nutrients >= nutrients {
                        site.soil_nutrients -= nutrients;
                        site.biomass_nutrients = site.biomass_nutrients.saturating_add(nutrients);
                        site.population += 1;
                        site.seed_bank -= 1;
                        site.stage = if site.population >= site.carrying_capacity / 2 {
                            EcologyStage::Mature
                        } else {
                            EcologyStage::Recovering
                        };
                        if definition.kind == ArcaneEcologyKind::Crystal {
                            let stage_capacity = definition.charge_capacity
                                / u64::from(definition.crystal_stages.max(1));
                            let earned_stage = (site.charge_total() / stage_capacity.max(1)) as u8;
                            site.crystal_stage = earned_stage
                                .min(definition.crystal_stages.saturating_sub(1))
                                .max(1);
                        }
                        report.growth_events += 1;
                    }
                } else if site.population != 0 {
                    site.seed_bank = site
                        .seed_bank
                        .saturating_add(1)
                        .min(site.carrying_capacity.saturating_mul(3));
                }
            }
        } else if !dross_stalled
            && day.is_multiple_of(u64::from(definition.regrowth_days.max(1)))
            && site.population != 0
        {
            lose_population(site, definition);
            if site.population == 0 {
                collapse(site, definition, &mut geography.dynamic.cells[index])?;
                report.collapse_events += 1;
            }
        }
        if dross_wilting
            && day.is_multiple_of(u64::from(definition.regrowth_days.max(1)))
            && site.population != 0
        {
            lose_population(site, definition);
            *report.dross_harm.entry(site.atlas_pos).or_default() += 1;
            if site.population == 0 {
                collapse(site, definition, &mut geography.dynamic.cells[index])?;
                report.collapse_events += 1;
            }
        }
        if site.population != 0
            && site.dross_total()
                > u64::from(definition.dross_tolerance)
                    .saturating_mul(u64::from(site.population.max(1)))
        {
            lose_population(site, definition);
            *report.dross_harm.entry(site.atlas_pos).or_default() += 1;
            if site.population == 0 {
                collapse(site, definition, &mut geography.dynamic.cells[index])?;
                report.collapse_events += 1;
            }
        }
    }
    report.processed = end - start;
    state.cursor = end as u32;
    if end == state.sites.len() {
        state.completed_days = day;
        state.in_progress_day = 0;
        state.cursor = 0;
    }
    report.completed_days = state.completed_days;
    Ok(report)
}

/// Register a placed ecological block independently from genesis. The caller
/// has already performed the physical item->environment Current disposition;
/// cultivation begins empty and must establish from real local inputs.
pub fn register_cultivated(
    geography: &mut ArcaneGeography,
    atlas: &PlanetAtlas,
    content_id: &str,
    pos: BlockPos,
    definition: &ArcaneEcologyDef,
) -> Result<u64, String> {
    if definition.kind == ArcaneEcologyKind::FiniteMineral {
        return Err("finite geology cannot be cultivated".into());
    }
    if geography.dynamic.ecology.sites.len() >= ECOLOGY_MAX_SITES {
        return Err("ecology site bound reached".into());
    }
    if geography
        .dynamic
        .ecology
        .sites
        .iter()
        .any(|site| site.block_pos() == Some(pos) && site.stage != EcologyStage::Harvested)
    {
        return Err("an ecology site already owns this block".into());
    }
    let id = geography.dynamic.ecology.next_cultivated_id;
    geography.dynamic.ecology.next_cultivated_id = id
        .checked_add(1)
        .ok_or_else(|| "cultivated ecology id overflow".to_string())?;
    let atlas_pos = atlas.atlas_pos(pos.surface());
    let nutrient_total =
        u32::from(atlas.genesis.ground.values()[atlas_pos.index(atlas.side())].baseline_fertility)
            .saturating_mul(8)
            .max(u32::from(definition.nutrient_per_day).saturating_mul(8));
    geography.dynamic.ecology.sites.push(EcologySite {
        id,
        content_id: content_id.into(),
        atlas_pos,
        surface_u: pos.u(),
        surface_v: pos.v(),
        materialized_y: pos.y(),
        variant: confluence_variant(atlas, atlas_pos),
        ownership: EcologyOwnership::Cultivated,
        stage: EcologyStage::Establishing,
        population: 1,
        carrying_capacity: definition.carrying_capacity,
        seed_bank: 1,
        crystal_stage: u8::from(definition.kind == ArcaneEcologyKind::Crystal),
        charge: [0; 6],
        dross: [0; 6],
        soil_nutrients: nutrient_total,
        biomass_nutrients: 0,
        detritus_nutrients: 0,
        cumulative_water_hu: 0,
        last_harvest_day: 0,
        harvests: 0,
        collapses: 0,
        fire_history: 0,
        protected: false,
    });
    Ok(id)
}

pub fn plan_harvest(
    geography: &ArcaneGeography,
    registry: &Registry,
    pos: BlockPos,
    tool_tier: u8,
) -> Option<EcologyHarvestPlan> {
    let site = geography
        .dynamic
        .ecology
        .sites
        .iter()
        .find(|site| site.block_pos() == Some(pos) && site.stage == EcologyStage::Mature)?;
    let definition = registry.arcane_ecology.get(&site.content_id)?;
    if definition.kind == ArcaneEcologyKind::Crystal
        && site.crystal_stage < definition.crystal_stages.saturating_sub(1)
    {
        return None;
    }
    let divisor = match definition.harvest {
        EcologyHarvestClass::Fruit => 4,
        EcologyHarvestClass::Prune | EcologyHarvestClass::Spore => 3,
        EcologyHarvestClass::Coppice => 2,
        EcologyHarvestClass::SeedPreserving if definition.kind != ArcaneEcologyKind::Crystal => 3,
        EcologyHarvestClass::SeedPreserving | EcologyHarvestClass::Destructive => 1,
    };
    let charge = site.charge.map(|units| units / divisor);
    let current = bands_to_current(charge).ok()?;
    let dross = site.dross.map(|units| units / divisor);
    let dross_current = bands_to_current(dross).ok()?;
    let leaves_bud = definition.kind == ArcaneEcologyKind::Crystal
        && definition.harvest == EcologyHarvestClass::SeedPreserving
        && tool_tier >= definition.preserving_tool_tier;
    let item_content = registry
        .block(registry.block_by_name[&site.content_id])
        .drops
        .map(|(item, _)| registry.item(item).name.clone())?;
    Some(EcologyHarvestPlan {
        site_id: site.id,
        current,
        charge,
        dross_current,
        dross,
        protected: site.protected,
        destructive: matches!(definition.harvest, EcologyHarvestClass::Destructive)
            || (definition.kind == ArcaneEcologyKind::Crystal && !leaves_bud),
        leaves_bud,
        item_content,
        water_hu: if site.content_id == "base:rainbell" {
            crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL
        } else {
            0
        },
    })
}

/// Whether a persistent ecological site owns this materialized block even
/// when it is not currently mature enough to harvest. This lets callers
/// reject a second same-tick harvest instead of falling through to ordinary
/// block loot and manufacturing an unaccounted duplicate.
pub fn owns_materialized_block(geography: &ArcaneGeography, pos: BlockPos) -> bool {
    geography
        .dynamic
        .ecology
        .sites
        .iter()
        .any(|site| site.block_pos() == Some(pos) && site.stage != EcologyStage::Harvested)
}

pub fn plan_finite_mineral_harvest(
    geography: &ArcaneGeography,
    atlas: &PlanetAtlas,
    pos: BlockPos,
    definition: &ArcaneEcologyDef,
) -> Option<Current> {
    if definition.kind != ArcaneEcologyKind::FiniteMineral {
        return None;
    }
    let atlas_pos = atlas.atlas_pos(pos.surface());
    let mut available = geography.dynamic.cells[atlas_pos.index(atlas.side())].ambient;
    let bands = take_weighted(
        &mut available,
        &definition.resonance,
        (definition.charge_capacity / 4).max(1),
    );
    let current = bands_to_current(bands).ok()?;
    (!current.is_empty()).then_some(current)
}

pub fn apply_finite_mineral_harvest(
    geography: &mut ArcaneGeography,
    atlas: &PlanetAtlas,
    pos: BlockPos,
    current: &Current,
) -> Result<(), String> {
    let atlas_pos = atlas.atlas_pos(pos.surface());
    let cell = &mut geography.dynamic.cells[atlas_pos.index(atlas.side())];
    for (name, units) in current.parts() {
        let slot = BASE_RESONANCES
            .iter()
            .position(|candidate| *candidate == name)
            .ok_or_else(|| format!("finite mineral named non-geographic resonance {name}"))?;
        let units = u16::try_from(*units)
            .map_err(|_| "finite mineral charge exceeds one cell band".to_string())?;
        cell.ambient[slot] = cell.ambient[slot]
            .checked_sub(units)
            .ok_or_else(|| "finite mineral charge was already taken".to_string())?;
    }
    let _ = cell;
    for (name, units) in current.parts() {
        let slot = BASE_RESONANCES
            .iter()
            .position(|candidate| *candidate == name)
            .expect("validated finite mineral resonance slot");
        crate::arcane_geography::record_geography_export(
            &mut geography.dynamic.ecology.exported,
            &mut geography.dynamic.dross_state.external_imported,
            slot,
            *units,
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Apply only after the ledger has accepted the exact Geography->Item move.
pub fn apply_harvest(
    geography: &mut ArcaneGeography,
    registry: &Registry,
    plan: &EcologyHarvestPlan,
    day: u64,
) -> Result<(), String> {
    let site = geography
        .dynamic
        .ecology
        .sites
        .iter_mut()
        .find(|site| site.id == plan.site_id)
        .ok_or_else(|| "harvest site disappeared".to_string())?;
    let definition = registry
        .arcane_ecology
        .get(&site.content_id)
        .ok_or_else(|| "harvest content disappeared".to_string())?;
    for (stored, taken) in site.charge.iter_mut().zip(plan.charge) {
        *stored = stored
            .checked_sub(taken)
            .ok_or_else(|| "harvest charge was already taken".to_string())?;
    }
    for (stored, taken) in site.dross.iter_mut().zip(plan.dross) {
        *stored = stored
            .checked_sub(taken)
            .ok_or_else(|| "harvest dross was already taken".to_string())?;
    }
    site.harvests = site.harvests.saturating_add(1);
    site.last_harvest_day = day;
    if plan.destructive {
        lose_population(site, definition);
        site.seed_bank = site.seed_bank.saturating_sub(2);
    } else {
        site.seed_bank = site.seed_bank.saturating_sub(1);
    }
    if definition.kind == ArcaneEcologyKind::Crystal {
        if plan.leaves_bud {
            site.crystal_stage = 1;
            site.population = 1;
            site.stage = EcologyStage::Recovering;
        } else {
            site.crystal_stage = 0;
            site.population = 0;
            site.seed_bank = 0;
            site.stage = EcologyStage::Harvested;
        }
    } else if site.population == 0 || site.seed_bank == 0 {
        site.stage = EcologyStage::Collapsed;
        site.collapses = site.collapses.saturating_add(1);
    } else {
        site.stage = EcologyStage::Recovering;
    }
    for (slot, (taken, dross)) in plan.charge.into_iter().zip(plan.dross).enumerate() {
        crate::arcane_geography::record_geography_export(
            &mut geography.dynamic.ecology.exported,
            &mut geography.dynamic.dross_state.external_imported,
            slot,
            u64::from(taken).saturating_add(u64::from(dross)),
        )
        .map_err(|error| error.to_string())?;
    }
    geography.dynamic.ecology.event_sequence = geography
        .dynamic
        .ecology
        .event_sequence
        .checked_add(1)
        .ok_or_else(|| "ecology event sequence overflow".to_string())?;
    Ok(())
}

pub fn apply_destructive_loss(
    geography: &mut ArcaneGeography,
    registry: &Registry,
    pos: BlockPos,
) -> Result<bool, String> {
    let Some(site_index) =
        geography.dynamic.ecology.sites.iter().position(|site| {
            site.block_pos() == Some(pos) && site.stage != EcologyStage::Harvested
        })
    else {
        return Ok(false);
    };
    let atlas_index = geography.dynamic.ecology.sites[site_index]
        .atlas_pos
        .index(geography.manifest.side);
    let content_id = geography.dynamic.ecology.sites[site_index]
        .content_id
        .clone();
    let definition = registry
        .arcane_ecology
        .get(&content_id)
        .ok_or_else(|| "destroyed ecology content disappeared".to_string())?;
    let disposition = registry
        .block_by_name
        .get(&content_id)
        .and_then(|id| registry.block(*id).arcane.as_ref())
        .map_or(ArcaneDisposition::Ambient, |arcane| arcane.on_destroy);
    let site = &mut geography.dynamic.ecology.sites[site_index];
    for slot in 0..6 {
        let target = match disposition {
            ArcaneDisposition::Ambient => &mut geography.dynamic.cells[atlas_index].ambient[slot],
            ArcaneDisposition::Dross | ArcaneDisposition::Scar => {
                &mut geography.dynamic.cells[atlas_index].dross[slot]
            }
        };
        let moved = site.charge[slot].min(u32::from(u16::MAX - *target));
        site.charge[slot] -= moved;
        *target += moved as u16;
        // Any capacity overflow remains in the destroyed site's litter state
        // and is still included in custody; it can be released later rather
        // than silently disappearing.
    }
    site.population = 0;
    site.seed_bank = if definition.kind == ArcaneEcologyKind::Crystal {
        0
    } else {
        site.seed_bank.saturating_sub(2)
    };
    site.crystal_stage = 0;
    site.stage = if site.seed_bank == 0 {
        EcologyStage::Harvested
    } else {
        EcologyStage::Collapsed
    };
    site.collapses = site.collapses.saturating_add(1);
    geography.dynamic.ecology.event_sequence = geography
        .dynamic
        .ecology
        .event_sequence
        .checked_add(1)
        .ok_or_else(|| "ecology event sequence overflow".to_string())?;
    Ok(true)
}

pub fn restore_with_seed(
    geography: &mut ArcaneGeography,
    registry: &Registry,
    content_id: &str,
    pos: BlockPos,
) -> bool {
    let Some(site) = geography
        .dynamic
        .ecology
        .sites
        .iter_mut()
        .find(|site| site.block_pos() == Some(pos))
    else {
        return false;
    };
    if site.content_id != content_id
        || !matches!(
            site.stage,
            EcologyStage::Collapsed | EcologyStage::Harvested
        )
    {
        return false;
    }
    let Some(definition) = registry.arcane_ecology.get(&site.content_id) else {
        return false;
    };
    if definition.kind == ArcaneEcologyKind::FiniteMineral {
        return false;
    }
    site.seed_bank = site.seed_bank.saturating_add(1).max(1);
    if site.population == 0 {
        site.population = 1;
    }
    site.stage = EcologyStage::Recovering;
    true
}

pub fn record_fire_at(geography: &mut ArcaneGeography, atlas: &PlanetAtlas, surface: SurfacePos) {
    let atlas_pos = atlas.atlas_pos(surface);
    for site in geography
        .dynamic
        .ecology
        .sites
        .iter_mut()
        .filter(|site| site.atlas_pos == atlas_pos)
    {
        site.fire_history = site.fire_history.saturating_add(1);
        if site.content_id == "base:ember_poppy"
            && site.stage == EcologyStage::Collapsed
            && site.seed_bank != 0
        {
            site.stage = EcologyStage::Recovering;
        }
    }
}

pub fn audit(registry: &Registry, state: &ArcaneEcologyState) -> Result<EcologyAudit, String> {
    if state.version != ARCANE_ECOLOGY_VERSION || state.sites.len() > ECOLOGY_MAX_SITES {
        return Err("unsupported or oversized arcane ecology state".into());
    }
    let mut audit = EcologyAudit::default();
    let mut ids = BTreeSet::new();
    for site in &state.sites {
        if site.id == 0 || !ids.insert(site.id) || site.content_id.is_empty() {
            return Err("arcane ecology has an invalid or duplicate site id".into());
        }
        audit.sites += 1;
        audit.natural_sites += usize::from(site.ownership != EcologyOwnership::Cultivated);
        audit.cultivated_sites += usize::from(site.ownership == EcologyOwnership::Cultivated);
        audit.collapsed_sites += usize::from(site.stage == EcologyStage::Collapsed);
        *audit.variants.entry(site.variant).or_default() += 1;
        if let Some(definition) = registry.arcane_ecology.get(&site.content_id) {
            if site.population > site.carrying_capacity
                || site.carrying_capacity != definition.carrying_capacity
                || site.charge_total() > definition.charge_capacity
                || (definition.kind == ArcaneEcologyKind::Crystal
                    && site.crystal_stage >= definition.crystal_stages)
            {
                return Err(format!(
                    "invalid lifecycle state at ecology site {}",
                    site.id
                ));
            }
            for role in &definition.roles {
                *audit.roles.entry(*role).or_default() += 1;
            }
        }
        for slot in 0..6 {
            audit.charge[slot] = audit.charge[slot]
                .checked_add(u64::from(site.charge[slot]))
                .ok_or_else(|| "ecology charge audit overflow".to_string())?;
            audit.dross[slot] = audit.dross[slot]
                .checked_add(u64::from(site.dross[slot]))
                .ok_or_else(|| "ecology dross audit overflow".to_string())?;
        }
        audit.nutrients = audit
            .nutrients
            .checked_add(site.nutrient_total())
            .ok_or_else(|| "ecology nutrient audit overflow".to_string())?;
        audit.cumulative_water_hu = audit
            .cumulative_water_hu
            .checked_add(site.cumulative_water_hu)
            .ok_or_else(|| "ecology water history overflow".to_string())?;
        audit.checksum = mix64(
            audit.checksum
                ^ site.id
                ^ site.charge_total().rotate_left(11)
                ^ site.dross_total().rotate_left(29)
                ^ site.nutrient_total().rotate_left(43)
                ^ u64::from(site.population),
        );
    }
    audit.exported = state.exported;
    Ok(audit)
}

/// Registry-independent custody totals used by the parent geography ledger.
/// Removed providers may make a lifecycle dormant, but cannot make its saved
/// charge disappear from conservation.
pub fn custody_totals(state: &ArcaneEcologyState) -> Result<([u64; 6], [u64; 6], u64), String> {
    if state.version != ARCANE_ECOLOGY_VERSION || state.sites.len() > ECOLOGY_MAX_SITES {
        return Err("unsupported or oversized arcane ecology state".into());
    }
    let mut charge = [0u64; 6];
    let mut dross = [0u64; 6];
    let mut checksum = 0u64;
    let mut ids = BTreeSet::new();
    for site in &state.sites {
        if site.id == 0 || !ids.insert(site.id) || site.content_id.is_empty() {
            return Err("arcane ecology has an invalid or duplicate site id".into());
        }
        for slot in 0..6 {
            charge[slot] = charge[slot]
                .checked_add(u64::from(site.charge[slot]))
                .ok_or_else(|| "ecology charge audit overflow".to_string())?;
            dross[slot] = dross[slot]
                .checked_add(u64::from(site.dross[slot]))
                .ok_or_else(|| "ecology dross audit overflow".to_string())?;
        }
        checksum = mix64(
            checksum
                ^ site.id
                ^ site.charge_total().rotate_left(11)
                ^ site.dross_total().rotate_left(29)
                ^ site.nutrient_total().rotate_left(43)
                ^ u64::from(site.population),
        );
    }
    for (slot, units) in state.exported.into_iter().enumerate() {
        checksum = mix64(checksum ^ units.rotate_left((slot * 7) as u32));
    }
    Ok((charge, dross, checksum))
}

pub fn validate(
    atlas: &PlanetAtlas,
    registry: &Registry,
    geography: &ArcaneGeography,
) -> Result<(), String> {
    let state = &geography.dynamic.ecology;
    let audit = audit(registry, state)?;
    for site in &state.sites {
        if site.atlas_pos.u >= atlas.side()
            || site.atlas_pos.v >= atlas.side()
            || site.surface_u >= crate::planet::FACE_BLOCKS
            || site.surface_v >= crate::planet::FACE_BLOCKS
        {
            return Err(format!(
                "ecology site {} is outside the finite planet",
                site.id
            ));
        }
        if let Some(definition) = registry.arcane_ecology.get(&site.content_id)
            && site.ownership != EcologyOwnership::Cultivated
            && !habitat_tags_suitable(atlas, geography, site.atlas_pos, definition, true)
        {
            return Err(format!(
                "ecology site {} ({}) violates its ordinary habitat",
                site.id, site.content_id
            ));
        }
    }
    if atlas.side() >= 16 {
        for role in [
            EcologyRole::Gatherer,
            EcologyRole::Reservoir,
            EcologyRole::Conductor,
            EcologyRole::Transformer,
            EcologyRole::Indicator,
            EcologyRole::Stabilizer,
            EcologyRole::Catalyst,
        ] {
            if audit.roles.get(&role).copied().unwrap_or(0) < 2 {
                return Err(format!(
                    "core ecology role {role:?} lacks redundant populations"
                ));
            }
        }
    }
    Ok(())
}

fn habitat_suitable_parts(
    atlas: &PlanetAtlas,
    controls: &[ArcaneControlCell],
    cells: &[ArcaneDynamicCell],
    pos: AtlasPos,
    definition: &ArcaneEcologyDef,
    stored_charge: u64,
    stored_dross: u64,
) -> bool {
    // This mirrors `habitat_suitable` without borrowing the whole geography
    // while its site vector is mutably sliced during an unloaded pass.
    let index = pos.index(atlas.side());
    let control = controls[index];
    let cell = cells[index];
    let local_available = cell
        .ambient_total()
        .saturating_add(stored_charge)
        .saturating_add(stored_dross);
    let richness = local_available
        .saturating_mul(1_000)
        .checked_div(u64::from(control.capacity).max(1))
        .unwrap_or_default()
        .min(1_000) as u16;
    if richness < definition.min_richness_permille
        || control.stability < definition.min_stability_permille
        || control.stability > definition.max_stability_permille
    {
        return false;
    }
    // The immutable ordinary habitat was validated at site creation. Runtime
    // succession rechecks the changeable magical bands here; water and hearts
    // are checked by the caller. Cultivated sites survive loss of a heart but
    // stop receiving its growth path.
    true
}

fn candidate_score(
    atlas: &PlanetAtlas,
    geography: &ArcaneGeography,
    pos: AtlasPos,
    definition: &ArcaneEcologyDef,
) -> u64 {
    let index = pos.index(atlas.side());
    let cell = geography.dynamic.cells[index];
    let control = geography.controls[index];
    let fertility = atlas.genesis.ground.values()[index].baseline_fertility;
    let resonance_fit = definition
        .resonance
        .iter()
        .filter_map(|(name, weight)| {
            BASE_RESONANCES
                .iter()
                .position(|candidate| *candidate == name)
                .map(|slot| u64::from(cell.ambient[slot]) * u64::from(*weight))
        })
        .sum::<u64>();
    resonance_fit
        .saturating_mul(16)
        .saturating_add(cell.ambient_total().saturating_mul(4))
        .saturating_add(u64::from(control.stability))
        .saturating_add(u64::from(fertility))
        .saturating_add(mix64(atlas_identity(pos)) & 0xff)
}

fn site_surface(atlas: &PlanetAtlas, pos: AtlasPos, id: u64) -> SurfacePos {
    let center = pos.center(atlas.side());
    let cell_blocks = u32::from(atlas.cell_blocks().max(1));
    let half = cell_blocks / 2;
    let jitter_u = (mix64(id) % u64::from(cell_blocks)) as i32 - half as i32;
    let jitter_v = (mix64(id.rotate_left(23)) % u64::from(cell_blocks)) as i32 - half as i32;
    SurfacePos::new(
        center.face,
        (center.u.floor() as i32 + jitter_u).clamp(0, i32::from(crate::planet::FACE_BLOCKS - 1))
            as u16,
        (center.v.floor() as i32 + jitter_v).clamp(0, i32::from(crate::planet::FACE_BLOCKS - 1))
            as u16,
    )
    .expect("clamped ecology site surface is canonical")
}

fn resonance_weights(definition: &BTreeMap<String, u16>) -> [u32; 6] {
    let mut weights = [0u32; 6];
    for (name, weight) in definition {
        if let Some(slot) = BASE_RESONANCES
            .iter()
            .position(|candidate| *candidate == name)
        {
            weights[slot] = u32::from(*weight);
        }
    }
    weights
}

fn take_weighted(
    source: &mut [u16; 6],
    resonance: &BTreeMap<String, u16>,
    requested: u64,
) -> [u32; 6] {
    if requested == 0 {
        return [0; 6];
    }
    let weights = resonance_weights(resonance);
    let total_weight = weights.iter().map(|weight| u64::from(*weight)).sum::<u64>();
    let available = source.iter().map(|units| u64::from(*units)).sum::<u64>();
    let wanted = requested.min(available);
    let mut out = [0u32; 6];
    let mut remaining = wanted;
    for slot in 0..6 {
        let share = wanted
            .saturating_mul(u64::from(weights[slot]))
            .checked_div(total_weight)
            .unwrap_or_default();
        let take = share.min(u64::from(source[slot])).min(remaining) as u16;
        source[slot] -= take;
        out[slot] += u32::from(take);
        remaining -= u64::from(take);
    }
    // A preferred band may be locally absent. Finish deterministically from
    // the actual local mixture rather than refusing a smaller viable uptake.
    for slot in 0..6 {
        if remaining == 0 {
            break;
        }
        let take = remaining.min(u64::from(source[slot])) as u16;
        source[slot] -= take;
        out[slot] += u32::from(take);
        remaining -= u64::from(take);
    }
    out
}

fn add_bands(target: &mut [u32; 6], moved: [u32; 6]) -> Result<(), String> {
    for (target, moved) in target.iter_mut().zip(moved) {
        *target = target
            .checked_add(moved)
            .ok_or_else(|| "ecology band overflow".to_string())?;
    }
    Ok(())
}

fn release_charge(
    site: &mut EcologySite,
    definition: &ArcaneEcologyDef,
    cell: &mut ArcaneDynamicCell,
) -> Result<(), String> {
    let mut remaining = u64::from(definition.release_per_day).min(site.charge_total());
    for slot in 0..6 {
        if remaining == 0 {
            break;
        }
        let room = u64::from(u16::MAX - cell.ambient[slot]);
        let moved = remaining.min(u64::from(site.charge[slot])).min(room) as u16;
        site.charge[slot] -= u32::from(moved);
        cell.ambient[slot] += moved;
        remaining -= u64::from(moved);
    }
    Ok(())
}

fn collapse(
    site: &mut EcologySite,
    definition: &ArcaneEcologyDef,
    cell: &mut ArcaneDynamicCell,
) -> Result<(), String> {
    site.stage = EcologyStage::Collapsed;
    site.collapses = site.collapses.saturating_add(1);
    let disposition = if definition.kind == ArcaneEcologyKind::Crystal {
        ArcaneDisposition::Dross
    } else {
        ArcaneDisposition::Ambient
    };
    for slot in 0..6 {
        let room = match disposition {
            ArcaneDisposition::Ambient => u32::from(u16::MAX - cell.ambient[slot]),
            ArcaneDisposition::Dross | ArcaneDisposition::Scar => {
                u32::from(u16::MAX - cell.dross[slot])
            }
        };
        let moved = site.charge[slot].min(room);
        site.charge[slot] -= moved;
        match disposition {
            ArcaneDisposition::Ambient => cell.ambient[slot] += moved as u16,
            ArcaneDisposition::Dross | ArcaneDisposition::Scar => cell.dross[slot] += moved as u16,
        }
    }
    Ok(())
}

fn lose_population(site: &mut EcologySite, definition: &ArcaneEcologyDef) {
    if site.population == 0 {
        return;
    }
    let before = u32::from(site.population);
    site.population -= 1;
    let nutrients = site.biomass_nutrients / before.max(1);
    site.biomass_nutrients -= nutrients;
    site.detritus_nutrients = site.detritus_nutrients.saturating_add(nutrients);
    if matches!(definition.harvest, EcologyHarvestClass::Destructive) {
        site.seed_bank = site.seed_bank.saturating_sub(1);
    }
}

fn mineralize_detritus(site: &mut EcologySite) {
    let moved = site.detritus_nutrients.div_ceil(16);
    site.detritus_nutrients -= moved;
    site.soil_nutrients = site.soil_nutrients.saturating_add(moved);
}

fn bands_to_current(bands: [u32; 6]) -> Result<Current, String> {
    Current::from_parts(
        BASE_RESONANCES
            .into_iter()
            .zip(bands)
            .filter(|(_, units)| *units != 0)
            .map(|(name, units)| (name.to_string(), u64::from(units))),
    )
    .map_err(|error| error.to_string())
}

fn atlas_identity(pos: AtlasPos) -> u64 {
    (u64::from(pos.face as u8) << 56) | (u64::from(pos.u) << 28) | u64::from(pos.v)
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58476d1ce4e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixture(seed: u32, side: u16) -> (PlanetAtlas, Registry, ArcaneGeography) {
        let atlas = PlanetAtlas::fixture(seed, side).unwrap();
        let registry = crate::registry::load(Path::new("__no_arcane_ecology_mods__"));
        let geography = ArcaneGeography::generate(
            &atlas,
            &registry,
            &crate::planet_atlas::CancellationToken::default(),
            |_| {},
        )
        .unwrap();
        (atlas, registry, geography)
    }

    #[test]
    fn genesis_sites_are_deterministic_and_conserved() {
        let (atlas, registry, first) = fixture(71, 8);
        let second = ArcaneGeography::generate(
            &atlas,
            &registry,
            &crate::planet_atlas::CancellationToken::default(),
            |_| {},
        )
        .unwrap();
        assert_eq!(first.dynamic.ecology, second.dynamic.ecology);
        assert_eq!(first.audit().unwrap(), second.audit().unwrap());
        assert!(audit(&registry, &first.dynamic.ecology).unwrap().sites > 0);
    }

    #[test]
    fn every_natural_site_obeys_ordinary_and_magical_habitat() {
        let (atlas, registry, geography) = fixture(72, 8);
        for site in &geography.dynamic.ecology.sites {
            let def = &registry.arcane_ecology[&site.content_id];
            assert!(habitat_suitable(&atlas, &geography, site.atlas_pos, def));
        }
    }

    #[test]
    fn unloaded_succession_never_changes_total_current() {
        let (atlas, registry, mut geography) = fixture(73, 8);
        let before = geography.audit().unwrap().accounted_total;
        let living = atlas
            .genesis
            .biomes
            .values()
            .iter()
            .map(|cell| cell.heart_assignment)
            .filter(|id| *id != 0)
            .collect::<BTreeSet<_>>();
        for day in 1..=48 {
            let mut water = geography
                .dynamic
                .ecology
                .sites
                .iter()
                .map(|site| (site.atlas_pos, 1_000_000))
                .collect();
            while geography.dynamic.ecology.completed_days < day {
                advance_toward(
                    &mut geography,
                    &atlas,
                    &registry,
                    day,
                    17,
                    EcologyConditions::new(&mut water, &living, &BTreeSet::new(), &BTreeSet::new()),
                )
                .unwrap();
            }
        }
        assert_eq!(geography.audit().unwrap().accounted_total, before);
    }

    fn materialize_for_test(site: &mut EcologySite, y: u8) -> BlockPos {
        site.materialized_y = y;
        site.stage = EcologyStage::Mature;
        site.block_pos().unwrap()
    }

    #[test]
    fn base_species_and_core_roles_have_redundant_seed_sites() {
        let (_, registry, geography) = fixture(74, 16);
        for species in [
            "base:rainbell",
            "base:hushwood",
            "base:stormvine",
            "base:cairnbloom",
            "base:ashlace",
            "base:pilgrim_root",
            "base:lantern_reed",
            "base:nightglass",
            "base:frostlace",
            "base:tidekelp",
            "base:echo_cap",
            "base:ember_poppy",
            "base:wellglass_bud",
        ] {
            let count = geography
                .dynamic
                .ecology
                .sites
                .iter()
                .filter(|site| site.content_id == species)
                .count();
            assert!(
                count >= 2,
                "{species} has only {count} deterministic seed sites"
            );
        }
        let ecology = audit(&registry, &geography.dynamic.ecology).unwrap();
        for role in [
            EcologyRole::Gatherer,
            EcologyRole::Reservoir,
            EcologyRole::Conductor,
            EcologyRole::Transformer,
            EcologyRole::Indicator,
            EcologyRole::Stabilizer,
            EcologyRole::Catalyst,
        ] {
            assert!(
                ecology.roles.get(&role).copied().unwrap_or(0) >= 4,
                "{role:?} lacks redundant planetary populations"
            );
        }
    }

    #[test]
    fn site_selection_has_no_cube_edge_dead_zone_or_pileup() {
        let (_, _, geography) = fixture(75, 16);
        let side = geography.manifest.side;
        let (mut edge, mut interior) = (0usize, 0usize);
        for site in &geography.dynamic.ecology.sites {
            if site.atlas_pos.u == 0
                || site.atlas_pos.v == 0
                || site.atlas_pos.u + 1 == side
                || site.atlas_pos.v + 1 == side
            {
                edge += 1;
            } else {
                interior += 1;
            }
        }
        let fraction = edge as f64 / (edge + interior) as f64;
        let cell_fraction = (4 * usize::from(side) - 4) as f64 / usize::from(side).pow(2) as f64;
        assert!(
            fraction > cell_fraction / 4.0 && fraction < cell_fraction * 4.0,
            "edge selection fraction {fraction:.3} diverges from topology area {cell_fraction:.3}"
        );
    }

    #[test]
    fn confluence_catalog_expresses_all_nine_physical_habitats() {
        let mut seen = BTreeSet::new();
        for seed in [3, 17, 41, 89] {
            let (_, registry, geography) = fixture(seed, 16);
            let ecology = audit(&registry, &geography.dynamic.ecology).unwrap();
            seen.extend(
                ecology
                    .variants
                    .into_iter()
                    .filter_map(|(habitat, count)| (count != 0).then_some(habitat)),
            );
        }
        assert_eq!(
            seen,
            ConfluenceHabitat::ALL.into_iter().collect(),
            "the accepted-seed suite must express every climate-causal confluence"
        );
    }

    #[test]
    fn coarse_ecology_is_sliced_and_inside_save_memory_and_cpu_budgets() {
        let (atlas, registry, mut geography) = fixture(0xb0d6e7, 32);
        let state_bytes = postcard::to_allocvec(&geography.dynamic.ecology).unwrap();
        assert!(geography.dynamic.ecology.sites.len() <= ECOLOGY_MAX_SITES);
        assert!(
            state_bytes.len() < 16 * 1024 * 1024,
            "coarse ecology save is {} MiB",
            state_bytes.len() / (1024 * 1024)
        );
        let before = geography.audit().unwrap().accounted_total;
        let mut water = geography
            .dynamic
            .ecology
            .sites
            .iter()
            .map(|site| (site.atlas_pos, 1_000_000))
            .collect::<BTreeMap<_, _>>();
        let living = atlas
            .biomes
            .countries
            .iter()
            .map(|country| country.id)
            .collect::<BTreeSet<_>>();
        let storming = water.keys().copied().collect::<BTreeSet<_>>();
        let started = std::time::Instant::now();
        let mut calls = 0usize;
        while geography.dynamic.ecology.completed_days < 1 {
            let report = advance_toward(
                &mut geography,
                &atlas,
                &registry,
                1,
                8,
                EcologyConditions::new(&mut water, &living, &storming, &BTreeSet::new()),
            )
            .unwrap();
            assert!(report.processed <= 8);
            calls += 1;
        }
        let elapsed = started.elapsed();
        assert!(calls > 1, "the fixture must actually exercise slicing");
        assert!(
            elapsed.as_secs_f32() < 2.0,
            "one coarse day took {elapsed:?}"
        );
        assert_eq!(geography.audit().unwrap().accounted_total, before);
        eprintln!(
            "ecology budget: sites={} save={} KiB slices={} day={elapsed:?}",
            geography.dynamic.ecology.sites.len(),
            state_bytes.len() / 1024,
            calls
        );
    }

    #[test]
    #[ignore = "operator probe for WILDFORGE_PROBE_WORLD production save"]
    fn production_ecology_budget_probe() {
        let root = std::env::var("WILDFORGE_PROBE_WORLD")
            .map(std::path::PathBuf::from)
            .expect("set WILDFORGE_PROBE_WORLD to a qualified production save");
        let atlas = PlanetAtlas::load(&root).unwrap();
        let registry = crate::registry::load(&root.join("mods"));
        let mut geography = ArcaneGeography::load(&root, &atlas).unwrap();
        let added = reconcile_content(&atlas, &registry, &mut geography).unwrap();
        let state_bytes = postcard::to_allocvec(&geography.dynamic.ecology).unwrap();
        let before = geography.audit().unwrap().accounted_total;
        let mut water = geography
            .dynamic
            .ecology
            .sites
            .iter()
            .map(|site| (site.atlas_pos, 1_000_000))
            .collect::<BTreeMap<_, _>>();
        let living = atlas
            .biomes
            .countries
            .iter()
            .map(|country| country.id)
            .collect::<BTreeSet<_>>();
        let target = geography.dynamic.ecology.completed_days.saturating_add(1);
        let started = std::time::Instant::now();
        let mut worst = std::time::Duration::ZERO;
        while geography.dynamic.ecology.completed_days < target {
            let slice = std::time::Instant::now();
            advance_toward(
                &mut geography,
                &atlas,
                &registry,
                target,
                512,
                EcologyConditions::new(&mut water, &living, &BTreeSet::new(), &BTreeSet::new()),
            )
            .unwrap();
            worst = worst.max(slice.elapsed());
        }
        assert!(worst.as_millis() < 25, "worst ecology slice {worst:?}");
        assert!(started.elapsed().as_secs() < 20);
        assert!(state_bytes.len() < 32 * 1024 * 1024);
        assert_eq!(geography.audit().unwrap().accounted_total, before);
        for site in geography.dynamic.ecology.sites.iter().take(12) {
            eprintln!(
                "  preview {} {} {},{} ({})",
                site.content_id,
                site.atlas_pos.face.name(),
                site.surface_u,
                site.surface_v,
                site.variant.label()
            );
        }
        eprintln!(
            "production ecology: added={added} sites={} save={} KiB day={:?} worst={worst:?}",
            geography.dynamic.ecology.sites.len(),
            state_bytes.len() / 1024,
            started.elapsed()
        );
    }

    #[test]
    fn crystal_harvest_is_exact_seed_preserving_and_single_shot() {
        let (_, registry, mut geography) = fixture(76, 16);
        let site_index = geography
            .dynamic
            .ecology
            .sites
            .iter()
            .position(|site| site.content_id == "base:wellglass_bud")
            .expect("fixture needs wellglass habitat");
        let pos = materialize_for_test(&mut geography.dynamic.ecology.sites[site_index], 48);
        geography.dynamic.ecology.sites[site_index].crystal_stage = 1;
        geography.dynamic.ecology.sites[site_index].charge = [7, 11, 13, 17, 19, 23];
        assert!(
            plan_harvest(&geography, &registry, pos, 3).is_none(),
            "a mature host population is not yet a fully grown crystal"
        );
        geography.dynamic.ecology.sites[site_index].crystal_stage = 3;
        let before = geography.dynamic.ecology.sites[site_index].charge_total();
        let exported_before = geography.dynamic.ecology.exported;
        let plan = plan_harvest(&geography, &registry, pos, 3).expect("mature crystal harvest");
        assert!(plan.leaves_bud);
        assert_eq!(plan.current.total(), before);
        apply_harvest(&mut geography, &registry, &plan, 9).unwrap();
        let site = &geography.dynamic.ecology.sites[site_index];
        assert_eq!(site.charge_total(), 0);
        assert_eq!(site.crystal_stage, 1);
        assert_eq!(site.stage, EcologyStage::Recovering);
        assert!(plan_harvest(&geography, &registry, pos, 3).is_none());
        for (slot, exported_before) in exported_before.iter().enumerate() {
            assert_eq!(
                geography.dynamic.ecology.exported[slot] - exported_before,
                u64::from(plan.charge[slot])
            );
        }

        let (_, registry, mut careless) = fixture(76, 16);
        let index = careless
            .dynamic
            .ecology
            .sites
            .iter()
            .position(|site| site.content_id == "base:wellglass_bud")
            .unwrap();
        let pos = materialize_for_test(&mut careless.dynamic.ecology.sites[index], 48);
        careless.dynamic.ecology.sites[index].crystal_stage = 3;
        careless.dynamic.ecology.sites[index].charge = [5; 6];
        let plan = plan_harvest(&careless, &registry, pos, 0).unwrap();
        assert!(plan.destructive && !plan.leaves_bud);
        apply_harvest(&mut careless, &registry, &plan, 9).unwrap();
        assert_eq!(
            careless.dynamic.ecology.sites[index].stage,
            EcologyStage::Harvested
        );
        assert_eq!(careless.dynamic.ecology.sites[index].seed_bank, 0);
    }

    #[test]
    fn wellglass_needs_a_seed_and_free_attachment_space_to_take_up_current() {
        let (atlas, registry, mut open) = fixture(76, 16);
        let index = open
            .dynamic
            .ecology
            .sites
            .iter()
            .position(|site| site.content_id == "base:wellglass_bud")
            .unwrap();
        let id = open.dynamic.ecology.sites[index].id;
        let atlas_pos = open.dynamic.ecology.sites[index].atlas_pos;
        let cell_index = atlas_pos.index(atlas.side());
        for slot in 0..6 {
            let returned = u16::try_from(open.dynamic.ecology.sites[index].charge[slot]).unwrap();
            open.dynamic.cells[cell_index].ambient[slot] += returned;
            open.dynamic.ecology.sites[index].charge[slot] = 0;
        }
        open.dynamic.ecology.sites[index].seed_bank = 1;
        open.dynamic.ecology.sites[index].population = 1;
        open.dynamic.ecology.sites[index].stage = EcologyStage::Establishing;
        let mut blocked = open.clone();
        let mut seedless = open.clone();
        seedless.dynamic.ecology.sites[index].seed_bank = 0;
        seedless.dynamic.ecology.sites[index].stage = EcologyStage::Harvested;
        let water = open
            .dynamic
            .ecology
            .sites
            .iter()
            .map(|site| (site.atlas_pos, 1_000_000))
            .collect::<BTreeMap<_, _>>();
        let living = atlas
            .biomes
            .countries
            .iter()
            .map(|country| country.id)
            .collect::<BTreeSet<_>>();
        for (geography, blocked_ids) in [
            (&mut open, BTreeSet::new()),
            (&mut blocked, BTreeSet::from([id])),
            (&mut seedless, BTreeSet::new()),
        ] {
            let mut water = water.clone();
            advance_toward(
                geography,
                &atlas,
                &registry,
                1,
                ECOLOGY_MAX_SITES,
                EcologyConditions::new(&mut water, &living, &BTreeSet::new(), &blocked_ids),
            )
            .unwrap();
        }
        assert!(open.dynamic.ecology.sites[index].charge_total() > 0);
        assert_eq!(blocked.dynamic.ecology.sites[index].charge_total(), 0);
        assert_eq!(seedless.dynamic.ecology.sites[index].charge_total(), 0);
    }

    #[test]
    fn sequestered_dross_leaves_in_tissue_without_becoming_clean_current() {
        let (_, registry, mut geography) = fixture(77, 16);
        let index = geography
            .dynamic
            .ecology
            .sites
            .iter()
            .position(|site| site.content_id == "base:ashlace")
            .expect("fixture needs an ashlace margin");
        let pos = materialize_for_test(&mut geography.dynamic.ecology.sites[index], 42);
        geography.dynamic.ecology.sites[index].charge = [0; 6];
        geography.dynamic.ecology.sites[index].dross = [12, 9, 6, 3, 15, 18];
        let before = geography.dynamic.ecology.sites[index].dross_total();
        let plan = plan_harvest(&geography, &registry, pos, 0).unwrap();
        assert!(plan.current.is_empty());
        assert!(plan.dross_current.total() > 0);
        assert_eq!(plan.dross_current.total(), before / 3);
        apply_harvest(&mut geography, &registry, &plan, 1).unwrap();
        assert_eq!(
            geography.dynamic.ecology.sites[index].dross_total() + plan.dross_current.total(),
            before
        );
    }

    #[test]
    fn overharvest_collapses_and_matching_common_seed_restores() {
        let (_, registry, mut geography) = fixture(78, 16);
        let index = geography
            .dynamic
            .ecology
            .sites
            .iter()
            .position(|site| site.content_id == "base:rainbell")
            .unwrap();
        let pos = materialize_for_test(&mut geography.dynamic.ecology.sites[index], 50);
        geography.dynamic.ecology.sites[index].population = 2;
        geography.dynamic.ecology.sites[index].seed_bank = 2;
        for day in 1..=2 {
            geography.dynamic.ecology.sites[index].stage = EcologyStage::Mature;
            let plan = plan_harvest(&geography, &registry, pos, 0).unwrap();
            apply_harvest(&mut geography, &registry, &plan, day).unwrap();
        }
        assert_eq!(
            geography.dynamic.ecology.sites[index].stage,
            EcologyStage::Collapsed
        );
        assert!(!restore_with_seed(
            &mut geography,
            &registry,
            "base:cairnbloom",
            pos
        ));
        assert!(restore_with_seed(
            &mut geography,
            &registry,
            "base:rainbell",
            pos
        ));
        let site = &geography.dynamic.ecology.sites[index];
        assert_eq!(site.stage, EcologyStage::Recovering);
        assert_eq!(site.population, 2);
        assert!(site.seed_bank > 0);
    }

    #[test]
    fn transformer_uptake_water_and_nutrients_are_conserved() {
        let (atlas, registry, mut geography) = fixture(79, 16);
        let index = geography
            .dynamic
            .ecology
            .sites
            .iter()
            .position(|site| site.content_id == "base:ashlace")
            .unwrap();
        let atlas_pos = geography.dynamic.ecology.sites[index].atlas_pos;
        let cell_index = atlas_pos.index(atlas.side());
        geography.dynamic.cells[cell_index].dross[5] =
            geography.dynamic.cells[cell_index].dross[5].saturating_add(200);
        let total_before = geography.audit().unwrap().accounted_total;
        let nutrients_before = geography.dynamic.ecology.sites[index].nutrient_total();
        let dross_before = geography.dynamic.cells[cell_index]
            .dross
            .into_iter()
            .map(u64::from)
            .sum::<u64>()
            + geography.dynamic.ecology.sites[index].dross_total();
        let water_before = geography.dynamic.ecology.sites[index].cumulative_water_hu;
        let mut water = geography
            .dynamic
            .ecology
            .sites
            .iter()
            .map(|site| (site.atlas_pos, 1_000_000))
            .collect();
        let living = atlas
            .genesis
            .biomes
            .values()
            .iter()
            .map(|cell| cell.heart_assignment)
            .collect();
        let _report = advance_toward(
            &mut geography,
            &atlas,
            &registry,
            1,
            ECOLOGY_MAX_SITES,
            EcologyConditions::new(&mut water, &living, &BTreeSet::new(), &BTreeSet::new()),
        )
        .unwrap();
        let site = &geography.dynamic.ecology.sites[index];
        assert_eq!(site.nutrient_total(), nutrients_before);
        assert_eq!(
            geography.dynamic.cells[cell_index]
                .dross
                .into_iter()
                .map(u64::from)
                .sum::<u64>()
                + site.dross_total(),
            dross_before
        );
        assert_eq!(geography.audit().unwrap().accounted_total, total_before);
        assert_eq!(
            site.cumulative_water_hu - water_before,
            u64::from(registry.arcane_ecology["base:ashlace"].water_per_day_hu)
                * u64::from(site.population.max(1))
        );
    }

    #[test]
    fn observations_report_state_without_ledger_numbers() {
        let (_, registry, mut geography) = fixture(80, 16);
        let index = geography
            .dynamic
            .ecology
            .sites
            .iter()
            .position(|site| site.content_id == "base:rainbell")
            .unwrap();
        geography.dynamic.ecology.sites[index].charge = [0; 6];
        let surface = geography.dynamic.ecology.sites[index].surface().unwrap();
        let observation = observation_at(&geography, &registry, surface, 72.0).unwrap();
        assert!(observation.text.contains("fold shut"));
        assert!(
            !observation
                .text
                .chars()
                .any(|character| character.is_ascii_digit())
        );
        let rooted = wellglass_structure([9, 0, 0, 0, 0, 0]);
        let tidal = wellglass_structure([0, 9, 0, 0, 0, 0]);
        let echoing = wellglass_structure([0, 0, 0, 0, 0, 9]);
        assert_ne!(rooted, tidal);
        assert_ne!(tidal, echoing);
        assert!(
            [rooted, tidal, echoing]
                .into_iter()
                .all(|text| !text.chars().any(|character| character.is_ascii_digit()))
        );
    }

    #[test]
    fn dead_heart_withdraws_growth_without_deleting_cultivation() {
        let (atlas, mut registry, mut living) = fixture(81, 16);
        let index = living
            .dynamic
            .ecology
            .sites
            .iter()
            .position(|site| site.content_id == "base:pilgrim_root")
            .unwrap();
        let country = atlas.genesis.biomes.values()[living.dynamic.ecology.sites[index]
            .atlas_pos
            .index(atlas.side())]
        .heart_assignment;
        assert_ne!(country, 0);
        registry
            .arcane_ecology
            .get_mut("base:pilgrim_root")
            .unwrap()
            .regrowth_days = 1;
        {
            let site = &mut living.dynamic.ecology.sites[index];
            site.population = 1;
            site.seed_bank = 8;
            site.stage = EcologyStage::Establishing;
            site.soil_nutrients = site.soil_nutrients.max(1_000);
        }
        let mut dead = living.clone();
        dead.dynamic.ecology.sites[index].ownership = EcologyOwnership::Cultivated;
        let mut living_water = living
            .dynamic
            .ecology
            .sites
            .iter()
            .map(|site| (site.atlas_pos, 1_000_000))
            .collect::<BTreeMap<_, _>>();
        let mut dead_water = living_water.clone();
        advance_toward(
            &mut living,
            &atlas,
            &registry,
            1,
            ECOLOGY_MAX_SITES,
            EcologyConditions::new(
                &mut living_water,
                &BTreeSet::from([country]),
                &BTreeSet::new(),
                &BTreeSet::new(),
            ),
        )
        .unwrap();
        advance_toward(
            &mut dead,
            &atlas,
            &registry,
            1,
            ECOLOGY_MAX_SITES,
            EcologyConditions::new(
                &mut dead_water,
                &BTreeSet::new(),
                &BTreeSet::new(),
                &BTreeSet::new(),
            ),
        )
        .unwrap();
        assert!(living.dynamic.ecology.sites[index].population > 1);
        let cultivated = &dead.dynamic.ecology.sites[index];
        assert_eq!(cultivated.ownership, EcologyOwnership::Cultivated);
        assert_ne!(cultivated.stage, EcologyStage::Harvested);
        assert!(
            dead.dynamic
                .ecology
                .sites
                .iter()
                .any(|site| site.id == cultivated.id)
        );
    }

    #[test]
    fn stormvine_draws_current_only_during_real_storm_state() {
        let (atlas, registry, mut calm) = fixture(82, 16);
        let index = calm
            .dynamic
            .ecology
            .sites
            .iter()
            .position(|site| site.content_id == "base:stormvine")
            .unwrap();
        let pos = calm.dynamic.ecology.sites[index].atlas_pos;
        calm.dynamic.ecology.sites[index].charge = [0; 6];
        let mut storm = calm.clone();
        let living = atlas
            .genesis
            .biomes
            .values()
            .iter()
            .map(|cell| cell.heart_assignment)
            .collect();
        let water = calm
            .dynamic
            .ecology
            .sites
            .iter()
            .map(|site| (site.atlas_pos, 1_000_000))
            .collect::<BTreeMap<_, _>>();
        let mut calm_water = water.clone();
        let mut storm_water = water;
        advance_toward(
            &mut calm,
            &atlas,
            &registry,
            1,
            ECOLOGY_MAX_SITES,
            EcologyConditions::new(&mut calm_water, &living, &BTreeSet::new(), &BTreeSet::new()),
        )
        .unwrap();
        advance_toward(
            &mut storm,
            &atlas,
            &registry,
            1,
            ECOLOGY_MAX_SITES,
            EcologyConditions::new(
                &mut storm_water,
                &living,
                &BTreeSet::from([pos]),
                &BTreeSet::new(),
            ),
        )
        .unwrap();
        assert_eq!(calm.dynamic.ecology.sites[index].charge_total(), 0);
        assert!(storm.dynamic.ecology.sites[index].charge_total() > 0);
    }

    #[test]
    fn charged_biomass_fire_has_an_explicit_disposition_and_history() {
        let (atlas, registry, mut geography) = fixture(83, 16);
        let index = geography
            .dynamic
            .ecology
            .sites
            .iter()
            .position(|site| site.content_id == "base:ember_poppy")
            .unwrap();
        let pos = materialize_for_test(&mut geography.dynamic.ecology.sites[index], 50);
        let atlas_index = geography.dynamic.ecology.sites[index]
            .atlas_pos
            .index(atlas.side());
        let moved = geography.dynamic.cells[atlas_index].ambient[2].min(40);
        geography.dynamic.cells[atlas_index].ambient[2] -= moved;
        geography.dynamic.ecology.sites[index].charge[2] += u32::from(moved);
        let before = geography.audit().unwrap().accounted_total;
        let fire_before = geography.dynamic.ecology.sites[index].fire_history;
        assert!(apply_destructive_loss(&mut geography, &registry, pos).unwrap());
        record_fire_at(&mut geography, &atlas, pos.surface());
        assert_eq!(geography.audit().unwrap().accounted_total, before);
        assert_eq!(
            geography.dynamic.ecology.sites[index].fire_history,
            fire_before + 1
        );
        assert_eq!(geography.dynamic.ecology.sites[index].charge_total(), 0);
    }

    #[test]
    fn content_retrogen_preserves_saved_sites_and_adds_empty_untouched_candidates() {
        let (atlas, registry, mut geography) = fixture(84, 16);
        let preserved = geography
            .dynamic
            .ecology
            .sites
            .iter()
            .find(|site| site.content_id != "base:rainbell")
            .unwrap()
            .clone();
        let mut retained = Vec::new();
        for mut site in std::mem::take(&mut geography.dynamic.ecology.sites) {
            if site.content_id == "base:rainbell" {
                let cell = &mut geography.dynamic.cells[site.atlas_pos.index(atlas.side())];
                for slot in 0..6 {
                    cell.ambient[slot] = cell.ambient[slot]
                        .checked_add(u16::try_from(site.charge[slot]).unwrap())
                        .unwrap();
                    cell.dross[slot] = cell.dross[slot]
                        .checked_add(u16::try_from(site.dross[slot]).unwrap())
                        .unwrap();
                    site.charge[slot] = 0;
                    site.dross[slot] = 0;
                }
            } else {
                retained.push(site);
            }
        }
        geography.dynamic.ecology.sites = retained;
        geography.dynamic.ecology.content_hash ^= 0x55aa;
        let added = reconcile_content(&atlas, &registry, &mut geography).unwrap();
        assert!(added >= 2);
        assert!(
            geography
                .dynamic
                .ecology
                .sites
                .iter()
                .any(|site| site.id == preserved.id && site == &preserved)
        );
        let new_sites = geography
            .dynamic
            .ecology
            .sites
            .iter()
            .filter(|site| site.content_id == "base:rainbell")
            .collect::<Vec<_>>();
        assert!(!new_sites.is_empty());
        assert!(new_sites.iter().all(|site| {
            site.ownership == EcologyOwnership::Retrogen
                && site.charge_total() == 0
                && site.dross_total() == 0
                && site.stage == EcologyStage::Establishing
        }));
    }
}

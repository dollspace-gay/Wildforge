//! The edifices that stand over a country's heart.
//!
//! A heart used to be a stub one to five blocks tall at the province
//! centre, which in play meant grid-searching by hand to find one. Each
//! country now raises something you can see from across its valley.
//!
//! These span several chunks, and a chunk cannot write into its
//! neighbours — so an edifice is not a stamping routine but a pure
//! question asked per column: given this offset from the site, what
//! block belongs at this height? Every chunk answers it for its own
//! columns and they agree at the seams by construction.
//!
//! See docs/heart-edifice-plan.md.

use crate::registry::BlockId;
use crate::worldgen::Biome;

/// How the mass is shaped. Four answers cover twelve countries.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Family {
    /// A trunk far past any natural tree, root flare, canopy overhead.
    Bole,
    /// Courses of worked stone, hollow, with a mouth and a chamber.
    Stepped,
    /// Pillars around the site, open to the sky.
    Ring,
    /// A crown over the ground rather than a mass upon it.
    Cap,
}

/// One country's monument, in numbers.
#[derive(Clone, Copy)]
pub struct Edifice {
    pub family: Family,
    /// Half-width of the footprint, in blocks.
    pub reach: i32,
    /// How far the crest stands over local ground.
    pub rise: i32,
    /// The mass itself.
    pub shell: &'static str,
    /// Trim: canopy for a bole, capstones for a ring, facing elsewhere.
    pub crown: &'static str,
}

/// What each country raises. Kept as one table for the same reason the
/// hearts are: the alternative is twelve of everything drifting apart.
pub fn edifice_of(biome: Biome) -> Edifice {
    use Family::*;
    let e = |family, reach, rise, shell, crown| Edifice {
        family,
        reach,
        rise,
        shell,
        crown,
    };
    match biome {
        // The world tree, and its kin. A bole that does not clear the
        // canopy around it is not a landmark, so these run tallest.
        Biome::Forest => e(Bole, 14, 38, "base:log", "base:leaves"),
        Biome::Taiga => e(Bole, 12, 36, "base:spruce_log", "base:spruce_leaves"),
        Biome::Jungle => e(Bole, 15, 42, "base:jungle_log", "base:jungle_leaves"),
        Biome::Swamp => e(Bole, 13, 30, "base:log", "base:leaves"),
        // Worked stone over a spring. The desert's is the pyramid.
        Biome::Desert => e(Stepped, 11, 26, "base:sandstone", "base:sandstone_bricks"),
        // Always dead, and it should read that way from a distance.
        Biome::Badlands => e(Stepped, 10, 20, "base:cracked_masonry", "base:shale"),
        Biome::Savanna => e(Stepped, 12, 16, "base:packed_earth", "base:mud"),
        Biome::Scrubland => e(Stepped, 8, 22, "base:slate", "base:slate_bricks"),
        // Pillars, open to the sky.
        Biome::Plains => e(Ring, 9, 14, "base:granite", "base:granite_bricks"),
        Biome::Tundra => e(Ring, 10, 11, "base:slate", "base:stone"),
        // A crown over the ground.
        Biome::Arctic => e(Cap, 9, 18, "base:ice", "base:snow"),
        Biome::Mountains => e(Cap, 11, 20, "base:stone", "base:marble_bricks"),
        // Not a province culture; nothing is ever labelled with it.
        Biome::Ocean => e(Ring, 9, 14, "base:granite", "base:granite_bricks"),
    }
}

/// The blocks an edifice resolves to, looked up once per world.
#[derive(Clone, Copy)]
pub struct Materials {
    pub shell: BlockId,
    pub crown: BlockId,
}

/// What stands at this offset from the site, if anything.
///
/// `dx`/`dz` are offsets from the site column, `dy` the height above
/// the site's ground. `None` leaves the terrain alone — which is how
/// doorways, chambers and the gaps between pillars are cut.
pub fn block_at(e: &Edifice, m: &Materials, dx: i32, dy: i32, dz: i32) -> Option<BlockId> {
    // The site's own column belongs to the spirit, at every height and
    // in every family. An edifice is a wrapper, never a lock: the axe
    // has to reach the heart. It also keeps the mass off a heart whose
    // ground came from the carved heightmap while the edifice's came
    // from a position-only estimate — where those disagree, building
    // here buried the very thing the monument marks. And it leaves the
    // stepped ones a light-well down to their chamber, which both the
    // ledger's search and the spawn-in-the-dark problem want.
    if dx == 0 && dz == 0 {
        return None;
    }
    match e.family {
        Family::Bole => bole(e, m, dx, dy, dz),
        Family::Stepped => stepped(e, m, dx, dy, dz),
        Family::Ring => ring(e, m, dx, dy, dz),
        Family::Cap => cap(e, m, dx, dy, dz),
    }
}

/// A hollow trunk with a flare of roots and a canopy above it. The
/// heart stands at the foot inside, and the way in is a gap in the
/// bark at ground level — an edifice is a wrapper, never a lock.
fn bole(e: &Edifice, m: &Materials, dx: i32, dy: i32, dz: i32) -> Option<BlockId> {
    let r2 = dx * dx + dz * dz;
    let trunk = 3;
    // Roots flare out at the base and taper away over six blocks.
    let flare = if dy < 6 { (6 - dy) / 2 } else { 0 };
    let outer = trunk + flare;
    let bark = dy <= e.rise - 8;
    if bark && r2 <= outer * outer {
        // Hollow above the doorway, so the chamber is walkable.
        let hollow = (1..=e.rise - 12).contains(&dy) && r2 <= (trunk - 1) * (trunk - 1);
        // A mouth on the -Z face: two wide, three tall.
        let mouth = (1..=3).contains(&dy) && dz < 0 && dx.abs() <= 1;
        if hollow || mouth {
            return None;
        }
        return Some(m.shell);
    }
    // The canopy: a broad dome riding the top third of the trunk.
    let crown_base = e.rise - 12;
    if dy >= crown_base {
        let t = (dy - crown_base) as f32 / 12.0_f32.max(1.0);
        // Widest a third of the way up, tapering to a point.
        let w = (e.reach as f32 * (1.0 - (t - 0.35).abs() * 1.4)).max(0.0) as i32;
        if w > 0 && r2 <= w * w {
            return Some(m.crown);
        }
    }
    None
}

/// Courses of stone stepping inward, hollow, with a passage in from
/// one face and a chamber over the heart.
fn stepped(e: &Edifice, m: &Materials, dx: i32, dy: i32, dz: i32) -> Option<BlockId> {
    if dy < 0 || dy > e.rise {
        return None;
    }
    // Each course pulls in, so the silhouette steps.
    let t = dy as f32 / e.rise as f32;
    let half = ((e.reach as f32) * (1.0 - t)).round() as i32;
    if dx.abs() > half || dz.abs() > half {
        return None;
    }
    // The chamber: a room over the site, tall enough to stand in.
    let room = e.reach / 3;
    if (1..=5).contains(&dy) && dx.abs() <= room && dz.abs() <= room {
        return None;
    }
    // The passage in, on the -Z face at ground level.
    if (1..=3).contains(&dy) && dx.abs() <= 1 && dz < 0 {
        return None;
    }
    // Facing on the outermost shell, plain stone within.
    let skin = dx.abs() == half || dz.abs() == half || dy == e.rise;
    Some(if skin { m.crown } else { m.shell })
}

/// Pillars standing around the site with lintels across their tops.
/// Nothing over the heart itself: a ring is open to the sky.
fn ring(e: &Edifice, m: &Materials, dx: i32, dy: i32, dz: i32) -> Option<BlockId> {
    if dy < 0 || dy > e.rise {
        return None;
    }
    let r2 = dx * dx + dz * dz;
    let inner = e.reach - 2;
    let on_ring = r2 <= e.reach * e.reach && r2 >= inner * inner;
    if !on_ring {
        return None;
    }
    // Eight uprights, and lintels bridging their heads.
    let ang = (dz as f32).atan2(dx as f32);
    let seg = (ang / std::f32::consts::TAU * 8.0 + 8.5).floor() as i32 % 8;
    let centred = {
        let a = seg as f32 / 8.0 * std::f32::consts::TAU;
        let (px, pz) = (
            (a.cos() * (e.reach - 1) as f32).round() as i32,
            (a.sin() * (e.reach - 1) as f32).round() as i32,
        );
        (dx - px).abs() <= 1 && (dz - pz).abs() <= 1
    };
    if dy >= e.rise - 1 {
        return Some(m.crown); // the lintel course, all the way round
    }
    centred.then_some(m.shell)
}

/// A crown over the ground: terraces on a summit, an arch of ice. The
/// site stays open — this stands around and above, not on top.
fn cap(e: &Edifice, m: &Materials, dx: i32, dy: i32, dz: i32) -> Option<BlockId> {
    if dy < 0 || dy > e.rise {
        return None;
    }
    let r2 = dx * dx + dz * dz;
    // Terraces: a stack of rings, each narrower and one course thick.
    let step = (dy / 4).max(0);
    let outer = e.reach - step * 2;
    if outer <= 1 {
        return None;
    }
    let inner = outer - 2;
    if r2 > outer * outer || r2 < inner * inner {
        return None;
    }
    // Leave the approach open on the -Z side so the site is walkable.
    if dz < 0 && dx.abs() <= 1 {
        return None;
    }
    Some(if dy % 4 == 0 { m.crown } else { m.shell })
}

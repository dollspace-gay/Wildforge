//! Locomotion: A* over the streamed mirror, waypoint steering, and
//! the follow standing-behavior. The agent moves only by walking the
//! same physics a player walks — the planner proposes, gravity
//! disposes.

use super::*;

/// A planned route: cells to walk, and whether it only gets partway.
type CellPath = (Vec<crate::planet::BlockPos>, bool);

/// Forward/right basis for a yaw (the camera convention: yaw 0 looks
/// +X, pi/2 looks +Z).
pub fn basis(yaw: f32) -> (Vec3, Vec3) {
    let fwd = Vec3::new(yaw.cos(), 0.0, yaw.sin());
    let right = Vec3::new(-yaw.sin(), 0.0, yaw.cos());
    (fwd, right)
}

pub fn cell_of(pos: crate::planet::EntityPos) -> Option<crate::planet::BlockPos> {
    crate::planet::EntityPos::new(pos.face(), pos.u(), pos.y() + 0.05, pos.v())
        .ok()?
        .block()
}

fn center(c: crate::planet::BlockPos) -> crate::planet::EntityPos {
    c.entity_at_height(0.0)
}

impl Agent {
    fn passable_at(&self, pos: crate::planet::BlockPos) -> bool {
        let b = self.world.get_block_at(pos);
        !self.reg.is_solid(b)
    }

    fn wet_at(&self, pos: crate::planet::BlockPos) -> bool {
        self.reg
            .water_volume(self.world.get_block_at(pos))
            .is_some()
    }

    /// Can the agent stand (or tread water) with feet in this cell?
    pub fn stands_at(&self, pos: crate::planet::BlockPos) -> bool {
        let Some(above) = pos.offset(0, 1, 0) else {
            return false;
        };
        let Some(below) = pos.offset(0, -1, 0) else {
            return false;
        };
        self.passable_at(pos)
            && self.passable_at(above)
            && (self.reg.is_solid(self.world.get_block_at(below))
                || self.wet_at(pos)
                || self.wet_at(below))
    }

    /// A* from start to goal over walkable cells: flat steps, one-up
    /// jumps, drops to three, and swimming. Returns the cell path
    /// (goal-inclusive), or a best-effort partial toward the goal,
    /// flagged. None when no progress is possible at all.
    pub fn astar(
        &self,
        start: crate::planet::BlockPos,
        goal: crate::planet::BlockPos,
    ) -> Option<CellPath> {
        use std::cmp::Reverse;
        use std::collections::{BinaryHeap, HashMap};
        let h = |c: crate::planet::BlockPos| {
            c.entity_center()
                .horizontal_distance_to(goal.entity_center())
                + (i32::from(c.y()) - i32::from(goal.y())).unsigned_abs() as f32
        };
        let mut open = BinaryHeap::new();
        let mut came: HashMap<crate::planet::BlockPos, crate::planet::BlockPos> = HashMap::new();
        let mut g: HashMap<crate::planet::BlockPos, f32> = HashMap::new();
        g.insert(start, 0.0);
        open.push((Reverse((h(start) * 10.0) as i64), start));
        let mut best = start;
        let mut best_h = h(start);
        let mut expanded = 0;
        while let Some((_, cur)) = open.pop() {
            expanded += 1;
            if expanded > 30_000 {
                break;
            }
            if h(cur) < best_h {
                best_h = h(cur);
                best = cur;
            }
            if cur.surface() == goal.surface()
                && (i32::from(cur.y()) - i32::from(goal.y())).abs() <= 1
            {
                let mut path = vec![cur];
                let mut c = cur;
                while let Some(&p) = came.get(&c) {
                    path.push(p);
                    c = p;
                }
                path.reverse();
                return Some((path, false));
            }
            let gc = g[&cur];
            let mut push =
                |n: crate::planet::BlockPos, cost: f32, came_from: crate::planet::BlockPos| {
                    let ng = gc + cost;
                    if g.get(&n).is_none_or(|&old| ng < old) {
                        g.insert(n, ng);
                        came.insert(n, came_from);
                        open.push((Reverse(((ng + h(n)) * 10.0) as i64), n));
                    }
                };
            let swimming = self.wet_at(cur);
            for direction in [
                crate::planet::Direction6::East,
                crate::planet::Direction6::North,
                crate::planet::Direction6::West,
                crate::planet::Direction6::South,
            ] {
                let Some(next) = crate::planet::step6(cur, direction).map(|step| step.pos) else {
                    continue;
                };
                // Same level.
                if self.stands_at(next) {
                    push(next, 1.0, cur);
                }
                // Step/jump up one (headroom over the current cell).
                if cur
                    .offset(0, 2, 0)
                    .is_some_and(|head| self.passable_at(head))
                    && let Some(up) = next.offset(0, 1, 0)
                    && self.stands_at(up)
                {
                    push(up, 1.6, cur);
                }
                // Drop down as far as three.
                for dy in 1..=3 {
                    if !self.passable_at(next)
                        || next
                            .offset(0, 1, 0)
                            .is_none_or(|above| !self.passable_at(above))
                    {
                        break;
                    }
                    let Some(down) = next.offset(0, -dy, 0) else {
                        break;
                    };
                    if self.stands_at(down) {
                        push(down, 1.0 + dy as f32 * 0.4, cur);
                        break;
                    }
                    if !self.passable_at(down) {
                        break;
                    }
                }
            }
            if swimming {
                // Vertical swimming in a water column.
                for dy in [1, -1] {
                    let Some(n) = cur.offset(0, dy, 0) else {
                        continue;
                    };
                    if self.wet_at(n) || self.stands_at(n) {
                        push(n, 1.4, cur);
                    }
                }
            }
        }
        // No route: hand back progress toward the goal if any exists.
        if best != start && best_h < h(start) - 2.0 {
            let mut path = vec![best];
            let mut c = best;
            while let Some(&p) = came.get(&c) {
                path.push(p);
                c = p;
            }
            path.reverse();
            return Some((path, true));
        }
        None
    }

    /// Plan and start walking. Returns whether a path (full or
    /// partial) exists; progress is reported through events.
    pub fn go_to(&mut self, goal: crate::planet::BlockPos) -> Result<bool, String> {
        let start = cell_of(self.player.pos).ok_or("agent is outside the voxel shell")?;
        // Snap the goal to a standable cell at-or-near the asked spot.
        let goal = (0..8)
            .find_map(|d| {
                [goal.offset(0, -d, 0), goal.offset(0, d, 0)]
                    .into_iter()
                    .flatten()
                    .find(|&pos| self.stands_at(pos))
            })
            .ok_or_else(|| "no standable ground at the goal".to_string())?;
        let Some((path, partial)) = self.astar(start, goal) else {
            return Err("no route from here".into());
        };
        self.behavior = Behavior::GoTo { path, goal };
        self.stuck_probe = (self.player.pos, 0.0);
        Ok(!partial)
    }

    /// Steer along the goto path; called from the behavior tick.
    pub(super) fn tick_goto(
        &mut self,
        mut path: Vec<crate::planet::BlockPos>,
        goal: crate::planet::BlockPos,
        dt: f32,
    ) -> physics::Input {
        // Pop reached waypoints.
        while let Some(&wp) = path.first() {
            let c = center(wp);
            let flat = self.player.pos.horizontal_distance_to(c);
            if flat < 0.45 && (self.player.pos.y() - f32::from(wp.y())).abs() < 1.4 {
                path.remove(0);
            } else {
                break;
            }
        }
        let Some(&wp) = path.first() else {
            self.event(format!(
                "arrived at {} {} {} {}",
                goal.face().name(),
                goal.u(),
                goal.y(),
                goal.v()
            ));
            return idle(self.player.in_water);
        };
        // Stuck? Replan once per probe window; give up if pinned.
        self.stuck_probe.1 += dt;
        if self.stuck_probe.1 > 2.0 {
            let moved = self.player.pos.distance_to(self.stuck_probe.0);
            self.stuck_probe = (self.player.pos, 0.0);
            if moved < 0.4 {
                let Some(start) = cell_of(self.player.pos) else {
                    return idle(self.player.in_water);
                };
                match self.astar(start, goal) {
                    Some((p, _)) if p.len() > 1 => path = p,
                    _ => {
                        self.event(format!(
                            "stuck at {} {:.0} {:.0} {:.0}; gave up the walk",
                            self.player.pos.face().name(),
                            self.player.pos.u(),
                            self.player.pos.y(),
                            self.player.pos.v()
                        ));
                        return idle(self.player.in_water);
                    }
                }
            }
        }
        let input = self.steer_toward(center(wp), i32::from(wp.y()));
        self.behavior = Behavior::GoTo { path, goal };
        input
    }

    /// Chase the leader's breadcrumb trail, hanging back `distance`.
    pub(super) fn tick_follow(&mut self, id: u32, distance: f32, dt: f32) -> physics::Input {
        let leader = self.players.get(&id).map(|(_, p, _)| *p);
        let Some(leader_pos) = leader else {
            // Gone from the stream: walk out the remaining trail, then
            // report at last-seen.
            let crumb = self.trail.get_mut(&id).and_then(|t| {
                while t
                    .front()
                    .is_some_and(|c| c.distance_to(self.player.pos) < 1.2)
                {
                    t.pop_front();
                }
                t.front().copied()
            });
            return match crumb {
                Some(c) => {
                    let input = self.steer_toward(c, c.y().floor() as i32);
                    self.behavior = Behavior::Follow { id, distance };
                    input
                }
                None => {
                    self.event("lost the trail; standing at last-seen".into());
                    idle(self.player.in_water)
                }
            };
        };
        let gap = leader_pos.distance_to(self.player.pos);
        // Drop crumbs we've reached; never chase crumbs inside the
        // personal-space ring around the leader.
        let target = {
            let t = self.trail.entry(id).or_default();
            while t.front().is_some_and(|c| {
                c.distance_to(self.player.pos) < 1.2 || c.distance_to(leader_pos) < distance
            }) {
                t.pop_front();
            }
            t.front().copied()
        };
        self.behavior = Behavior::Follow { id, distance };
        if gap <= distance + 0.5 && target.is_none() {
            self.stuck_probe = (self.player.pos, 0.0);
            return idle(self.player.in_water); // close enough: stand or tread water
        }
        // Stuck on the trail: replan through A* straight to the leader.
        self.stuck_probe.1 += dt;
        if self.stuck_probe.1 > 2.5 {
            let moved = self.player.pos.distance_to(self.stuck_probe.0);
            self.stuck_probe = (self.player.pos, 0.0);
            if moved < 0.4 {
                let route = cell_of(self.player.pos)
                    .zip(cell_of(leader_pos))
                    .and_then(|(start, goal)| self.astar(start, goal));
                if let Some((path, _)) = route {
                    let t = self.trail.entry(id).or_default();
                    t.clear();
                    t.extend(path.into_iter().map(center));
                } else {
                    self.event("can't reach you from here".into());
                }
            }
        }
        let aim = target.unwrap_or(leader_pos);
        self.steer_toward(aim, aim.y().floor() as i32)
    }

    /// Face and walk toward a point; jump for lips and walls.
    fn steer_toward(
        &mut self,
        target: crate::planet::EntityPos,
        target_feet_y: i32,
    ) -> physics::Input {
        let delta = self.player.pos.local_delta_to(target);
        let d = Vec3::new(delta.x, 0.0, delta.z);
        if d.length() > 0.01 {
            self.yaw = d.z.atan2(d.x);
        }
        let climbing = target_feet_y as f32 > self.player.pos.y() + 0.3;
        physics::Input {
            forward: 1.0,
            strafe: 0.0,
            jump: (climbing && self.player.on_ground)
                || self.player.pushed_wall
                || self.player.in_water,
            sprint: d.length() > 6.0,
        }
    }
}

pub fn idle(in_water: bool) -> physics::Input {
    physics::Input {
        forward: 0.0,
        strafe: 0.0,
        // Agents think in multi-second macros. Ordinary player swim input is
        // their competence-layer equivalent of treading water between turns.
        jump: in_water,
        sprint: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_agent_treads_water_without_moving_horizontally() {
        let wet = idle(true);
        assert!(wet.jump);
        assert_eq!(wet.forward, 0.0);
        assert_eq!(wet.strafe, 0.0);
        assert!(!wet.sprint);

        assert!(!idle(false).jump);
    }
}

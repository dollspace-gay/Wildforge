//! Locomotion: A* over the streamed mirror, waypoint steering, and
//! the follow standing-behavior. The agent moves only by walking the
//! same physics a player walks — the planner proposes, gravity
//! disposes.

use super::*;

/// A planned route: cells to walk, and whether it only gets partway.
type CellPath = (Vec<(i32, i32, i32)>, bool);

/// Forward/right basis for a yaw (the camera convention: yaw 0 looks
/// +X, pi/2 looks +Z).
pub fn basis(yaw: f32) -> (Vec3, Vec3) {
    let fwd = Vec3::new(yaw.cos(), 0.0, yaw.sin());
    let right = Vec3::new(-yaw.sin(), 0.0, yaw.cos());
    (fwd, right)
}

pub fn cell_of(pos: Vec3) -> (i32, i32, i32) {
    (
        pos.x.floor() as i32,
        (pos.y + 0.05).floor() as i32,
        pos.z.floor() as i32,
    )
}

fn center(c: (i32, i32, i32)) -> Vec3 {
    Vec3::new(c.0 as f32 + 0.5, c.1 as f32, c.2 as f32 + 0.5)
}

impl Agent {
    fn passable(&self, x: i32, y: i32, z: i32) -> bool {
        let b = self.world.get_block(x, y, z);
        !self.reg.is_solid(b)
    }

    fn wet(&self, x: i32, y: i32, z: i32) -> bool {
        self.reg
            .water_volume(self.world.get_block(x, y, z))
            .is_some()
    }

    /// Can the agent stand (or tread water) with feet in this cell?
    pub fn stands(&self, x: i32, y: i32, z: i32) -> bool {
        self.passable(x, y, z)
            && self.passable(x, y + 1, z)
            && (self.reg.is_solid(self.world.get_block(x, y - 1, z))
                || self.wet(x, y, z)
                || self.wet(x, y - 1, z))
    }

    /// A* from start to goal over walkable cells: flat steps, one-up
    /// jumps, drops to three, and swimming. Returns the cell path
    /// (goal-inclusive), or a best-effort partial toward the goal,
    /// flagged. None when no progress is possible at all.
    pub fn astar(&self, start: (i32, i32, i32), goal: (i32, i32, i32)) -> Option<CellPath> {
        use std::cmp::Reverse;
        use std::collections::{BinaryHeap, HashMap};
        let h = |c: (i32, i32, i32)| {
            ((c.0 - goal.0).abs() + (c.1 - goal.1).abs() + (c.2 - goal.2).abs()) as f32
        };
        let mut open = BinaryHeap::new();
        let mut came: HashMap<(i32, i32, i32), (i32, i32, i32)> = HashMap::new();
        let mut g: HashMap<(i32, i32, i32), f32> = HashMap::new();
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
            if (cur.0, cur.2) == (goal.0, goal.2) && (cur.1 - goal.1).abs() <= 1 {
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
            let mut push = |n: (i32, i32, i32), cost: f32, came_from: (i32, i32, i32)| {
                let ng = gc + cost;
                if g.get(&n).is_none_or(|&old| ng < old) {
                    g.insert(n, ng);
                    came.insert(n, came_from);
                    open.push((Reverse(((ng + h(n)) * 10.0) as i64), n));
                }
            };
            let swimming = self.wet(cur.0, cur.1, cur.2);
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, nz) = (cur.0 + dx, cur.2 + dz);
                // Same level.
                if self.stands(nx, cur.1, nz) {
                    push((nx, cur.1, nz), 1.0, cur);
                }
                // Step/jump up one (headroom over the current cell).
                if self.passable(cur.0, cur.1 + 2, cur.2) && self.stands(nx, cur.1 + 1, nz) {
                    push((nx, cur.1 + 1, nz), 1.6, cur);
                }
                // Drop down as far as three.
                for dy in 1..=3 {
                    if !self.passable(nx, cur.1, nz) || !self.passable(nx, cur.1 + 1, nz) {
                        break;
                    }
                    if self.stands(nx, cur.1 - dy, nz) {
                        push((nx, cur.1 - dy, nz), 1.0 + dy as f32 * 0.4, cur);
                        break;
                    }
                    if !self.passable(nx, cur.1 - dy, nz) {
                        break;
                    }
                }
            }
            if swimming {
                // Vertical swimming in a water column.
                for dy in [1, -1] {
                    let n = (cur.0, cur.1 + dy, cur.2);
                    if self.wet(n.0, n.1, n.2) || self.stands(n.0, n.1, n.2) {
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
    pub fn go_to(&mut self, goal: (i32, i32, i32)) -> Result<bool, String> {
        let start = cell_of(self.player.pos);
        // Snap the goal to a standable cell at-or-near the asked spot.
        let goal = (0..8)
            .find_map(|d| {
                [(goal.0, goal.1 - d, goal.2), (goal.0, goal.1 + d, goal.2)]
                    .into_iter()
                    .find(|&(x, y, z)| self.stands(x, y, z))
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
        mut path: Vec<(i32, i32, i32)>,
        goal: (i32, i32, i32),
        dt: f32,
    ) -> physics::Input {
        // Pop reached waypoints.
        while let Some(&wp) = path.first() {
            let c = center(wp);
            let flat = Vec3::new(c.x - self.player.pos.x, 0.0, c.z - self.player.pos.z).length();
            if flat < 0.45 && (self.player.pos.y - wp.1 as f32).abs() < 1.4 {
                path.remove(0);
            } else {
                break;
            }
        }
        let Some(&wp) = path.first() else {
            self.event(format!("arrived at {} {} {}", goal.0, goal.1, goal.2));
            return idle();
        };
        // Stuck? Replan once per probe window; give up if pinned.
        self.stuck_probe.1 += dt;
        if self.stuck_probe.1 > 2.0 {
            let moved = (self.player.pos - self.stuck_probe.0).length();
            self.stuck_probe = (self.player.pos, 0.0);
            if moved < 0.4 {
                let start = cell_of(self.player.pos);
                match self.astar(start, goal) {
                    Some((p, _)) if p.len() > 1 => path = p,
                    _ => {
                        self.event(format!(
                            "stuck at {:.0} {:.0} {:.0}; gave up the walk",
                            self.player.pos.x, self.player.pos.y, self.player.pos.z
                        ));
                        return idle();
                    }
                }
            }
        }
        let input = self.steer_toward(center(wp), wp.1);
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
                    .is_some_and(|c| (*c - self.player.pos).length() < 1.2)
                {
                    t.pop_front();
                }
                t.front().copied()
            });
            return match crumb {
                Some(c) => {
                    let input = self.steer_toward(c, c.y.floor() as i32);
                    self.behavior = Behavior::Follow { id, distance };
                    input
                }
                None => {
                    self.event("lost the trail; standing at last-seen".into());
                    idle()
                }
            };
        };
        let gap = (leader_pos - self.player.pos).length();
        // Drop crumbs we've reached; never chase crumbs inside the
        // personal-space ring around the leader.
        let target = {
            let t = self.trail.entry(id).or_default();
            while t.front().is_some_and(|c| {
                (*c - self.player.pos).length() < 1.2 || (*c - leader_pos).length() < distance
            }) {
                t.pop_front();
            }
            t.front().copied()
        };
        self.behavior = Behavior::Follow { id, distance };
        if gap <= distance + 0.5 && target.is_none() {
            self.stuck_probe = (self.player.pos, 0.0);
            return idle(); // close enough: stand and wait
        }
        // Stuck on the trail: replan through A* straight to the leader.
        self.stuck_probe.1 += dt;
        if self.stuck_probe.1 > 2.5 {
            let moved = (self.player.pos - self.stuck_probe.0).length();
            self.stuck_probe = (self.player.pos, 0.0);
            if moved < 0.4 {
                let start = cell_of(self.player.pos);
                let goal = cell_of(leader_pos);
                if let Some((path, _)) = self.astar(start, goal) {
                    let t = self.trail.entry(id).or_default();
                    t.clear();
                    t.extend(path.into_iter().map(center));
                } else {
                    self.event("can't reach you from here".into());
                }
            }
        }
        let aim = target.unwrap_or(leader_pos);
        self.steer_toward(aim, aim.y.floor() as i32)
    }

    /// Face and walk toward a point; jump for lips and walls.
    fn steer_toward(&mut self, target: Vec3, target_feet_y: i32) -> physics::Input {
        let d = Vec3::new(
            target.x - self.player.pos.x,
            0.0,
            target.z - self.player.pos.z,
        );
        if d.length() > 0.01 {
            self.yaw = d.z.atan2(d.x);
        }
        let climbing = target_feet_y as f32 > self.player.pos.y + 0.3;
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

pub fn idle() -> physics::Input {
    physics::Input {
        forward: 0.0,
        strafe: 0.0,
        jump: false,
        sprint: false,
    }
}

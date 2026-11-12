//! The Ender Dragon's flight and its parts. The dragon takes no `LivingEntity.travel`: its `aiStep`
//! flies it, through blocks, and places eight boxes around itself every tick. What the server's AI
//! decides comes in: from 1.9 its phase's target, speeds and whether it sits, and in 1.8, which has
//! no phases, the target it picks. `inWall`, which `aiStep` finds at the end of the tick before, comes
//! in too: it needs the blocks' tags and the mobGriefing rule.
//!
//! The formula broke at these releases (the bytecode of each of the 50 was compared):
//! - 1.8: its own steering. The turn comes from the speed, and so does the span that a phase takes
//!   from the distance; the aim is the target after the AI, the rest the target before it.
//! - 1.9: phases (1.9.4 EnderDragon.java:224-263).
//! - 1.14: a climb of 0.01 in place of 0.1F, none at all straight under the target (it would be
//!   infinite), and the slide in doubles (1.13.2 :227-266, 1.14.4 EnderDragon.java:207-239).
//! - 1.17: `Math.sqrt`, and no turn straight under the target (1.16.5 :252-289, 1.17 :243-285).
//! - 1.18.2: the turn in floats (bytecode; 1.19 EnderDragon.java:258-260).
//! - 1.21.2: `applyEffectsFromBlocks` after the flight, which does nothing: Entity.isAffectedByBlocks
//!   is false for `noPhysics` (1.21.3 EnderDragon.java, 26.3 Entity.java:1036).
//!
//! The parts broke at 1.9 (a neck, and another head), 1.11 (`es[0]`, a yaw, in the head's height until
//! then), 1.14 (a left wing of height 3 until then), 1.15 (`tickPart`, whose offset is summed first)
//! and 1.21.2 (DragonFlightHistory, whose yaws subtract as floats).

use crate::math::{
    atan2_vanilla, java_float, wrap_degrees, wrap_degrees_f32, Aabb, Vec3,
};
use crate::projectile_hits::target_hitbox;
use crate::simulation::{mth_cos, mth_sin, position_of_bounding_box, relative_movement, yaw_radians};
use crate::version::MinecraftVersion;

/// DragonFlightHistory.LENGTH.
const HISTORY_LENGTH: usize = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DragonFlightSample {
    pub y: f64,
    pub yaw: f32,
}

/// DragonFlightHistory, the `positions` of a dragon before 1.21.2: where it was, tick by tick, for
/// the parts that follow it. Newest first, as `get(delay)` reads it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DragonFlightHistory {
    samples: Vec<DragonFlightSample>,
}

impl DragonFlightHistory {
    /// A history of these samples, newest first. Delays past the last read the last.
    #[must_use]
    pub fn new(samples: Vec<DragonFlightSample>) -> Self {
        Self { samples }
    }

    /// DragonFlightHistory.record, before the tick's flight: its first fills every delay (26.3
    /// DragonFlightHistory.java:23-33, 1.8.8 EnderDragon.java:130-146).
    pub fn record(&mut self, y: f64, yaw: f32) {
        let sample = DragonFlightSample { y, yaw };
        if self.samples.is_empty() {
            self.samples = vec![sample; HISTORY_LENGTH];
        } else {
            self.samples.insert(0, sample);
            self.samples.truncate(HISTORY_LENGTH);
        }
    }

    /// DragonFlightHistory#get, EnderDragon#getLatencyPos before 1.21.2: the sample `delay` ticks old.
    #[must_use]
    pub fn get(&self, delay: usize) -> DragonFlightSample {
        self.samples
            .get(delay)
            .or(self.samples.last())
            .copied()
            .unwrap_or_default()
    }

    #[must_use]
    pub fn samples(&self) -> &[DragonFlightSample] {
        &self.samples
    }
}

/// A dragon as its AI leaves it, on the server.
#[derive(Clone, Debug, PartialEq)]
pub struct DragonState {
    pub position: Vec3,
    pub velocity: Vec3,
    /// Entity.getYRot, in degrees.
    pub yaw: f32,
    /// EnderDragon.yRotA.
    pub yaw_acceleration: f32,
    /// EnderDragon.inWall: the last tick's.
    pub in_wall: bool,
    /// Entity's kept box, before 1.17.
    pub bounding_box: Option<Aabb>,
    /// DragonPhaseInstance.getFlyTargetLocation: None flies nowhere. In 1.8 the target after the AI.
    pub target: Option<Vec3>,
    /// 1.8: the target the tick began with, which steers it.
    pub steering_target: Option<Vec3>,
    /// DragonPhaseInstance.getFlySpeed, as the tick begins.
    pub fly_speed: f32,
    /// DragonPhaseInstance.getTurnSpeed, as the tick begins.
    pub turn_speed: f32,
    /// DragonPhaseInstance.isSitting.
    pub sitting: bool,
    /// Where it was, with this tick's sample once `tick_dragon` has recorded it.
    pub history: DragonFlightHistory,
}

/// `Mth.clamp` of a double; a NaN stays one (1.9.4 Mth.java, 1.20.4 Mth.java).
fn clamp(value: f64, min: f64, max: f64) -> f64 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

/// `Mth#clamp` of a float; a NaN stays one.
fn clamp_f32(value: f32, min: f32, max: f32) -> f32 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

/// Vec3.normalize's vector in the direction of the dragon's nose (`dir`).
fn heading(velocity_y: f64, yaw: f32, release: MinecraftVersion) -> Vec3 {
    let turn = yaw_radians(yaw, release);
    Vec3::new(
        f64::from(mth_sin(turn, release) as f32),
        velocity_y,
        f64::from(-(mth_cos(turn, release) as f32)),
    )
    .normalized(release)
}

/// Entity.move of what has `noPhysics`, which a dragon always has: through everything (1.8.8
/// Entity.java:574-576, 1.14.4 :431-433, 1.17 :574-575, 26.3 :766-767).
fn fly(state: &mut DragonState, release: MinecraftVersion) {
    let movement = if state.in_wall {
        state.velocity.scale(f64::from(0.8_f32))
    } else {
        state.velocity
    };
    match state.bounding_box.filter(|_| !release.at_least(1, 17, 0)) {
        Some(bounds) => {
            let moved = bounds.translated(movement);
            state.bounding_box = Some(moved);
            state.position = position_of_bounding_box(moved);
        }
        None => state.position = state.position + movement,
    }
}

/// The steering of 1.8 (1.8.8 EnderDragon.java:186-228).
fn flight_1_8(state: &mut DragonState, target: Vec3, release: MinecraftVersion) {
    let steering = state.steering_target.unwrap_or(target);
    let (dx, dy, dz) = (
        steering.x - state.position.x,
        steering.y - state.position.y,
        steering.z - state.position.z,
    );
    let climb = dy / java_float((dx * dx + dz * dz).sqrt());
    let climb = clamp(climb, f64::from(-0.6_f32), f64::from(0.6_f32));
    state.velocity.y += climb * f64::from(0.1_f32);
    state.yaw = wrap_degrees_f32(state.yaw);
    let wanted = 180.0 - atan2_vanilla(dx, dz, release) * 180.0 / f64::from(std::f32::consts::PI);
    let turn = clamp(wrap_degrees(wanted - f64::from(state.yaw)), -50.0, 50.0);
    let aim = Vec3::new(
        target.x - state.position.x,
        target.y - state.position.y,
        target.z - state.position.z,
    )
    .normalized(release);
    let direction = heading(state.velocity.y, state.yaw, release);
    let dot = (direction.dot(aim) as f32 + 0.5) / 1.5;
    let dot = if dot < 0.0 { 0.0 } else { dot };
    state.yaw_acceleration *= 0.8;
    let horizontal = (state.velocity.x * state.velocity.x + state.velocity.z * state.velocity.z).sqrt();
    let speed_float = java_float(horizontal) as f32 * 1.0 + 1.0;
    let speed = (horizontal * 1.0 + 1.0).min(40.0);
    let turning = f64::from(0.7_f32) / speed / f64::from(speed_float);
    state.yaw_acceleration = (f64::from(state.yaw_acceleration) + turn * turning) as f32;
    state.yaw += state.yaw_acceleration * 0.1;
    let span = (2.0 / (speed + 1.0)) as f32;
    let push = 0.06 * (dot * span + (1.0 - span));
    state.velocity = state.velocity
        + relative_movement(
            Vec3::new(0.0, 0.0, -1.0),
            f64::from(push),
            yaw_radians(state.yaw, release),
            release,
        );
    fly(state, release);
    let actual = state.velocity.normalized(release);
    let slide = 0.8 + 0.15 * ((actual.dot(direction) as f32 + 1.0) / 2.0);
    state.velocity.x *= f64::from(slide);
    state.velocity.z *= f64::from(slide);
    state.velocity.y *= f64::from(0.91_f32);
}

/// The dragon's flight step, and nothing else of its tick: 26.3 EnderDragon.java:201-236, 1.21.5
/// EnderDragon.java:186-225 for the rest of the 1.9 line. A dragon with no target stays as it is.
pub fn tick_dragon_flight(state: &mut DragonState, release: MinecraftVersion) {
    let Some(target) = state.target else {
        return;
    };
    // (1.8.8 EnderDragon.java:181-185 steers by its own target with a fixed 0.6; 1.9.4 EnderDragon.java:231-239 by
    // the phase's target and fly speed)
    if !release.at_least(1, 9, 0) {
        flight_1_8(state, target, release);
        return;
    }
    // (1.13.2 EnderDragon.java:241-242 divides by any distance and adds 0.1F of it to the speed; 1.14.4
    // EnderDragon.java:214-219 only by one above 0, and adds 0.01)
    let modern = release.at_least(1, 14, 0);
    let (dx, mut dy, dz) = (
        target.x - state.position.x,
        target.y - state.position.y,
        target.z - state.position.z,
    );
    let distance = dx * dx + dy * dy + dz * dz;
    let most = f64::from(state.fly_speed);
    let horizontal = (dx * dx + dz * dz).sqrt();
    // (1.16.5 EnderDragon.java:258; 1.17 EnderDragon.java:244)
    let horizontal = if release.at_least(1, 17, 0) {
        horizontal
    } else {
        java_float(horizontal)
    };
    if !modern || horizontal > 0.0 {
        dy = clamp(dy / horizontal, -most, most);
    }
    if modern {
        // (Vec3.add turns a -0.0 of the other axes into 0.0)
        state.velocity = state.velocity + Vec3::new(0.0, dy * 0.01, 0.0);
    } else {
        state.velocity.y += dy * f64::from(0.1_f32);
    }
    state.yaw = wrap_degrees_f32(state.yaw);
    let aim = Vec3::new(dx, target.y - state.position.y, dz).normalized(release);
    let direction = heading(state.velocity.y, state.yaw, release);
    let dot = (direction.dot(aim) as f32 + 0.5) / 1.5;
    let dot = if dot.is_nan() || dot > 0.0 { dot } else { 0.0 };
    // (1.16.5 EnderDragon.java:272; 1.17 EnderDragon.java:257)
    let turns = !release.at_least(1, 17, 0)
        || dx.abs() > f64::from(1.0e-5_f32)
        || dz.abs() > f64::from(1.0e-5_f32);
    if turns {
        state.yaw_acceleration *= 0.8;
        // (1.18.1 EnderDragon.java:240; 1.18.2 EnderDragon.java:258)
        if release.at_least(1, 18, 2) {
            let wanted = 180.0 - atan2_vanilla(dx, dz, release) as f32 * 57.295776 - state.yaw;
            let turn = clamp_f32(wrap_degrees_f32(wanted), -50.0, 50.0);
            state.yaw_acceleration += turn * state.turn_speed;
        } else {
            let wanted = 180.0 - atan2_vanilla(dx, dz, release) * 57.295_776_367_187_5 - f64::from(state.yaw);
            let turn = clamp(wrap_degrees(wanted), -50.0, 50.0);
            state.yaw_acceleration =
                (f64::from(state.yaw_acceleration) + turn * f64::from(state.turn_speed)) as f32;
        }
        state.yaw += state.yaw_acceleration * 0.1;
    }
    let span = (2.0 / (distance + 1.0)) as f32;
    let push = 0.06 * (dot * span + (1.0 - span));
    state.velocity = state.velocity
        + relative_movement(
            Vec3::new(0.0, 0.0, -1.0),
            f64::from(push),
            yaw_radians(state.yaw, release),
            release,
        );
    fly(state, release);
    let actual = state.velocity.normalized(release);
    if modern {
        let slide = 0.8 + 0.15 * (actual.dot(direction) + 1.0) / 2.0;
        state.velocity = Vec3::new(
            state.velocity.x * slide,
            state.velocity.y * f64::from(0.91_f32),
            state.velocity.z * slide,
        );
    } else {
        let slide = 0.8 + 0.15 * ((actual.dot(direction) as f32 + 1.0) / 2.0);
        state.velocity.x *= f64::from(slide);
        state.velocity.z *= f64::from(slide);
        state.velocity.y *= f64::from(0.91_f32);
    }
}

/// One of the boxes a dragon is hit in. The dragon itself is not pickable (26.3 EnderDragon.java:744,
/// 1.8.8 EnderDragon.java), so these are what a projectile strikes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragonPart {
    /// "head", "neck", "body", "tail" or "wing".
    pub name: &'static str,
    /// Its entity id less the dragon's: the ids that follow the dragon's, in this order (26.3
    /// EnderDragon.java:836-844; the server's constructor takes them in the same order).
    pub id_offset: i32,
    pub position: Vec3,
    pub bounding_box: Aabb,
}

/// The sizes the parts take: set every tick before 1.14, and for the left wing wrongly (1.8.8
/// EnderDragon.java:231-241, 1.13.2 :270-286), by the constructor from 1.14 (1.14.4 :61-84).
fn part_size(index: usize, release: MinecraftVersion) -> (f32, f32) {
    // (1.8.8 EnderDragon.java:232 has a head of 3 and no neck; 1.9.4 EnderDragon.java:268-269 a head of 1 and a neck)
    let neck = usize::from(release.at_least(1, 9, 0));
    let head = if release.at_least(1, 9, 0) { 1.0 } else { 3.0 };
    match index {
        0 => (head, head),
        1 if neck == 1 => (3.0, 3.0),
        i if i == 1 + neck => (5.0, 3.0),
        i if i < 5 + neck => (2.0, 2.0),
        i if i == 5 + neck => (4.0, 2.0),
        _ => (4.0, if release.at_least(1, 14, 0) { 2.0 } else { 3.0 }),
    }
}

fn degrees_to_radians(degrees: f32, release: MinecraftVersion) -> f32 {
    yaw_radians(degrees, release) as f32
}

/// The parts where a tick of `aiStep` leaves them (26.3 EnderDragon.java:249-309): after the flight,
/// from the history with the tick's sample in it.
#[must_use]
pub fn dragon_parts(state: &DragonState, release: MinecraftVersion) -> Vec<DragonPart> {
    let history = &state.history;
    let legacy = !release.at_least(1, 9, 0);
    // (1.14.4 EnderDragon.java:275 adds the offsets to the dragon's y one by one; 1.15 EnderDragon.java:323-324 sums
    // them first and adds the sum, :372)
    let summed = release.at_least(1, 15, 0);
    let sin = |angle: f32| mth_sin(f64::from(angle), release) as f32;
    let cos = |angle: f32| mth_cos(f64::from(angle), release) as f32;
    let at = state.position;
    let tilt = (history.get(5).y - history.get(10).y) as f32 * 10.0;
    let tilt = if legacy {
        tilt / 180.0 * std::f32::consts::PI
    } else {
        tilt * 0.017453292
    };
    let (cos_tilt, sin_tilt) = (cos(tilt), sin(tilt));
    let yaw = degrees_to_radians(state.yaw, release);
    let (sin_yaw, cos_yaw) = (sin(yaw), cos(yaw));
    let neck_angle = yaw - state.yaw_acceleration * 0.01;
    let (sin_neck, cos_neck) = (sin(neck_angle), cos(neck_angle));
    let place = |dx: f32, dy: f64, dz: f32| {
        Vec3::new(at.x + f64::from(dx), at.y + dy, at.z + f64::from(dz))
    };
    let mut positions = Vec::with_capacity(8);
    if legacy {
        let dy = history.get(0).y - history.get(5).y;
        let height = at.y + dy + f64::from(-sin_tilt * 5.5);
        positions.push(Vec3::new(
            at.x + f64::from(sin_neck * 5.5 * cos_tilt),
            height,
            at.z + f64::from(-cos_neck * 5.5 * cos_tilt),
        ));
    } else {
        // (1.9 and 1.10 take the newest sample's yaw for its height, es[0]: 1.9.4
        // EnderDragon.java:350, 1.10.2 EnderDragon.java:355; 1.11.2 EnderDragon.java:370 es[1])
        let newest = if release.at_least(1, 11, 0) {
            history.get(0).y
        } else {
            f64::from(history.get(0).yaw)
        };
        let lift = if state.sitting {
            -1.0
        } else {
            (history.get(5).y - newest) as f32
        };
        for reach in [6.5_f32, 5.5] {
            let height = if summed {
                at.y + f64::from(lift + sin_tilt * reach)
            } else {
                at.y + f64::from(lift) + f64::from(sin_tilt * reach)
            };
            positions.push(Vec3::new(
                at.x + f64::from(sin_neck * reach * cos_tilt),
                height,
                at.z + f64::from(-cos_neck * reach * cos_tilt),
            ));
        }
    }
    positions.push(place(sin_yaw * 0.5, 0.0, -cos_yaw * 0.5));
    let base = history.get(5);
    for index in 0..3 {
        let sample = history.get(12 + index * 2);
        // (1.21.1 EnderDragon.java:333; 1.21.2 EnderDragon.java:306)
        let wrapped = if release.at_least(1, 21, 2) {
            wrap_degrees(f64::from(sample.yaw - base.yaw))
        } else {
            wrap_degrees(f64::from(sample.yaw) - f64::from(base.yaw))
        } as f32;
        let angle = if legacy {
            yaw + wrapped * std::f32::consts::PI / 180.0 * 1.0
        } else {
            yaw + wrapped * 0.017453292
        };
        let (sin_tail, cos_tail) = (sin(angle), cos(angle));
        let length = (index + 1) as f32 * 2.0;
        // (`y` is -sin before 1.9)
        let tilt_by = if legacy { -sin_tilt } else { sin_tilt };
        let drop = f64::from((length + 1.5) * tilt_by);
        let height = if summed {
            at.y + (sample.y - base.y - drop + 1.5)
        } else {
            at.y + (sample.y - base.y) - drop + 1.5
        };
        positions.push(Vec3::new(
            at.x + f64::from(-(sin_yaw * 1.5 + sin_tail * length) * cos_tilt),
            height,
            at.z + f64::from((cos_yaw * 1.5 + cos_tail * length) * cos_tilt),
        ));
    }
    positions.push(place(cos_yaw * 4.5, 2.0, sin_yaw * 4.5));
    positions.push(place(cos_yaw * -4.5, 2.0, sin_yaw * -4.5));
    let names: &[&'static str] = if legacy {
        &["head", "body", "tail", "tail", "tail", "wing", "wing"]
    } else {
        &["head", "neck", "body", "tail", "tail", "tail", "wing", "wing"]
    };
    positions
        .into_iter()
        .enumerate()
        .map(|(index, position)| {
            let (width, height) = part_size(index, release);
            DragonPart {
                name: names[index],
                id_offset: index as i32 + 1,
                position,
                bounding_box: target_hitbox(position, width, height, 1.0),
            }
        })
        .collect()
}

/// The history's sample, the flight and where the parts end: `EnderDragon#aiStep` of a dragon that is alive and
/// has its AI, with what its phase decided in `state`.
pub fn tick_dragon(state: &mut DragonState, release: MinecraftVersion) -> Vec<DragonPart> {
    state.yaw = wrap_degrees_f32(state.yaw);
    state.history.record(state.position.y, state.yaw);
    tick_dragon_flight(state, release);
    dragon_parts(state, release)
}

/// A living entity near the dragon's wings: not in creative or spectator mode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragonPushTarget {
    pub id: i32,
    pub position: Vec3,
    pub bounding_box: Aabb,
}

/// One `Entity.push` of the wings. An entity under both wings has one from each.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragonPush {
    pub id: i32,
    pub impulse: Vec3,
}

/// EnderDragon.knockBack of both wings, which runs where a dragon was not hurt recently (the
/// caller's to say): the box around a wing, two lower, throws what is in it away from the body. Over
/// the body's centre the push is unbounded before 1.16.4 (1.8.8 EnderDragon.java:329-339, 1.16.3
/// by the bytecode, 1.16.5 :410-422, 26.3 EnderDragon.java:366-382).
#[must_use]
pub fn dragon_wing_pushes(
    parts: &[DragonPart],
    targets: &[DragonPushTarget],
    release: MinecraftVersion,
) -> Vec<DragonPush> {
    let mut pushes = Vec::new();
    let Some(body) = parts.iter().find(|part| part.name == "body") else {
        return pushes;
    };
    let middle_x = (body.bounding_box.min.x + body.bounding_box.max.x) / 2.0;
    let middle_z = (body.bounding_box.min.z + body.bounding_box.max.z) / 2.0;
    for wing in parts.iter().filter(|part| part.name == "wing") {
        let reach = wing
            .bounding_box
            .inflate(Vec3::new(4.0, 2.0, 4.0))
            .translated(Vec3::new(0.0, -2.0, 0.0));
        for target in targets.iter().filter(|target| target.bounding_box.intersects(reach)) {
            let x = target.position.x - middle_x;
            let z = target.position.z - middle_z;
            let distance = x * x + z * z;
            // (1.16.3 EnderDragon.java:418; 1.16.4 EnderDragon.java:418)
            let distance = if release.at_least(1, 16, 4) {
                distance.max(0.1)
            } else {
                distance
            };
            pushes.push(DragonPush {
                id: target.id,
                impulse: Vec3::new(x / distance * 4.0, f64::from(0.2_f32), z / distance * 4.0),
            });
        }
    }
    pushes
}

/// The entity lists a dragon's parts enter (Level.getEntities): after the dragon, and only when the
/// dragon's own box meets the search, until 1.21.4, then after every other entity of the level
/// (1.21.3 Level.java:578-589, 1.21.4 :583, 26.3 Level.java:551).
#[must_use]
pub fn parts_follow_dragon(release: MinecraftVersion) -> bool {
    !release.at_least(1, 21, 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    const R1_8: MinecraftVersion = MinecraftVersion::new(1, 8, 8);
    const R1_10: MinecraftVersion = MinecraftVersion::new(1, 10, 2);
    const R1_11: MinecraftVersion = MinecraftVersion::new(1, 11, 2);
    const R1_13: MinecraftVersion = MinecraftVersion::new(1, 13, 2);
    const R1_14: MinecraftVersion = MinecraftVersion::new(1, 14, 4);
    const R1_16_3: MinecraftVersion = MinecraftVersion::new(1, 16, 3);
    const R1_16_4: MinecraftVersion = MinecraftVersion::new(1, 16, 4);
    const R1_18: MinecraftVersion = MinecraftVersion::new(1, 18, 1);
    const R1_18_2: MinecraftVersion = MinecraftVersion::new(1, 18, 2);
    const R26_3: MinecraftVersion = MinecraftVersion::new(26, 3, 0);

    fn flying() -> DragonState {
        DragonState {
            position: Vec3::new(0.5, 100.0, 0.5),
            velocity: Vec3::new(0.1, 0.0, -0.2),
            yaw: 30.0,
            yaw_acceleration: 2.0,
            in_wall: false,
            bounding_box: None,
            target: Some(Vec3::new(40.0, 110.0, -20.0)),
            steering_target: None,
            fly_speed: 0.6,
            turn_speed: 0.5,
            sitting: false,
            history: DragonFlightHistory::default(),
        }
    }

    fn flown(state: DragonState, release: MinecraftVersion) -> DragonState {
        let mut state = state;
        tick_dragon_flight(&mut state, release);
        state
    }

    #[test]
    fn no_target_flies_nowhere() {
        let mut state = flying();
        state.target = None;
        assert_eq!(flown(state.clone(), R26_3), state);
    }

    #[test]
    fn the_climb_is_a_tenth_as_much_from_1_14() {
        let (before, after) = (flown(flying(), R1_13), flown(flying(), R1_14));
        let climb = 10.0 / (39.5_f64 * 39.5 + 20.5 * 20.5).sqrt();
        assert!((before.velocity.y / 0.91 - climb * 0.1).abs() < 1.0e-4);
        assert!((after.velocity.y / 0.91 - climb * 0.01).abs() < 1.0e-5);
    }

    #[test]
    fn straight_under_the_target_is_no_turn_from_1_17_and_no_nan_from_1_14() {
        let mut state = flying();
        // (0.0 / 0.0 in the climb before 1.14: 1.13.2 EnderDragon.java:230)
        state.target = Some(Vec3::new(0.5, 100.0, 0.5));
        assert!(flown(state.clone(), R1_13).velocity.y.is_nan());
        assert!(flown(state.clone(), R1_14).velocity.y.is_finite());
        assert_ne!(flown(state.clone(), R1_14).yaw_acceleration, state.yaw_acceleration);
        assert_eq!(flown(state.clone(), R26_3).yaw_acceleration, state.yaw_acceleration);
    }

    #[test]
    fn the_turn_goes_to_floats_at_1_18_2() {
        let mut state = flying();
        state.yaw = 77.123;
        state.turn_speed = 0.0357;
        assert_ne!(flown(state.clone(), R1_18).yaw_acceleration, flown(state, R1_18_2).yaw_acceleration);
    }

    #[test]
    fn a_wall_slows_the_move_and_not_the_motion() {
        let mut state = flying();
        let free = flown(state.clone(), R26_3);
        state.in_wall = true;
        let held = flown(state, R26_3);
        assert_eq!(free.velocity, held.velocity);
        let moved = |flown: &DragonState| flown.position.x - 0.5;
        assert!((moved(&held) - moved(&free) * f64::from(0.8_f32)).abs() < 1.0e-12);
    }

    #[test]
    fn a_kept_box_places_it_before_1_17() {
        let mut state = flying();
        state.bounding_box = Some(Aabb::new(Vec3::new(-7.5, 100.0, -7.5), Vec3::new(8.5, 108.0, 8.5)));
        let flown = flown(state, R1_13);
        assert_eq!(flown.position, position_of_bounding_box(flown.bounding_box.unwrap()));
        assert_ne!(flown.position.x, 0.5);
    }

    #[test]
    fn one_point_eight_has_no_phases() {
        let mut state = flying();
        // (neither speed reaches it: they come from its motion)
        let flown_once = flown(state.clone(), R1_8);
        state.fly_speed = 0.1;
        state.turn_speed = 0.9;
        assert_eq!(flown(state.clone(), R1_8).velocity, flown_once.velocity);
        assert_ne!(flown(state, R1_10).velocity, flown_once.velocity);
    }

    #[test]
    fn one_point_eight_steers_by_the_target_it_began_with() {
        let mut state = flying();
        let own = flown(state.clone(), R1_8);
        state.steering_target = Some(Vec3::new(-40.0, 90.0, 60.0));
        let steered = flown(state, R1_8);
        assert_ne!(steered.yaw, own.yaw);
    }

    #[test]
    fn the_history_fills_on_its_first_sample_then_moves_on() {
        let mut history = DragonFlightHistory::default();
        history.record(5.0, 1.0);
        assert_eq!(history.samples().len(), HISTORY_LENGTH);
        history.record(6.0, 2.0);
        assert_eq!(history.get(0).y, 6.0);
        assert_eq!(history.get(1).y, 5.0);
        assert_eq!(history.get(63).y, 5.0);
    }

    fn parted(release: MinecraftVersion) -> Vec<DragonPart> {
        let mut state = flying();
        state.history = DragonFlightHistory::new(
            (0..20)
                .map(|delay| DragonFlightSample {
                    y: 100.0 + f64::from(delay) * 0.25,
                    yaw: 30.0 - delay as f32,
                })
                .collect(),
        );
        dragon_parts(&state, release)
    }

    #[test]
    fn a_neck_from_1_9() {
        assert_eq!(parted(R1_8).len(), 7);
        assert_eq!(parted(R1_8)[1].name, "body");
        assert_eq!(parted(R1_14).len(), 8);
        assert_eq!(parted(R1_14)[1].name, "neck");
        assert_eq!(parted(R1_14)[7].id_offset, 8);
    }

    #[test]
    fn the_left_wing_stands_three_high_before_1_14() {
        let height = |parts: &[DragonPart], index: usize| {
            let bounds = parts[index].bounding_box;
            bounds.max.y - bounds.min.y
        };
        assert_eq!(height(&parted(R1_13), 6), 2.0);
        assert_eq!(height(&parted(R1_13), 7), 3.0);
        assert_eq!(height(&parted(R1_14), 7), 2.0);
    }

    #[test]
    fn the_head_of_1_9_and_1_10_rises_with_a_yaw() {
        let head = |release| parted(release)[0].position.y;
        assert!(head(R1_10) - head(R1_11) > 20.0);
    }

    #[test]
    fn a_sitting_dragon_lowers_its_head() {
        let mut state = flying();
        state.history = DragonFlightHistory::new(vec![
            DragonFlightSample { y: 100.0, yaw: 30.0 };
            20
        ]);
        let up = dragon_parts(&state, R26_3)[0].position.y;
        state.sitting = true;
        assert_eq!(dragon_parts(&state, R26_3)[0].position.y, up - 1.0);
    }

    #[test]
    fn the_wings_throw_away_from_the_body_and_cap_over_it_from_1_16_4() {
        let parts = parted(R1_16_4);
        let body = parts.iter().find(|part| part.name == "body").unwrap().bounding_box;
        let (middle_x, middle_z) = ((body.min.x + body.max.x) / 2.0, (body.min.z + body.max.z) / 2.0);
        let at = Vec3::new(middle_x + 0.01, 100.0, middle_z);
        let target = DragonPushTarget {
            id: 7,
            position: at,
            bounding_box: Aabb::new(at - Vec3::new(0.3, 0.0, 0.3), at + Vec3::new(0.3, 1.8, 0.3)),
        };
        let pushed = dragon_wing_pushes(&parts, &[target], R1_16_4);
        assert_eq!(pushed.len(), 2);
        assert!((pushed[0].impulse.x - 0.01 / 0.1 * 4.0).abs() < 1.0e-9);
        assert_eq!(pushed[0].impulse.y, f64::from(0.2_f32));
        let uncapped = dragon_wing_pushes(&parts, &[target], R1_16_3);
        assert!(uncapped[0].impulse.x > 100.0);
    }

    #[test]
    fn a_part_is_found_through_its_dragon_before_1_21_4() {
        let near = Aabb::new(Vec3::ZERO, Vec3::new(2.0, 2.0, 2.0));
        let far = Aabb::new(Vec3::new(100.0, 0.0, 0.0), Vec3::new(116.0, 8.0, 16.0));
        let mut part = crate::projectile_hits::ProjectileTarget::new(2, Vec3::ZERO, near);
        assert!(part.in_search(near));
        part.dragon_box = Some(far);
        assert!(!part.in_search(near));
        part.dragon_box = Some(near);
        assert!(part.in_search(near));
    }

    #[test]
    fn nothing_out_of_reach_of_the_wings_is_pushed() {
        let parts = parted(R26_3);
        let at = Vec3::new(30.0, 100.0, 30.0);
        let target = DragonPushTarget {
            id: 7,
            position: at,
            bounding_box: Aabb::new(at, at + Vec3::new(0.6, 1.8, 0.6)),
        };
        assert!(dragon_wing_pushes(&parts, &[target], R26_3).is_empty());
    }
}

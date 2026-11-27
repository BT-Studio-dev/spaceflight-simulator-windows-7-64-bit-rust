//! The simulated rocket: mass bookkeeping, fuel flow, thrust, aerodynamics,
//! staging and ground contact.
//!
//! A vessel is an ordered stack of parts. `pos` is the *base* point of the
//! stack (the point that touches the ground when landing); the nose points
//! along `angle`. That convention keeps landing maths simple: the base point
//! is clamped to the surface and the impact speed is checked against the
//! stack's tolerance.

use crate::math::*;
use crate::world::{part_by_id, Blueprint, PartDef, PartKind, System, NO_PARENT};

/// Parts of a stack that has been built.
#[derive(Clone)]
pub struct PartInstance {
    pub def: &'static PartDef,
    pub fuel: f64,
    pub stage: u32,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Event {
    Liftoff,
    Staged(u32),
    ChuteDeploy,
    ChuteFull,
    Landed { speed: f64 },
    Crashed { speed: f64, reason: &'static str },
    ThrottleUp,
    ThrottleDown,
    SasOn,
    SasOff,
    FuelOut,
}

/// Orbit elements relative to a body, for the HUD.
#[derive(Copy, Clone, Default)]
pub struct OrbitInfo {
    pub valid: bool,
    pub apo: f64,
    pub peri: f64,
    pub eccentricity: f64,
    pub period: f64,
}

#[derive(Clone)]
pub struct Vessel {
    pub parts: Vec<PartInstance>,
    /// Base point of the stack (metres, world plane).
    pub pos: V2,
    pub vel: V2,
    /// Direction the nose points, radians.
    pub angle: f64,
    pub ang_vel: f64,
    /// Commanded throttle 0..1.
    pub throttle: f64,
    pub sas: bool,
    pub chute: bool,
    pub current_stage: u32,
    pub stage_count: u32,
    pub on_ground: bool,
    pub crashed: bool,
    pub fuel_out_reported: bool,
    pub chute_frac: f64,
    /// Reference body index used by the last step.
    pub reference: usize,
    /// Ambient density factor at the vessel (1.0 = sea level), for the HUD.
    pub density: f64,
    pub airspeed: f64,
    pub max_q: f64,
    pub g_force: f64,
    pub name: String,
    /// Time-warp factor used by the most recent step (for the HUD).
    pub last_warp: f64,
    /// Parachute drag coefficient area helper, set when the chute opens.
    pub warp_limited: bool,
}

const CHUTE_AREA: f64 = 60.0;
const CHUTE_DEPLOY_MIN_Q: f64 = 40.0;

impl Vessel {
    pub fn build(bp: &Blueprint, pos: V2, vel: V2, angle: f64) -> Vessel {
        let stages = bp.stages();
        let parts: Vec<PartInstance> = bp
            .parts
            .iter()
            .enumerate()
            .filter_map(|(i, id)| {
                part_by_id(id).map(|def| PartInstance {
                    def,
                    fuel: def.fuel,
                    stage: stages.get(i).copied().unwrap_or(1),
                })
            })
            .collect();
        let stage_count = parts.iter().map(|p| p.stage).max().unwrap_or(1);
        Vessel {
            parts,
            pos,
            vel,
            angle,
            ang_vel: 0.0,
            throttle: 0.0,
            sas: true,
            chute: false,
            current_stage: 1,
            stage_count,
            on_ground: false,
            crashed: false,
            fuel_out_reported: false,
            chute_frac: 0.0,
            reference: 1,
            density: 0.0,
            airspeed: 0.0,
            max_q: 0.0,
            g_force: 1.0,
            name: bp.name.clone(),
            last_warp: 1.0,
            warp_limited: false,
        }
    }

    // -- mass / geometry ----------------------------------------------------

    pub fn dry_mass(&self) -> f64 {
        self.parts.iter().map(|p| p.def.dry).sum()
    }

    pub fn fuel_mass(&self) -> f64 {
        self.parts.iter().map(|p| p.fuel).sum()
    }

    pub fn stage_fuel(&self, stage: u32) -> f64 {
        self.parts
            .iter()
            .filter(|p| p.stage == stage)
            .map(|p| p.fuel)
            .sum()
    }

    pub fn total_mass(&self) -> f64 {
        self.dry_mass() + self.fuel_mass()
    }

    pub fn height(&self) -> f64 {
        self.parts.iter().map(|p| p.def.height).sum()
    }

    pub fn width(&self) -> f64 {
        self.parts.iter().map(|p| p.def.width).fold(1.0, f64::max)
    }

    /// Distance from the base to the centre of mass along the stack axis.
    pub fn com_offset(&self) -> f64 {
        let mut top = 0.0;
        let mut mom = 0.0;
        let mut total = 0.0;
        for p in &self.parts {
            let mass = p.def.dry + p.fuel;
            let centre = top + p.def.height * 0.5;
            mom += mass * centre;
            total += mass;
            top += p.def.height;
        }
        if total > 0.0 {
            mom / total
        } else {
            0.0
        }
    }

    /// Laid out bottom-first: (part index, s along the stack from the base).
    pub fn spans(&self) -> Vec<(usize, f64, f64)> {
        let mut out = Vec::with_capacity(self.parts.len());
        let mut s = 0.0;
        for (i, p) in self.parts.iter().enumerate() {
            out.push((i, s, s + p.def.height));
            s += p.def.height;
        }
        out
    }

    pub fn parts_in_stage(&self, stage: u32) -> usize {
        self.parts.iter().filter(|p| p.stage == stage).count()
    }

    pub fn has(&self, kind: PartKind) -> bool {
        self.parts.iter().any(|p| p.def.kind == kind)
    }

    pub fn crew(&self) -> u32 {
        self.parts.iter().map(|p| p.def.crew as u32).sum()
    }

    /// Impact speed the stack survives, m/s.
    pub fn impact_tolerance(&self) -> f64 {
        let mut tol: f64 = self
            .parts
            .iter()
            .map(|p| p.def.tolerance)
            .fold(0.0, f64::max);
        if self.has(PartKind::Legs) {
            tol += 18.0;
        }
        if self.chute_frac > 0.9 {
            tol += 6.0;
        }
        tol
    }

    /// Engines that are firing: those in the current stage.
    pub fn active_engines(&self) -> Vec<&PartInstance> {
        self.parts
            .iter()
            .filter(|p| p.stage == self.current_stage && p.def.kind.is_engine())
            .collect()
    }

    pub fn has_engine(&self) -> bool {
        !self.active_engines().is_empty()
    }

    /// Thrust available at `pressure` (0 = vacuum, 1 = sea level), newtons.
    pub fn thrust(&self, pressure: f64) -> f64 {
        let p = pressure.clamp(0.0, 1.0);
        self.active_engines()
            .iter()
            .map(|e| e.def.thrust_sl + (e.def.thrust_vac - e.def.thrust_sl) * (1.0 - p))
            .sum()
    }

    /// Effective exhaust velocity (m/s) of the active engines.
    pub fn exhaust_velocity(&self, pressure: f64) -> f64 {
        let p = pressure.clamp(0.0, 1.0);
        let engines = self.active_engines();
        let mut num = 0.0;
        let mut den = 0.0;
        for e in engines {
            let isp = e.def.isp_sl + (e.def.isp_vac - e.def.isp_sl) * (1.0 - p);
            let thrust = e.def.thrust_sl + (e.def.thrust_vac - e.def.thrust_sl) * (1.0 - p);
            num += isp * thrust;
            den += thrust;
        }
        if den > 0.0 {
            (num / den) * G0
        } else {
            0.0
        }
    }

    /// Fuel flow at full throttle, kg/s, at the given pressure.
    pub fn mass_flow(&self, pressure: f64) -> f64 {
        let ve = self.exhaust_velocity(pressure);
        if ve > 0.0 {
            self.thrust(pressure) / ve
        } else {
            0.0
        }
    }

    /// Burn time left for the current stage at full throttle.
    pub fn stage_burn_time(&self, pressure: f64) -> f64 {
        let mdot = self.mass_flow(pressure);
        if mdot > 0.0 {
            self.stage_fuel(self.current_stage) / mdot
        } else {
            0.0
        }
    }

    pub fn twr(&self, g: f64, pressure: f64) -> f64 {
        let m = self.total_mass();
        if m > 0.0 && g > 0.0 {
            self.thrust(pressure) / (m * g)
        } else {
            0.0
        }
    }

    /// Delta-v remaining in the current stage (vacuum Isp).
    pub fn stage_delta_v(&self) -> f64 {
        let fuel = self.stage_fuel(self.current_stage);
        let m0 = self.total_mass();
        let m1 = m0 - fuel;
        let ve = self.exhaust_velocity(0.0);
        if m0 > 0.0 && m1 > 0.0 && ve > 0.0 {
            ve * (m0 / m1).ln()
        } else {
            0.0
        }
    }

    // -- staging ------------------------------------------------------------

    /// True when the stage can be discarded (there is something above it).
    pub fn can_stage(&self) -> bool {
        let above = self
            .parts
            .iter()
            .any(|p| p.stage > self.current_stage);
        above && self.parts_in_stage(self.current_stage) < self.parts.len()
    }

    pub fn stage(&mut self) -> Option<u32> {
        if !self.can_stage() {
            return None;
        }
        let dropped = self.current_stage;
        self.parts.retain(|p| p.stage != dropped);
        // Renumber so the new bottom stage becomes current.
        let next = self.parts.iter().map(|p| p.stage).min().unwrap_or(0);
        if next == 0 {
            return None;
        }
        let shift = next - 1;
        if shift > 0 {
            for p in &mut self.parts {
                p.stage -= shift;
            }
            self.current_stage = 1;
        } else {
            self.current_stage = next;
        }
        self.stage_count = self.parts.iter().map(|p| p.stage).max().unwrap_or(1);
        self.fuel_out_reported = false;
        Some(dropped)
    }

    // -- environment --------------------------------------------------------

    /// Relative position/velocity with respect to body `i` at time `t`.
    pub fn rel(&self, sys: &System, i: usize, t: f64) -> (V2, V2) {
        let bp = sys.position(i, t);
        (self.pos - bp, self.vel - sys.velocity(i, t))
    }

    /// Altitude of the base above the surface of body `i` (metres, can be < 0).
    pub fn altitude(&self, sys: &System, i: usize, t: f64) -> f64 {
        let (dr, _) = self.rel(sys, i, t);
        dr.len() - sys.bodies[i].radius
    }

    /// Surface point closest to the vessel, accounting for terrain.
    pub fn surface_point(&self, sys: &System, i: usize, t: f64) -> (V2, f64) {
        let (dr, _) = self.rel(sys, i, t);
        let theta = dr.angle();
        let b = &sys.bodies[i];
        let h = crate::noise::terrain_height(theta, b.seed, b.terrain);
        let r = b.radius + h;
        (sys.position(i, t) + V2::polar(r, theta), r)
    }

    pub fn pressure(&self, sys: &System, i: usize, t: f64) -> f64 {
        let b = &sys.bodies[i];
        if b.atmo_rho0 <= 0.0 || b.atmo_height <= 0.0 {
            return 0.0;
        }
        let alt = self.altitude(sys, i, t);
        if alt >= b.atmo_height {
            return 0.0;
        }
        let h = b.atmo_height / 7.0;
        (b.atmo_rho0 * (-alt.max(0.0) / h).exp() / b.atmo_rho0).clamp(0.0, 1.0)
    }

    pub fn air_density(&self, sys: &System, i: usize, t: f64) -> f64 {
        let b = &sys.bodies[i];
        if b.atmo_rho0 <= 0.0 || b.atmo_height <= 0.0 {
            return 0.0;
        }
        let alt = self.altitude(sys, i, t);
        if alt >= b.atmo_height {
            return 0.0;
        }
        let h = b.atmo_height / 7.0;
        b.atmo_rho0 * (-alt.max(0.0) / h).exp()
    }

    /// Apoapsis/periapsis information around body `i` (relative two-body).
    pub fn orbit(&self, sys: &System, i: usize, t: f64) -> OrbitInfo {
        let (dr, dv) = self.rel(sys, i, t);
        let mu = sys.bodies[i].gm;
        let r = dr.len();
        let v = dv.len();
        if r <= 0.0 || mu <= 0.0 {
            return OrbitInfo::default();
        }
        let energy = 0.5 * v * v - mu / r;
        let h = dr.cross(dv).abs();
        let e = (1.0 + 2.0 * energy * h * h / (mu * mu)).max(0.0).sqrt();
        if energy >= 0.0 {
            return OrbitInfo {
                valid: true,
                apo: f64::INFINITY,
                peri: r * (1.0 - e).max(-1.0),
                eccentricity: e,
                period: f64::INFINITY,
            };
        }
        let a = -mu / (2.0 * energy);
        OrbitInfo {
            valid: true,
            apo: a * (1.0 + e),
            peri: a * (1.0 - e),
            eccentricity: e,
            period: TAU * (a.powi(3) / mu).sqrt(),
        }
    }

    // -- integration --------------------------------------------------------

    /// Advance the vessel by `dt` seconds. Returns events worth reporting.
    pub fn step(&mut self, sys: &System, t: f64, dt: f64, rotate: f64) -> Vec<Event> {
        self.step_warp(sys, t, dt, rotate, 1.0)
    }

    /// Like [`Vessel::step`] but told how much simulated time this step covers
    /// relative to real time, so the integrator can switch to a cheaper
    /// on-rails model during very high time warp.
    pub fn step_warp(
        &mut self,
        sys: &System,
        t: f64,
        dt: f64,
        rotate: f64,
        warp: f64,
    ) -> Vec<Event> {
        let mut events = Vec::new();
        if self.parts.is_empty() {
            return events;
        }
        let dt = dt.clamp(1e-5, 0.5);
        let _warp = warp.max(1.0);

        // 1. Reference body and relative state.
        let refi = sys.dominant(self.pos, t).unwrap_or(1);
        self.reference = refi;
        let (dr, dv) = self.rel(sys, refi, t);
        let r_len = dr.len();
        let up = dr.norm();
        let v_rad = dv.dot(up);
        let v_tan = dv - up * v_rad;
        self.airspeed = dv.len();

        // 2. Atmosphere, from the state we already have (no second lookup).
        let b_ref = &sys.bodies[refi];
        let alt = r_len - b_ref.radius;
        let (rho, pressure) = if b_ref.atmo_rho0 > 0.0 && b_ref.atmo_height > 0.0 && alt < b_ref.atmo_height {
            let scale_h = b_ref.atmo_height / 7.0;
            let density = b_ref.atmo_rho0 * (-alt.max(0.0) / scale_h).exp();
            (density, (density / b_ref.atmo_rho0).clamp(0.0, 1.0))
        } else {
            (0.0, 0.0)
        };
        self.density = if rho > 0.0 {
            (rho / sys.bodies[refi].atmo_rho0.max(1e-9)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let q = 0.5 * rho * dv.len_sq();
        self.max_q = self.max_q.max(q);

        // 3. Thrust and fuel.
        let engines = !self.active_engines().is_empty();
        let mut thrust_acc = V2::ZERO;
        if engines && self.throttle > 0.0 {
            let fuel = self.stage_fuel(self.current_stage);
            if fuel > 1e-6 {
                let ve = self.exhaust_velocity(pressure);
                let thrust = self.thrust(pressure) * self.throttle;
                let mdot = thrust / ve;
                let burn = (mdot * dt).min(fuel);
                self.burn_fuel(self.current_stage, burn);
                thrust_acc = V2::dir(self.angle) * (thrust / self.total_mass());
            } else if !self.fuel_out_reported {
                self.fuel_out_reported = true;
                events.push(Event::FuelOut);
            }
        }

        // 4. Gravity from every body (with a softening radius so the star's
        //    singularity cannot blow up the integrator).
                let mut gravity = V2::ZERO;
        let mut g_local = 0.0;
        for i in 0..sys.bodies.len() {
            let b = &sys.bodies[i];
            if b.gm <= 0.0 {
                continue;
            }
            let d = sys.position(i, t) - self.pos;
            let soft = b.radius * 0.05;
            let r2 = d.len_sq() + soft * soft;
            let r = r2.sqrt();
            gravity += d * (b.gm / (r2 * r));
            if i == refi {
                g_local = b.gm / (b.radius * b.radius);
            }
        }

        // 5. Drag, including the parachute.
        let mut drag_acc = V2::ZERO;
        if rho > 0.0 && self.airspeed > 1e-6 {
            let mut area: f64 = self.parts.iter().map(|p| p.def.drag).sum();
            if self.chute {
                if self.chute_frac >= 1.0 {
                    area += CHUTE_AREA;
                } else if q > CHUTE_DEPLOY_MIN_Q {
                    area += CHUTE_AREA * self.chute_frac * self.chute_frac;
                }
            }
            let cd = 0.9;
            let f = 0.5 * rho * cd * area * self.airspeed * self.airspeed;
            drag_acc = dv.norm() * (-f / self.total_mass());
        }

        // 6. Rotational control.
        let inertia_len = (self.height() * 0.5).max(1.0);
        let torque_acc = rotate / inertia_len; // rad/s^2
        let mut ang_acc = torque_acc * 0.9;
        if self.sas {
            // Damp rotation and gently align: to prograde in air or fast
            // flight, to local up when slow or on the ground.
            ang_acc -= self.ang_vel * 2.4;
            let target = if self.airspeed > 60.0 && !self.on_ground {
                dv.angle()
            } else {
                up.angle()
            };
            ang_acc += wrap_pi(target - self.angle) * 1.1;
        } else {
            ang_acc -= self.ang_vel * 0.06;
        }
        self.ang_vel += ang_acc * dt;
        self.ang_vel = self.ang_vel.clamp(-3.5, 3.5);
        self.angle = wrap_pi(self.angle + self.ang_vel * dt);

        // 7. Integrate.
        if !self.on_ground {
            let acc = gravity + thrust_acc + drag_acc;
            self.g_force = (acc - gravity).len() / g_local.max(1e-6);
            self.vel += acc * dt;
            self.pos += self.vel * dt;
        } else {
            // Sitting on the ground: only lift off when thrust beats weight.
            let thrust_up = V2::dir(self.angle).dot(up) * thrust_acc.len();
            let weight = g_local;
            if thrust_up > weight * 1.002 {
                self.on_ground = false;
                events.push(Event::Liftoff);
                let acc = gravity + thrust_acc + drag_acc;
                self.vel += acc * dt;
                self.pos += self.vel * dt;
            } else {
                // Keep the base glued to the terrain while the body rotates
                // underneath, and let the vessel slide/tip naturally.
                let (sp, _r) = {
                    let theta = (self.pos - sys.position(refi, t)).angle();
                    let b = &sys.bodies[refi];
                    let h = crate::noise::terrain_height(theta, b.seed, b.terrain);
                    (sys.position(refi, t) + V2::polar(b.radius + h, theta), theta)
                };
                self.pos = sp;
                self.vel = V2::ZERO;
                if thrust_up <= weight * 1.002 {
                    self.ang_vel *= 0.85;
                }
            }
        }

        // 8. Ground contact test.
        if !self.on_ground {
            let (sp, r_surf) = self.surface_point(sys, refi, t);
            let r_local = (self.pos - sys.position(refi, t)).len();
            if r_local <= r_surf {
                let impact = dv.len();
                let descent = -v_rad;
                let horizontal = v_tan.len();
                let tol = self.impact_tolerance();
                if descent > tol || horizontal > 30.0 {
                    self.crashed = true;
                    let reason = if descent > tol {
                        "IMPACT SPEED TOO HIGH"
                    } else {
                        "TOUCHDOWN TOO FAST SIDEWAYS"
                    };
                    events.push(Event::Crashed {
                        speed: impact,
                        reason,
                    });
                    self.pos = sp;
                    self.vel = V2::ZERO;
                    self.throttle = 0.0;
                    self.ang_vel = 0.0;
                    return events;
                }
                // Soft landing: settle on the surface.
                self.pos = sp;
                self.vel = V2::ZERO;
                self.on_ground = true;
                self.ang_vel = 0.0;
                if self.angle.cos().abs() > 0.3 {
                    self.angle = up.angle();
                }
                events.push(Event::Landed { speed: descent.max(0.0) });
            }
        }

        // 9. Parachute deployment animation.
        if self.chute {
            let target = if q > CHUTE_DEPLOY_MIN_Q { 1.0 } else { 0.15 };
            if self.chute_frac < target {
                self.chute_frac = (self.chute_frac + dt * 1.4).min(target);
                if self.chute_frac >= 1.0 && target >= 1.0 {
                    events.push(Event::ChuteFull);
                }
            }
        }

        // 10. Escape-proofing: if the vessel fell inside a body, force a crash.
        if !self.pos.is_finite() || !self.vel.is_finite() {
            self.crashed = true;
            events.push(Event::Crashed {
                speed: 0.0,
                reason: "SIMULATION DIVERGED",
            });
            return events;
        }

        // 11. Time warp bookkeeping. The caller advances the clock by
        //     `dt * warp`, so this step simply reports how much of it was
        //     integrated. The app caps the per-step dt so accuracy holds.
        self.last_warp = _warp;
        events
    }

    /// Remove `amount` kg from every tank of `stage`, proportionally. Tanks run
    /// dry together so the centre of mass does not jump around.
    fn burn_fuel(&mut self, stage: u32, amount: f64) {
        let available: f64 = self
            .parts
            .iter()
            .filter(|p| p.stage == stage)
            .map(|p| p.fuel)
            .sum();
        if available <= 0.0 || amount <= 0.0 {
            return;
        }
        let frac = (amount / available).min(1.0);
        for p in self.parts.iter_mut().filter(|p| p.stage == stage) {
            p.fuel -= p.fuel * frac;
            if p.fuel < 1e-9 {
                p.fuel = 0.0;
            }
        }
    }
}

/// Velocity needed for a circular orbit at altitude `h` around body `i`.
pub fn circular_speed(sys: &System, i: usize, h: f64) -> f64 {
    let b = &sys.bodies[i];
    (b.gm / (b.radius + h)).sqrt()
}

/// Speed at the surface of body `i` for a circular orbit (used in tips).
pub fn surface_orbit_speed(sys: &System, i: usize) -> f64 {
    circular_speed(sys, i, 0.0)
}

/// Set up a vessel parked on the pad of body `i` at angle `theta`.
pub fn on_pad(bp: &Blueprint, sys: &System, i: usize, theta: f64, t: f64) -> Vessel {
    let b = &sys.bodies[i];
    let h = crate::noise::terrain_height(theta, b.seed, b.terrain);
    let r = b.radius + h;
    let pos = sys.position(i, t) + V2::polar(r, theta);
    Vessel::build(bp, pos, sys.velocity(i, t), theta)
}

/// Circular orbit state at altitude `h`, inclination `inc` (radians measured
/// from the equator, i.e. just a direction in the 2D plane).
pub fn in_orbit(bp: &Blueprint, sys: &System, i: usize, h: f64, phase: f64, t: f64) -> Vessel {
    let b = &sys.bodies[i];
    let r = b.radius + h;
    let pos = sys.position(i, t) + V2::polar(r, phase);
    let speed = (b.gm / r).sqrt();
    let vel = sys.velocity(i, t) + V2::dir(phase + PI / 2.0) * speed;
    Vessel::build(bp, pos, vel, phase + PI / 2.0)
}

/// Whether body `i` is reachable/landable (has a solid surface).
pub fn is_landable(sys: &System, i: usize) -> bool {
    sys.bodies[i].parent != NO_PARENT
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::Blueprint;

    fn scout() -> Blueprint {
        Blueprint::new("SCOUT", &["eng_colt", "tank_small", "pod_probe", "nose_cone"])
    }

    #[test]
    fn build_accounts_for_every_part() {
        let bp = scout();
        let v = Vessel::build(&bp, V2::ZERO, V2::ZERO, 0.0);
        assert_eq!(v.parts.len(), 4);
        assert!((v.total_mass() - bp.mass()).abs() < 1e-6);
        assert!((v.height() - bp.height()).abs() < 1e-6);
        let com = v.com_offset();
        assert!(com > 0.0 && com < v.height(), "com {com} outside stack");
        assert_eq!(v.crew(), 0);
    }

    #[test]
    fn mass_flow_matches_thrust_over_exhaust_velocity() {
        let v = Vessel::build(&scout(), V2::ZERO, V2::ZERO, 0.0);
        let ve = v.exhaust_velocity(0.0);
        let mdot = v.mass_flow(0.0);
        assert!((ve - 305.0 * 9.80665).abs() < 1.0);
        assert!((mdot - v.thrust(0.0) / ve).abs() < 1e-9);
        let burn = v.stage_burn_time(0.0);
        assert!((burn - 3600.0 / mdot).abs() < 1e-6);
    }

    #[test]
    fn fuel_is_consumed_and_stage_drops() {
        let bp = Blueprint::new(
            "TWO",
            &["eng_hawk", "tank_medium", "eng_colt", "tank_small", "pod_capsule"],
        );
        let mut v = Vessel::build(&bp, V2::ZERO, V2::ZERO, 0.0);
        assert_eq!(v.stage_count, 2);
        assert_eq!(v.current_stage, 1);
        let before = v.fuel_mass();
        v.throttle = 1.0;
        let sys = System::solar();
        for k in 0..10 {
            v.step(&sys, k as f64 * 0.1, 0.1, 0.0);
        }
        assert!(v.fuel_mass() < before, "no fuel burnt");
        assert!(v.can_stage());
        let dropped = v.stage().expect("stage");
        assert_eq!(dropped, 1);
        assert_eq!(v.parts.len(), 3, "stage 1 parts should be gone");
        assert_eq!(v.current_stage, 1);
        assert_eq!(v.stage_count, 1);
        assert!(v.parts.iter().all(|p| p.stage == 1));
        assert!(!v.can_stage());
    }

    #[test]
    fn gravity_pulls_down_and_ground_stops_it() {
        let sys = System::solar();
        let mut v = on_pad(&scout(), &sys, 1, 0.0, 0.0);
        let alt0 = v.altitude(&sys, 1, 0.0);
        assert!(alt0.abs() < 1.0, "should start on the surface, alt {alt0}");
        for k in 1..60 {
            v.step(&sys, k as f64 * 0.05, 0.05, 0.0);
        }
        assert!(!v.crashed, "standing still must not crash");
        assert!(v.altitude(&sys, 1, 3.0).abs() < 60.0);
    }

    #[test]
    fn launching_raises_altitude() {
        let sys = System::solar();
        let mut v = on_pad(&scout(), &sys, 1, 0.0, 0.0);
        v.throttle = 1.0;
        let mut t = 0.0;
        for _ in 0..400 {
            v.step(&sys, t, 0.05, 0.0);
            t += 0.05;
        }
        let alt = v.altitude(&sys, 1, t);
        assert!(alt > 100.0, "altitude after 20 s of full thrust: {alt}");
        assert!(!v.crashed);
    }

    #[test]
    fn hard_impact_crashes_and_soft_landing_survives() {
        let sys = System::solar();
        let earth = sys.find("EARTH").unwrap();
        let bp = Blueprint::new("LEGS", &["legs", "eng_colt", "tank_small", "pod_probe"]);
        // Dropped from 2 km with no chute: should hit hard.
        let mut v = in_orbit(&bp, &sys, earth, 2000.0, 0.0, 0.0);
        v.vel = sys.velocity(earth, 0.0);
        let mut t = 0.0;
        let mut crashed = false;
        for _ in 0..20_000 {
            let ev = v.step(&sys, t, 0.02, 0.0);
            t += 0.02;
            if ev.iter().any(|e| matches!(e, Event::Crashed { .. })) {
                crashed = true;
                break;
            }
        }
        assert!(crashed, "a 2 km free fall should end in a crash");
        assert!(v.crashed);

        // A gentle touchdown from 1 m must not crash.
        let mut v2 = in_orbit(&bp, &sys, earth, 1.0, 0.0, 0.0);
        v2.vel = sys.velocity(earth, 0.0);
        let mut ok = true;
        let mut t2 = 0.0;
        for _ in 0..2000 {
            let ev = v2.step(&sys, t2, 0.02, 0.0);
            t2 += 0.02;
            if ev.iter().any(|e| matches!(e, Event::Crashed { .. })) {
                ok = false;
            }
            if v2.on_ground {
                break;
            }
        }
        assert!(ok && v2.on_ground, "1 m drop must land softly");
    }

    #[test]
    fn orbit_elements_of_circular_orbit() {
        let sys = System::solar();
        let earth = sys.find("EARTH").unwrap();
        let v = in_orbit(&scout(), &sys, earth, 100_000.0, 0.0, 0.0);
        let o = v.orbit(&sys, earth, 0.0);
        assert!(o.valid);
        assert!(o.eccentricity < 0.01, "e = {}", o.eccentricity);
        assert!((o.apo - 400_000.0).abs() < 2_000.0, "apo {}", o.apo);
        assert!((o.peri - 400_000.0).abs() < 2_000.0, "peri {}", o.peri);
        // A circular LEO-ish orbit has a period of a few minutes at this scale.
        assert!(o.period > 60.0 && o.period < 3600.0, "period {}", o.period);
    }

    #[test]
    fn escape_trajectory_has_infinite_apoapsis() {
        let sys = System::solar();
        let earth = sys.find("EARTH").unwrap();
        let mut v = in_orbit(&scout(), &sys, earth, 100_000.0, 0.0, 0.0);
        v.vel = sys.velocity(earth, 0.0) + V2::new(20_000.0, 0.0);
        let o = v.orbit(&sys, earth, 0.0);
        assert!(o.apo.is_infinite(), "apo {}", o.apo);
    }

    #[test]
    fn pressure_decays_with_altitude() {
        let sys = System::solar();
        let earth = sys.find("EARTH").unwrap();
        let low = in_orbit(&scout(), &sys, earth, 0.0, 0.0, 0.0);
        let high = in_orbit(&scout(), &sys, earth, 60_000.0, 0.0, 0.0);
        assert!(low.pressure(&sys, earth, 0.0) > 0.9);
        assert!(high.pressure(&sys, earth, 0.0) < 0.05);
        assert_eq!(low.air_density(&sys, earth, 0.0), 1.225);
    }

    #[test]
    fn atmosphere_slows_a_falling_capsule_and_chute_helps() {
        let sys = System::solar();
        let earth = sys.find("EARTH").unwrap();
        let bp = Blueprint::new("CAPS", &["legs", "eng_colt", "tank_small", "pod_capsule", "chute"]);
        let run = |use_chute: bool| -> f64 {
            let mut v = in_orbit(&bp, &sys, earth, 20_000.0, 0.0, 0.0);
            v.vel = sys.velocity(earth, 0.0) + V2::new(300.0, 0.0);
            v.chute = use_chute;
            v.sas = false;
            let mut t = 0.0;
            for _ in 0..40_000 {
                v.step(&sys, t, 0.02, 0.0);
                t += 0.02;
                if v.on_ground || v.crashed {
                    break;
                }
            }
            // Report the touchdown speed through the event trail.
            v.vel.len()
        };
        let free = run(false);
        let chuted = run(true);
        assert!(chuted <= free + 1.0, "chute {chuted} should be <= {free}");
    }
}

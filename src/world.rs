//! The solar system: celestials, their Keplerian motion and the part catalogue.
//!
//! Distances are in metres and follow the stylised "space simulator" scale the
//! game is modelled on: the Earth is 300 km in radius so that a low orbit is a
//! few minutes of flight, while interplanetary distances keep their real-world
//! proportions (1 AU = 1.496e11 m).

use crate::math::{V2, AU, G0, TAU};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum BodyKind {
    Star,
    Planet,
    Moon,
}

#[derive(Copy, Clone)]
pub struct Body {
    pub name: &'static str,
    pub kind: BodyKind,
    /// Mean radius (metres).
    pub radius: f64,
    /// Gravitational parameter GM (m^3/s^2).
    pub gm: f64,
    /// Index of the parent body (or `USIZE_MAX` for the star at the origin).
    pub parent: usize,
    /// Circular orbital radius around the parent (metres).
    pub orbit_a: f64,
    /// Orbital phase at epoch (radians).
    pub orbit_phase: f64,
    /// Rotation phase at epoch (radians).
    pub spin_phase: f64,
    /// Rotation period in seconds; negative for retrograde. 0 = tidally locked.
    pub spin_period: f64,
    /// Atmosphere: height above the surface and sea-level density (kg/m^3).
    pub atmo_height: f64,
    pub atmo_rho0: f64,
    /// Amplitude of the terrain profile, 0 for bodies you cannot land on.
    pub terrain: f64,
    /// Surface colours (low, high).
    pub color_lo: (u8, u8, u8),
    pub color_hi: (u8, u8, u8),
    /// Atmosphere tint.
    pub atmo_color: (u8, u8, u8),
    pub seed: u32,
    /// Roughness of the surface for the renderer's mottling.
    pub mottle: f32,
}

pub const NO_PARENT: usize = usize::MAX;

impl Body {
    #[inline]
    pub fn surface_gravity(&self) -> f64 {
        self.gm / (self.radius * self.radius)
    }

    #[inline]
    pub fn escape_velocity(&self) -> f64 {
        (2.0 * self.gm / self.radius).sqrt()
    }

    /// Rotation angle at mission time `t`.
    #[inline]
    pub fn spin(&self, t: f64) -> f64 {
        if self.spin_period == 0.0 {
            // Tidally locked: face the parent.
            return self.orbit_angle(t);
        }
        self.spin_phase + TAU * t / self.spin_period
    }

    #[inline]
    pub fn orbit_angle(&self, t: f64) -> f64 {
        self.orbit_phase + self.mean_motion() * t
    }

    #[inline]
    pub fn mean_motion(&self) -> f64 {
        if self.parent == NO_PARENT || self.orbit_a <= 0.0 {
            return 0.0;
        }
        (self.gm_parent() / self.orbit_a.powi(3)).sqrt()
    }

    /// Gravitational parameter of this body's parent. Resolved through the
    /// catalogue, so no global state or initialisation order is involved.
    pub fn gm_parent(&self) -> f64 {
        match self.parent {
            NO_PARENT => 0.0,
            p => System::solar_ref().bodies[p].gm,
        }
    }

    pub fn orbital_velocity(&self) -> f64 {
        if self.parent == NO_PARENT || self.orbit_a <= 0.0 {
            0.0
        } else {
            (self.gm_parent() / self.orbit_a).sqrt()
        }
    }
}

#[derive(Clone)]
pub struct System {
    pub bodies: Vec<Body>,
}

impl System {
    /// The catalogue itself, built once. Body orbital motion is a pure
    /// function of time, so a shared immutable copy is safe and lets
    /// [`Body::gm_parent`] resolve parents without plumbing.
    pub fn solar_ref() -> &'static System {
        static SYSTEM: std::sync::OnceLock<System> = std::sync::OnceLock::new();
        SYSTEM.get_or_init(System::build_solar)
    }

    /// A fresh (cheap, 4-body) copy of the catalogue.
    pub fn solar() -> System {
        System::build_solar()
    }

    fn build_solar() -> System {
        System {
            bodies: vec![
                // Sol: real-world mass and radius, sits at the origin.
                Body {
                    name: "SOL",
                    kind: BodyKind::Star,
                    radius: 6.96e8,
                    gm: 1.327_124_4e20,
                    parent: NO_PARENT,
                    orbit_a: 0.0,
                    orbit_phase: 0.0,
                    spin_phase: 0.0,
                    spin_period: 2.16e6,
                    atmo_height: 0.0,
                    atmo_rho0: 0.0,
                    terrain: 0.0,
                    color_lo: (255, 214, 120),
                    color_hi: (255, 250, 226),
                    atmo_color: (255, 190, 90),
                    seed: 1,
                    mottle: 0.55,
                },
                // Earth: 300 km radius, 1 g at the surface (game scale).
                Body {
                    name: "EARTH",
                    kind: BodyKind::Planet,
                    radius: 3.0e5,
                    gm: 8.82e11,
                    parent: 0,
                    orbit_a: AU,
                    orbit_phase: 0.0,
                    spin_phase: 0.0,
                    spin_period: 86_164.0,
                    atmo_height: 70_000.0,
                    atmo_rho0: 1.225,
                    terrain: 1400.0,
                    color_lo: (24, 62, 132),
                    color_hi: (74, 142, 88),
                    atmo_color: (110, 180, 255),
                    seed: 0x1234_5678,
                    mottle: 1.0,
                },
                // Moon: 75 km radius, tidal lock, 12 000 km out.
                Body {
                    name: "MOON",
                    kind: BodyKind::Moon,
                    radius: 7.5e4,
                    gm: 9.11e9,
                    parent: 1,
                    orbit_a: 1.2e7,
                    orbit_phase: 1.15,
                    spin_phase: 0.0,
                    spin_period: 0.0,
                    atmo_height: 0.0,
                    atmo_rho0: 0.0,
                    terrain: 900.0,
                    color_lo: (86, 86, 92),
                    color_hi: (186, 186, 190),
                    atmo_color: (0, 0, 0),
                    seed: 0x2222_3344,
                    mottle: 1.2,
                },
                // Mars: 170 km radius, thin air, rusty.
                Body {
                    name: "MARS",
                    kind: BodyKind::Planet,
                    radius: 1.7e5,
                    gm: 1.069e11,
                    parent: 0,
                    orbit_a: 1.524 * AU,
                    orbit_phase: 2.35,
                    spin_phase: 0.4,
                    spin_period: 88_642.0,
                    atmo_height: 40_000.0,
                    atmo_rho0: 0.020,
                    terrain: 2200.0,
                    color_lo: (118, 52, 30),
                    color_hi: (206, 128, 78),
                    atmo_color: (230, 150, 110),
                    seed: 0x9abc_def0,
                    mottle: 0.9,
                },
            ],
        }
    }

    pub fn len(&self) -> usize {
        self.bodies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bodies.is_empty()
    }

    /// Position of body `i` at mission time `t` (metres, heliocentric plane).
    pub fn position(&self, i: usize, t: f64) -> V2 {
        let b = &self.bodies[i];
        if b.parent == NO_PARENT {
            V2::ZERO
        } else {
            let p = self.position(b.parent, t);
            p + V2::polar(b.orbit_a, b.orbit_angle(t))
        }
    }

    /// Velocity of body `i` at mission time `t`, by finite difference of the
    /// analytic position (cheap and accurate enough for encounter maths).
    pub fn velocity(&self, i: usize, t: f64) -> V2 {
        let dt = 30.0;
        (self.position(i, t + dt) - self.position(i, t - dt)) / (2.0 * dt)
    }

    /// Find a body by name.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.bodies.iter().position(|b| b.name == name)
    }

    /// The body whose surface/atmosphere the given point is inside of, if any.
    pub fn dominant(&self, p: V2, t: f64) -> Option<usize> {
        let mut best: Option<(usize, f64)> = None;
        for i in 0..self.bodies.len() {
            let d = (self.position(i, t) - p).len() / self.bodies[i].radius;
            if best.map_or(true, |(_, bd)| d < bd) {
                best = Some((i, d));
            }
        }
        best.map(|(i, _)| i)
    }
}

// ---------------------------------------------------------------------------
// Rocket parts
// ---------------------------------------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PartKind {
    Command,
    Tank,
    Engine,
    Booster,
    Chute,
    Legs,
    Nose,
}

impl PartKind {
    pub fn label(&self) -> &'static str {
        match self {
            PartKind::Command => "POD",
            PartKind::Tank => "TANK",
            PartKind::Engine => "ENGINE",
            PartKind::Booster => "BOOSTER",
            PartKind::Chute => "CHUTE",
            PartKind::Legs => "LEGS",
            PartKind::Nose => "NOSE",
        }
    }

    pub fn is_engine(&self) -> bool {
        matches!(self, PartKind::Engine | PartKind::Booster)
    }
}

#[derive(Copy, Clone)]
pub struct PartDef {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: PartKind,
    /// Dry mass, kg.
    pub dry: f64,
    /// Propellant capacity, kg.
    pub fuel: f64,
    /// Thrust at sea level and in vacuum, N.
    pub thrust_sl: f64,
    pub thrust_vac: f64,
    /// Specific impulse (s) at sea level / vacuum.
    pub isp_sl: f64,
    pub isp_vac: f64,
    /// Size in metres: width (diameter) and height along the stack.
    pub width: f64,
    pub height: f64,
    /// Drag area (m^2).
    pub drag: f64,
    pub crew: u8,
    /// Impact speed the part survives, m/s.
    pub tolerance: f64,
    pub desc: &'static str,
}

/// Drag area of a stack part is proportional to its frontal area; computed at
/// construction time because floating point maths is not allowed in `const fn`
/// on the pinned toolchain (Rust 1.77).
fn drag_area(width: f64, height: f64) -> f64 {
    width * height * 0.6
}

const fn part(
    id: &'static str,
    name: &'static str,
    kind: PartKind,
    dry: f64,
    fuel: f64,
    width: f64,
    height: f64,
    tolerance: f64,
    desc: &'static str,
) -> PartDef {
    PartDef {
        id,
        name,
        kind,
        dry,
        fuel,
        thrust_sl: 0.0,
        thrust_vac: 0.0,
        isp_sl: 0.0,
        isp_vac: 0.0,
        width,
        height,
        drag: 0.0,
        crew: 0,
        tolerance,
        desc,
    }
}

const fn engine(
    id: &'static str,
    name: &'static str,
    kind: PartKind,
    dry: f64,
    width: f64,
    height: f64,
    thrust_sl: f64,
    thrust_vac: f64,
    isp_sl: f64,
    isp_vac: f64,
    desc: &'static str,
) -> PartDef {
    PartDef {
        id,
        name,
        kind,
        dry,
        fuel: 0.0,
        thrust_sl,
        thrust_vac,
        isp_sl,
        isp_vac,
        width,
        height,
        drag: 0.0,
        crew: 0,
        tolerance: 14.0,
        desc,
    }
}

const PARTS_RAW: &[PartDef] = &[
    part(
        "pod_capsule",
        "CAPSULE",
        PartKind::Command,
        1200.0,
        0.0,
        2.4,
        2.6,
        12.0,
        "Crew capsule: power, SAS and a heat-shielded base.",
    ),
    part(
        "pod_probe",
        "PROBE",
        PartKind::Command,
        320.0,
        0.0,
        1.6,
        1.2,
        18.0,
        "Lightweight probe core. No crew, but very light.",
    ),
    part(
        "pod_tug",
        "TUGBOAT",
        PartKind::Command,
        2600.0,
        0.0,
        3.0,
        3.2,
        10.0,
        "Heavy command module with big batteries and RCS.",
    ),
    part(
        "tank_small",
        "TANK S",
        PartKind::Tank,
        180.0,
        3600.0,
        1.8,
        3.0,
        20.0,
        "Small methalox tank.",
    ),
    part(
        "tank_medium",
        "TANK M",
        PartKind::Tank,
        520.0,
        18_000.0,
        2.4,
        6.0,
        18.0,
        "Medium tank: the workhorse of the stack.",
    ),
    part(
        "tank_large",
        "TANK L",
        PartKind::Tank,
        1350.0,
        62_000.0,
        3.2,
        10.0,
        16.0,
        "Heavy first-stage tank.",
    ),
    part(
        "tank_huge",
        "TANK XL",
        PartKind::Tank,
        3400.0,
        190_000.0,
        4.4,
        16.0,
        14.0,
        "Super-heavy core stage tank.",
    ),
    engine(
        "eng_colt",
        "COLT",
        PartKind::Engine,
        420.0,
        1.8,
        1.8,
        48_000.0,
        62_000.0,
        235.0,
        305.0,
        "Small vacuum-optimised engine for upper stages.",
    ),
    engine(
        "eng_hawk",
        "HAWK",
        PartKind::Engine,
        1650.0,
        2.4,
        2.6,
        235_000.0,
        290_000.0,
        250.0,
        310.0,
        "Medium engine, the default choice for a second stage.",
    ),
    engine(
        "eng_titan",
        "TITAN",
        PartKind::Engine,
        5200.0,
        3.4,
        3.8,
        820_000.0,
        980_000.0,
        245.0,
        295.0,
        "Big lifter engine with huge thrust.",
    ),
    PartDef {
        id: "boost_solid",
        name: "SRB",
        kind: PartKind::Booster,
        dry: 2400.0,
        fuel: 26_000.0,
        thrust_sl: 480_000.0,
        thrust_vac: 540_000.0,
        isp_sl: 205.0,
        isp_vac: 232.0,
        width: 2.0,
        height: 9.0,
        drag: 2.0 * 9.0 * 0.5,
        crew: 0,
        tolerance: 12.0,
        desc: "Solid booster: cheap thrust for the first few kilometres.",
    },
    part(
        "nose_cone",
        "NOSE",
        PartKind::Nose,
        180.0,
        0.0,
        1.8,
        2.0,
        22.0,
        "Aerodynamic nose cone: cuts drag noticeably.",
    ),
    part(
        "chute",
        "PARACHUTE",
        PartKind::Chute,
        140.0,
        0.0,
        1.4,
        0.9,
        26.0,
        "Deploy above 8 km/s of airspeed to land gently.",
    ),
    part(
        "legs",
        "LANDING LEGS",
        PartKind::Legs,
        220.0,
        0.0,
        3.2,
        1.0,
        30.0,
        "Wide legs: raises the impact tolerance of the stack.",
    ),
];

/// The full part catalogue, with drag areas filled in once on first use.
pub fn parts() -> &'static [PartDef] {
    static TABLE: std::sync::OnceLock<Vec<PartDef>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut v: Vec<PartDef> = PARTS_RAW.to_vec();
        for p in v.iter_mut() {
            p.drag = drag_area(p.width, p.height);
        }
        v
    })
}

pub fn part_by_id(id: &str) -> Option<&'static PartDef> {
    parts().iter().find(|p| p.id == id)
}

pub fn part_index(id: &str) -> Option<usize> {
    parts().iter().position(|p| p.id == id)
}

// ---------------------------------------------------------------------------
// Blueprints (the saved shape of a rocket before it is built)
// ---------------------------------------------------------------------------

/// A stack of part ids, bottom first. Staging is derived from engine placement.
#[derive(Clone, Debug, PartialEq)]
pub struct Blueprint {
    pub name: String,
    pub parts: Vec<&'static str>,
}

impl Blueprint {
    pub fn new(name: &str, parts: &[&'static str]) -> Blueprint {
        Blueprint {
            name: name.to_string(),
            parts: parts.to_vec(),
        }
    }

    pub fn mass(&self) -> f64 {
        self.parts
            .iter()
            .filter_map(|id| part_by_id(id))
            .map(|p| p.dry + p.fuel)
            .sum()
    }

    pub fn height(&self) -> f64 {
        self.parts
            .iter()
            .filter_map(|id| part_by_id(id))
            .map(|p| p.height)
            .sum()
    }

    pub fn has_command(&self) -> bool {
        self.parts
            .iter()
            .filter_map(|id| part_by_id(id))
            .any(|p| p.kind == PartKind::Command)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.parts.is_empty() {
            return Err("the stack is empty".into());
        }
        if !self.has_command() {
            return Err("add a command pod or probe core".into());
        }
        if self
            .parts
            .iter()
            .filter_map(|id| part_by_id(id))
            .any(|p| p.kind.is_engine())
        {
            Ok(())
        } else {
            Err("the stack has no engine".into())
        }
    }

    /// Stage numbers per part, bottom-first. Each engine starts a new stage;
    /// parts above it belong to that stage until the next engine is reached.
    pub fn stages(&self) -> Vec<u32> {
        let mut out = Vec::with_capacity(self.parts.len());
        let mut stage = 1u32;
        let mut seen_engine = false;
        for id in &self.parts {
            if let Some(p) = part_by_id(id) {
                if p.kind.is_engine() {
                    if seen_engine {
                        stage += 1;
                    }
                    seen_engine = true;
                }
            }
            out.push(stage);
        }
        // Anything below the first engine (rare) counts as stage 1.
        out
    }

    pub fn stage_count(&self) -> u32 {
        *self.stages().iter().max().unwrap_or(&0)
    }

    /// The delta-v of the whole stack, idealised (no drag, no gravity loss).
    pub fn delta_v(&self) -> f64 {
        let stages = self.stages();
        let mut total = 0.0;
        let mut upper_mass: f64 = self
            .parts
            .iter()
            .filter_map(|id| part_by_id(id))
            .map(|p| p.dry + p.fuel)
            .sum();
        let n = self.stage_count();
        for s in 1..=n {
            let mut stage_dry = 0.0;
            let mut stage_fuel = 0.0;
            let mut ve = 0.0;
            for (i, id) in self.parts.iter().enumerate() {
                let p = match part_by_id(id) {
                    Some(p) => p,
                    None => continue,
                };
                if stages[i] != s {
                    continue;
                }
                stage_dry += p.dry;
                stage_fuel += p.fuel;
                if p.kind.is_engine() && p.thrust_vac > 0.0 {
                    ve = p.isp_vac * G0;
                }
            }
            if ve <= 0.0 {
                upper_mass -= stage_dry + stage_fuel;
                continue;
            }
            let m0 = upper_mass;
            let m1 = (upper_mass - stage_fuel).max(stage_dry * 0.1);
            if m0 > 0.0 && m1 > 0.0 {
                total += ve * (m0 / m1).ln();
            }
            upper_mass -= stage_dry + stage_fuel;
        }
        total
    }

    pub fn twr(&self, g: f64) -> f64 {
        let thrust: f64 = self
            .parts
            .iter()
            .filter_map(|id| part_by_id(id))
            .filter(|p| p.kind.is_engine())
            .map(|p| p.thrust_sl)
            .sum();
        let mass = self.mass();
        if mass <= 0.0 {
            0.0
        } else {
            thrust / (mass * g)
        }
    }

    pub fn presets() -> Vec<Blueprint> {
        vec![
            Blueprint::new(
                "SCOUT",
                &["eng_colt", "tank_small", "pod_probe", "nose_cone"],
            ),
            Blueprint::new(
                "SPARROW",
                &[
                    "eng_hawk", "tank_medium", "eng_colt", "tank_small", "pod_capsule", "nose_cone",
                ],
            ),
            Blueprint::new(
                "PATHFINDER",
                &[
                    "eng_titan",
                    "tank_large",
                    "tank_large",
                    "eng_hawk",
                    "tank_medium",
                    "eng_colt",
                    "tank_small",
                    "pod_capsule",
                    "nose_cone",
                    "chute",
                ],
            ),
            Blueprint::new(
                "LUNAR EXPRESS",
                &[
                    "eng_titan", "tank_large", "eng_hawk", "tank_medium", "legs", "pod_capsule",
                    "chute", "nose_cone",
                ],
            ),
        ]
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_orbits_are_consistent() {
        let s = System::solar();
        assert_eq!(s.len(), 4);
        let earth = s.find("EARTH").unwrap();
        let v = s.velocity(earth, 0.0).len();
        assert!(
            (v - 29_780.0).abs() < 200.0,
            "Earth orbital speed should be ~29.8 km/s, got {v}"
        );
        let moon = s.find("MOON").unwrap();
        let vm = s.velocity(moon, 0.0).len();
        assert!(
            (vm - s.bodies[moon].orbital_velocity()).abs() < 20.0,
            "moon speed {vm}"
        );
        // Tidally locked moon must always face Earth.
        let b = &s.bodies[moon];
        assert_eq!(b.spin_period, 0.0);
        assert!((b.spin(1234.0) - b.orbit_angle(1234.0)).abs() < 1e-9);
    }

    #[test]
    fn earth_surface_gravity_is_one_g() {
        let s = System::solar();
        let e = &s.bodies[s.find("EARTH").unwrap()];
        assert!((e.surface_gravity() - 9.8).abs() < 0.05, "{}", e.surface_gravity());
    }

    #[test]
    fn orbits_are_periodic() {
        let s = System::solar();
        let moon = s.find("MOON").unwrap();
        let period = TAU / s.bodies[moon].mean_motion();
        let p0 = s.position(moon, 0.0);
        let p1 = s.position(moon, period);
        assert!((p0 - p1).len() < 1.0, "moon orbit not periodic");
    }

    #[test]
    fn part_table_is_sane() {
        for p in parts() {
            assert!(p.dry > 0.0, "{} dry mass", p.id);
            assert!(p.width > 0.0 && p.height > 0.0, "{} size", p.id);
            assert!(part_by_id(p.id).is_some());
            if p.kind.is_engine() {
                assert!(p.thrust_vac >= p.thrust_sl, "{} thrust ordering", p.id);
                assert!(p.isp_vac >= p.isp_sl, "{} isp ordering", p.id);
                assert!(p.fuel > 0.0 || p.kind == PartKind::Engine, "{} fuel", p.id);
            }
            if p.kind == PartKind::Tank {
                assert!(p.fuel > 0.0, "{} fuel", p.id);
            }
        }
    }

    #[test]
    fn staging_and_delta_v() {
        let bp = Blueprint::new(
            "SCOUT",
            &["eng_colt", "tank_small", "pod_probe", "nose_cone"],
        );
        assert!(bp.validate().is_ok());
        assert_eq!(bp.stage_count(), 1);
        let dv = bp.delta_v();
        assert!(dv > 1000.0 && dv < 20_000.0, "delta-v {dv}");

        let two = Blueprint::new(
            "TWO",
            &["eng_hawk", "tank_medium", "eng_colt", "tank_small", "pod_capsule"],
        );
        assert_eq!(two.stage_count(), 2);
        assert!(two.delta_v() > dv, "two stages should beat one");
        assert!(two.twr(9.8) > 0.0);

        let bad = Blueprint::new("BAD", &["tank_small", "nose_cone"]);
        assert!(bad.validate().is_err());
    }

    #[test]
    fn presets_are_valid() {
        for bp in Blueprint::presets() {
            assert!(bp.validate().is_ok(), "preset {} invalid: {:?}", bp.name, bp.validate());
            assert!(bp.delta_v() > 500.0, "preset {} too weak", bp.name);
        }
    }
}

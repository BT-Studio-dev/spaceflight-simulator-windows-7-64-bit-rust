//! Spaceflight Simulator: a dependency-free 2D space programme in Rust.
//!
//! The crate is split so the simulation is completely independent of the
//! platform layer:
//!
//! * [`math`], [`rng`], [`noise`], [`font`], [`png`] - small std-only helpers.
//! * [`world`] - the solar system, parts and saved blueprints.
//! * [`rocket`] - vehicle physics: mass, thrust, drag, staging, ground contact.
//! * [`render`] - software rasteriser, camera and scene drawing.
//! * [`ui`] - Aero-styled Windows 7 HUD and menus.
//! * [`app`] - the game state machine shared by the native and CI renderers.
//! * [`platform`] - Win32 window + GDI present path (Windows only).

// Rasteriser and drawing entry points legitimately take many coordinates.
#![allow(clippy::needless_range_loop)]
#![allow(clippy::too_many_arguments)]

pub mod app;
pub mod font;
pub mod fx;
pub mod math;
pub mod noise;
pub mod platform;
pub mod png;
pub mod render;
pub mod rng;
pub mod rocket;
pub mod selftest;
pub mod trace;
pub mod ui;
pub mod world;

/// Product version reported in the title bar and `--version`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The Windows build this binary is certified against.
pub const TARGET_OS: &str = "Windows 7 SP1 (6.1.7601) x64";

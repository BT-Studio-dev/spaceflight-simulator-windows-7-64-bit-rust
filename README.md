# Spaceflight Simulator — for Windows 7 SP1 x64

A small, complete 2D space programme written in **Rust with zero third-party
crates**, built specifically to run on a stock **Windows 7 SP1 64-bit** machine
(the kind of box with a Core 2 Duo and 2 GB of RAM).

Build rockets on the pad, launch them, stage, fly to orbit, and land on the
Moon or Mars. Everything you see is drawn by a hand-written software renderer:
no GPU, no DirectX, no OpenGL, no graphics driver beyond plain GDI.

*(Screenshots are rendered by `cargo run --release --bin screenshots -- --out screenshots`;
CI uploads the frames as a build artifact.)*

## What is in it

**34 parts across 12 categories** — 5 command pods (probe, capsule, lander can,
orbital lab, tugboat), 9 tanks from 900 kg to 230 t of propellant, 7 engines from
the 26 kN SPARROW to the 4.4 MN TITAN CLUSTER, 2 solid boosters, decouplers,
fairings, trusses, science bays, batteries, 2 parachutes and 2 sets of legs.
The palette scrolls with the mouse wheel and shows a scrollbar.

**8 bodies** — Sol, Venus, Earth, the Moon, Mars, Phobos, Deimos and Ceres, each
with radius, surface gravity, atmosphere and procedural terrain. Moons orbit
their planet; planets orbit the star; every surface is landable.

**6 stock rockets** — SCOUT, SPARROW, PATHFINDER, LUNAR EXPRESS, PROBE SCOUT,
HEAVY LIFTER, LANDER HOPPER and STRAP-ON, each validated to lift off (TWR > 1.05).

## Why this build is Windows 7 friendly

| Concern | What this build does |
| --- | --- |
| Toolchain | Pinned to **Rust 1.77.1** — the last stable release whose standard library still supports Windows 7 SP1 / 8 (support was removed in 1.78). A `rust-toolchain.toml` in the repo enforces it. |
| C runtime | Statically linked CRT (`+crt-static`), so **no Visual C++ redistributable** is needed. |
| APIs | Only `kernel32`, `user32` and `gdi32` entry points that exist on Windows 7 SP1; no DPI helpers introduced in Windows 8/10, no WinRT, no DirectX. |
| Dependencies | **None.** No `winapi`, no `bindgen`, no serde, no image library. The Win32 bindings, PNG writer/reader, bitmap font, noise and rasteriser are all in this repository. |
| Memory | Fixed-size framebuffer, no unbounded allocations, designed for 2 GB RAM. |
| Binary | PE32+ AMD64, no runtime DLLs beyond the Windows system DLLs. |

## Download

Grab `spaceflight-windows7-x64.zip` from the
[latest release](../../releases/latest). Unzip anywhere and double-click
`spaceflight.exe` — there is no installer and nothing to register.

Verify the download against `checksums.txt` (SHA-256) if you like.

## Running

```
spaceflight                  launch the simulator window
spaceflight --self-test      headless end-to-end self test (prints a report)
spaceflight --demo DIR       render demo PNG frames, no window needed
spaceflight --size 1024x640  initial window size
spaceflight --fullscreen     borderless full screen
spaceflight --version        version and target information
```

On a Core 2 Duo / 2 GB machine the simulator runs at the 30 Hz fixed timestep
using a 1280x760 framebuffer; use `--size 1024x600` if you want more headroom.
See [docs/WINDOWS7.md](docs/WINDOWS7.md) for notes specific to Windows 7 and
[docs/CONTROLS.md](docs/CONTROLS.md) for the full key list.

## What is in the game

* **Rocket builder** — a scrollable 34-part catalogue (see above). Stage numbers
  are derived from the part order, an engine clicked under a tank wraps a new
  lower stage, and the builder shows mass, delta-v and thrust-to-weight live.
* **Physics** — Newtonian gravity from every body, atmospheric drag with an
  exponential density profile, altitude-dependent engine thrust and Isp, mass
  flow, staging, parachutes, terrain collision with impact tolerance, and a
  surface normal force so a landed rocket sits still instead of sinking.
* **System** — 8 bodies, each with procedural terrain, atmosphere colour and
  rotation: Sol, Venus, Earth, the Moon (tidally locked), Mars, Phobos, Deimos
  and Ceres.
* **Flight** — SAS, throttle, time warp, trajectory prediction, apsis markers,
  HUD with altitude/speed/apogee/periapsis/eccentricity, and a mission log.
* **Rendering** — software rasteriser with anti-aliased discs, rings, glows and
  polygons; a 5x7 bitmap font drawn by hand; stars, planet shading, atmosphere
  haze, particle effects; a PNG writer for screenshots.

## Building from source

```powershell
rustup toolchain install 1.77.1
cargo build --release --target x86_64-pc-windows-msvc
.\target\x86_64-pc-windows-msvc\release\spaceflight.exe
```

The repository has no dependencies, so the build needs no network access beyond
the toolchain itself. On Linux/macOS you can still run all tests and generate the
demo screenshots headlessly:

```bash
cargo test --all-targets
cargo run --release --bin screenshots -- --out screenshots
```

## Continuous integration

* `.github/workflows/check.yml` — Linux job: build, full test suite, screenshot
  rendering, self test, clippy, format check.
* `.github/workflows/windows7.yml` — Windows job: Rust 1.77.1, tests, release
  build for `x86_64-pc-windows-msvc`, self test against the real `.exe`, PE
  header checks (AMD64, PE32+, no dynamic CRT), a GUI smoke run that creates and
  finds the window, then packages `spaceflight-windows7-x64.zip`. Pushing a `v*`
  tag publishes that zip on the GitHub release.

## Tests

65 unit/integration tests plus the end-to-end self test. They cover the maths, noise, PNG round-trips, part/stage bookkeeping, the
blueprint stage derivation, orbit element maths, rocket integration (gravity,
landing, liftoff, staging, drag), camera behaviour, canvas rasterisation and
clipping, scene composition (pad view, planet disc, off-screen culling), the UI
stacks and the end-to-end scripted ascent to orbit.

## License

MIT.

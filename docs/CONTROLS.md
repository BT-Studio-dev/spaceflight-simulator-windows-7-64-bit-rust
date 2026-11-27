# Controls

The simulator is keyboard and mouse driven: build on the pad, then fly.

## On the pad (rocket builder)

| Input | Action |
| --- | --- |
| **Left click** a palette entry | Add that part to the top of the stack |
| **Left click** the stack / pad buttons | Select the part, use the on-screen buttons |
| **Right click** a placed part | Remove it from the stack |
| **ENTER** | Launch the rocket |
| **ESC** | Quit |
| **F1** | Toggle the on-screen control overlay |

The pad readout shows the stack's mass, delta-v and thrust-to-weight ratio as
you build. A stack needs a command pod or probe core, an engine, and a
thrust-to-weight ratio comfortably above 1.0 to leave the ground.

Stage numbers are derived from the part order: each engine starts a stage, and
everything above it belongs to that stage. An engine clicked while the bottom of
the stack is a tank wraps a **new lower stage** underneath, which is how you add
a second stage.

## In flight

| Input | Action |
| --- | --- |
| **W / UP** | Throttle up |
| **S / DOWN** | Throttle down |
| **Z** | Full throttle |
| **X** | Cut throttle |
| **SPACE** | Stage separation (drops the burnt-out bottom stage) |
| **C** | Arm the parachute (deploys when dynamic pressure allows) |
| **T** | Toggle SAS (holds the nose steady, points prograde when fast) |
| **A / LEFT**, **D / RIGHT** | Rotate the vessel |
| **P** or **ESC** | Pause |
| **R** | Revert to the launch pad |
| **F** | Re-centre the camera on the vessel |
| **M** | Cycle the focus body |
| **+ / -** (or numpad) | Time warp up / down |
| **F1** | Toggle the control overlay |
| **F2** | Save a PNG screenshot next to the executable |

## Mouse

| Input | Action |
| --- | --- |
| **Wheel** | Zoom in / out (the camera always stays centred on the vessel) |
| **Right drag** | Pan the camera |

## Reading the HUD

* **Altitude** is measured from the base of the vessel to the terrain directly
  below it — so it reads near zero on the pad even though the terrain has hills.
* **APO / PERI** are the apsides of the current two-body orbit around the
  reference body. `APO inf` means the trajectory is unbound (an escape).
* **TWR** below 1.0 on the pad means the rocket will not lift off; add thrust or
  remove mass.
* **Q** is dynamic pressure in pascals. The parachute needs about 40 Pa before it
  will inflate, and heavy stacks see tens of kPa while climbing through the lower
  atmosphere — that is the load the airframe is carrying.

## Tips

1. Climb straight up until you clear the hills, then start pitching over. Every
   degree of pitch converts climb rate into the horizontal speed you need for
   orbit.
2. Stage when the fuel gauge for the current stage empties — the HUD flashes
   `FUEL OUT` when the active stage is dry.
3. Watch periapsis, not apogee: an orbit exists once **periapsis** is above the
   atmosphere, which is 70 km on Earth and 40 km on Mars.
4. For a landing, burn retrograde until the descent rate is under the vessel's
   impact tolerance (roughly 12-20 m/s; landing legs add ~18 m/s). A parachute
   brings a capsule down at about 39 m/s, so keep the legs and a little thrust
   for the last few hundred metres.

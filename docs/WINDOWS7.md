# Running on Windows 7 SP1 x64

This build is made for a stock Windows 7 SP1 (build 7601) 64-bit installation —
for example a Core 2 Duo E8400 with 2 GB of RAM and integrated graphics.

## Requirements

* Windows 7 **SP1** (build 7601), 64-bit. Windows Vista and earlier are not
  supported; Windows 8, 8.1, 10 and 11 all work too.
* ~10 MB of disk space for the unzipped folder.
* Nothing else: no .NET, no Visual C++ redistributable, no DirectX runtime, no
  graphics driver beyond the standard GDI stack.

## Install

1. Download `spaceflight-windows7-x64.zip` from the release page.
2. Right-click → **Extract All**. Windows' built-in zip support is fine.
3. Double-click `spaceflight.exe`.

Windows may show a SmartScreen prompt because the binary is not code-signed.
Choose "More info" → "Run anyway" if you trust the checksum in `checksums.txt`.

## Why the toolchain is pinned

Rust dropped Windows 7 support from its standard library in **1.78**. This
project therefore pins **1.77.1** in `rust-toolchain.toml`, and CI builds the
release binary with that exact toolchain. Building on a newer Rust would still
compile, but the produced `.exe` would call APIs that do not exist on Windows 7.

The CRT is linked statically (`+crt-static` in `.cargo/config.toml`), which is
what removes the "The program can't start because VCRUNTIME140.dll is missing"
class of failures on a clean Windows 7 install.

## Performance on a Core 2 Duo / 2 GB machine

* The simulator is a 30 Hz fixed-timestep simulation with a software renderer.
* Default window is 1280x760. If the frame rate feels low, start it smaller:

  ```
  spaceflight.exe --size 1024x600
  ```

* Large zooms where a whole planet fills the screen are the heaviest case. The
  renderer switches to a simplified ground mode there, which is bounded by the
  screen size rather than the planet size, so it stays interactive.
* Everything is drawn into one memory-mapped DIB; there is no intermediate
  surface allocation per frame, so peak memory stays small and predictable.

## Troubleshooting

| Symptom | Fix |
| --- | --- |
| "not a valid Win32 application" | You downloaded the 32-bit file by mistake, or the download was truncated. Re-download the `x64` zip and check the SHA-256. |
| Missing `VCRUNTIME140.dll` | You are running a locally built binary without `.cargo/config.toml`. Use the release zip, which is statically linked. |
| Window opens but is black | Update the graphics driver, or run with `--size 800x600`. The renderer only needs GDI. |
| Antivirus blocks the exe | Common for unsigned small binaries; add an exclusion or build it yourself from source. |
| Nothing happens on double-click | Run `spaceflight.exe --self-test` from `cmd` and check the printed report. |

## Verifying a build yourself

```
spaceflight.exe --version      # toolchain, target triple and build profile
spaceflight.exe --self-test    # renders real frames headlessly and checks them
```

`--self-test` exercises the renderer, the PNG writer, a full scripted ascent to
orbit, a crash, a parachute landing, the pause path and window resizing, and
returns a non-zero exit code if anything fails.

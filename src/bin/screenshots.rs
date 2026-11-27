//! CI helper: render the game's screens to PNG without opening a window.
//!
//! This is what proves on a Windows runner that the renderer, HUD and menus
//! produce correct pixels even when no interactive desktop is attached.
//!
//! usage: screenshots [--out DIR] [--size WxH]

use std::path::PathBuf;
use spaceflight::app::Game;
use spaceflight::selftest::render_demo_frames;

fn main() {
    let mut out = PathBuf::from("screenshots");
    let mut w = 1280;
    let mut h = 760;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--out" => {
                if let Some(v) = args.get(i + 1) {
                    out = PathBuf::from(v);
                    i += 1;
                }
            }
            "--size" => {
                if let Some(v) = args.get(i + 1) {
                    if let Some((sw, sh)) = v.split_once(['x', 'X']) {
                        if let (Ok(a), Ok(b)) = (sw.parse(), sh.parse()) {
                            w = a;
                            h = b;
                        }
                    }
                    i += 1;
                }
            }
            other => {
                eprintln!("unknown argument: {}", other);
                std::process::exit(64);
            }
        }
        i += 1;
    }

    let mut game = Game::new(w, h);
    match render_demo_frames(&mut game, &out) {
        Ok(n) => {
            println!("rendered {} frames at {}x{} into {}", n, w, h, out.display());
            // Also run the self test so CI has a single command covering both.
            match spaceflight::selftest::run(&out.join("selftest")) {
                Ok(report) => {
                    report.print();
                    if !report.all_ok() {
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("self-test error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("failed to render frames: {}", e);
            std::process::exit(1);
        }
    }
}

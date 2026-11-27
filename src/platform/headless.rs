//! Headless platform for non-Windows hosts (Linux CI, unit tests).
//!
//! It runs the exact same [`Game`] state machine and renders real frames to
//! PNG files, so continuous integration can verify the visual output of the
//! simulator on machines that have no Windows desktop session.

use std::io;

use crate::app::Game;
use crate::png;

pub struct Options {
    pub width: i32,
    pub height: i32,
    pub fullscreen: bool,
}

pub const APP_TITLE: &str = "Spaceflight Simulator";

/// There is no window server off Windows; the native window check is a no-op.
pub fn window_test() -> io::Result<()> {
    println!("window test is only meaningful on Windows - skipped");
    Ok(())
}

/// Render a numbered frame to `dir`; returns the byte size written.
pub fn save_frame(game: &Game, dir: &std::path::Path, name: &str) -> io::Result<usize> {
    let path = dir.join(format!("{}.png", name));
    png::write_png(
        &path,
        game.canvas.w as u32,
        game.canvas.h as u32,
        &game.canvas.px,
    )?;
    Ok(std::fs::metadata(&path)?.len() as usize)
}

/// Non-Windows "run": render a scripted demo sequence to PNG files instead of
/// opening a window. Returns a process exit code.
pub fn run(opts: Options) -> io::Result<i32> {
    println!(
        "{} {} - headless build (no window on this platform)",
        APP_TITLE,
        crate::VERSION
    );
    println!("Rendering a demo sequence to ./out/ ...");
    let mut game = Game::new(opts.width, opts.height);
    let out = std::path::PathBuf::from("out");
    std::fs::create_dir_all(&out)?;
    let frames = crate::selftest::render_demo_frames(&mut game, &out)?;
    println!("Wrote {} frames to {}", frames, out.display());
    let _ = opts.fullscreen;
    Ok(0)
}

pub fn error_box(title: &str, text: &str) {
    eprintln!("[{}] {}", title, text);
}

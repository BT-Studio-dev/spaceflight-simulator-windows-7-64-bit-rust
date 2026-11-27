//! Spaceflight Simulator - entry point.
//!
//! Command line:
//!   spaceflight                  launch the simulator window
//!   spaceflight --self-test      run the end-to-end self test, print a report
//!   spaceflight --demo DIR       render demo PNG frames (no window)
//!   spaceflight --size WxH       initial window size (default 1280x760)
//!   spaceflight --fullscreen     borderless full-screen
//!   spaceflight --version        print version and target information

use spaceflight::app::Game;
use spaceflight::platform::native;

fn parse_size(s: &str) -> Option<(i32, i32)> {
    let (w, h) = s.split_once(['x', 'X'])?;
    let w: i32 = w.trim().parse().ok()?;
    let h: i32 = h.trim().parse().ok()?;
    if (320..=7680).contains(&w) && (240..=4320).contains(&h) {
        Some((w, h))
    } else {
        None
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut width = 1280;
    let mut height = 760;
    let mut fullscreen = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" | "/?" => {
                print_help();
                return;
            }
            "--version" | "-V" => {
                println!("Spaceflight Simulator {}", spaceflight::VERSION);
                println!("target OS: {}", spaceflight::TARGET_OS);
                println!("arch: {}", std::env::consts::ARCH);
                println!("os: {}", std::env::consts::OS);
                return;
            }
            "--self-test" => {
                let dir = std::env::temp_dir().join("spaceflight-selftest");
                match spaceflight::selftest::run(&dir) {
                    Ok(report) => {
                        report.print();
                        std::process::exit(if report.all_ok() { 0 } else { 1 });
                    }
                    Err(e) => {
                        eprintln!("self-test failed to run: {}", e);
                        std::process::exit(2);
                    }
                }
            }
            "--demo" => {
                let dir = args
                    .get(i + 1)
                    .cloned()
                    .unwrap_or_else(|| "out".to_string());
                let mut game = Game::new(width, height);
                match spaceflight::selftest::render_demo_frames(&mut game, std::path::Path::new(&dir))
                {
                    Ok(n) => {
                        println!("wrote {} demo frames to {}", n, dir);
                        return;
                    }
                    Err(e) => {
                        eprintln!("demo render failed: {}", e);
                        std::process::exit(2);
                    }
                }
            }
            "--size" => {
                if let Some(s) = args.get(i + 1) {
                    match parse_size(s) {
                        Some((w, h)) => {
                            width = w;
                            height = h;
                        }
                        None => {
                            eprintln!("invalid --size value {s:?}, expected WxH (e.g. 1280x760)");
                            std::process::exit(64);
                        }
                    }
                    i += 1;
                }
            }
            "--fullscreen" => fullscreen = true,
            other => {
                eprintln!("unknown argument: {}", other);
                print_help();
                std::process::exit(64);
            }
        }
        i += 1;
    }

    let opts = native::Options {
        width,
        height,
        fullscreen,
    };
    match native::run(opts) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            native::error_box(
                "Spaceflight Simulator",
                &format!("Could not start the simulator:\n{}", e),
            );
            std::process::exit(1);
        }
    }
}

fn print_help() {
    println!(
        "Spaceflight Simulator {}\n\
         Target: {}\n\n\
         USAGE:\n  \
         spaceflight [--size WxH] [--fullscreen]\n  \
         spaceflight --self-test\n  \
         spaceflight --demo DIR\n  \
         spaceflight --version\n\n\
         CONTROLS:\n  \
         W / S        throttle up / down\n  \
         Z / X        full / zero throttle\n  \
         SPACE        stage separation\n  \
         C            arm parachute\n  \
         T            toggle SAS\n  \
         A / D        rotate\n  \
         wheel        zoom\n  \
         right drag   pan camera\n  \
         F            follow vessel\n  \
         R            revert to launch\n  \
         P / ESC      pause\n  \
         F1           help overlay\n  \
         F2           screenshot (PNG)\n",
        spaceflight::VERSION,
        spaceflight::TARGET_OS
    );
}

#[cfg(test)]
mod tests {
    use super::parse_size;

    #[test]
    fn size_parsing() {
        assert_eq!(parse_size("1280x760"), Some((1280, 760)));
        assert_eq!(parse_size("800X600"), Some((800, 600)));
        assert_eq!(parse_size(" 1024 x 768 "), Some((1024, 768)));
        assert_eq!(parse_size("10x10"), None);
        assert_eq!(parse_size("abc"), None);
        assert_eq!(parse_size("800"), None);
    }
}

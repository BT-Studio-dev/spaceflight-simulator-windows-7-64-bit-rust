//! Platform layer.
//!
//! * [`win32`] - the real Windows 7 window, input pump and GDI blitter.
//! * [`headless`] - a no-op stand-in so the same `main` compiles and runs on
//!   Linux/CI (used for tests and screenshot generation).
//!
//! Only `std` and raw `extern "system"` declarations are used: no `windows`
//! crate, no `winapi`, nothing to download at build time.

#[cfg(windows)]
pub mod win32;
#[cfg(windows)]
pub use win32 as native;

#[cfg(not(windows))]
pub mod headless;
#[cfg(not(windows))]
pub use headless as native;

/// Swap the red and blue channels in place, turning the canvas' `0x00RRGGBB`
/// pixels into the `0x00BBGGRR` layout a 32-bit Windows DIB expects.
/// Pulled out of the Win32 layer so it is unit-tested on every platform.
pub fn rgb_to_bgr_inplace(px: &mut [u32]) {
    for p in px.iter_mut() {
        let r = (*p >> 16) & 0xFF;
        let b = *p & 0xFF;
        *p = (b << 16) | (*p & 0x0000_FF00) | r;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_swap_is_an_involution() {
        let mut px = vec![0x0011_2233u32, 0x00AA_BBCC, 0x0];
        let orig = px.clone();
        rgb_to_bgr_inplace(&mut px);
        assert_eq!(px[0], 0x0033_2211);
        assert_eq!(px[1], 0x00CC_BBAA);
        assert_eq!(px[2], 0);
        rgb_to_bgr_inplace(&mut px);
        assert_eq!(px, orig);
    }
}

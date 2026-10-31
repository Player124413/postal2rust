//! p2game: boots POSTAL 2 content from the user's install on the uscript VM.
//!
//! ```text
//! p2game --config                                   print the boot configuration from the inis
//! p2game --scan [--out DIR]                         instantiate every map, write level_scan.tsv
//! p2game --map X --headless [--frames N]            boot one level headless, print a native/error histogram
//! p2game --boot-all [--frames N] [--out DIR]        headless boot of every map, aggregate natives_needed.tsv
//! p2game --smoke [--intro-secs S] [--out DIR]       headless end-to-end: menu, New Game, intro, day map, walk, fire
//! p2game --smoke --windowed [--out DIR]            the same flow in the window, with a screenshot per stage
//! p2game [--map X]                                  windowed: render the level with VM-driven actor transforms
//! ```
//! Common options: `--game DIR` (install root), `--game-class Pkg.Class`, `--options \"?k=v\"`.
//! The whole host lives in the library (`p2game::app`); this binary only picks the allocator.

/// The script VM allocates heavily (values, frames, strings); mimalloc keeps that fast where the
/// Windows system heap degrades over a session.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    p2game::app::main_entry();
}

//! Android library of POSTAL 2 Rust/Bevy. Built for `arm64-v8a` with `cargo ndk`, loaded by the
//! app's `NativeActivity` (see `android/` in the repository). All game logic is in `p2game`.

use bevy::prelude::*;

// mimalloc is used on desktop only. On Android the game crashed with SIGSEGV on the Java main
// thread, far from any game code, which is the pattern of heap corruption. Bionic's allocator is
// used there to test whether the crash goes away.
#[cfg(not(target_os = "android"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// `#[bevy_main]` generates the `android_main` entry the NativeActivity calls.
#[bevy_main]
#[allow(dead_code)]
fn main() {
    #[cfg(target_os = "android")]
    {
        use std::path::PathBuf;
        let app = bevy::android::ANDROID_APP.get();
        let data: PathBuf = app
            .and_then(|a| a.external_data_path().or_else(|| a.internal_data_path()))
            .unwrap_or_else(|| PathBuf::from("/data/local/tmp/postal2rust"));
        p2game::app::android_main(data);
    }
}

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
        // Diagnostics: the launcher's "empty scene" button writes this flag. Only Bevy runs, with
        // no game code, to tell a device or driver problem from a problem in the game.
        let flag = data.join("test_scene");
        if flag.is_file() {
            let _ = std::fs::remove_file(&flag);
            p2game::android_log::start(&data);
            empty_scene();
            return;
        }
        p2game::app::android_main(data);
    }
}

#[cfg(target_os = "android")]
fn empty_scene() {
    App::new()
        .add_plugins(DefaultPlugins)
        .insert_resource(ClearColor(Color::srgb(0.2, 0.4, 0.8)))
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn((Camera2d, if cfg!(target_os = "android") { bevy::render::view::Msaa::Off } else { bevy::render::view::Msaa::default() }));
        })
        .run();
}

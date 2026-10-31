# POSTAL 2 Rust/Bevy on Android

Android port: a launcher app (Kotlin) plus the same Rust engine built as `libp2android.so`
(64-bit ARM, `arm64-v8a`). Only the **debug** APK is built.

## Install

1. Download `Postal2Rust-android-arm64-debug` from the latest **android-debug** workflow run
   (Actions tab), unzip it and install `app-debug.apk`. Allow installs from this source.
2. Open **POSTAL 2 Rust** and tap **1. Choose the POSTAL 2 folder and copy**. Pick the folder that
   contains `System`, `Maps`, `Textures` (on PC: `steamapps/common/POSTAL2Complete`). The launcher
   copies it into the app's own folder (`Android/data/com.postal2rust.game/files/POSTAL2`), a few GB.
   An interrupted copy can be resumed: files that are already complete are skipped.
3. Tap **2. Play**.

No game files are in the APK. You need your own copy of POSTAL 2.

## Updating

Install the new APK over the old one. Do not uninstall first:

- the package name (`com.postal2rust.game`) and the signing key are fixed, so Android treats the
  new APK as an update;
- `versionCode` is the number of commits on the branch, so every new build is newer;
- the copied game, saves and settings live in the app's external files folder and survive updates.

Uninstalling the app deletes that folder (and the copy of the game with it).

## Touch controls

- Left: the **MOVE** stick (WASD). Right and bottom: buttons (fire, alt fire, reload, jump, ...).
- Drag on empty screen to look. A short tap clicks there (menus, map, inventory).
- Gear button **UI** (top right) opens the editor: **Edit mode ON**, tap a control, drag it to move,
  drag its corner handle to resize, change its action (`<` `>`), opacity, add or delete controls.
  **Save and close** writes the layout to the user folder. Launcher: **Customize touch controls**
  opens the game straight in the editor.

Labels in the game are English (Bevy's built-in font has no Cyrillic). The launcher is in Russian.

## Frame rate

The default cap is **60 FPS** with vsync. The launcher can choose 30, 60, 90 or 120 FPS. The
engine's own **F3** overlay shows the frame time.

## Building (CI)

`.github/workflows/android.yml` builds on every push to `arena/**` or `ci/**`:

1. `cargo ndk -t arm64-v8a -P 26 build -p p2android --profile release` writes `libp2android.so`
   into `android/app/src/main/jniLibs/`. The release profile is used even for the debug APK, so the
   engine code is optimized.
2. `gradle :app:assembleDebug` builds and signs the APK with `app/keystore/debug.keystore`.

The debug keystore is committed on purpose (password `android`). It is not a secret: it only
exists so that every build signs with the same key and updates the previous one.

To build locally you need JDK 17, the Android SDK (platform 35), NDK r27, `cargo-ndk`, the
`aarch64-linux-android` Rust target, and Gradle 8.10 (there is no wrapper in the repository):

```
cargo ndk -t arm64-v8a -P 26 -o android/app/src/main/jniLibs build -p p2android --profile release
cd android && gradle :app:assembleDebug
```

## Architecture

| Piece | Where |
|---|---|
| Launcher (copy the game, FPS, editor entry) | `android/app/src/main/java/com/postal2rust/game/LauncherActivity.kt` |
| Native entry (`android_main`) | `crates/p2android` |
| Host shared with desktop | `crates/p2game/src/app.rs` |
| Touch controls and layout editor | `crates/p2game/src/touch.rs` |

The launcher hands over settings through two small files in the app folder: `android.cfg`
(`fps=N`) and the flag `start_touch_editor`.

## Status and what is not verified yet

- CI status: the **android-debug** workflow builds the desktop library and binary
  (`cargo check`) and the debug APK. Both jobs pass on the current branch. Failing steps are
  repeated as annotations on the check run, so the reason shows up without opening the raw logs.
- Not run on a device or emulator yet. Launching the game, touch input and the layout editor
  still need a manual check on a phone.
- Not measured yet: 60 FPS on real phones, memory use with a full map, battery and heat. Use F3 on
  a phone and report numbers.
- Textures are decoded to RGBA8 as on desktop. Mobile GPUs cannot read the DXT/BC textures of the
  original game, so a compressed format is needed for lower memory. ETC2 is mandatory in OpenGL ES 3.0
  and in Vulkan, so every 64-bit Android GPU that runs this build can read it; converting the textures
  is a separate task and is only needed if memory is short. Source: the
  [Android developer guide on texture compression](https://developer.android.com/guide/playcore/asset-delivery/texture-compression).
- Only `arm64-v8a` is built. Emulators on x86_64 hosts need an extra ABI.

## Known problem: native crash on Honor ALI-NX1 (Adreno 710)

Status: **not solved**. The game starts, reaches the first frames of the level, then the process
dies with SIGSEGV (null pointer dereference). The crashing thread changes from run to run, which
points to memory corruption in the GPU driver rather than a bug in the game code.

Ruled out so far:
- mimalloc: the crash also happens with Bevy's system allocator.
- Audio: the crash also happens with `audio=off` (no OS audio device).
- Bindless texture arrays: disabled on Android, the crash remains.
- Asynchronous pipeline compilation: disabled on Android, the crash remains.
- BC (DXT) textures on the GPU: disabled on Android (decoded to RGBA8 on the CPU). One run survived
  about 13 seconds afterwards, the next one still crashed.
- Empty Bevy scene (the launcher button «Диагностика: пустая сцена»): works, so the device and the Vulkan
  driver can draw a frame.
- OpenGL ES: no GPU is found when the renderer starts, because Bevy creates the renderer before the
  Android window exists and OpenGL needs a surface to pick an adapter (`bevy_render`,
  `renderer/mod.rs`). Vulkan does not need one.

Diagnostic switches in the launcher (saved in `android.cfg`): graphics backend, audio on/off,
empty scene. Logs: "Отправить лог" in the launcher; the native crash is also in `exit.txt`.

Not yet tried: removing game features one by one (fog, colour grading, particles, the HUD canvas)
to find the one the driver does not survive.

# Flax Reader for Android

The app uses the same Rust core as the desktop app (AO3 client and parser, library, search, export, reading positions). Only the interface is Android-specific.

- **`rust/`**: `libao3core.so`, a small bridge over the core. Kotlin calls one JNI function, `Core.call(method, json)`, and gets JSON back. There is no binding generator and no JNA at runtime.
- **`app/`**: the app itself. It uses the platform's own views and `StaticLayout`, with no AndroidX or other libraries, to keep memory low.
  - The reader paginates with real line metrics, the same way the desktop reader does.

The release build uses about 25 MB on the library screen and about 33 MB with a 120,000-word novel open (total PSS), and starts cold in 0.7–0.8 s. That was measured on the emulator (Android 15, x86_64) with version 2.5; real phones differ somewhat.

## Build

Requirements:
- the Android SDK with NDK r27 or newer
- the Rust targets `aarch64-linux-android` and `x86_64-linux-android`
- a JDK 17 or newer

```powershell
# 1. Build the Rust bridge for phones (arm64) and the emulator (x86_64) into app/src/main/jniLibs
powershell -File android/build-rust.ps1

# 2. Build the app
cd android
.\gradlew.bat assembleRelease      # app/build/outputs/apk/release/app-release.apk
```

The release APK carries only the ARM (arm64) build of the Rust bridge, which every current phone and tablet uses. To try it in the emulator on a PC, add the emulator's x86-64 build too: `.\gradlew.bat assembleRelease -Pemulator`.

The release build is signed with the debug key, so it installs for testing. Publishing to a store needs your own signing key.

If Gradle fails with `Unable to establish loopback connection`, Java can't create its socket file in your temp folder (the path may be too long). Point it at a short folder first:

```powershell
$env:JAVA_TOOL_OPTIONS = "-Djdk.net.unixdomain.tmpdir=C:\Users\<you>\.gtmp"
```

## Testing with a library that isn't yours

The app stores its data in its private `files/` folder, using the same `library.json`, `config.json` and `works/` files as the desktop app. A debug build can be filled through `adb exec-in run-as org.ao3reader ...`.

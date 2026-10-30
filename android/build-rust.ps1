# Builds the Rust bridge (libao3core.so) for Android and copies it into the app's jniLibs.
#   powershell -File android/build-rust.ps1            # release, phone (arm64) + emulator (x86_64)
#   powershell -File android/build-rust.ps1 -Debug     # faster debug build
param([switch]$Debug)
$ErrorActionPreference = "Stop"

$sdk = if ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { "$env:LOCALAPPDATA\Android\Sdk" }
$ndk = Get-ChildItem "$sdk\ndk" -Directory | Sort-Object Name -Descending | Select-Object -First 1
if (-not $ndk) { throw "No Android NDK found under $sdk\ndk" }
$bin = "$($ndk.FullName)\toolchains\llvm\prebuilt\windows-x86_64\bin"

$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$crate = Join-Path $root "rust"
$target = Join-Path (Split-Path -Parent $root) "target\android-bridge"
$jni = Join-Path $root "app\src\main\jniLibs"
$profile = if ($Debug) { "debug" } else { "release" }

$targets = @{ "aarch64-linux-android" = "arm64-v8a"; "x86_64-linux-android" = "x86_64" }
foreach ($t in $targets.Keys) {
    $clang = "$bin\$t" + "26-clang.cmd"
    $u = $t.Replace("-", "_").ToUpper()
    Set-Item "env:CARGO_TARGET_$($u)_LINKER" $clang
    Set-Item "env:CC_$($t.Replace('-', '_'))" $clang
    Set-Item "env:AR_$($t.Replace('-', '_'))" "$bin\llvm-ar.exe"
    $cargoArgs = @("build", "--target", $t, "--manifest-path", "$crate\Cargo.toml", "--target-dir", $target)
    if (-not $Debug) { $cargoArgs += "--release" }
    # cargo reports progress on stderr; don't let PowerShell treat that as a failure.
    $ErrorActionPreference = "Continue"
    & cargo @cargoArgs 2>&1 | ForEach-Object { "$_" }
    $ErrorActionPreference = "Stop"
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed for $t" }
    $abi = $targets[$t]
    New-Item -ItemType Directory -Force "$jni\$abi" | Out-Null
    Copy-Item "$target\$t\$profile\libao3core.so" "$jni\$abi\libao3core.so" -Force
    "{0,-24} {1:N0} KB" -f $abi, ((Get-Item "$jni\$abi\libao3core.so").Length / 1KB)
}

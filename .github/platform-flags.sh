#!/bin/sh
# Maps changed paths (stdin, one per line) to CI flags (stdout, key=value).
#
# A path without a platform rule is shared and affects every platform. A missing or
# too-wide rule costs CI time but never coverage, so keep the shared default as the fallback.
# Rules follow the cfg gates: module gates in src/lib.rs, src/ipc.rs, src/audio.rs,
# src/notification.rs, src/resident.rs and src/tray.rs; test gates are file-level #![cfg].
set -eu

# Prints the platforms that a changed path affects, separated by spaces.
platforms_for() {
  case "$1" in
    src/linux/* | tests/linux_apply.rs | tests/linux_deb_lifecycle.sh | tests/linux_upgrade_ipc.sh | tests/console_setup_linux.py | tests/live/linux_* | package-deb.sh | package-linux.sh)
      echo linux ;;
    # src/linux/mod.rs compiles this file into the Linux build as well.
    src/mac/input.rs)
      echo 'linux macos' ;;
    src/mac/* | tests/mac_resident.rs | tests/macos_app_lifecycle.sh | package-macos.sh | package-dmg.sh | check-release-macos.sh | check-release-dmg.sh)
      echo macos ;;
    # Compiled under cfg(unix): Linux and macOS.
    src/unix/* | tests/https_off.rs | tests/log_local_offset.rs | tests/perf_statusline.rs | tests/plugin_hook_script.rs | tests/statusline.rs | tests/unix_audio.rs | tests/unix_hook.rs | tests/unix_ipc.rs | tests/unsupported_platform.rs | tests/usage_local_reset.rs)
      echo 'linux macos' ;;
    tests/audio_log.rs)
      echo 'windows macos' ;;
    src/audio_windows.rs | src/console_window.rs | src/detached.rs | src/https.rs | src/local.rs | src/overlay.rs | src/pipe.rs | src/process_stats.rs | src/resident.rs | src/resident_overlay.rs | src/restart.rs | src/shell.rs | src/tray.rs | src/tray_native_tests.rs)
      echo windows ;;
    tests/app_icon.rs | tests/config_notice.rs | tests/hook_lifecycle.rs | tests/ipc_diagnostics.rs | tests/local.rs | tests/native_assets.rs | tests/native_notice.rs | tests/native_overlay.rs | tests/perf_baseline.rs | tests/process.rs | tests/process_stats.rs | tests/process_wait.rs | tests/resident.rs | tests/resident_fixture.rs | tests/resident_perf_log.rs | tests/resident_render_faults.rs | tests/runner_contract.rs | tests/tray.rs | tests/tray_about.rs | tests/tray_restart.rs | tests/usage_live.rs | tests/window_wait.rs)
      echo windows ;;
    *.ps1 | installer/* | build-installer.ps1 | check-release.ps1 | package-release.ps1)
      echo windows ;;
    *)
      echo all ;;
  esac
}

code=false
windows=false
linux=false
macos=false

while IFS= read -r path; do
  case "$path" in
    '' | *.md) continue ;;
  esac
  code=true
  for platform in $(platforms_for "$path"); do
    case "$platform" in
      all) windows=true; linux=true; macos=true ;;
      windows) windows=true ;;
      linux) linux=true ;;
      macos) macos=true ;;
    esac
  done
done

os_list=''
if [ "$windows" = true ]; then os_list="$os_list\"windows-latest\","; fi
if [ "$macos" = true ]; then os_list="$os_list\"macos-latest\","; fi
if [ "$linux" = true ]; then os_list="$os_list\"ubuntu-latest\","; fi
os_list="[${os_list%,}]"

printf 'code=%s\nwindows=%s\nlinux=%s\nmacos=%s\nos=%s\n' "$code" "$windows" "$linux" "$macos" "$os_list"

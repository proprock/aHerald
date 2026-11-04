#!/bin/sh
# Maps changed paths (stdin, one per line) to CI flags (stdout, key=value). With --all it
# ignores stdin and turns every flag on, which is the nightly full run.
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
    tests/statusline_windows.rs | tests/app_icon.rs |tests/config_notice.rs | tests/hook_lifecycle.rs | tests/ipc_diagnostics.rs | tests/local.rs | tests/native_assets.rs | tests/native_notice.rs | tests/native_overlay.rs | tests/perf_baseline.rs | tests/process.rs | tests/process_stats.rs | tests/process_wait.rs | tests/resident.rs | tests/resident_fixture.rs | tests/resident_perf_log.rs | tests/resident_render_faults.rs | tests/runner_contract.rs | tests/tray.rs | tests/tray_about.rs | tests/tray_restart.rs | tests/usage_live.rs | tests/window_wait.rs)
      echo windows ;;
    *.ps1 | installer/* | build-installer.ps1 | check-release.ps1 | package-release.ps1)
      echo windows ;;
    *)
      echo all ;;
  esac
}

# Prints the platforms whose package or installer check a changed path can affect. The
# checks install the built program and run its setup, so the install-time sources count too;
# everything else is covered by the full run (--all) and by the release workflow.
packaging_for() {
  case "$1" in
    installer/* | *.ps1 | tests/installer_*)
      echo windows ;;
    package-deb.sh | package-linux.sh | tests/linux_deb_lifecycle.sh | tests/linux_upgrade_ipc.sh | tests/console_setup_linux.py | src/linux/*)
      echo linux ;;
    package-macos.sh | package-dmg.sh | check-release-macos.sh | check-release-dmg.sh | tests/macos_app_lifecycle.sh | src/mac/*)
      echo macos ;;
    Cargo.toml | Cargo.lock | build.rs | assets/* | resources/* | .github/* | src/main.rs | src/cli.rs | src/paths.rs | src/restart.rs | src/default_config.rs | src/codex_install.rs | src/*setup*.rs)
      echo 'windows linux macos' ;;
  esac
}

code=false
deps=false
workflows=false
windows=false
linux=false
macos=false
pkg_windows=false
pkg_linux=false
pkg_macos=false

if [ "${1:-}" = --all ]; then
  code=true deps=true workflows=true windows=true linux=true macos=true
  pkg_windows=true pkg_linux=true pkg_macos=true
else
  while IFS= read -r path; do
    case "$path" in
      '' | *.md) continue ;;
    esac
    code=true
    case "$path" in
      Cargo.toml | Cargo.lock) deps=true ;;
      .github/*) workflows=true ;;
    esac
    for platform in $(platforms_for "$path"); do
      case "$platform" in
        all) windows=true; linux=true; macos=true ;;
        windows) windows=true ;;
        linux) linux=true ;;
        macos) macos=true ;;
      esac
    done
    for platform in $(packaging_for "$path"); do
      case "$platform" in
        windows) pkg_windows=true ;;
        linux) pkg_linux=true ;;
        macos) pkg_macos=true ;;
      esac
    done
  done
fi

# These runner images match the ones in release.yml, so CI tests what ships. Linux stays on
# 22.04 for the glibc floor of the Debian package.
os_list=''
if [ "$windows" = true ]; then os_list="$os_list\"windows-latest\","; fi
if [ "$macos" = true ]; then os_list="$os_list\"macos-15\","; fi
if [ "$linux" = true ]; then os_list="$os_list\"ubuntu-22.04\","; fi
os_list="[${os_list%,}]"

printf 'code=%s\ndeps=%s\nworkflows=%s\nwindows=%s\nlinux=%s\nmacos=%s\npkg_windows=%s\npkg_linux=%s\npkg_macos=%s\nos=%s\n' \
  "$code" "$deps" "$workflows" "$windows" "$linux" "$macos" "$pkg_windows" "$pkg_linux" "$pkg_macos" "$os_list"

# Releasing aHerald

Releases are built by `.github/workflows/release.yml` from a pushed `vX.Y.Z` tag on `master`.
One tag produces the Windows, macOS, Linux and `aherald-ping` artifacts, a draft GitHub
Release, and — after approval — the published release and a Homebrew cask update.
Never tag, push a tag, approve `publish` or merge a cask change without the owner's
explicit go-ahead.

## 1. Prepare the release pull request

1. Start from an up-to-date `master` with a clean working tree, and create
   `chore/release-X.Y.Z`.
2. Check the contents: every ticket in this release is closed in `DONE.md`; leftovers have
   new IDs in `TODO.md`; `CHANGELOG.md` `[Unreleased]` describes every user-visible change and
   names the known limitations. Claim no platform support that was not verified.
3. Choose the version (Semantic Versioning; `-rc.N` for a trial run that the cask skips) and set
   it in every place at once:
   - `Cargo.toml` (`[package] version`)
   - `crates/aherald-ping/Cargo.toml` (must equal the main crate; `tests/workspace.rs` checks it)
   - `plugins/aherald/.claude-plugin/plugin.json` (`version`)
   - `Cargo.lock`: run `cargo check --locked --workspace` after the edits; if it complains, run
     `cargo check --workspace` once and commit the lock file.
4. In `CHANGELOG.md`, rename `## [Unreleased]` to `## [X.Y.Z] - YYYY-MM-DD`, add a new empty
   `## [Unreleased]` above it, and add the compare link at the bottom:
   `[X.Y.Z]: https://github.com/proprock/aHerald/compare/vPREV...vX.Y.Z`. The workflow
   publishes exactly that section as the release notes and stops if it is missing or empty.
5. Commit `chore(release): X.Y.Z`, push, and open a pull request. CI must pass (the `CI gate`
   check).

## 2. Verify before tagging

- Run the checks from `AGENTS.md` "Verification and reporting" on the machine you have.
  Platforms you cannot run locally are covered by CI on the pull request and by the
  latest nightly run on `master`: check that the most recent nightly is green. The same nightly run publishes the rolling `nightly` prerelease (`docs/nightly.md`); a stable release is
  built by `release.yml` through the same `package.yml` workflow.
- Run the manual checks the release notes claim, per platform you changed: a card at 100%
  and 150% scale, the tray menu, a hook round trip from a real agent. Record what was checked
  by hand and on which OS version; say "not checked" for the rest.
- Optional packaging preview on the host OS (artifacts land in `dist/`):
  - Windows: `cargo build --release --target x86_64-pc-windows-msvc`, `./scripts/check-release.ps1`,
    `./scripts/package-release.ps1`, `./scripts/build-installer.ps1` (needs Inno Setup and `llvm-readobj`).
  - macOS: `cargo build --release --target aarch64-apple-darwin`,
    `./scripts/package-macos.sh target/aarch64-apple-darwin/release/aherald`, `./scripts/package-dmg.sh`.
  - Linux: `cargo build --release`, `./scripts/package-linux.sh target/release/aherald`,
    `sh ./scripts/package-deb.sh target/release/aherald`.

## 3. Tag and publish

1. Merge the release pull request into `master`.
2. Tag the merge commit and push the tag:

   ```text
   git switch master && git pull --ff-only
   git tag -a vX.Y.Z -m "aHerald X.Y.Z"
   git push origin vX.Y.Z
   ```

3. The workflow runs:
   - `verify_tag`: the tag is `vX.Y.Z[-pre]`, is an annotated or lightweight tag, and is
     reachable from `master`;
   - `package` (the reusable `package.yml`, which the nightly workflow also calls) runs four jobs in
     parallel, each checking that the tag equals the crate version, running the test suite without
     the noisy timing checks (the nightly run keeps them) and the memory check (it runs alone in every CI run), building, packaging, verifying a clean extraction, attesting build
     provenance and uploading the artifacts with `.sha256` files: Windows x64, macOS arm64,
     Linux x86-64 GNU, `aherald-ping` for Linux musl (x86_64 and aarch64, plus a container test);
   - `draft_release`: checks that every platform delivered its files and checksums, extracts the
     CHANGELOG section and creates (or refreshes) the draft;
   - `publish`: waits for approval in the `release` environment;
   - `update_tap` (not for pre-releases): opens a pull request on `proprock/homebrew-tap` with
     the new version and checksum and merges it once the tap's check passes (needs the
     `TAP_TOKEN` secret).
4. Open the draft release, check the notes and the attached files, then approve `publish`.

To rebuild an existing tag, run the workflow manually (Actions → Release → Run workflow) with
the tag. A published release is never overwritten; the draft step refuses it.

## 4. After the release

- Download the files and check them: `sha256sum -c <file>.sha256` and
  `gh attestation verify <file> --repo proprock/aHerald`.
- Smoke test on each platform you can reach, as a normal user in a clean location:
  - Windows: the portable zip (`aherald.exe --version`, `--demo`) and the setup EXE (install,
    harness selection, uninstall);
  - macOS: `brew upgrade --cask aherald` (or the disk image), clear the quarantine as in the
    README, `--version` and `--demo`;
  - Linux: `sudo apt install ./aherald-vX.Y.Z-linux-x86_64-gnu.deb`, `aherald --version`, the
    console setup;
  - `aherald-ping-vX.Y.Z-linux-x86_64-musl --self-test` inside a container.
- Move the released tickets as usual; follow-ups go to `TODO.md` with new IDs.

## If a release is bad

- Do not move or delete a published tag. Fix forward with a patch version.
- Until the fix is out: edit the GitHub Release back to a draft (or mark it pre-release), and
  revert the cask commit in `proprock/homebrew-tap` so `brew` users stay on the previous version.
- Note the problem under the fixed version in `CHANGELOG.md`.

## Artifacts

| File | Contents |
|---|---|
| `aherald-vX.Y.Z-windows-x64.zip` | `aherald.exe`, `aherald-cli.exe`, `config.example.toml`, `LICENSE`, `README.md`, `CHANGELOG.md`, `START-HERE.md` (from `docs/portable.md`), `MANIFEST.txt` (version and SHA-256 of every file), `assets/`, `examples/`, `docs/` |
| `aherald-vX.Y.Z-windows-x64-setup.exe` | Per-user Inno Setup installer with harness selection |
| `aherald-vX.Y.Z-macos-arm64.zip`, `.dmg` | `aHerald.app` (ad-hoc signed, not notarized) |
| `aherald-vX.Y.Z-linux-x86_64-gnu.tar.gz`, `.deb` | Portable archive and Debian package (glibc ≥ 2.35) |
| `aherald-ping-vX.Y.Z-linux-{x86_64,aarch64}-musl` | Static sender for guests in containers and VMs |

Every file has a `.sha256` and a build-provenance attestation. A first `config.toml` is created
on first start (beside the executable for portable installs, in the user's configuration
directory for system packages).

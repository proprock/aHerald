# Nightly build

Every night the `Nightly` workflow (`.github/workflows/nightly.yml`) runs the full checks on
`master` on Windows, macOS and Linux. When they pass, it publishes the build as the single
prerelease **aHerald Nightly**: <https://github.com/proprock/aHerald/releases/tag/nightly>.

## Stable or nightly

- A nightly is the latest green `master`. It carries the version of `Cargo.toml`, so its files
  are named like the stable ones (`aherald-v0.15.0-windows-x64.zip`); the release notes name the
  commit and the UTC build time.
- Like the stable builds it is unsigned (the macOS app is ad-hoc signed, not notarized). It may
  break and is not meant for production. The Homebrew cask never points to it.
- Assets: Windows x64 ZIP and Setup EXE, macOS ARM64 ZIP and DMG, Linux x64 TAR.GZ and DEB,
  `aherald-ping` for Linux musl x86_64 and aarch64, one `.sha256` per file and `SHA256SUMS`.

## Schedule and manual runs

The cron is `37 5 * * *` UTC (06:37 in CET, 07:37 in CEST). Run it by hand with
`gh workflow run nightly.yml`. A manual run on `master` publishes too; on another branch it builds
the packages but publishes nothing.

A scheduled night in which `master` has not moved since the commit named in the current nightly
release builds nothing: the full CI, packaging and publication are skipped (the run is green and
short, and its summary says "No change since <sha>"). The compiler canary and the dependency audit
still run, since a new compiler or advisory needs no commit.

## Retention and failures

There is one release. Each successful run moves the `nightly` tag to the new commit, replaces
the assets and rewrites the notes; nothing accumulates. Between the tag move and the end of the
upload the tag can point at the new commit while the assets are still the old ones, because the two
are not one operation; during that time the notes say "Update in progress".

A failed night leaves the previous nightly in place, and the failed run is the signal. If the
release shows "Update in progress" for long, run the workflow again: a release without a finished
note is always rebuilt. The workflow touches only the `nightly` tag and release, and refuses to
continue if that release is not a published prerelease.

GitHub's "immutable releases" setting must stay off for this repository (it is off today): an
immutable release cannot be edited and its tag cannot move.

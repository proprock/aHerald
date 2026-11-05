# Signing the Windows release files (AH-218)

Build provenance is attested for every release file (`gh attestation verify`). This page covers
the Authenticode signature, which removes the SmartScreen warning. macOS is not covered: Developer
ID signing and notarization need the paid Apple Developer Program, so the disk image stays ad-hoc
signed and the README keeps the **Open Anyway** step.

## How the release workflow signs

`release.yml` signs only when the repository variable `SIGNPATH_ORGANIZATION_ID` is set. Without
it the Windows files are published unsigned, exactly as before.

1. After `cargo build`, `aherald.exe` and `aherald-cli.exe` are signed
   (artifact configuration `windows-executables`). The portable archive and the installer are built
   from the signed files, so every copy carries the signature.
2. After `build-installer.ps1`, the setup EXE is signed (`windows-installer`) and its `.sha256`
   file is written again.
3. Each step fails the job unless `Get-AuthenticodeSignature` reports `Valid`.

The step is `.github/actions/signpath-sign`. It uploads the files as a temporary GitHub artifact,
asks SignPath to sign it (SignPath checks the artifact against this repository and the workflow
run), and copies the signed files back over the originals.

## One-time setup

1. Apply to the SignPath Foundation (free for open-source projects, the repository must be public
   and MIT-licensed; both hold): https://signpath.org/apply
2. In SignPath, create the project with slug `aherald` and the signing policy `release-signing`.
   Connect it to GitHub (trusted build system) and restrict the policy to this repository.
   Require manual approval of the signing request for `release-signing`.
3. Add two artifact configurations (zip root, files signed by name):

   `windows-executables`
   ```xml
   <artifact-configuration xmlns="http://signpath.io/artifact-configuration/v1">
     <zip-file>
       <pe-file path="aherald.exe"><authenticode-sign/></pe-file>
       <pe-file path="aherald-cli.exe"><authenticode-sign/></pe-file>
     </zip-file>
   </artifact-configuration>
   ```

   `windows-installer`
   ```xml
   <artifact-configuration xmlns="http://signpath.io/artifact-configuration/v1">
     <zip-file>
       <pe-file path="*-setup.exe"><authenticode-sign/></pe-file>
     </zip-file>
   </artifact-configuration>
   ```
4. In GitHub, add the secret `SIGNPATH_API_TOKEN` and the variable `SIGNPATH_ORGANIZATION_ID`.
   The token is a repository secret, not an `environment: release` secret: that environment has a
   required reviewer, and putting it on the package job would ask for approval before the build as
   well as before publishing. SignPath's own approval of each signing request is the gate instead.

## Acceptance (not yet run)

- A tag build signs both passes and the job log shows the signer.
- On a clean Windows 11 machine the downloaded setup EXE and the extracted `aherald.exe` start
  without a SmartScreen warning. (A new publisher can still see a reputation prompt for a while.)
- `gh attestation verify` still passes for the signed files.

The slugs, the SignPath file-matching syntax and the SignPath action's behaviour with a
single-file artifact are taken from SignPath's documentation, not run here: no SignPath account
exists yet, so the first tagged build after onboarding is the real test. Run it as a pre-release
tag (`v0.x.y-rc.1`) first.

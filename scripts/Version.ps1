# Shared by the packaging scripts: `. (Join-Path $PSScriptRoot 'Version.ps1')`. Reads the
# program's version from `cargo metadata` instead of matching lines of Cargo.toml.

# The version of the `aherald` package. Run from anywhere inside the repository.
function Get-CrateVersion {
    $json = & cargo metadata --no-deps --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'Cannot read the crate version with cargo metadata' }
    $package = ($json | ConvertFrom-Json).packages | Where-Object { $_.name -eq 'aherald' } | Select-Object -First 1
    if (-not $package) { throw 'cargo metadata does not list the aherald package' }
    $package.version
}

# Throws unless $Tag is "v" followed by $Version.
function Assert-TagMatches([string]$Tag, [string]$Version) {
    if ($Tag -ne "v$Version") { throw "Tag '$Tag' does not match the crate version $Version" }
}

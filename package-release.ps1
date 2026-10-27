param(
    [string]$Executable = (Join-Path $PSScriptRoot 'target/x86_64-pc-windows-msvc/release/aherald.exe'),
    [string]$OutDir = (Join-Path $PSScriptRoot 'dist'),
    [string]$Tag
)
# Builds the portable archive and proves that a clean extraction runs. The release workflow
# calls this script, so a local run and the published archive are made the same way.
$ErrorActionPreference = 'Stop'
$cargo = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'Cargo.toml')
$version = ($cargo | Where-Object { $_ -match '^version\s*=\s*"([^"]+)"' } | Select-Object -First 1) -replace '^version\s*=\s*"([^"]+)".*', '$1'
if (-not $version) { throw 'Cannot read the crate version from Cargo.toml' }
if (-not $Tag) { $Tag = "v$version" }
if ($Tag -ne "v$version") { throw "Tag $Tag does not match Cargo.toml version $version" }
if (-not (Test-Path -LiteralPath $Executable)) { throw "Build the release executable first: $Executable" }

$name = "aherald-$Tag-windows-x64"
$root = Join-Path $OutDir $name
$zip = Join-Path $OutDir "$name.zip"
Remove-Item -LiteralPath $root, $zip, "$zip.sha256" -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Path $root | Out-Null

Copy-Item -LiteralPath $Executable -Destination $root
foreach ($file in 'aHerald.example.toml', 'LICENSE', 'README.md', 'CHANGELOG.md') {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot $file) -Destination $root
}
foreach ($dir in 'assets', 'examples', 'docs') {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot $dir) -Destination $root -Recurse
}
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'docs/portable.md') -Destination (Join-Path $root 'START-HERE.md')

# Metadata: what this archive is and the hash of every file in it (the manifest lists itself last).
$files = Get-ChildItem -LiteralPath $root -Recurse -File | Sort-Object FullName
$lines = @("aHerald $version", "Platform: Windows 10/11 x64", "Archive: $name.zip", '', 'SHA-256  file')
foreach ($file in $files) {
    $relative = $file.FullName.Substring($root.Length + 1).Replace('\', '/')
    $lines += '{0}  {1}' -f (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLower(), $relative
}
Set-Content -LiteralPath (Join-Path $root 'MANIFEST.txt') -Value $lines -Encoding ascii

Compress-Archive -LiteralPath $root -DestinationPath $zip
$hash = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLower()
Set-Content -LiteralPath "$zip.sha256" -Value "$hash  $name.zip" -Encoding ascii

# Clean extraction: a fresh folder, no administrator rights, no other runtime installed.
$check = Join-Path ([IO.Path]::GetTempPath()) "aherald-extract-$PID"
Remove-Item -LiteralPath $check -Recurse -Force -ErrorAction SilentlyContinue
try {
    Expand-Archive -LiteralPath $zip -DestinationPath $check
    $app = Join-Path $check $name
    foreach ($required in 'aherald.exe', 'aHerald.example.toml', 'LICENSE', 'README.md', 'START-HERE.md', 'MANIFEST.txt',
        'assets/turn_complete', 'examples/hooks.windows.json', 'docs/configuration.md') {
        if (-not (Test-Path -LiteralPath (Join-Path $app $required))) { throw "Archive lacks $required" }
    }
    # A GUI-subsystem program: PowerShell waits for it and collects stdout only through a pipe.
    $reported = & (Join-Path $app 'aherald.exe') --version | Out-String
    if ($reported.Trim() -ne "aHerald $version") { throw "Extracted aherald.exe reported '$reported'" }
    $identity = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
    [pscustomobject]@{
        Archive = $zip
        Bytes = (Get-Item -LiteralPath $zip).Length
        Sha256 = $hash
        Version = $reported.Trim()
        ExtractedAsAdministrator = $identity.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    }
}
finally {
    Remove-Item -LiteralPath $check -Recurse -Force -ErrorAction SilentlyContinue
}

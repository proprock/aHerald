param(
    [string]$Tag,
    [string]$Compiler,
    [string]$InstallerAppId
)
# Inno Setup is a build tool only; the resulting installer uses native Windows APIs.
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'Version.ps1')
Push-Location $repo
try { $version = Get-CrateVersion } finally { Pop-Location }
if (-not $Tag) { $Tag = "v$version" }
Assert-TagMatches $Tag $version
$package = Join-Path $repo "dist/aherald-$Tag-windows-x64"
foreach ($required in 'aherald.exe', 'aherald-cli.exe') {
    if (-not (Test-Path -LiteralPath (Join-Path $package $required))) { throw 'Run scripts/package-release.ps1 first' }
}
if (-not $Compiler) {
    $candidates = @(
        (Join-Path $env:LOCALAPPDATA 'Programs/Inno Setup 7/ISCC.exe'),
        (Join-Path $env:ProgramFiles 'Inno Setup 7/ISCC.exe'),
        (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6/ISCC.exe')
    )
    $Compiler = $candidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
}
if (-not $Compiler) { throw 'Install Inno Setup 7, or pass -Compiler PATH to ISCC.exe (6.4 or later)' }
$arguments = @("/DAppVersion=$version", "/DPackageDir=$package")
if ($InstallerAppId) { $arguments += "/DInstallerAppId=$InstallerAppId" }
& $Compiler @arguments (Join-Path $repo 'installer/windows.iss')
if ($LASTEXITCODE -ne 0) { throw "Inno Setup compilation failed: $LASTEXITCODE" }
$suffix = if ($InstallerAppId) { '-test' } else { '' }
$installer = Join-Path $repo "dist/aherald-$Tag-windows-x64-setup$suffix.exe"
$hash = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLower()
Set-Content -LiteralPath "$installer.sha256" -Value "$hash  $([IO.Path]::GetFileName($installer))" -Encoding ascii
Get-Item -LiteralPath $installer | Select-Object FullName, Length

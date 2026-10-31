param([string]$LifecycleScript = (Join-Path $PSScriptRoot 'installer_windows.ps1'))
$ErrorActionPreference = 'Stop'
$root = Join-Path ([IO.Path]::GetTempPath()) ('aherald-installer-diagnostics-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root | Out-Null
$installer = Join-Path $root 'broken-setup-test.exe'
& rustc --edition 2024 (Join-Path $PSScriptRoot 'fixtures/installer_failure.rs') -o $installer
if ($LASTEXITCODE -ne 0) { throw 'Failure fixture compilation failed' }
$output = & pwsh -NoProfile -File $LifecycleScript -Installer $installer 2>&1 | Out-String
$code = $LASTEXITCODE
if ($code -eq 0 -or -not $output.Contains('Preflight failure installed application files')) {
    throw "The original failure was lost during cleanup (exit $code): $output"
}
if ($output.Contains('This command cannot be run')) {
    throw "Cleanup attempted to launch a missing uninstaller: $output"
}
Write-Output 'Installer diagnostics preserve the original failure when no uninstaller exists.'

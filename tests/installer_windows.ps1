param([Parameter(Mandatory)][string]$Installer)
# Black-box installer acceptance. Requires the separately compiled test AppId.
$ErrorActionPreference = 'Stop'
$Installer = (Resolve-Path -LiteralPath $Installer).Path
if ([IO.Path]::GetFileName($Installer) -notlike '*-setup-test.exe') { throw 'Use build-installer.ps1 -InstallerAppId aHerald.Native.InstallerTest' }
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('aherald-installer-test-' + [guid]::NewGuid().ToString('N'))
$appDir = Join-Path $testRoot 'App'
$codexDir = Join-Path $testRoot 'Codex'
$claudeDir = Join-Path $testRoot 'Claude'
$receipt = Join-Path $appDir 'aherald-setup.json'
$uninstaller = Join-Path $appDir 'unins000.exe'
$previousCodex = $env:CODEX_HOME
$previousClaude = $env:CLAUDE_CONFIG_DIR
$receiptText = $null
$ownedResidents = [Collections.Generic.List[Diagnostics.Process]]::new()
$fakeExe = Join-Path $testRoot "old-resident.exe"
New-Item -ItemType Directory -Path $codexDir, $claudeDir | Out-Null

function Assert-Test($Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
function Invoke-InstallerProcess([string]$File, [string[]]$Arguments) {
    $process = Start-Process -FilePath $File -ArgumentList $Arguments -WindowStyle Hidden -PassThru
    if (-not $process.WaitForExit(45000)) {
        & taskkill.exe /PID $process.Id /T /F | Out-Null
        throw "Installer timed out: $File"
    }
    $process.Refresh()
    return $process.ExitCode
}
function Install-Test([string]$Tasks, [string]$LogName) {
    Invoke-InstallerProcess $Installer @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-', ('/DIR="' + $appDir + '"'), '/GROUP="aHerald Installer Test"', ('/TASKS="' + $Tasks + '"'), ('/LOG="' + (Join-Path $testRoot $LogName) + '"'))
}
function Start-TestResident([string]$File) {
    $dir = Split-Path -Parent $File
    foreach ($name in 'resident.pid','quit-called','quit-request','graceful-exit') {
        Remove-Item -LiteralPath (Join-Path $dir $name) -ErrorAction SilentlyContinue
    }
    $p = Start-Process -FilePath $File -ArgumentList '--resident' -WindowStyle Hidden -PassThru
    $ownedResidents.Add($p)
    $timer = [Diagnostics.Stopwatch]::StartNew()
    while (-not (Test-Path -LiteralPath (Join-Path $dir 'resident.pid'))) {
        if ($p.HasExited -or $timer.Elapsed.TotalSeconds -gt 5) { throw 'Test resident did not start' }
        Start-Sleep -Milliseconds 20
    }
    return $p
}
& rustc --edition 2024 -O (Join-Path $PSScriptRoot 'fixtures/installer_resident.rs') -o $fakeExe
if ($LASTEXITCODE -ne 0) { throw 'Old resident fixture compilation failed' }
try {
    $env:CODEX_HOME = $codexDir
    $env:CLAUDE_CONFIG_DIR = $claudeDir
    Set-Content -LiteralPath (Join-Path $claudeDir 'settings.json') -Value '{invalid' -Encoding utf8
    $code = Install-Test 'codex,claude' 'preflight.log'
    Assert-Test ($code -ne 0) 'Malformed hook configuration was accepted'
    Assert-Test (-not (Test-Path -LiteralPath (Join-Path $appDir 'aherald.exe'))) 'Preflight failure installed application files'
    Assert-Test (-not (Test-Path -LiteralPath (Join-Path $codexDir 'hooks.json'))) 'Preflight failure wrote Codex hooks'

    $foreign = '{"theme":"dark","hooks":{"Stop":[{"hooks":[{"type":"command","command":"my-notifier"}]}]}}'
    [IO.File]::WriteAllText((Join-Path $claudeDir 'settings.json'), $foreign)
    Assert-Test ((Install-Test 'codex,claude' 'install.log') -eq 0) 'Install failed'
    $settings = Get-Content -LiteralPath (Join-Path $claudeDir 'settings.json') -Raw | ConvertFrom-Json
    Assert-Test ($settings.theme -eq 'dark') 'Foreign setting changed'
    Assert-Test ($settings.hooks.Stop[0].hooks[0].command -eq 'my-notifier') 'Foreign hook changed'
    $own = $settings.hooks.Stop[1].hooks[0]
    Assert-Test ($own.command -eq (Join-Path $appDir 'aherald.exe')) 'Installed hook has the wrong executable path'
    Assert-Test (($own.args -join ' ') -eq '--hook --agent claude --resident') 'Claude hook does not use exec form'
    Assert-Test (Test-Path -LiteralPath (Join-Path $codexDir 'hooks.json')) 'Codex was not configured'
    Assert-Test (Test-Path -LiteralPath $receipt) 'Setup receipt missing'
    Assert-Test ((Get-Content -LiteralPath (Join-Path $claudeDir 'settings.json.aherald-backup') -Raw).Trim() -eq $foreign) 'Backup did not preserve the original settings'

    # Exercise upgrade against a real locked EXE with the old CLI boundary.
    $installedExe = Join-Path $appDir 'aherald.exe'
    $realHash = (Get-FileHash -LiteralPath $installedExe).Hash
    Copy-Item -LiteralPath $fakeExe -Destination $installedExe -Force
    $oldResident = Start-TestResident $installedExe
    # Preflight errors must not interrupt the running application.
    $claudeSettingsFile = Join-Path $claudeDir 'settings.json'
    $validClaudeSettings = [IO.File]::ReadAllText($claudeSettingsFile)
    [IO.File]::WriteAllText($claudeSettingsFile, '{invalid')
    Assert-Test ((Install-Test 'codex,claude' 'running-preflight-error.log') -ne 0) 'Running upgrade accepted invalid hooks'
    Assert-Test (-not $oldResident.HasExited) 'Preflight failure stopped the old process'
    Assert-Test (-not (Test-Path -LiteralPath (Join-Path $appDir 'quit-called'))) 'Preflight failure requested shutdown'
    [IO.File]::WriteAllText($claudeSettingsFile, $validClaudeSettings)

    # Acknowledging quit without exiting must fail before file replacement.
    [IO.File]::WriteAllText((Join-Path $appDir 'block-quit'), 'yes')
    Assert-Test ((Install-Test 'codex,claude' 'running-stop-timeout.log') -ne 0) 'Upgrade ignored a process that did not exit'
    Assert-Test (-not $oldResident.HasExited) 'Installer forcibly terminated the old process'
    Assert-Test ((Get-FileHash -LiteralPath $installedExe).Hash -eq (Get-FileHash -LiteralPath $fakeExe).Hash) 'Shutdown failure replaced application files'
    Remove-Item -LiteralPath (Join-Path $appDir 'block-quit')
    Assert-Test ((Install-Test 'codex,claude' 'running-upgrade.log') -eq 0) 'Upgrade did not stop the running installed program'
    Assert-Test ($oldResident.WaitForExit(1000)) 'Upgrade returned before the old process exited'
    Assert-Test (Test-Path -LiteralPath (Join-Path $appDir 'graceful-exit')) 'Old process was not stopped gracefully'
    Assert-Test ((Get-FileHash -LiteralPath $installedExe).Hash -eq $realHash) 'Upgrade did not replace the old executable'

    # A status PID from another installation must never receive quit.
    $otherDir = Join-Path $testRoot 'OtherApp'
    New-Item -ItemType Directory $otherDir | Out-Null
    $otherExe = Join-Path $otherDir 'aherald.exe'
    Copy-Item -LiteralPath $fakeExe -Destination $otherExe
    $otherResident = Start-TestResident $otherExe
    Copy-Item -LiteralPath $fakeExe -Destination $installedExe -Force
    [IO.File]::WriteAllText((Join-Path $appDir 'resident.pid'), $otherResident.Id.ToString())
    Remove-Item -LiteralPath (Join-Path $appDir 'quit-called') -ErrorAction SilentlyContinue
    Assert-Test ((Install-Test 'codex,claude' 'other-resident-upgrade.log') -eq 0) 'Upgrade with an unrelated resident failed'
    Assert-Test (-not $otherResident.HasExited) 'Upgrade stopped another installation'
    Assert-Test (-not (Test-Path -LiteralPath (Join-Path $appDir 'quit-called'))) 'Upgrade sent quit for another installation'
    [IO.File]::WriteAllText((Join-Path $otherDir 'quit-request'), 'yes')
    Assert-Test ($otherResident.WaitForExit(5000)) 'Other test resident cleanup failed'

    $config = Join-Path $appDir 'config.toml'
    [IO.File]::WriteAllText($config, '# user settings must survive an upgrade')
    Assert-Test ((Install-Test 'claude' 'upgrade.log') -eq 0) 'Upgrade/reconfiguration failed'
    Assert-Test ((Get-Content -LiteralPath $config -Raw) -eq '# user settings must survive an upgrade') 'Upgrade replaced user configuration'
    $codex = Get-Content -LiteralPath (Join-Path $codexDir 'hooks.json') -Raw | ConvertFrom-Json
    Assert-Test ($null -eq $codex.hooks) 'Deselected Codex hooks remained installed'

    $customUsage = "# keep custom settings" + [Environment]::NewLine + "[notifications]" + [Environment]::NewLine + "scale = 1.5" + [Environment]::NewLine + "[claude.usage]" + [Environment]::NewLine + "enabled = true # keep quota note" + [Environment]::NewLine + "source = 'statusline'" + [Environment]::NewLine
    [IO.File]::WriteAllText($config, $customUsage)
    Assert-Test ((Install-Test 'claude,usagecodex,usageclaude' 'usage-enable.log') -eq 0) 'Usage selection install failed'
    $readUsage = (& (Join-Path $appDir 'aherald.exe') --setup-settings | Out-String)
    Assert-Test ($readUsage.Contains('codex_usage=true') -and $readUsage.Contains('claude_usage=true')) 'Usage checkboxes were not applied'
    $afterUsage = Get-Content -LiteralPath $config -Raw
    Assert-Test ($afterUsage.Contains("source = 'statusline'") -and $afterUsage.Contains('# keep quota note') -and $afterUsage.Contains('scale = 1.5')) 'Usage setup replaced custom settings/comments/source'
    Assert-Test ((Get-Content -LiteralPath ($config + '.aherald-backup') -Raw) -eq $customUsage) 'Usage backup missing original config'
    # No /TASKS means use the live config, including edits made outside the installer.
    Assert-Test ((Invoke-InstallerProcess $Installer @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-', ('/DIR="' + $appDir + '"'), '/GROUP="aHerald Installer Test"')) -eq 0) 'Default upgrade failed'
    Assert-Test ((Get-Content -LiteralPath $config -Raw) -eq $afterUsage) 'Default upgrade changed configured usage'
    Assert-Test ((Install-Test 'claude,usageclaude' 'usage-disable.log') -eq 0) 'Usage deselection failed'
    $readUsage = (& (Join-Path $appDir 'aherald.exe') --setup-settings | Out-String)
    Assert-Test ($readUsage.Contains('codex_usage=false') -and $readUsage.Contains('claude_usage=true')) 'Usage checkboxes did not disable only Codex'

    $receiptText = Get-Content -LiteralPath $receipt -Raw
    [IO.File]::WriteAllText($receipt, '{invalid')
    $null = Invoke-InstallerProcess $uninstaller @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', ('/LOG="' + (Join-Path $testRoot 'uninstall-blocked.log') + '"'))
    Assert-Test (Test-Path -LiteralPath (Join-Path $appDir 'aherald.exe')) 'Failed cleanup deleted the executable'
    [IO.File]::WriteAllText($receipt, $receiptText)
    $env:CODEX_HOME = Join-Path $testRoot 'OtherCodex'
    $env:CLAUDE_CONFIG_DIR = Join-Path $testRoot 'OtherClaude'
    Assert-Test ((Invoke-InstallerProcess $uninstaller @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', ('/LOG="' + (Join-Path $testRoot 'uninstall.log') + '"'))) -eq 0) 'Uninstall failed'
    Assert-Test (-not (Test-Path -LiteralPath (Join-Path $appDir 'aherald.exe'))) 'Uninstall left the executable'
    $restored = Get-Content -LiteralPath (Join-Path $claudeDir 'settings.json') -Raw | ConvertFrom-Json
    Assert-Test (($restored | ConvertTo-Json -Depth 20 -Compress) -eq (($foreign | ConvertFrom-Json) | ConvertTo-Json -Depth 20 -Compress)) 'Uninstall did not restore foreign settings/hooks'
    Assert-Test (Test-Path -LiteralPath $config) 'Uninstall deleted user configuration'
    # With no hooks selected, exercise uninstaller shutdown with the same boundary.
    Assert-Test ((Install-Test '' 'shutdown-uninstall-install.log') -eq 0) 'Empty-selection reinstall failed'
    Copy-Item -LiteralPath $fakeExe -Destination $installedExe -Force
    $uninstallResident = Start-TestResident $installedExe
    Assert-Test ((Invoke-InstallerProcess $uninstaller @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART')) -eq 0) 'Running uninstall failed'
    Assert-Test ($uninstallResident.WaitForExit(1000)) 'Uninstall returned before the process exited'
    Assert-Test (Test-Path -LiteralPath (Join-Path $appDir 'graceful-exit')) 'Uninstall did not stop the process gracefully'
    Assert-Test (-not (Test-Path -LiteralPath $installedExe)) 'Running uninstall left its executable'
    Write-Output "Installer lifecycle passed. Evidence: $testRoot"
}
finally {
    foreach ($resident in $ownedResidents) {
        if (-not $resident.HasExited) { $resident.Kill($true) }
        $null = $resident.WaitForExit(5000)
        $resident.Dispose()
    }
    if (Test-Path -LiteralPath (Join-Path $appDir 'aherald.exe')) {
        if ($receiptText) { [IO.File]::WriteAllText($receipt, $receiptText) }
        $null = Invoke-InstallerProcess $uninstaller @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART')
    }
    $env:CODEX_HOME = $previousCodex
    $env:CLAUDE_CONFIG_DIR = $previousClaude
}

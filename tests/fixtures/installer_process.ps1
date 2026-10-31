function Invoke-InstallerProcess([string]$File, [string[]]$Arguments) {
    # Inno bootstrap processes may exit before their install/uninstall child. PowerShell's
    # -Wait uses a Windows job to wait for the complete tree. A dedicated worker lets the
    # caller keep a bounded wait and terminate only its own tree on timeout.
    $payload = @{ File = $File; Arguments = $Arguments } | ConvertTo-Json -Compress
    $encodedPayload = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($payload))
    $worker = '$ErrorActionPreference = ''Stop''; $p = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String(''' + $encodedPayload + ''')) | ConvertFrom-Json; $child = Start-Process -FilePath $p.File -ArgumentList @($p.Arguments) -WindowStyle Hidden -Wait -PassThru; exit $child.ExitCode'
    $encodedWorker = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($worker))
    $shell = (Get-Process -Id $PID).Path
    $process = Start-Process -FilePath $shell -ArgumentList @('-NoProfile', '-NonInteractive', '-EncodedCommand', $encodedWorker) -WindowStyle Hidden -PassThru
    if (-not $process.WaitForExit(45000)) {
        & taskkill.exe /PID $process.Id /T /F | Out-Null
        throw "Installer timed out: $File"
    }
    $process.Refresh()
    return $process.ExitCode
}

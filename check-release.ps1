param(
    [string]$Executable = (Join-Path $PSScriptRoot 'target/x86_64-pc-windows-msvc/release/aherald.exe')
)
$ErrorActionPreference = 'Stop'
$tool = Get-Command llvm-readobj -ErrorAction Stop
$report = & $tool.Source --file-headers --coff-imports $Executable
if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect PE executable' }
$text = $report -join "`n"
if ($text -notmatch 'Machine: IMAGE_FILE_MACHINE_AMD64' -or $text -notmatch 'Subsystem: IMAGE_SUBSYSTEM_WINDOWS_GUI') {
    throw 'Expected an x64 GUI executable'
}
$imports = @($report | ForEach-Object { if ($_ -match '^\s+Name: (.+\.dll)$') { $Matches[1] } })
if ($imports.Count -eq 0) { throw 'No PE imports found' }
if ($imports | Where-Object { $_ -match '^(VCRUNTIME|MSVCP|MSVCR|ucrtbase|api-ms-win-crt)' }) {
    throw 'Release still depends on a separately distributed C/C++ runtime'
}
[pscustomobject]@{
    Executable = (Resolve-Path -LiteralPath $Executable).Path
    Bytes = (Get-Item -LiteralPath $Executable).Length
    Architecture = 'x64'
    Subsystem = 'Windows GUI'
    Imports = ($imports -join ', ')
}

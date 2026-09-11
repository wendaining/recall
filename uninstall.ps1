# recall uninstaller for Windows
#
# Usage:
#     irm https://raw.githubusercontent.com/wendaining/recall/master/uninstall.ps1 | iex
#
# Keeps your configuration and command history.

$ErrorActionPreference = 'Stop'

$markerStart = '# >>> recall installer >>>'
$markerEnd = '# <<< recall installer <<<'

function Say($message) { Write-Host $message }

function Remove-ManagedBlock($path) {
    if (-not $path -or -not (Test-Path -LiteralPath $path)) { return }
    $lines = @(Get-Content -LiteralPath $path -ErrorAction SilentlyContinue)
    if ($lines -notcontains $markerStart) { return }

    $kept = New-Object System.Collections.Generic.List[string]
    $skip = $false
    foreach ($line in $lines) {
        if ($line.Trim() -eq $markerStart) { $skip = $true; continue }
        if ($line.Trim() -eq $markerEnd) { $skip = $false; continue }
        if (-not $skip) { $kept.Add($line) }
    }
    Set-Content -LiteralPath $path -Value $kept
    Say "Removed installer-managed setup from $path"
}

foreach ($profilePath in @($PROFILE.CurrentUserAllHosts, $PROFILE.CurrentUserCurrentHost)) {
    Remove-ManagedBlock $profilePath
}

$candidates = @()
if ($env:RECALL_INSTALL_DIR) {
    $candidates += (Join-Path $env:RECALL_INSTALL_DIR 'recall.exe')
} else {
    $candidates += (Join-Path $HOME '.local\bin\recall.exe')
    $candidates += (Join-Path $env:LOCALAPPDATA 'recall\bin\recall.exe')
    $candidates += (Join-Path $env:LOCALAPPDATA 'Microsoft\WindowsApps\recall.exe')
}

$found = $false
$removed = $false
foreach ($candidate in $candidates) {
    if (Test-Path -LiteralPath $candidate) {
        $found = $true
        try {
            Remove-Item -LiteralPath $candidate -Force
            Say "Removed $candidate"
            $removed = $true
        } catch {
            Say "Could not remove $candidate`: $_"
        }
    }
}

if (-not $found) {
    Say 'No recall binary was found in the installer locations.'
}

Say ''
if ($found -and -not $removed) {
    Say 'The shell setup was removed, but the recall binary is still installed.'
    exit 1
}
Say 'recall has been uninstalled from your shell setup.'
Say 'Your configuration and command history were kept.'

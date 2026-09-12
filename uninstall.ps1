# recall uninstaller for Windows
#
# Usage:
#     irm https://raw.githubusercontent.com/wendaining/recall/master/uninstall.ps1 | iex
#
# Keeps your configuration and command history.

$ErrorActionPreference = 'Stop'

$markerPairs = @{
    '# >>> recall installer >>>' = '# <<< recall installer <<<'
    '# >>> recall setup bootstrap >>>' = '# <<< recall setup bootstrap <<<'
    '# >>> recall setup integration >>>' = '# <<< recall setup integration <<<'
    '# >>> recall setup integration-no-eol >>>' = '# <<< recall setup integration-no-eol <<<'
}

function Say($message) { Write-Host $message }

function Remove-ManagedBlock($path) {
    if (-not $path -or -not (Test-Path -LiteralPath $path)) { return }
    $lines = @(Get-Content -LiteralPath $path -ErrorAction SilentlyContinue)
    $hasManagedBlock = $false
    foreach ($line in $lines) {
        if ($markerPairs.ContainsKey($line.Trim())) {
            $hasManagedBlock = $true
            break
        }
    }
    if (-not $hasManagedBlock) { return }

    $recall = Get-Command recall -ErrorAction SilentlyContinue
    if ($recall) {
        & $recall.Source setup pwsh --profile $path --remove 2>$null | Out-Null
        if ($LASTEXITCODE -eq 0) {
            Say "Removed recall-managed setup from $path"
            return
        }
    }

    $kept = New-Object System.Collections.Generic.List[string]
    $expectedEnd = $null
    foreach ($line in $lines) {
        $trimmed = $line.Trim()
        if ($expectedEnd) {
            if ($trimmed -eq $expectedEnd) { $expectedEnd = $null }
            continue
        }
        if ($markerPairs.ContainsKey($trimmed)) {
            $expectedEnd = $markerPairs[$trimmed]
            continue
        }
        $kept.Add($line)
    }
    if ($expectedEnd) {
        Say "Could not safely remove malformed recall setup from $path"
        return
    }
    Set-Content -LiteralPath $path -Value $kept
    Say "Removed recall-managed setup from $path"
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

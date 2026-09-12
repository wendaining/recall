# recall installer for Windows
#
# Usage (PowerShell 7 or Windows PowerShell 5.1):
#     irm https://raw.githubusercontent.com/wendaining/recall/master/install.ps1 | iex
#
# Environment overrides:
#     RECALL_INSTALL_DIR        install directory (default: $HOME\.local\bin)
#     RECALL_VERSION            release tag to install (default: latest)
#     RECALL_NO_MODIFY_PROFILE  set to skip $PROFILE changes
#     RECALL_PROXY_SETUP        auto, hooks, or terminal (default: auto)
#     RECALL_IMPORT_HISTORY     yes, no, or ask (default: ask)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$repo = 'wendaining/recall'
function Say($message) { Write-Host $message }
function Die($message) {
    Write-Error "recall installer: $message"
    exit 1
}

function Resolve-ProxySetup {
    $value = if ($env:RECALL_PROXY_SETUP) {
        $env:RECALL_PROXY_SETUP.ToLowerInvariant()
    } else {
        'auto'
    }
    switch ($value) {
        'auto' { return 'auto' }
        'shell' { return 'auto' }
        'hooks' { return 'hooks' }
        'none' { return 'hooks' }
        'terminal' { return 'terminal' }
        default { Die 'RECALL_PROXY_SETUP must be auto, hooks, or terminal' }
    }
}

function Should-ImportHistory($label, $path) {
    $preference = if ($env:RECALL_IMPORT_HISTORY) {
        $env:RECALL_IMPORT_HISTORY.ToLowerInvariant()
    } else {
        'ask'
    }
    switch ($preference) {
        { $_ -in @('1', 'true', 'yes') } { return $true }
        { $_ -in @('0', 'false', 'no') } { return $false }
        { $_ -in @('', 'ask') } { }
        default { Die 'RECALL_IMPORT_HISTORY must be yes, no, or ask' }
    }

    try {
        if ([Console]::IsInputRedirected) { return $false }
    } catch {
        return $false
    }
    $answer = Read-Host "Import existing $label from $path`? [Y/n]"
    return $answer -in @('', 'y', 'Y', 'yes', 'YES', 'Yes')
}

function Offer-HistoryImport($kind, $shell, $path, $label) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return }
    if (-not (Should-ImportHistory $label $path)) { return }

    if ($kind -eq 'atuin') {
        & $destination import atuin --path $path
    } else {
        & $destination import history $shell --path $path
    }
    if ($LASTEXITCODE -eq 0) {
        Say "Imported $label."
    } else {
        Say "Warning: could not import $label; installation will continue."
    }
}

function Show-Logo {
    @(
        '             .-=================-.'
        "          .-'                     ``-."
        "        .'       +----------+        ``."
        '       /        /    >_      \         \'
        '      ;        |      _       |         ;'
        '      |        |              |         |'
        '      ;        |              |         ;'
        '       \        \            /         /'
        "        ``.       +----------+        .'"
        "          ``-.                     _.-'"
        "             ``-=================-'"
    ) | ForEach-Object { Write-Host $_ }
}

Show-Logo

# GitHub requires TLS 1.2, which older Windows PowerShell does not negotiate by
# default.
try {
    [Net.ServicePointManager]::SecurityProtocol =
        [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
} catch {
}

$arch = $env:PROCESSOR_ARCHITECTURE
if ($env:PROCESSOR_ARCHITEW6432) { $arch = $env:PROCESSOR_ARCHITEW6432 }
switch ($arch) {
    'AMD64' { $target = 'x86_64-pc-windows-msvc' }
    default { Die "unsupported architecture: $arch" }
}

$headers = @{
    'Accept'               = 'application/vnd.github+json'
    'X-GitHub-Api-Version' = '2022-11-28'
    'User-Agent'           = 'recall-installer'
}

if ($env:RECALL_VERSION) {
    $tag = $env:RECALL_VERSION
} else {
    try {
        $release = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest" -Headers $headers
    } catch {
        Die "failed to query the latest GitHub release: $_"
    }
    $tag = $release.tag_name
}
if (-not $tag) { Die 'latest GitHub release did not contain a tag' }

$archive = "recall-$tag-$target.zip"
$downloadBase = "https://github.com/$repo/releases/download/$tag"
$tempDir = Join-Path ([IO.Path]::GetTempPath()) ("recall-install-" + [guid]::NewGuid().ToString('n'))
New-Item -ItemType Directory -Path $tempDir -Force | Out-Null

try {
    Say "Installing recall $tag for $target..."
    try {
        Invoke-WebRequest "$downloadBase/$archive" -OutFile (Join-Path $tempDir $archive)
        Invoke-WebRequest "$downloadBase/SHA256SUMS" -OutFile (Join-Path $tempDir 'SHA256SUMS')
    } catch {
        Die "failed to download $archive`: $_"
    }

    $sumLine = Select-String -Path (Join-Path $tempDir 'SHA256SUMS') -Pattern ([regex]::Escape($archive)) |
        Select-Object -First 1
    if (-not $sumLine) { Die "SHA256SUMS does not contain $archive" }
    $expected = ($sumLine.Line -split '\s+')[0].ToLowerInvariant()
    $actual = (Get-FileHash (Join-Path $tempDir $archive) -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $expected) { Die "checksum mismatch for $archive" }
    Say 'Checksum verified.'

    Expand-Archive -Path (Join-Path $tempDir $archive) -DestinationPath $tempDir -Force
    $payload = Join-Path $tempDir 'recall.exe'
    if (-not (Test-Path $payload)) { Die 'archive does not contain recall.exe' }

    if ($env:RECALL_INSTALL_DIR) {
        $installDir = $env:RECALL_INSTALL_DIR
    } else {
        $installDir = Join-Path $HOME '.local\bin'
    }
    New-Item -ItemType Directory -Path $installDir -Force | Out-Null
    $destination = Join-Path $installDir 'recall.exe'
    Copy-Item $payload $destination -Force

    # Add the install directory to the user PATH if it is not there yet.
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (-not $userPath) { $userPath = '' }
    if (($userPath -split ';') -notcontains $installDir) {
        $newPath = if ($userPath) { "$installDir;$userPath" } else { $installDir }
        [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    }
    if (($env:Path -split ';') -notcontains $installDir) {
        $env:Path = "$installDir;$env:Path"
    }

    $proxySetup = Resolve-ProxySetup
    $shellConfigured = $false
    if (-not $env:RECALL_NO_MODIFY_PROFILE) {
        $profilePath = $PROFILE.CurrentUserCurrentHost
        $setupMode = if ($proxySetup -eq 'terminal') { 'hooks' } else { $proxySetup }
        & $destination setup pwsh --mode $setupMode --profile $profilePath
        if ($LASTEXITCODE -ne 0) { Die "failed to configure $profilePath" }
        $shellConfigured = $true
    } else {
        Say 'Shell integration: skipped (RECALL_NO_MODIFY_PROFILE)'
    }

    $configPath = (& $destination config path | Out-String).Trim()
    if ($configPath -and -not (Test-Path $configPath)) {
        $configDir = Split-Path -Parent $configPath
        if ($configDir) { New-Item -ItemType Directory -Path $configDir -Force | Out-Null }
        # WriteAllText is UTF-8 without a BOM, which the TOML parser requires.
        [System.IO.File]::WriteAllText($configPath, (& $destination config default | Out-String))
        Say "Config: created $configPath"
    } elseif ($configPath) {
        Say "Config: kept existing $configPath"
    }

    $historyDataDir = if ($env:XDG_DATA_HOME -and [IO.Path]::IsPathRooted($env:XDG_DATA_HOME)) {
        $env:XDG_DATA_HOME
    } else {
        Join-Path $HOME '.local\share'
    }
    Offer-HistoryImport 'atuin' '' (Join-Path $historyDataDir 'atuin\history.db') 'atuin history'
    $zshHome = if ($env:ZDOTDIR) { $env:ZDOTDIR } else { $HOME }
    Offer-HistoryImport 'history' 'zsh' (Join-Path $zshHome '.zsh_history') 'zsh history'
    Offer-HistoryImport 'history' 'bash' (Join-Path $HOME '.bash_history') 'bash history'
    Offer-HistoryImport 'history' 'fish' (Join-Path $historyDataDir 'fish\fish_history') 'fish history'
    if ($env:APPDATA) {
        $psHistory = Join-Path $env:APPDATA 'Microsoft\Windows\PowerShell\PSReadLine\ConsoleHost_history.txt'
        Offer-HistoryImport 'history' 'pwsh' $psHistory 'PowerShell history'
    }

    Say ''
    Say 'recall is ready.'
    Say ''
    Say "Installed: $(& $destination --version) at $destination"
    if (-not $shellConfigured) {
        Say "Output: not configured; run 'recall shell' manually"
    } elseif ($proxySetup -eq 'auto') {
        Say 'Output: automatic capture in new interactive shells'
        Say "Alternative: run 'recall setup pwsh --mode hooks' for hooks only"
    } elseif ($proxySetup -eq 'hooks') {
        Say "Output: hooks only; run 'recall shell' when capture is needed"
    } else {
        Say 'Output: terminal-managed compatibility mode'
        Say "Set the Windows Terminal profile's Command line to: $destination shell"
    }
    Say 'Open a new terminal, then run recall or press Alt+R to browse history.'
    Say "Before capturing sensitive work, review: $configPath"
} finally {
    Remove-Item -LiteralPath $tempDir -Recurse -Force -ErrorAction SilentlyContinue
}

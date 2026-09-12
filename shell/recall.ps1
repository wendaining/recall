# recall shell integration for PowerShell
#
# Add to $PROFILE:
#     recall init pwsh | Out-String | Invoke-Expression
#
# When running under `recall shell` (RECALL_PROXY_ACTIVE=1), command metadata is
# written as private OSC markers straight into the terminal stream, where the
# proxy parses and strips them. Otherwise a metadata-only record is written in
# the background.
#
# Works with PowerShell 7 and Windows PowerShell 5.1.

if (-not (Get-Module PSReadLine -ErrorAction Ignore)) {
    Write-Warning "recall: PSReadLine is required; PowerShell integration disabled."
} elseif (-not (Get-Command recall -ErrorAction Ignore)) {
    Write-Warning "recall: the 'recall' executable must be on PATH; integration disabled."
} elseif (-not $global:__RecallInstalled) {
    $global:__RecallInstalled = $true
    $global:__RecallActiveId = ''
    $global:__RecallLastCommand = ''
    $global:__RecallLastStartTicks = 0

    if ($env:RECALL_SESSION) {
        $global:__RecallSession = $env:RECALL_SESSION
    } else {
        $global:__RecallSession = (& recall uuid 2>$null | Out-String).Trim()
    }

    function global:__recall_json_escape {
        param([string]$Value)

        $builder = New-Object System.Text.StringBuilder
        foreach ($ch in $Value.ToCharArray()) {
            $code = [int]$ch
            if ($code -eq 0x22) {
                [void]$builder.Append('\"')
            } elseif ($code -eq 0x5C) {
                [void]$builder.Append('\\')
            } elseif ($code -eq 0x08) {
                [void]$builder.Append('\b')
            } elseif ($code -eq 0x0C) {
                [void]$builder.Append('\f')
            } elseif ($code -eq 0x0A) {
                [void]$builder.Append('\n')
            } elseif ($code -eq 0x0D) {
                [void]$builder.Append('\r')
            } elseif ($code -eq 0x09) {
                [void]$builder.Append('\t')
            } elseif ($code -lt 0x20) {
                [void]$builder.Append('\u{0:x4}' -f $code)
            } else {
                [void]$builder.Append($ch)
            }
        }
        $builder.ToString()
    }

    function global:__recall_should_skip {
        param([string]$Command)

        if ([string]::IsNullOrWhiteSpace($Command)) {
            return $true
        }
        return ($Command -match '^\s*recall(\s|$)')
    }

    # Emit a private OSC marker carrying a JSON control message. The marker is
    # written as raw UTF-8 bytes so non-ASCII commands survive regardless of the
    # console output code page.
    function global:__recall_emit {
        param([string]$Payload)

        try {
            # Write through the console writer so the marker is ordered with the
            # host's own output instead of racing its buffer.
            [Console]::Write(([char]27) + ']9999;' + $Payload + ([char]7))
            [Console]::Out.Flush()
        } catch {
        }
    }

    # Quote one argument using the Windows CRT rules so `recall record` receives
    # commands containing spaces and quotes intact.
    function global:__recall_quote_arg {
        param([string]$Arg)

        if ($Arg.Length -gt 0 -and $Arg -notmatch '[ \t"]') {
            return $Arg
        }
        $builder = New-Object System.Text.StringBuilder
        [void]$builder.Append('"')
        $backslashes = 0
        foreach ($ch in $Arg.ToCharArray()) {
            if ($ch -eq '\') {
                $backslashes++
                continue
            }
            if ($ch -eq '"') {
                [void]$builder.Append('\' * ($backslashes * 2 + 1))
                [void]$builder.Append('"')
                $backslashes = 0
                continue
            }
            if ($backslashes -gt 0) {
                [void]$builder.Append('\' * $backslashes)
                $backslashes = 0
            }
            [void]$builder.Append($ch)
        }
        if ($backslashes -gt 0) {
            [void]$builder.Append('\' * ($backslashes * 2))
        }
        [void]$builder.Append('"')
        $builder.ToString()
    }

    function global:__recall_record {
        param([string]$Command, [string]$Cwd, [int]$ExitCode, [long]$DurationNs)

        try {
            $args = @(
                'record',
                '--command', $Command,
                '--cwd', $Cwd,
                '--exit', $ExitCode,
                '--duration-ns', $DurationNs,
                '--shell', 'pwsh'
            ) | ForEach-Object { __recall_quote_arg $_ }
            $start = New-Object System.Diagnostics.ProcessStartInfo
            $start.FileName = 'recall'
            $start.Arguments = ($args -join ' ')
            $start.UseShellExecute = $false
            $start.CreateNoWindow = $true
            [void][System.Diagnostics.Process]::Start($start)
        } catch {
        }
    }

    $global:__RecallPrevReadLine = $function:PSConsoleHostReadLine
    $global:__RecallPrevPrompt = $function:prompt

    # The end boundary travels inside the prompt string itself: the host writes
    # command output first and the prompt after, so markers embedded in the
    # returned prompt land after the output. `prompt` tells the proxy to discard
    # the prompt text until the matching `end`.
    function global:prompt {
        $lastStatus = $?
        $lastNative = $global:LASTEXITCODE
        $exitCode = if ($lastStatus) { 0 } elseif ($lastNative) { $lastNative } else { 1 }

        $prefix = ''
        $suffix = ''

        if ($env:RECALL_PROXY_ACTIVE -and $global:__RecallActiveId) {
            $duration = ''
            if ($global:__RecallLastStartTicks) {
                $durationNs = ([DateTime]::UtcNow.Ticks - $global:__RecallLastStartTicks) * 100
                $duration = ',"duration_ns":' + $durationNs
            }
            $payload = '{{"type":"end","id":"{0}","exit":{1}{2}}}' -f `
                $global:__RecallActiveId, $exitCode, $duration
            $prefix = ([char]27) + ']9999;{"type":"prompt"}' + ([char]7)
            $suffix = ([char]27) + ']9999;' + $payload + ([char]7)
            $global:__RecallActiveId = ''
        } elseif (-not $env:RECALL_PROXY_ACTIVE -and $global:__RecallLastCommand) {
            $durationNs = 0
            if ($global:__RecallLastStartTicks) {
                $durationNs = ([DateTime]::UtcNow.Ticks - $global:__RecallLastStartTicks) * 100
            }
            __recall_record $global:__RecallLastCommand $env:__RecallLastCwd $exitCode $durationNs
            $global:__RecallLastCommand = ''
        }

        # Restore the previous status so the user's prompt (oh-my-posh,
        # starship, ...) still reports the right exit code.
        $global:LASTEXITCODE = $lastNative
        if ($lastStatus) {
            $null = 1
        } else {
            $null = Get-Item -LiteralPath '__recall_no_such_status__' -ErrorAction SilentlyContinue
        }

        $prefix + (& $global:__RecallPrevPrompt) + $suffix
    }

    # Wrapping PSConsoleHostReadLine gives the preexec hook: after it returns the
    # accepted line and before that line runs, emit the `start` marker.
    function global:PSConsoleHostReadLine {
        $line = & $global:__RecallPrevReadLine

        $global:__RecallLastCommand = ''
        $global:__RecallLastStartTicks = 0

        if ($line -and -not (__recall_should_skip $line)) {
            $global:__RecallLastCommand = $line
            $global:__RecallLastStartTicks = [DateTime]::UtcNow.Ticks
            $env:__RecallLastCwd = $PWD.Path

            if ($env:RECALL_PROXY_ACTIVE) {
                $id = [guid]::NewGuid().ToString()
                $global:__RecallActiveId = $id
                $epochTicks = [DateTime]::UnixEpoch.Ticks
                $startedNs = ($global:__RecallLastStartTicks - $epochTicks) * 100
                $payload = '{{"type":"start","id":"{0}","command":"{1}","cwd":"{2}","started_at":{3}}}' -f `
                    $id, (__recall_json_escape $line), (__recall_json_escape $PWD.Path), $startedNs
                __recall_emit $payload
            }
        }

        return $line
    }

    # --- TUI widget ---------------------------------------------------------
    # The TUI renders to stderr, so stdout can be captured here. The key is set
    # by `ui.search_key` in the config and injected by `recall init`.
    Set-PSReadLineKeyHandler -Chord '@RECALL_SEARCH_KEY@' -BriefDescription 'recall search' -ScriptBlock {
        $previousEncoding = [Console]::OutputEncoding
        try {
            [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
            $output = & recall search --cmd-only
            $code = $LASTEXITCODE
        } finally {
            [Console]::OutputEncoding = $previousEncoding
        }
        if ($output) {
            $text = ($output | Out-String).TrimEnd("`r", "`n")
            [Microsoft.PowerShell.PSConsoleReadLine]::RevertLine()
            [Microsoft.PowerShell.PSConsoleReadLine]::Insert($text)
            if ($code -eq 2) {
                [Microsoft.PowerShell.PSConsoleReadLine]::AcceptLine()
            }
        }
    }
    $env:RECALL_HOOKS_ACTIVE = '1'
}

# Experimental kit38 supervisor. No installation, host authorization, personal
# configuration, Java management or application UDP. Dot-sourcing loads only
# pure helpers for focused tests; a normal invocation requires a real console.
[CmdletBinding()]
param(
    [string]$Binary = 'C:/AuroraConnectLab38/aurora-connect-lab.exe',
    [string]$HostIpv6 = '',
    [string]$HostPeer = ''
)

function Initialize-MgIo {
    if ('AuroraMinecraftGuestIo' -as [type]) { return }
    Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Text;
using System.Threading.Tasks;
public static class AuroraMinecraftGuestIo {
    public static async Task<string> LineAsync(StreamReader reader, int maximum) {
        var text = new StringBuilder(); var one = new char[1];
        while (true) {
            int count = await reader.ReadAsync(one, 0, 1).ConfigureAwait(false);
            if (count == 0) { if (text.Length == 0) return null; throw new IOException("mg_partial_event"); }
            if (one[0] == '\n') return text.ToString();
            if (text.Length >= maximum) throw new IOException("mg_event_limit");
            text.Append(one[0]);
        }
    }
    public static async Task<long> CountAsync(StreamReader reader) {
        var buffer = new char[1024]; long total = 0;
        while (true) {
            int count = await reader.ReadAsync(buffer, 0, buffer.Length).ConfigureAwait(false);
            if (count == 0) return total;
            if (total <= long.MaxValue - count) total += count;
        }
    }
}
'@
}

function Get-MgIpv6([string]$Text) {
    if (-not $Text -or $Text.Length -gt 45 -or $Text -cnotmatch '^[a-fA-F0-9:]+$') { throw 'mg_ipv6_invalid' }
    $address = $null
    if (-not [Net.IPAddress]::TryParse($Text, [ref]$address) -or $address.AddressFamily -ne [Net.Sockets.AddressFamily]::InterNetworkV6) { throw 'mg_ipv6_invalid' }
    $b = $address.GetAddressBytes()
    $doc = $b[0] -eq 0x20 -and $b[1] -eq 0x01 -and $b[2] -eq 0x0d -and $b[3] -eq 0xb8
    $docNew = $b[0] -eq 0x3f -and $b[1] -eq 0xff -and ($b[2] -band 0xf0) -eq 0
    if ($address.ScopeId -ne 0 -or ($b[0] -band 0xe0) -ne 0x20 -or $doc -or $docNew) { throw 'mg_ipv6_not_candidate' }
    return $address.ToString()
}
function Require-MgPeer([string]$Peer) {
    if ($Peer -cnotmatch '^[a-f0-9]{64}$') { throw 'mg_peer_invalid' }
}
function Require-MgBinary([string]$Path) {
    $item = Get-Item -LiteralPath $Path -Force
    if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or $item.Name -cne 'aurora-connect-lab.exe') { throw 'mg_binary_invalid' }
    # No linked parent directory: never silently redirect a trusted kit path.
    $parent = $item.Directory
    while ($parent) {
        if (($parent.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'mg_binary_invalid' }
        $parent = $parent.Parent
    }
    if ((Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256).Hash.ToLowerInvariant() -cne '5c1f3b3e61c2a975401d43f5c5d5568d9801a7252b7381451acb8026e6fd2030') { throw 'mg_binary_pin_mismatch' }
    return $item.FullName
}
function Require-MgDirectory([string]$Path) {
    $item = Get-Item -LiteralPath $Path -Force
    if (-not $item.PSIsContainer) { throw 'mg_evidence_root_invalid' }
    while ($item) {
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'mg_evidence_root_invalid' }
        $item = $item.Parent
    }
}
function New-MgEvidence([string]$Root, [string]$Attempt) {
    if ($Attempt -cnotmatch '^[a-f0-9]{32}$') { throw 'mg_evidence_root_invalid' }
    Require-MgDirectory $Root
    $path = Join-Path $Root ('attempt-' + $Attempt)
    if (Test-Path -LiteralPath $path) { throw 'mg_evidence_already_exists' }
    $null = [IO.Directory]::CreateDirectory($path)
    Require-MgDirectory $path
    # Claim only a fresh generated directory. This marker contains no identity.
    $marker = New-Object IO.FileStream((Join-Path $path 'owner'), [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    $marker.Dispose()
    return $path
}
function Save-MgSummary([string]$Directory, [string]$Text) {
    Require-MgDirectory $Directory
    if ($Text.Length -gt 8192) { throw 'mg_evidence_limit' }
    $stream = New-Object IO.FileStream((Join-Path $Directory 'summary.json'), [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try { $bytes = (New-Object Text.UTF8Encoding($false)).GetBytes($Text); $stream.Write($bytes,0,$bytes.Length) } finally { $stream.Dispose() }
}
function Convert-MgEvent([string]$Line) {
    if (-not $Line -or $Line.Length -gt 4096 -or $Line -match '[\x00-\x08\x0b\x0c\x0e-\x1f]') { throw 'mg_event_invalid' }
    # The pinned CLI emits literal keys. Reject escaped and duplicate keys before
    # ConvertFrom-Json can collapse them (all allowed keys are unique globally).
    $names = New-Object 'Collections.Generic.HashSet[string]' ([StringComparer]::Ordinal)
    foreach ($match in [regex]::Matches($Line, '"([^"\\]*)"\s*:')) {
        if (-not $names.Add($match.Groups[1].Value)) { throw 'mg_event_invalid' }
    }
    if ($Line -match '"(?:[^"\\]|\\.)*\\(?:[^"\\]|\\.)*"\s*:') { throw 'mg_event_invalid' }
    try { $value = $Line | ConvertFrom-Json } catch { throw 'mg_event_invalid' }
    if ($value -isnot [pscustomobject] -or $value.PSObject.Properties.Name -cnotcontains 'event' -or $value.event -isnot [string]) { throw 'mg_event_invalid' }
    return $value
}
function Require-MgKeys($Value, [string]$Keys) {
    if ((@($Value.PSObject.Properties.Name | Sort-Object) -join '|') -cne $Keys) { throw 'mg_event_invalid' }
}
function Require-MgIdentity([string]$Line, [string]$ExpectedHost) {
    $value = Convert-MgEvent $Line
    Require-MgKeys $value 'event|next|peer'
    if ($value.event -cne 'guest_identity' -or $value.next -cne 'authorize this public peer at host, then enter ready' -or $value.peer -isnot [string]) { throw 'mg_identity_invalid' }
    Require-MgPeer $value.peer
    if ($value.peer -ceq $ExpectedHost) { throw 'mg_identity_invalid' }
    return $value
}
function Require-MgReady([string]$Line) {
    $value = Convert-MgEvent $Line
    Require-MgKeys $value 'event|experimental|mode|next|tcp'
    if ($value.event -cne 'guest_ready' -or $value.mode -cne 'tcp' -or $value.tcp -cne '127.0.0.1:42010' -or $value.next -cne 'enter stop to close' -or $value.experimental -isnot [bool] -or -not $value.experimental) { throw 'mg_ready_invalid' }
    return $value
}
function Require-MgInteger($Value, [long]$Maximum) {
    if (($Value -isnot [int] -and $Value -isnot [long]) -or $Value -lt 0 -or $Value -gt $Maximum) { throw 'mg_status_invalid' }
}
function Require-MgStatus([string]$Line) {
    $value = Convert-MgEvent $Line
    Require-MgKeys $value 'event|evidence|runtimeHomologated|summary|voice|worldJoined'
    if ($value.event -cne 'minecraft_status_received' -or $value.evidence -cne 'status_and_matching_pong_only' -or $value.voice -cne 'not_tested' -or $value.runtimeHomologated -isnot [bool] -or $value.runtimeHomologated -or $value.worldJoined -isnot [bool] -or $value.worldJoined) { throw 'mg_status_invalid' }
    $summary = $value.summary
    Require-MgKeys $summary 'connectMicros|pingMicros|protocolMatchesRequested|reportedPlayersMax|reportedPlayersOnline|reportedProtocol|requestedProtocol|statusMicros|totalMicros'
    foreach ($key in @('requestedProtocol','reportedProtocol')) {
        Require-MgInteger $summary.$key 2147483647
        if ($summary.$key -ne 767) { throw 'mg_status_protocol_mismatch' }
    }
    if ($summary.protocolMatchesRequested -isnot [bool] -or -not $summary.protocolMatchesRequested) { throw 'mg_status_protocol_mismatch' }
    foreach ($key in @('connectMicros','statusMicros','pingMicros','totalMicros')) { Require-MgInteger $summary.$key 10000000 }
    foreach ($key in @('reportedPlayersOnline','reportedPlayersMax')) {
        if ($null -ne $summary.$key) { Require-MgInteger $summary.$key 4294967295 }
    }
    return $value
}
function Get-MgStatusDue([long]$ElapsedMs, [int]$Count, [long]$LastStartedMs) {
    # Initial request counts toward the same fixed budget; no retries/catch-up.
    return ($ElapsedMs -ge 0 -and $ElapsedMs -lt 900000 -and $Count -ge 0 -and $Count -lt 45 -and ($Count -eq 0 -or $ElapsedMs - $LastStartedMs -ge 20000))
}
function Require-MgDeadline([long]$NowMs, [long]$UntilMs) {
    if ($NowMs -lt 0 -or $UntilMs -le 0 -or $UntilMs -gt 1500000 -or $NowMs -ge $UntilMs) { throw 'mg_deadline' }
}
function Get-MgCommand([string]$Phase, [string]$Line) {
    if ($Line -ceq 'stop') { return 'stop' }
    if ($Phase -ceq 'authorization' -and $Line -ceq 'ready') { return 'ready' }
    return 'invalid'
}
function Get-MgNativeCode([string]$Code) {
    # Exact Error::code / MinecraftStatusError strings in the pinned kit38.
    $allowed = @('lab_operation_failed','invalid_configuration','transport_failed','timed_out','unauthorized','expired_or_revoked','service_not_authorized','capability_unavailable','stale_service_revision','invalid_protocol','replayed_request','resource_limit','destination_unavailable','datagram_too_large','closed','local_io_failed','status_connection_failed','status_io_failed','status_timed_out','invalid_status_request','invalid_status_varint','status_frame_too_large','truncated_status_response','invalid_status_packet','invalid_status_utf8','status_string_too_long','invalid_status_json','invalid_status_schema','status_pong_mismatch')
    if ($Code -cin $allowed) { return $Code }
    return $null
}
function Update-MgInput($State) {
    while ([Console]::KeyAvailable) {
        $key = [Console]::ReadKey($true)
        if ($key.Key -eq [ConsoleKey]::C -and ($key.Modifiers -band [ConsoleModifiers]::Control)) { throw 'mg_cancelled' }
        if ($key.Key -eq [ConsoleKey]::Enter) {
            [Console]::WriteLine()
            $State.line = $State.buffer.ToString()
            $null = $State.buffer.Clear()
            return
        }
        if ($key.Key -eq [ConsoleKey]::Backspace) {
            if ($State.buffer.Length -gt 0) { $State.buffer.Length--; [Console]::Write("`b `b") }
        } elseif ([int]$key.KeyChar -ge 32 -and [int]$key.KeyChar -le 126) {
            if ($State.buffer.Length -ge 80) { throw 'mg_input_limit' }
            $null = $State.buffer.Append($key.KeyChar)
            [Console]::Write($key.KeyChar)
        }
    }
}
function Read-MgInput($State, [long]$UntilMs, $Guest = $null) {
    while ($State.clock.ElapsedMilliseconds -lt $UntilMs) {
        if ($null -ne $Guest -and ($Guest.process.HasExited -or $Guest.pending.IsCompleted)) { throw 'mg_guest_closed_before_authorization' }
        Update-MgInput $State
        if ($null -ne $State.line) { Require-MgDeadline $State.clock.ElapsedMilliseconds $UntilMs; $line = $State.line; $State.line = $null; return $line }
        Start-Sleep -Milliseconds 50
    }
    throw 'mg_preparation_deadline'
}
function Start-MgChild($State, [string]$Role) {
    $until = $(if ($Role -ceq 'status') { $State.sessionEnd } else { 600000 })
    Require-MgDeadline $State.clock.ElapsedMilliseconds $until
    $State.binary = Require-MgBinary $State.binary
    Require-MgDeadline $State.clock.ElapsedMilliseconds $until
    if ($Role -ceq 'guest') {
        if ($State.guestStarted) { throw 'mg_guest_already_started' }
        $args = @('guest-tcp', $State.hostPeer, ('[' + $State.hostIpv6 + ']:42000'), $State.hostPeer, '127.0.0.1:42010', '900')
    } elseif ($Role -ceq 'status') {
        $args = @('minecraft-status', '127.0.0.1:42010', 'localhost', '43001', '767')
    } else { throw 'mg_child_role_invalid' }
    $info = New-Object Diagnostics.ProcessStartInfo
    $info.FileName = $State.binary; $info.Arguments = $args -join ' '
    $info.WorkingDirectory = [IO.Path]::GetDirectoryName($State.binary)
    $info.UseShellExecute = $false; $info.CreateNoWindow = $true
    $info.RedirectStandardInput = $true; $info.RedirectStandardOutput = $true; $info.RedirectStandardError = $true
    foreach ($name in @('DEBUG','RUST_LOG','SSLKEYLOGFILE')) { $info.EnvironmentVariables.Remove($name) }
    if ($info.PSObject.Properties.Name -contains 'StandardInputEncoding') { $info.StandardInputEncoding = New-Object Text.UTF8Encoding($false) }
    $child = New-Object Diagnostics.Process
    $child.StartInfo = $info
    try { if (-not $child.Start()) { throw 'mg_child_start_failed' } } catch { $child.Dispose(); throw 'mg_child_start_failed' }
    $owned = [pscustomobject]@{process=$child;role=$Role;pending=[AuroraMinecraftGuestIo]::LineAsync($child.StandardOutput,4096);stderr=[AuroraMinecraftGuestIo]::CountAsync($child.StandardError);forced=$false;reaped=$false;stopped=$false;exitCode=$null}
    $State.owned.Add($owned)
    if ($Role -ceq 'guest') { $State.guestStarted = $true }
    Require-MgDeadline $State.clock.ElapsedMilliseconds $until
    return $owned
}
function Read-MgEvent($State, $Owned, [long]$UntilMs) {
    while (-not $Owned.pending.IsCompleted) {
        if ($State.clock.ElapsedMilliseconds -ge $UntilMs) { throw 'mg_event_deadline' }
        Update-MgInput $State
        if ($null -ne $State.line) {
            $command = Get-MgCommand $State.phase $State.line; $State.line = $null
            if ($command -ceq 'stop') { $State.stopRequested = $true; throw 'mg_user_stop' }
            Write-Host 'Aguarde a resposta; para encerrar, digite stop e Enter.'
        }
        Start-Sleep -Milliseconds 50
    }
    if ($State.clock.ElapsedMilliseconds -ge $UntilMs) { throw 'mg_event_deadline' }
    if ($Owned.pending.IsFaulted -or $Owned.pending.IsCanceled -or -not $Owned.pending.Result) { throw 'mg_child_event_failed' }
    $line = $Owned.pending.Result
    $Owned.pending = [AuroraMinecraftGuestIo]::LineAsync($Owned.process.StandardOutput,4096)
    $event = Convert-MgEvent $line
    if ($event.event -cin @('guest_identity','guest_ready','minecraft_status_received','guest_stopped','failed')) { $State.lastEvent = $event.event }
    if ($event.event -ceq 'failed') {
        Require-MgKeys $event 'code|event|hint'
        if ($event.code -is [string]) { $State.nativeCode = Get-MgNativeCode $event.code }
        throw 'mg_native_failed'
    }
    if ($State.clock.ElapsedMilliseconds -ge $UntilMs) { throw 'mg_event_deadline' }
    return $line
}
function Complete-MgStatus($State, $Owned) {
    if (-not $Owned.process.WaitForExit(3000) -or $Owned.process.ExitCode -ne 0 -or -not $Owned.stderr.Wait(3000) -or $Owned.stderr.IsFaulted -or $Owned.stderr.Result -ne 0 -or -not $Owned.pending.Wait(3000) -or $Owned.pending.IsFaulted -or $null -ne $Owned.pending.Result) { throw 'mg_status_child_failed' }
    $Owned.reaped = $true
    $Owned.process.Dispose()
    $null = $State.owned.Remove($Owned)
}
function Stop-MgOwned($Owned, [bool]$Ready, $State = $null) {
    try {
        if (-not $Owned.process.HasExited) {
            # Broken stdin must never skip reap/kill of this same owned child.
            try {
                if ($Owned.role -ceq 'guest' -and $Ready) { $Owned.process.StandardInput.WriteLine('stop'); $Owned.process.StandardInput.Flush() }
                else { $Owned.process.StandardInput.Close() }
            } catch { }
            if (-not $Owned.process.WaitForExit(5000)) { $Owned.forced = $true; $Owned.process.Kill(); $null = $Owned.process.WaitForExit(5000) }
        }
        $Owned.reaped = $Owned.process.HasExited
        if ($Owned.reaped) { $Owned.exitCode = $Owned.process.ExitCode }
        if ($Owned.reaped) {
            # A transport closure can emit guest_stopped then failed. Drain only
            # three bounded events after exit; preserve the static code as well.
            $eofObserved = $false
            for ($index = 0; $index -lt 3; $index++) {
                if (-not $Owned.pending.Wait(1000) -or $Owned.pending.IsFaulted) { return $false }
                if ($null -eq $Owned.pending.Result) { $eofObserved = $true; break }
                $event = Convert-MgEvent $Owned.pending.Result
                if ($Owned.role -ceq 'guest' -and $event.event -ceq 'guest_stopped') {
                    Require-MgKeys $event 'event'
                    if ($Owned.stopped) { return $false }
                    $Owned.stopped = $Ready -and $Owned.role -ceq 'guest' -and $Owned.exitCode -eq 0
                    if ($State) { $State.lastEvent = 'guest_stopped' }
                } elseif ($Owned.role -ceq 'status' -and $event.event -ceq 'minecraft_status_received') {
                    # Stop may arrive while a STATUS result is finishing. Validate
                    # and discard this result; cancellation is no GUI/world proof.
                    $null = Require-MgStatus $Owned.pending.Result
                    if ($State) { $State.lastEvent = 'minecraft_status_received' }
                } elseif ($event.event -ceq 'failed') {
                    Require-MgKeys $event 'code|event|hint'
                    if ($State -and $event.code -is [string]) { $State.nativeCode = Get-MgNativeCode $event.code; $State.lastEvent = 'failed' }
                } else { return $false }
                $Owned.pending = [AuroraMinecraftGuestIo]::LineAsync($Owned.process.StandardOutput,4096)
            }
            if (-not $eofObserved) { return $false }
        }
        if (-not $Owned.stderr.Wait(3000) -or $Owned.stderr.IsFaulted -or $Owned.stderr.Result -ne 0) { return $false }
        return ($Owned.reaped -and -not $Owned.forced -and ($Owned.role -cne 'guest' -or -not $Ready -or $Owned.stopped))
    } catch { return $false } finally { if ($Owned.reaped) { $Owned.process.Dispose() } }
}
function Invoke-MgSupervisor([string]$BinaryPath, [string]$Ipv6, [string]$Peer) {
    $ErrorActionPreference = 'Stop'
    Set-StrictMode -Version 2
    $state = [pscustomobject]@{binary=$BinaryPath;hostIpv6=$Ipv6;hostPeer=$Peer;clock=[Diagnostics.Stopwatch]::StartNew();phase='preflight';buffer=(New-Object Text.StringBuilder);line=$null;owned=(New-Object 'Collections.Generic.List[object]');guestStarted=$false;stopRequested=$false;attempt=[Guid]::NewGuid().ToString('N');lastEvent='none';nativeCode=$null;sessionEnd=$null}
    $originalCtrlC = $false; $consoleChanged = $false; $guest = $null; $ready = $false; $failed = $false; $cleanup = $true; $statuses = 0; $code = 'mg_not_started'; $completedNormally = $false
    $evidence = $null; $lastSummary = $null; $sessionStart = $null; $lastStatusExit = $null
    try {
        if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT -or -not [Environment]::Is64BitProcess -or [Console]::IsInputRedirected) { throw 'mg_console_required' }
        $originalCtrlC = [Console]::TreatControlCAsInput
        [Console]::TreatControlCAsInput = $true; $consoleChanged = $true
        $null = [Console]::KeyAvailable
        $state.binary = Require-MgBinary $state.binary
        $evidence = New-MgEvidence ([IO.Path]::GetDirectoryName($state.binary)) $state.attempt
        if (-not $state.hostIpv6) { Write-Host 'Cole o IPv6 atual fornecido pelo agente e Enter:'; $state.hostIpv6 = Read-MgInput $state 600000 }
        $state.hostIpv6 = Get-MgIpv6 $state.hostIpv6
        if (-not $state.hostPeer) { Write-Host 'Cole o peer publico atual fornecido pelo agente e Enter:'; $state.hostPeer = Read-MgInput $state 600000 }
        Require-MgPeer $state.hostPeer
        Initialize-MgIo
        $state.phase = 'identity'
        $guest = Start-MgChild $state 'guest'
        $line = Read-MgEvent $state $guest ([Math]::Min(600000, $state.clock.ElapsedMilliseconds + 10000))
        $null = Require-MgIdentity $line $state.hostPeer
        Write-Output $line
        Write-Host 'Envie SOMENTE guest_identity ao agente. Mantenha este terminal aberto.'
        Write-Host 'Quando o agente confirmar host_ready/autorizacao, digite ready e Enter aqui.'
        Write-Host 'Nao execute novamente. Para cancelar, digite stop e Enter ou Ctrl+C.'
        $state.phase = 'authorization'
        while ($true) {
            if ($guest.process.HasExited -or $guest.pending.IsCompleted) { throw 'mg_guest_closed_before_authorization' }
            $command = Get-MgCommand $state.phase (Read-MgInput $state 600000 $guest)
            if ($command -ceq 'stop') { $state.stopRequested = $true; throw 'mg_cancelled' }
            if ($command -ceq 'ready') { break }
            Write-Host 'Digite apenas ready apos a autorizacao, ou stop para cancelar.'
        }
        $sessionStart = $state.clock.ElapsedMilliseconds
        $sessionEnd = [Math]::Min(1500000, $sessionStart + 900000)
        $state.sessionEnd = $sessionEnd
        $state.phase = 'guest_connect_and_authorize'
        Require-MgDeadline $state.clock.ElapsedMilliseconds 600000
        $guest.process.StandardInput.WriteLine('ready'); $guest.process.StandardInput.Flush()
        # The native handshake timeout remains 10s. Endpoint bind and awaited
        # endpoint close occur outside it; allow a bounded 5s reporting margin
        # for the static native error, without retry or changing session limits.
        $line = Read-MgEvent $state $guest ([Math]::Min($sessionEnd, $state.clock.ElapsedMilliseconds + 15000))
        $null = Require-MgReady $line
        $ready = $true
        Write-Output $line
        $lastStatus = 0
        while ($true) {
            $now = $state.clock.ElapsedMilliseconds
            if ($now -ge $sessionEnd) { throw 'mg_session_deadline' }
            if ($guest.process.HasExited -or $guest.pending.IsCompleted) { throw 'mg_guest_closed' }
            Update-MgInput $state
            if ($null -ne $state.line) {
                $command = Get-MgCommand 'game' $state.line; $state.line = $null
                if ($command -ceq 'stop') { $state.stopRequested = $true; $code = 'mg_user_stopped'; $completedNormally = $true; break }
                Write-Host 'Para encerrar: saia do mundo no Minecraft, depois digite stop e Enter.'
            }
            if (Get-MgStatusDue ($now - $sessionStart) $statuses $lastStatus) {
                $state.phase = 'game_status'; $lastStatus = $now - $sessionStart; $statuses++
                $statusChild = Start-MgChild $state 'status'
                $value = Require-MgStatus (Read-MgEvent $state $statusChild ([Math]::Min($sessionEnd, $state.clock.ElapsedMilliseconds + 7000)))
                Complete-MgStatus $state $statusChild
                $lastSummary = $value.summary; $lastStatusExit = 0
                [ordered]@{event='minecraft_guest_status';sequence=$statuses;summary=$value.summary;worldJoined=$false;runtimeHomologated=$false;voice='not_tested'} | ConvertTo-Json -Compress -Depth 4 | Write-Output
                if ($statuses -eq 1) {
                    Write-Host 'STATUS/pong767 passou. No Minecraft: Multijogador > Conexao direta > 127.0.0.1:42010.'
                    Write-Host 'Avise quando entrar e caminhar/ver terreno. STATUS nao comprova mundo.'
                    Write-Host 'Mantenha este terminal aberto. Para encerrar, saia do mundo e digite stop e Enter.'
                }
                $state.phase = 'game'
            }
            Start-Sleep -Milliseconds 50
        }
    } catch {
        $failed = $true; $code = 'mg_supervisor_failed'
        if ($_.Exception.Message -cmatch '^mg_[a-z_]+$') { $code = $_.Exception.Message }
        if ($code -ceq 'mg_cancelled') { $state.stopRequested = $true }
        # Literal stop during a probe has the same outcome as stop between them.
        # Ctrl+C, pre-ready stop and cancellation remain distinct failures.
        if ($code -ceq 'mg_user_stop' -and $ready -and $state.stopRequested) { $completedNormally = $true; $failed = $false; $code = 'mg_user_stopped' }
        else { [ordered]@{event='minecraft_guest_failed';attempt=$state.attempt;code=$code;nativeCode=$state.nativeCode;lastEvent=$state.lastEvent;phase=$state.phase;natDiagnosis='not_inferred';worldJoined=$false;voice='not_tested'} | ConvertTo-Json -Compress | Write-Output }
    } finally {
        if (-not $completedNormally -and -not $failed) { $failed = $true; $code = 'mg_interrupted' }
        foreach ($owned in @($state.owned.ToArray())) { if (-not (Stop-MgOwned $owned $ready $state)) { $cleanup = $false } }
        if ($consoleChanged) { try { [Console]::TreatControlCAsInput = $originalCtrlC } catch { $cleanup = $false } }
        $children = @($state.owned.ToArray() | ForEach-Object { [ordered]@{role=$_.role;exitCode=$_.exitCode;reaped=$_.reaped;forced=$_.forced} })
        $report = [ordered]@{event='minecraft_guest_closed';attempt=$state.attempt;code=$code;nativeCode=$state.nativeCode;lastEvent=$state.lastEvent;phase=$state.phase;failed=($failed -or -not $cleanup);cleanupPassed=$cleanup;children=$children;elapsedMs=$state.clock.ElapsedMilliseconds;sessionElapsedMs=$(if ($null -ne $sessionStart) { $state.clock.ElapsedMilliseconds - $sessionStart } else { $null });guestStarted=$state.guestStarted;guestStopped=($null -ne $guest -and $guest.stopped);statusRequests=$statuses;lastStatusSummary=$lastSummary;lastStatusExitCode=$lastStatusExit;maximumStatusRequests=45;maximumSessionSeconds=900;maximumPreparationSeconds=600;maximumTotalSeconds=1500;worldJoined=$false;guiObserved=$false;personalConfigurationChanged=$false;relay=$false;evidenceSaved=$false}
        if ($evidence) {
            try { $report.evidenceSaved = $true; Save-MgSummary $evidence ($report | ConvertTo-Json -Compress -Depth 5) }
            catch { $report.evidenceSaved = $false; $report.failed = $true; $failed = $true }
        }
        $report | ConvertTo-Json -Compress -Depth 5 | Write-Output
        if ($report.evidenceSaved) { Write-Host ('Resumo redigido salvo em attempt-' + $state.attempt + '/summary.json dentro do kit.') }
    }
    return (-not $failed -and $cleanup)
}

if ($MyInvocation.InvocationName -ne '.') {
    $mgSuccess = $false
    Invoke-MgSupervisor $Binary $HostIpv6 $HostPeer | ForEach-Object {
        if ($_ -is [bool]) { $mgSuccess = $_ } else { Write-Output $_ }
    }
    if (-not $mgSuccess) { exit 1 }
}

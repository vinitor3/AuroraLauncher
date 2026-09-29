param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$manifest = Join-Path $repo 'apps/connect-lab/Cargo.toml'
if (-not $SkipBuild) {
    & cargo build --locked --manifest-path $manifest
    if ($LASTEXITCODE -ne 0) { throw 'connect_lab_build_failed' }
}
$exe = Join-Path $repo 'apps/connect-lab/target/debug/aurora-connect-lab.exe'
if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw 'connect_lab_binary_missing' }
$processes = New-Object 'System.Collections.Generic.List[System.Diagnostics.Process]'

function Start-Lab([string[]]$LabArguments) {
    # Arguments are only the fixed subcommands, socket literals, public IDs and seconds.
    foreach ($arg in $LabArguments) { if ($arg -notmatch '^[a-zA-Z0-9:._\[\]-]+$') { throw 'unsafe_lab_argument' } }
    $info = New-Object System.Diagnostics.ProcessStartInfo
    $info.FileName = $exe
    $info.Arguments = $LabArguments -join ' '
    $info.WorkingDirectory = $repo
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardInput = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    if ($info.PSObject.Properties.Name -contains 'StandardInputEncoding') {
        $info.StandardInputEncoding = New-Object Text.UTF8Encoding($false)
    }
    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $info
    if (-not $process.Start()) { throw 'lab_process_start_failed' }
    $processes.Add($process)
    return $process
}

function Read-Event($Process, [string]$Expected) {
    $line = $Process.StandardOutput.ReadLineAsync()
    if (-not $line.Wait(30000)) { throw "timeout_waiting_$Expected" }
    if (-not $line.Result) { throw "process_ended_before_$Expected" }
    $value = $line.Result | ConvertFrom-Json
    if ($value.event -ne $Expected) {
        $code = 'unknown'
        if ($value.event -eq 'failed' -and $value.code -match '^[a-z_]+$') { $code = $value.code }
        throw "unexpected_event_waiting_${Expected}_${code}"
    }
    return $value
}

try {
    $echo = Start-Lab @('echo', '127.0.0.1:0', '127.0.0.1:0', '120')
    $targets = Read-Event $echo 'echo_ready'
    # Reserve an ephemeral UDP port only long enough to choose a literal transport
    # bind. A race is handled by host bind failure, never by killing a port owner.
    $reservation = New-Object Net.Sockets.UdpClient
    $reservation.Client.Bind((New-Object Net.IPEndPoint([Net.IPAddress]::Loopback,0)))
    $hostAddress = $reservation.Client.LocalEndPoint.ToString()
    $reservation.Dispose()
    $hostAgent = Start-Lab @('host', $hostAddress, $targets.tcp, $targets.udp, '120')
    $hostIdentity = Read-Event $hostAgent 'host_identity'
    $guestAgent = Start-Lab @('guest', $hostIdentity.peer, $hostAddress, $hostIdentity.session, '127.0.0.1:0', '127.0.0.1:0', '120')
    $guestIdentity = Read-Event $guestAgent 'guest_identity'
    $hostAgent.StandardInput.WriteLine($guestIdentity.peer)
    $hostAgent.StandardInput.Flush()
    $null = Read-Event $hostAgent 'host_ready'
    $guestAgent.StandardInput.WriteLine('ready')
    $guestAgent.StandardInput.Flush()
    $bridges = Read-Event $guestAgent 'guest_ready'
    $probe = Start-Lab @('probe', $bridges.tcp, $bridges.udp)
    $result = Read-Event $probe 'probe_passed'
    if (-not $probe.WaitForExit(5000) -or $probe.ExitCode -ne 0) { throw 'probe_failed' }
    # Only aggregate non-secret evidence leaves the orchestration script. Pins,
    # ephemeral connectivity metadata and opaque application payloads are omitted.
    $hostAgent.Refresh()
    $guestAgent.Refresh()
    [ordered]@{ result='PASS'; agents=2; controlledEndpointProcess=1; tcpBytesEachDirection=$result.tcpBytesEachDirection; udpPackets=$result.udpPackets; udpLocalSources=$result.localSources; elapsedMs=$result.elapsedMs; udpApplicationRoundtripMicrosP50=$result.udpApplicationRoundtripMicrosP50; udpApplicationRoundtripMicrosMax=$result.udpApplicationRoundtripMicrosMax; udpAdjacentRoundtripVariationMicrosMax=$result.udpAdjacentRoundtripVariationMicrosMax; hostPeakWorkingSetBytes=$hostAgent.PeakWorkingSet64; guestPeakWorkingSetBytes=$guestAgent.PeakWorkingSet64; hostCpuMs=$hostAgent.TotalProcessorTime.TotalMilliseconds; guestCpuMs=$guestAgent.TotalProcessorTime.TotalMilliseconds; maxDatagramPayload=$bridges.maxDatagramPayload; topology='same_machine_loopback'; relay='disabled'; minecraft='not_tested'; simpleVoiceChat='not_tested'; plasmoVoice='not_tested' } | ConvertTo-Json
    foreach ($pair in @(@($guestAgent,'guest_stopped'), @($hostAgent,'host_stopped'), @($echo,'echo_stopped'))) {
        $pair[0].StandardInput.WriteLine('stop')
        $pair[0].StandardInput.Flush()
        $null = Read-Event $pair[0] $pair[1]
        if (-not $pair[0].WaitForExit(5000) -or $pair[0].ExitCode -ne 0) { throw 'lab_shutdown_failed' }
    }
    Write-Output 'CLEANUP_PASS: owned agents and endpoint process exited; no keys or configuration persisted.'
} finally {
    foreach ($process in $processes) {
        if (-not $process.HasExited) {
            try { $process.StandardInput.WriteLine('stop'); $process.StandardInput.Flush() } catch { }
            if (-not $process.WaitForExit(5000)) { $process.Kill(); $process.WaitForExit() }
        }
        $process.Dispose()
    }
}

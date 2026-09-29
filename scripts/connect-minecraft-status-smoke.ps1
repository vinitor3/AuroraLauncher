param([ValidateRange(1,65535)][int]$GamePort = 43001, [switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not $SkipBuild) {
    & cargo build --locked --manifest-path (Join-Path $repo 'apps/connect-lab/Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw 'connect_lab_build_failed' }
}
$exe = Join-Path $repo 'apps/connect-lab/target/debug/aurora-connect-lab.exe'
if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw 'connect_lab_binary_missing' }
$processes = New-Object 'System.Collections.Generic.List[System.Diagnostics.Process]'
function Get-PlanHash([string]$Path) {
    # Use the runtime API directly; inherited PSModulePath can hide Get-FileHash
    # when Windows PowerShell is launched by Node from PowerShell 7.
    $stream = [IO.File]::OpenRead($Path)
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        if ($stream.Length -gt 16384) { throw 'lab_plan_too_large' }
        return ([BitConverter]::ToString($sha.ComputeHash($stream))).Replace('-','').ToLowerInvariant()
    } finally { $sha.Dispose(); $stream.Dispose() }
}
function Start-Agent([string[]]$AgentArguments) {
    foreach ($arg in $AgentArguments) { if ($arg -notmatch '^[a-zA-Z0-9:._/\\\[\]-]+$') { throw 'unsafe_lab_argument' } }
    $info = New-Object System.Diagnostics.ProcessStartInfo
    $info.FileName = $exe
    $info.Arguments = $AgentArguments -join ' '
    $info.WorkingDirectory = $repo
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardInput = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    if ($info.PSObject.Properties.Name -contains 'StandardInputEncoding') { $info.StandardInputEncoding = New-Object Text.UTF8Encoding($false) }
    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $info
    if (-not $process.Start()) { throw 'lab_process_start_failed' }
    $processes.Add($process)
    return $process
}
function Read-Event($Process, [string]$Expected) {
    $deadline = [Diagnostics.Stopwatch]::StartNew()
    $text = New-Object Text.StringBuilder
    $buffer = New-Object char[] 1
    do {
        $remaining = 30000 - [int]$deadline.ElapsedMilliseconds
        if ($remaining -le 0) { throw 'lab_event_deadline_or_exit' }
        $read = $Process.StandardOutput.ReadAsync($buffer,0,1)
        if (-not $read.Wait($remaining) -or $read.Result -ne 1) { throw 'lab_event_deadline_or_exit' }
        if ($buffer[0] -eq "`n") { break }
        if ($text.Length -ge 16384) { throw 'lab_event_too_large' }
        $null = $text.Append($buffer[0])
    } while ($true)
    $value = $text.ToString() | ConvertFrom-Json
    if ($value.event -ne $Expected) {
        $code = 'unknown'
        if ($value.event -eq 'failed' -and $value.code -match '^[a-z_]+$') { $code = $value.code }
        throw "unexpected_lab_event_${Expected}_$code"
    }
    return $value
}
try {
    # Only the QUIC transport needs a UDP port; there is no UDP application service.
    $reservation = New-Object Net.Sockets.UdpClient
    try {
        $reservation.Client.Bind((New-Object Net.IPEndPoint([Net.IPAddress]::Loopback,0)))
        $hostAddress = $reservation.Client.LocalEndPoint.ToString()
    } finally { $reservation.Dispose() }
    $planRelative = '.aurora/connect-validation/minecraft-status-plan-' + [Guid]::NewGuid().ToString('N') + '.json'
    $planPath = Join-Path $repo $planRelative
    # Inspect existing ancestors top-down before any mkdir/write; static junctions
    # are rejected. Concurrent same-user parent replacement is outside this lab.
    $directories = New-Object 'System.Collections.Generic.List[string]'
    $directory = Split-Path $planPath
    while ($directory) {
        $directories.Add($directory)
        $next = Split-Path $directory
        if ($next -eq $directory) { break }
        $directory = $next
    }
    for ($i=$directories.Count-1; $i -ge 0; $i--) {
        $directory = $directories[$i]
        if (Test-Path -LiteralPath $directory) {
            $entry = Get-Item -LiteralPath $directory -Force
            if (-not $entry.PSIsContainer -or ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'unsafe_lab_directory' }
        } else { $null = [IO.Directory]::CreateDirectory($directory) }
    }
    $plan = [ordered]@{
        schemaVersion=1; bind=$hostAddress; durationSeconds=60
        services=@([ordered]@{serviceId='game';protocol='tcp';endpoint="127.0.0.1:$GamePort";revision=1;requiredCapability='reliableStream';maxDatagramBytes=1400;udpReturnPolicy=@{type='replyBudget'}})
    }
    # New generated loopback-only lab plan; no personal configuration is modified.
    $bytes = (New-Object Text.UTF8Encoding($false)).GetBytes(($plan | ConvertTo-Json -Depth 6))
    $file = [IO.File]::Open($planPath,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
    try { $file.Write($bytes,0,$bytes.Length) } finally { $file.Dispose() }
    $hash = Get-PlanHash $planPath
    $hostAgent = Start-Agent @('host-plan',$planRelative,$hash)
    $hostIdentity = Read-Event $hostAgent 'host_identity'
    if ($hostIdentity.planHash -ne $hash -or $hostIdentity.approvedServices.Count -ne 1 -or $hostIdentity.approvedServices[0].serviceId -ne 'game') { throw 'lab_service_plan_mismatch' }
    $guestAgent = Start-Agent @('guest-tcp',$hostIdentity.peer,$hostAddress,$hostIdentity.session,'127.0.0.1:0','60')
    $guestIdentity = Read-Event $guestAgent 'guest_identity'
    $hostAgent.StandardInput.WriteLine($guestIdentity.peer); $hostAgent.StandardInput.Flush()
    $null = Read-Event $hostAgent 'host_ready'
    $guestAgent.StandardInput.WriteLine('ready'); $guestAgent.StandardInput.Flush()
    $bridge = Read-Event $guestAgent 'guest_ready'
    if ($bridge.mode -ne 'tcp' -or $bridge.PSObject.Properties.Name -contains 'udp') { throw 'unexpected_udp_application_service' }
    $probe = Start-Agent @('minecraft-status',$bridge.tcp,'localhost',"$GamePort",'767')
    $status = Read-Event $probe 'minecraft_status_received'
    if (-not $probe.WaitForExit(5000) -or $probe.ExitCode -ne 0 -or -not $status.summary.protocolMatchesRequested) { throw 'minecraft_status_failed_or_protocol_mismatch' }
    foreach ($pair in @(@($guestAgent,'guest_stopped'),@($hostAgent,'host_stopped'))) {
        $pair[0].StandardInput.WriteLine('stop'); $pair[0].StandardInput.Flush()
        $null = Read-Event $pair[0] $pair[1]
        if (-not $pair[0].WaitForExit(5000) -or $pair[0].ExitCode -ne 0) { throw 'lab_shutdown_failed' }
    }
    if ($hash -ne (Get-PlanHash $planPath)) { throw 'lab_plan_changed' }
    # A status fixture can answer this too. Caller must attach independent runtime
    # provenance; this script never starts Java or promotes runtime homologation.
    [ordered]@{result='PASS';agents=2;applicationServices=@('game/tcp');relay='disabled';topology='same_machine_loopback';status=$status.summary;evidence='status_and_matching_pong_only';runtimeHomologated=$false;worldJoined=$false;simpleVoiceChat='not_tested';plasmoVoice='not_tested';cleanup='owned_agents_exited';endpointLifecycle='caller_owned'} | ConvertTo-Json -Depth 6
} finally {
    foreach ($process in $processes) {
        if (-not $process.HasExited) {
            try { $process.StandardInput.WriteLine('stop'); $process.StandardInput.Flush() } catch { }
            if (-not $process.WaitForExit(5000)) { $process.Kill(); $process.WaitForExit() }
        }
        $process.Dispose()
    }
}

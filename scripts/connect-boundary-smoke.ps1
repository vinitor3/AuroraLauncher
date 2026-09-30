param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$exe = Join-Path $repo 'apps/connect-lab/target/debug/aurora-connect-lab.exe'
$manifest = Join-Path $repo 'apps/connect-lab/Cargo.toml'
$owned = New-Object 'System.Collections.Generic.List[System.Diagnostics.Process]'
$stderrReads = @{}
$bridges = $null
$success = $false

function Start-Boundary([string[]]$LabArguments) {
    foreach ($argument in $LabArguments) {
        if ($argument -notmatch '^[a-zA-Z0-9:._/\\\[\]-]+$') { throw 'boundary_unsafe_argument' }
    }
    $info = New-Object Diagnostics.ProcessStartInfo
    $info.FileName = $exe
    $info.Arguments = $LabArguments -join ' '
    $info.WorkingDirectory = $repo
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardInput = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.EnvironmentVariables.Remove('DEBUG')
    $info.EnvironmentVariables.Remove('RUST_LOG')
    $info.EnvironmentVariables.Remove('SSLKEYLOGFILE')
    if ($info.PSObject.Properties.Name -contains 'StandardInputEncoding') { $info.StandardInputEncoding = New-Object Text.UTF8Encoding($false) }
    $child = New-Object Diagnostics.Process
    $child.StartInfo = $info
    if (-not $child.Start()) { throw 'boundary_start_failed' }
    $owned.Add($child)
    $stderrReads[$child.Id] = $child.StandardError.ReadToEndAsync()
    return $child
}

function Read-Boundary($Child, [string]$Expected) {
    $line = $Child.StandardOutput.ReadLineAsync()
    if (-not $line.Wait(20000) -or -not $line.Result -or $line.Result.Length -gt 16384) { throw 'boundary_event_timeout_or_limit' }
    try { $value = $line.Result | ConvertFrom-Json } catch { throw 'boundary_event_invalid' }
    if ($value.event -ne $Expected) { throw 'boundary_event_unexpected' }
    return $value
}

function Require-BoundaryLoopback([string]$Address) {
    if ($Address -cnotmatch '^127\.0\.0\.1:([1-9][0-9]{0,4})$' -or [int]$Matches[1] -gt 65535) { throw 'boundary_non_loopback' }
}

function Require-BoundaryServices($Actual, $Expected) {
    if (@($Actual).Count -ne 2) { throw 'boundary_services_invalid' }
    $keys = 'endpoint|maxDatagramBytes|protocol|requiredCapability|revision|serviceId|udpReturnPolicy'
    for ($index = 0; $index -lt 2; $index++) {
        $service = $Actual[$index]
        if ((@($service.PSObject.Properties.Name | Sort-Object) -join '|') -cne $keys) { throw 'boundary_services_invalid' }
        foreach ($key in @('endpoint','maxDatagramBytes','protocol','requiredCapability','revision','serviceId')) {
            if ($service.$key -cne $Expected[$index][$key]) { throw 'boundary_services_invalid' }
        }
        $policy = $service.udpReturnPolicy
        $wanted = $Expected[$index].udpReturnPolicy
        if ((@($policy.PSObject.Properties.Name | Sort-Object) -join '|') -cne (@($wanted.Keys | Sort-Object) -join '|')) { throw 'boundary_services_invalid' }
        foreach ($key in $wanted.Keys) { if ($policy.$key -cne $wanted[$key]) { throw 'boundary_services_invalid' } }
    }
}

function Stop-Boundary($Child, [string]$Event) {
    if ($Child.HasExited) { throw 'boundary_child_exited_before_stop' }
    $Child.StandardInput.WriteLine('stop')
    $Child.StandardInput.Flush()
    $null = Read-Boundary $Child $Event
    if (-not $Child.WaitForExit(5000) -or $Child.ExitCode -ne 0) { throw 'boundary_stop_failed' }
}

try {
    if (-not $SkipBuild) {
        & cargo build --offline --locked --manifest-path $manifest --bin aurora-connect-lab
        if ($LASTEXITCODE -ne 0) { throw 'boundary_build_failed' }
    }
    if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw 'boundary_binary_missing' }
    $echo = Start-Boundary @('echo-boundary', '127.0.0.1:0', '127.0.0.1:0', '120')
    $targets = Read-Boundary $echo 'echo_ready'
    Require-BoundaryLoopback $targets.tcp
    Require-BoundaryLoopback $targets.udp
    $reservation = New-Object Net.Sockets.UdpClient
    try {
        $reservation.Client.Bind((New-Object Net.IPEndPoint([Net.IPAddress]::Loopback, 0)))
        $hostAddress = $reservation.Client.LocalEndPoint.ToString()
    } finally { $reservation.Dispose() }
    $planParent = Join-Path $repo '.aurora/connect-validation'
    foreach ($folder in @((Join-Path $repo '.aurora'), $planParent)) {
        if (-not (Test-Path -LiteralPath $folder)) { $null = New-Item -ItemType Directory -Path $folder }
        $folderItem = Get-Item -LiteralPath $folder -Force
        if (-not $folderItem.PSIsContainer -or ($folderItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'boundary_linked_plan_directory' }
    }
    $planRelative = '.aurora/connect-validation/boundary-native-' + [Guid]::NewGuid().ToString('N') + '.json'
    $planPath = Join-Path $repo $planRelative
    $plan = [ordered]@{
        schemaVersion=1; bind=$hostAddress; durationSeconds=120
        services=@(
            [ordered]@{ serviceId='game'; protocol='tcp'; endpoint=$targets.tcp; revision=1; requiredCapability='reliableStream'; maxDatagramBytes=1400; udpReturnPolicy=@{ type='replyBudget' } },
            [ordered]@{ serviceId='aux'; protocol='udp'; endpoint=$targets.udp; revision=1; requiredCapability='datagram'; maxDatagramBytes=1264; udpReturnPolicy=@{ type='authorizedDownstream'; bytesPerSecond=100000; datagramsPerSecond=100 } }
        )
    }
    $planStream = New-Object IO.FileStream($planPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {
        $planBytes = (New-Object Text.UTF8Encoding($false)).GetBytes(($plan | ConvertTo-Json -Depth 6))
        $planStream.Write($planBytes, 0, $planBytes.Length)
    } finally { $planStream.Dispose() }
    $review = Start-Boundary @('plan', $planRelative)
    $preview = Read-Boundary $review 'plan_ready'
    if (-not $review.WaitForExit(5000) -or $review.ExitCode -ne 0 -or $preview.plan.hash -ne (Get-FileHash -LiteralPath $planPath -Algorithm SHA256).Hash.ToLowerInvariant()) { throw 'boundary_plan_review_failed' }
    $denied = Start-Boundary @('host-plan', $planRelative, ('0' * 64))
    $denial = Read-Boundary $denied 'failed'
    if ($denial.code -ne 'plan_changed_since_review' -or -not $denied.WaitForExit(5000) -or $denied.ExitCode -ne 1) { throw 'boundary_unapproved_plan_accepted' }
    $hostAgent = Start-Boundary @('host-plan', $planRelative, $preview.plan.hash)
    $identity = Read-Boundary $hostAgent 'host_identity'
    if ($identity.planHash -ne $preview.plan.hash -or $identity.peer -cnotmatch '^[a-f0-9]{64}$' -or $identity.session -ne $identity.peer -or $identity.bind -ne $hostAddress) { throw 'boundary_host_identity_invalid' }
    Require-BoundaryServices $identity.approvedServices $plan.services
    $guestAgent = Start-Boundary @('guest', $identity.peer, $hostAddress, $identity.session, '127.0.0.1:0', '127.0.0.1:0', '120')
    $guestIdentity = Read-Boundary $guestAgent 'guest_identity'
    if ($guestIdentity.peer -cnotmatch '^[a-f0-9]{64}$' -or $guestIdentity.peer -eq $identity.peer) { throw 'boundary_guest_identity_invalid' }
    $hostAgent.StandardInput.WriteLine($guestIdentity.peer)
    $hostAgent.StandardInput.Flush()
    $null = Read-Boundary $hostAgent 'host_ready'
    $guestAgent.StandardInput.WriteLine('ready')
    $guestAgent.StandardInput.Flush()
    $bridges = Read-Boundary $guestAgent 'guest_ready'
    Require-BoundaryLoopback $bridges.tcp
    Require-BoundaryLoopback $bridges.udp
    if ($bridges.maxDatagramPayload -ne 1264) { throw 'boundary_negotiated_payload_mismatch' }
    $probe = Start-Boundary @('probe-boundary', $bridges.tcp, $bridges.udp, '1264')
    $result = Read-Boundary $probe 'boundary_probe_passed'
    if (-not $probe.WaitForExit(5000) -or $probe.ExitCode -ne 0 -or $result.expectedPayload -ne 1264 -or $result.tcpBytesEachDirection -ne 1048576 -or -not $result.tcpConcurrentWithUdp -or $result.tcpBytesForwardedDuringUdp -le 0 -or $result.udpLocalSources -ne 2 -or $result.udpHostAssociationsObserved -ne 2 -or $result.udpExactEchoes -ne 4 -or $result.udpOversizeRequests -ne 2 -or $result.udpOversizeReachedEndpoint -ne 0 -or $result.udpOversizeReturnsEmitted -ne 4 -or $result.udpOversizeReturnsReachedGuest -ne 0 -or $result.udpValidAfterRejection -ne 2 -or $result.udpEndpointBytes -ne 5056) { throw 'boundary_probe_result_invalid' }
    Stop-Boundary $guestAgent 'guest_stopped'
    Stop-Boundary $hostAgent 'host_stopped'
    Stop-Boundary $echo 'echo_stopped'
    $tcpPort = [int]($bridges.tcp.Split(':')[-1])
    $udpPort = [int]($bridges.udp.Split(':')[-1])
    $tcp = New-Object Net.Sockets.TcpListener([Net.IPAddress]::Loopback, $tcpPort)
    $udp = New-Object Net.Sockets.UdpClient
    try {
        $tcp.Start()
        $udp.Client.Bind((New-Object Net.IPEndPoint([Net.IPAddress]::Loopback, $udpPort)))
    } finally { $tcp.Stop(); $udp.Dispose() }
    $echoTcp = New-Object Net.Sockets.TcpListener([Net.IPAddress]::Loopback, [int]($targets.tcp.Split(':')[-1]))
    $echoUdp = New-Object Net.Sockets.UdpClient
    $transportUdp = New-Object Net.Sockets.UdpClient
    try {
        $echoTcp.Start()
        $echoUdp.Client.Bind((New-Object Net.IPEndPoint([Net.IPAddress]::Loopback, [int]($targets.udp.Split(':')[-1]))))
        $transportUdp.Client.Bind((New-Object Net.IPEndPoint([Net.IPAddress]::Loopback, [int]($hostAddress.Split(':')[-1]))))
    } finally { $echoTcp.Stop(); $echoUdp.Dispose(); $transportUdp.Dispose() }
    foreach ($child in $owned) {
        $stderr = $stderrReads[$child.Id]
        if (-not $stderr.Wait(5000) -or $stderr.Result.Length -ne 0) { throw 'boundary_unexpected_stderr' }
    }
    if ($preview.plan.hash -ne (Get-FileHash -LiteralPath $planPath -Algorithm SHA256).Hash.ToLowerInvariant()) { throw 'boundary_plan_changed_after_run' }
    $success = $true
    $result | ConvertTo-Json -Compress -Depth 6
    [ordered]@{ event='boundary_cleanup_passed'; agents=2; guestPortsRebound=$true; hostAndEchoPortsRebound=$true; forced=$false; personalConfigurationChanged=$false; relay=$false } | ConvertTo-Json -Compress
} catch {
    $code = 'boundary_supervisor_failed'
    if ($_.Exception.Message -cmatch '^boundary_[a-z_]+$') { $code = $_.Exception.Message }
    [ordered]@{ event='boundary_failed'; code=$code; relay=$false } | ConvertTo-Json -Compress
} finally {
    foreach ($child in $owned) {
        if (-not $child.HasExited) {
            try { $child.StandardInput.WriteLine('stop'); $child.StandardInput.Flush() } catch { }
            if (-not $child.WaitForExit(5000)) { $child.Kill(); $child.WaitForExit() }
        }
        $child.Dispose()
    }
    $identity = $null
    $guestIdentity = $null
}
if (-not $success) { exit 1 }

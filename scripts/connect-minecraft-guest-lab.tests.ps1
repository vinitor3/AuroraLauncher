$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2
. (Join-Path $PSScriptRoot 'connect-minecraft-guest-lab.ps1')
$passed = 0
function Pass-Mg { $script:passed++ }
function Deny-Mg([scriptblock]$Action) {
    $denied = $false
    try { & $Action | Out-Null } catch { $denied = $true }
    if (-not $denied) { throw 'mg_fixture_expected_denial' }
    Pass-Mg
}
function Equal-Mg($Actual, $Expected) {
    if ($Actual -cne $Expected) { throw 'mg_fixture_mismatch' }
    Pass-Mg
}
$hostPin = 'a' * 64
$guestPin = 'b' * 64
Equal-Mg (Get-MgIpv6 '2001:4860:4860::8888') '2001:4860:4860::8888'
foreach ($text in @('','127.0.0.1','::1','::','fe80::1','fc00::1','ff02::1','2001:db8::1','3fff::1','3fff:f::1','2001:4860::1%2','[2001:4860::1]','2001:4860::1;whoami',('a' * 46))) { Deny-Mg { Get-MgIpv6 $text } }
Require-MgPeer $hostPin; Pass-Mg
foreach ($text in @('',('a' * 63),('a' * 65),('A' * 64),('g' * 64),($hostPin + ' '),($hostPin + ';'))) { Deny-Mg { Require-MgPeer $text } }
$identity = '{"event":"guest_identity","peer":"' + $guestPin + '","next":"authorize this public peer at host, then enter ready"}'
Equal-Mg (Require-MgIdentity $identity $hostPin).peer $guestPin
Deny-Mg { Require-MgIdentity $identity $guestPin }
Deny-Mg { Require-MgIdentity ($identity.Replace('guest_identity','guest_ready')) $hostPin }
Deny-Mg { Require-MgIdentity ($identity.Replace('then enter ready','then enter guest_ready')) $hostPin }
foreach ($text in @('[{}]','true','null','{}','{',('x' * 4097),'{"event":"x","event":"x"}','{"e\u0076ent":"x"}','{"event":"x","summary":{"event":"y"}}')) { Deny-Mg { Convert-MgEvent $text } }
$ready = '{"event":"guest_ready","experimental":true,"mode":"tcp","next":"enter stop to close","tcp":"127.0.0.1:42010"}'
Equal-Mg (Require-MgReady $ready).tcp '127.0.0.1:42010'
foreach ($text in @($ready.Replace('42010','42011'),$ready.Replace('tcp"','udp"'),$ready.Replace('true','"true"'),$ready.Replace('true','false'),$ready.Replace('127.0.0.1','0.0.0.0'),$ready.Replace('"experimental":true,',''))) { Deny-Mg { Require-MgReady $text } }
$status = '{"event":"minecraft_status_received","evidence":"status_and_matching_pong_only","runtimeHomologated":false,"worldJoined":false,"voice":"not_tested","summary":{"requestedProtocol":767,"reportedProtocol":767,"protocolMatchesRequested":true,"reportedPlayersOnline":0,"reportedPlayersMax":2,"connectMicros":10,"statusMicros":20,"pingMicros":30,"totalMicros":60}}'
Equal-Mg (Require-MgStatus $status).summary.reportedProtocol 767
foreach ($text in @($status.Replace('"reportedProtocol":767','"reportedProtocol":768'),$status.Replace('"requestedProtocol":767','"requestedProtocol":"767"'),$status.Replace('"protocolMatchesRequested":true','"protocolMatchesRequested":false'),$status.Replace('"worldJoined":false','"worldJoined":true'),$status.Replace('"runtimeHomologated":false','"runtimeHomologated":true'),$status.Replace('"totalMicros":60','"totalMicros":10000001'),$status.Replace('"pingMicros":30','"pingMicros":-1'),$status.Replace('"reportedPlayersOnline":0','"reportedPlayersOnline":4294967296'),$status.Replace('"reportedPlayersMax":2','"reportedPlayersMax":2.5'),$status.Replace('"reportedPlayersMax":2','"reportedPlayersMax":true'),$status.Replace('"summary":','"unexpected":'))) { Deny-Mg { Require-MgStatus $text } }
Equal-Mg (Require-MgStatus ($status.Replace('"reportedPlayersOnline":0','"reportedPlayersOnline":null'))).summary.reportedPlayersOnline $null
Equal-Mg (Get-MgCommand 'authorization' 'ready') 'ready'
Equal-Mg (Get-MgCommand 'authorization' 'guest_ready') 'invalid'
Equal-Mg (Get-MgCommand 'game' 'ready') 'invalid'
Equal-Mg (Get-MgCommand 'game' 'stop') 'stop'
Equal-Mg (Get-MgCommand 'authorization' 'STOP') 'invalid'
foreach ($nativeCode in @('invalid_configuration','transport_failed','timed_out','unauthorized','expired_or_revoked','service_not_authorized','capability_unavailable','stale_service_revision','invalid_protocol','replayed_request','resource_limit','destination_unavailable','datagram_too_large','closed','local_io_failed','lab_operation_failed','status_connection_failed','status_timed_out','status_pong_mismatch')) { Equal-Mg (Get-MgNativeCode $nativeCode) $nativeCode }
foreach ($nativeCode in @('timeout','io','configuration','some-secret-value','status_connection_failed IP','TIMED_OUT')) { Equal-Mg (Get-MgNativeCode $nativeCode) $null }
Equal-Mg (Get-MgStatusDue 0 0 0) $true
Equal-Mg (Get-MgStatusDue 19999 1 0) $false
Equal-Mg (Get-MgStatusDue 20000 1 0) $true
Equal-Mg (Get-MgStatusDue 899999 44 879999) $true
Equal-Mg (Get-MgStatusDue 900000 44 880000) $false
Equal-Mg (Get-MgStatusDue 899999 45 0) $false
Equal-Mg (Get-MgStatusDue 0 -1 0) $false
Equal-Mg (Get-MgStatusDue -1 0 0) $false
Require-MgDeadline 599999 600000; Pass-Mg
Require-MgDeadline 899999 900000; Pass-Mg
Require-MgDeadline 1499999 1500000; Pass-Mg
foreach ($case in @(@{now=600000;until=600000},@{now=600001;until=600000},@{now=900000;until=900000},@{now=-1;until=600000},@{now=0;until=0},@{now=0;until=1500001})) { Deny-Mg { Require-MgDeadline $case.now $case.until } }
# Slow requests cause no catch-up burst and remain below the fixed budget.
$count = 0; $last = 0
for ($now = 0; $now -lt 900000; $now += 500) {
    if (Get-MgStatusDue $now $count $last) { $count++; $last = $now }
}
Equal-Mg $count 45
Initialize-MgIo
function New-MgReader([string]$Text) {
    return New-Object IO.StreamReader((New-Object IO.MemoryStream(,([Text.Encoding]::UTF8.GetBytes($Text)))))
}
foreach ($case in @(@{text="ok`n";maximum=2;answer='ok'},@{text="`n";maximum=1;answer=''})) {
    $reader = New-MgReader $case.text
    try { $task = [AuroraMinecraftGuestIo]::LineAsync($reader,$case.maximum); $null = $task.Wait(1000); Equal-Mg $task.Result $case.answer } finally { $reader.Dispose() }
}
foreach ($text in @('partial',"oversize`n")) {
    $reader = New-MgReader $text
    try { Deny-Mg { $task = [AuroraMinecraftGuestIo]::LineAsync($reader,3); $null = $task.Wait(1000) } } finally { $reader.Dispose() }
}
$reader = New-MgReader ('x' * 2097152)
try { $task = [AuroraMinecraftGuestIo]::CountAsync($reader); $null = $task.Wait(3000); Equal-Mg $task.Result 2097152 } finally { $reader.Dispose() }
# Owned-process stand-ins exercise broken stdin without starting any process.
function New-MgFakeProcess([bool]$Broken) {
    $input = [pscustomobject]@{broken=$Broken;written=$null}
    $input | Add-Member ScriptMethod WriteLine { param($line) if ($this.broken) { throw 'mg_fixture_broken_stdin' }; $this.written=$line }
    $input | Add-Member ScriptMethod Flush { }
    $input | Add-Member ScriptMethod Close { }
    $process = [pscustomobject]@{HasExited=$false;ExitCode=0;StandardInput=$input;StandardOutput=$null;broken=$Broken;waits=0;kills=0;disposed=$false}
    $process | Add-Member ScriptMethod WaitForExit { param($ms) $this.waits++; if ($this.broken -and $this.kills -eq 0) { return $false }; $this.HasExited=$true; return $true }
    $process | Add-Member ScriptMethod Kill { $this.kills++; $this.HasExited=$true; $this.ExitCode=137 }
    $process | Add-Member ScriptMethod Dispose { $this.disposed=$true }
    return $process
}
foreach ($broken in @($false,$true)) {
    $process = New-MgFakeProcess $broken
    $outReader = New-MgReader $(if ($broken) { '' } else { "{`"event`":`"guest_stopped`"}`n" })
    $errorReader = New-MgReader ''
    try {
        $process.StandardOutput = $outReader
        $owned = [pscustomobject]@{process=$process;role='guest';pending=[AuroraMinecraftGuestIo]::LineAsync($outReader,4096);stderr=[AuroraMinecraftGuestIo]::CountAsync($errorReader);forced=$false;reaped=$false;stopped=$false;exitCode=$null}
        $state = [pscustomobject]@{lastEvent='guest_ready';nativeCode=$null}
        Equal-Mg (Stop-MgOwned $owned $true $state) (-not $broken)
        Equal-Mg $owned.reaped $true
        Equal-Mg $owned.forced $broken
        Equal-Mg $process.disposed $true
        Equal-Mg $process.kills $(if ($broken) { 1 } else { 0 })
        Equal-Mg $owned.stopped (-not $broken)
        if (-not $broken) { Equal-Mg $process.StandardInput.written 'stop'; Equal-Mg $state.lastEvent 'guest_stopped' }
        else { Equal-Mg $process.waits 2 }
    } finally { $outReader.Dispose(); $errorReader.Dispose() }
}
foreach ($case in @(@{line=$status;allowed=$true},@{line='{"event":"guest_stopped"}';allowed=$false})) {
    $process = New-MgFakeProcess $false
    $outReader = New-MgReader ($case.line + "`n")
    $errorReader = New-MgReader ''
    try {
        $process.StandardOutput = $outReader
        $owned = [pscustomobject]@{process=$process;role='status';pending=[AuroraMinecraftGuestIo]::LineAsync($outReader,4096);stderr=[AuroraMinecraftGuestIo]::CountAsync($errorReader);forced=$false;reaped=$false;stopped=$false;exitCode=$null}
        $state = [pscustomobject]@{lastEvent='guest_ready';nativeCode=$null}
        Equal-Mg (Stop-MgOwned $owned $true $state) $case.allowed
        Equal-Mg $owned.reaped $true
        Equal-Mg $process.disposed $true
    } finally { $outReader.Dispose(); $errorReader.Dispose() }
}
# Own temporary evidence only. No kit/binary, listeners, native children or Java.
$temp = Join-Path ([IO.Path]::GetTempPath()) ('aurora-guest-fixtures-' + [Guid]::NewGuid().ToString('N'))
$null = [IO.Directory]::CreateDirectory($temp)
try {
    $attempt = [Guid]::NewGuid().ToString('N')
    $folder = New-MgEvidence $temp $attempt
    Deny-Mg { New-MgEvidence $temp $attempt }
    Deny-Mg { New-MgEvidence $temp '../escape' }
    Save-MgSummary $folder '{"phase":"game_status","code":"mg_native_failed"}'
    Equal-Mg ([IO.File]::ReadAllText((Join-Path $folder 'summary.json'))) '{"phase":"game_status","code":"mg_native_failed"}'
    $bytes = [IO.File]::ReadAllBytes((Join-Path $folder 'summary.json'))
    Equal-Mg $bytes[0] 123
    Deny-Mg { Save-MgSummary $folder 'overwrite' }
    Deny-Mg { Save-MgSummary $folder ('x' * 8193) }
    Deny-Mg { Require-MgDirectory (Join-Path $folder 'summary.json') }
} finally {
    # Checked exact fixture root, only our generated tree, one shell end-to-end.
    $resolved = [IO.Path]::GetFullPath($temp)
    if ($resolved.StartsWith([IO.Path]::GetFullPath([IO.Path]::GetTempPath()),[StringComparison]::OrdinalIgnoreCase) -and [IO.Path]::GetFileName($resolved) -cmatch '^aurora-guest-fixtures-[a-f0-9]{32}$') { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
foreach ($path in @($PSCommandPath,(Join-Path $PSScriptRoot 'connect-minecraft-guest-lab.ps1'))) {
    $tokens = $null; $errors = $null
    $null = [Management.Automation.Language.Parser]::ParseFile($path,[ref]$tokens,[ref]$errors)
    Equal-Mg @($errors).Count 0
}
[ordered]@{event='minecraft_guest_gate_fixtures_passed';passed=$passed;network=$false;nativeChildren=0;javaStarted=$false;personalConfigurationChanged=$false}|ConvertTo-Json -Compress

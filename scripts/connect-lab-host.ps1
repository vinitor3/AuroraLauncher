param(
    [string]$Binary = '',
    [string]$TransportBind = '0.0.0.0:42000',
    [ValidateRange(10,3600)][int]$Seconds = 600
)
$ErrorActionPreference = 'Stop'
if (-not $Binary) { $Binary = Join-Path $PSScriptRoot '../apps/connect-lab/target/debug/aurora-connect-lab.exe' }
$Binary = [IO.Path]::GetFullPath($Binary)
if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) { throw 'lab_binary_missing' }
if ($TransportBind -notmatch '^(?:[0-9.]+|\[[a-fA-F0-9:]+\]):[0-9]{1,5}$') { throw 'invalid_transport_bind' }
Write-Output 'EXPERIMENTAL: two-computer controlled echo, no Minecraft/voice/config changes.'
Write-Output ('Binary SHA256: ' + (Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash)
Write-Output 'Private connectivity candidates for your guest (do not post these in shared logs):'
Get-NetIPAddress -AddressFamily IPv4,IPv6 -ErrorAction SilentlyContinue |
    Where-Object { $_.AddressState -eq 'Preferred' -and $_.IPAddress -notin @('127.0.0.1','::1') -and $_.IPAddress -notlike 'fe80:*' -and $_.IPAddress -notlike '169.254.*' } |
    ForEach-Object { Write-Output ('  ' + $_.InterfaceAlias + ': ' + $_.IPAddress) }
$info = New-Object Diagnostics.ProcessStartInfo
$info.FileName = $Binary
$info.Arguments = "echo 127.0.0.1:42001 127.0.0.1:42002 $Seconds"
$info.UseShellExecute = $false
$info.CreateNoWindow = $true
$info.RedirectStandardInput = $true
$info.RedirectStandardOutput = $true
$info.RedirectStandardError = $true
$echoProcess = New-Object Diagnostics.Process
$echoProcess.StartInfo = $info
try {
    if (-not $echoProcess.Start()) { throw 'echo_start_failed' }
    $ready = $echoProcess.StandardOutput.ReadLineAsync()
    if (-not $ready.Wait(10000) -or -not $ready.Result -or ($ready.Result | ConvertFrom-Json).event -ne 'echo_ready') { throw 'echo_not_ready_or_port_occupied' }
    & $Binary host $TransportBind 127.0.0.1:42001 127.0.0.1:42002 $Seconds
    if ($LASTEXITCODE -ne 0) { throw 'host_failed_sanitized_code_above' }
} finally {
    if ($echoProcess.Id -and -not $echoProcess.HasExited) {
        try { $echoProcess.StandardInput.WriteLine('stop'); $echoProcess.StandardInput.Flush() } catch { }
        if (-not $echoProcess.WaitForExit(5000)) { $echoProcess.Kill(); $echoProcess.WaitForExit() }
    }
    $echoProcess.Dispose()
}

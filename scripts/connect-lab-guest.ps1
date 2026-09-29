param(
    [string]$Binary = '',
    [string]$HostPeer = '',
    [string]$HostAddress = '',
    [string]$Session = '',
    [ValidateRange(10,3600)][int]$Seconds = 600
)
$ErrorActionPreference = 'Stop'
if (-not $Binary) { $Binary = Join-Path $PSScriptRoot '../apps/connect-lab/target/debug/aurora-connect-lab.exe' }
$Binary = [IO.Path]::GetFullPath($Binary)
if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) { throw 'lab_binary_missing' }
Write-Output 'EXPERIMENTAL: public pins stay between participants; never enter account secrets.'
Write-Output ('Binary SHA256: ' + (Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash)
if (-not $HostPeer) { $HostPeer = Read-Host 'Host public peer' }
if (-not $HostAddress) { $HostAddress = Read-Host 'Host direct IP:42000 or [IPv6]:42000' }
if (-not $Session) { $Session = Read-Host 'Host session' }
Write-Output 'After guest_ready, use another terminal for the existing probe command:'
Write-Output ('& ''' + $Binary.Replace("'", "''") + ''' probe 127.0.0.1:42010 127.0.0.1:42011')
& $Binary guest $HostPeer $HostAddress $Session 127.0.0.1:42010 127.0.0.1:42011 $Seconds
if ($LASTEXITCODE -ne 0) { throw 'guest_failed_sanitized_code_above' }

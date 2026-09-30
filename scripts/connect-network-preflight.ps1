# Read-only inventory for a private Aurora Connect transport test. No packets,
# listeners, mappings, firewall changes, DLL loads or personal files are used.
# Dot-sourcing defines helpers for the adjacent dependency-free fixture tests;
# normal execution accepts no arguments and always emits one redacted JSON value.

function New-AuroraNetworkReport {
    [ordered]@{
        schema = 1
        status = 'NAO_VERIFICADO'
        code = 'network_preflight_failed'
        windowsX64 = $null
        powershell64Bit = $null
        powershellSupported = $null
        vcRuntime140FilePresent = $null
        ipv4PreferredCount = $null
        ipv4PrivateCount = $null
        ipv4OtherCount = $null
        ipv6PreferredCount = $null
        ipv6GlobalCandidateCount = $null
        ipv6GlobalCandidate = $null
        ipv4DefaultRoute = $null
        ipv6DefaultRoute = $null
        ipv6SameInterfaceCandidate = $null
        networkReachabilityVerified = $false
        udpReachabilityVerified = $false
    }
}

function Get-AuroraNetworkFacts {
    param([object[]]$Addresses, [object[]]$Routes)
    Set-StrictMode -Version 2
    if ($Addresses.Count -gt 2048 -or $Routes.Count -gt 4096) { throw 'network_preflight_inventory_limit' }
    $facts = [ordered]@{
        ipv4PreferredCount = 0; ipv4PrivateCount = 0; ipv4OtherCount = 0
        ipv6PreferredCount = 0; ipv6GlobalCandidateCount = 0; ipv6GlobalCandidate = $false
        ipv4DefaultRoute = $false; ipv6DefaultRoute = $false; ipv6SameInterfaceCandidate = $false
    }
    $candidates = New-Object 'System.Collections.Generic.HashSet[uint32]'
    foreach ($entry in $Addresses) {
        if ([string]$entry.AddressState -notin @('Preferred', 'Deprecated', 'Tentative', 'Duplicate', 'Invalid')) {
            throw 'network_preflight_inventory_invalid'
        }
        if ([string]$entry.AddressState -ne 'Preferred') { continue }
        $index = [uint32]$entry.InterfaceIndex
        if ($index -eq 0) { throw 'network_preflight_inventory_invalid' }
        $text = [string]$entry.IPAddress
        $ip = $null
        if ($text.Length -gt 128 -or -not [System.Net.IPAddress]::TryParse($text, [ref]$ip)) {
            throw 'network_preflight_inventory_invalid'
        }
        $bytes = $ip.GetAddressBytes()
        if ([string]$entry.AddressFamily -eq 'IPv4' -and $bytes.Length -eq 4) {
            $facts.ipv4PreferredCount++
            if ($bytes[0] -eq 10 -or ($bytes[0] -eq 172 -and $bytes[1] -ge 16 -and $bytes[1] -le 31) -or
                ($bytes[0] -eq 192 -and $bytes[1] -eq 168)) { $facts.ipv4PrivateCount++ }
            else { $facts.ipv4OtherCount++ }
            # All non-RFC1918 IPv4 addresses are simply "other". In particular,
            # shared, loopback, link-local, documentation and reserved ranges
            # are never reported as a public or Internet-reachable address.
        }
        elseif ([string]$entry.AddressFamily -eq 'IPv6' -and $bytes.Length -eq 16) {
            $facts.ipv6PreferredCount++
            # Candidate only: 2000::/3, excluding the IANA documentation
            # prefixes 2001:db8::/32 and 3fff::/20 (RFC 9637).
            # This also excludes ULA, link-local, multicast, loopback and mapped
            # IPv4. No candidate is a proof of routing, firewall or UDP access.
            $documentation = ($bytes[0] -eq 0x20 -and $bytes[1] -eq 0x01 -and $bytes[2] -eq 0x0d -and $bytes[3] -eq 0xb8) -or
                ($bytes[0] -eq 0x3f -and $bytes[1] -eq 0xff -and ($bytes[2] -band 0xf0) -eq 0)
            if (($bytes[0] -band 0xe0) -eq 0x20 -and -not $documentation) {
                $facts.ipv6GlobalCandidateCount++
                $facts.ipv6GlobalCandidate = $true
                [void]$candidates.Add($index)
            }
        }
        else { throw 'network_preflight_inventory_invalid' }
    }
    foreach ($route in $Routes) {
        if ([string]$route.DestinationPrefix -notin @('0.0.0.0/0', '::/0')) { continue }
        if ([string]$route.State -notin @('Alive', 'Dead', 'Probe')) { throw 'network_preflight_inventory_invalid' }
        if ([string]$route.State -ne 'Alive') { continue }
        $index = [uint32]$route.InterfaceIndex
        if ($index -eq 0) { throw 'network_preflight_inventory_invalid' }
        if ([string]$route.DestinationPrefix -eq '0.0.0.0/0' -and [string]$route.AddressFamily -eq 'IPv4') {
            $facts.ipv4DefaultRoute = $true
        }
        elseif ([string]$route.DestinationPrefix -eq '::/0' -and [string]$route.AddressFamily -eq 'IPv6') {
            $facts.ipv6DefaultRoute = $true
            if ($candidates.Contains($index)) { $facts.ipv6SameInterfaceCandidate = $true }
        }
        else { throw 'network_preflight_inventory_invalid' }
    }
    return $facts
}

function Invoke-AuroraNetworkPreflight {
    # Function scope preserves the caller's ErrorActionPreference.
    $ErrorActionPreference = 'Stop'
    $VerbosePreference = 'SilentlyContinue'
    $DebugPreference = 'SilentlyContinue'
    $InformationPreference = 'SilentlyContinue'
    $WarningPreference = 'SilentlyContinue'
    $ProgressPreference = 'SilentlyContinue'
    Set-StrictMode -Version 2
    $report = New-AuroraNetworkReport
    $failureCode = 'network_preflight_platform_query_failed'
    try {
        $report.windowsX64 = [Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT -and
            [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString() -eq 'X64'
        $report.powershell64Bit = [Environment]::Is64BitProcess
        $report.powershellSupported = $PSVersionTable.PSVersion -ge [version]'5.1'
        if (-not $report.windowsX64 -or -not $report.powershellSupported) {
            $report.code = 'network_preflight_platform_unsupported'
            return $report
        }
        $failureCode = 'network_preflight_runtime_query_failed'
        $windows = [Environment]::GetFolderPath([Environment+SpecialFolder]::Windows)
        if ([string]::IsNullOrWhiteSpace($windows)) { throw 'network_preflight_runtime_query_failed' }
        # Sysnative avoids WOW64 redirection when PowerShell itself is 32-bit.
        $system = if ($report.powershell64Bit) { 'System32' } else { 'Sysnative' }
        $dll = Join-Path (Join-Path $windows $system) 'VCRUNTIME140.dll'
        try {
            $item = Get-Item -LiteralPath $dll -Force -ErrorAction Stop -WarningAction SilentlyContinue 3>$null 4>$null 5>$null 6>$null
            if ($item.PSIsContainer) { throw 'network_preflight_runtime_query_failed' }
            $report.vcRuntime140FilePresent = $true
        }
        catch [System.Management.Automation.ItemNotFoundException] { $report.vcRuntime140FilePresent = $false }

        $failureCode = 'network_preflight_address_query_failed'
        $addresses = New-Object 'System.Collections.Generic.List[object]'
        Get-NetIPAddress -AddressFamily IPv4,IPv6 -ErrorAction Stop -WarningAction SilentlyContinue 3>$null 4>$null 5>$null 6>$null | ForEach-Object {
            if ($addresses.Count -ge 2048) { $failureCode = 'network_preflight_inventory_limit'; throw 'network_preflight_inventory_limit' }
            [void]$addresses.Add($_)
        }
        $failureCode = 'network_preflight_route_query_failed'
        $routes = New-Object 'System.Collections.Generic.List[object]'
        Get-NetRoute -AddressFamily IPv4,IPv6 -ErrorAction Stop -WarningAction SilentlyContinue 3>$null 4>$null 5>$null 6>$null | ForEach-Object {
            if ($routes.Count -ge 4096) { $failureCode = 'network_preflight_inventory_limit'; throw 'network_preflight_inventory_limit' }
            [void]$routes.Add($_)
        }
        $failureCode = 'network_preflight_inventory_invalid'
        $facts = Get-AuroraNetworkFacts -Addresses $addresses.ToArray() -Routes $routes.ToArray()
        foreach ($key in $facts.Keys) { $report[$key] = $facts[$key] }
        $report.status = 'OBSERVADO'
        $report.code = 'network_preflight_observed'
    }
    catch { $report.code = $failureCode }
    return $report
}

if ($MyInvocation.InvocationName -ne '.') {
    if ($args.Count -ne 0) {
        $report = New-AuroraNetworkReport
        $report.code = 'network_preflight_arguments'
    }
    else { $report = Invoke-AuroraNetworkPreflight }
    $report | ConvertTo-Json -Compress
    if ($report.status -ne 'OBSERVADO') { exit 1 }
}

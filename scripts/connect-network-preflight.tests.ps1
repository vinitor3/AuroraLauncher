# Dependency-free PowerShell 5.1 fixture tests. No Pester, network, DLL load,
# admin permission or production inventory is required by these tests.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2
. (Join-Path $PSScriptRoot 'connect-network-preflight.ps1')
$script:passed = 0
$script:failed = 0

function Assert-AuroraTest([bool]$Condition) {
    if (-not $Condition) { throw 'network_preflight_test_assertion' }
}
function Test-AuroraCase([scriptblock]$Body) {
    try { & $Body; $script:passed++ }
    catch { $script:failed++ }
}
function New-AuroraAddress([string]$Text, [string]$Family, [uint32]$Index = 7, [string]$State = 'Preferred') {
    [pscustomobject]@{ IPAddress = $Text; AddressFamily = $Family; InterfaceIndex = $Index; AddressState = $State }
}
function New-AuroraRoute([string]$Family, [uint32]$Index = 7, [string]$State = 'Alive') {
    [pscustomobject]@{ DestinationPrefix = $(if ($Family -eq 'IPv6') { '::/0' } else { '0.0.0.0/0' });
        AddressFamily = $Family; InterfaceIndex = $Index; State = $State }
}

Test-AuroraCase {
    $facts = Get-AuroraNetworkFacts -Addresses @() -Routes @()
    Assert-AuroraTest ($facts.ipv4PreferredCount -eq 0 -and $facts.ipv6PreferredCount -eq 0 -and
        -not $facts.ipv6GlobalCandidate -and -not $facts.ipv6SameInterfaceCandidate)
}
Test-AuroraCase {
    $addresses = @('10.0.0.1', '172.16.0.1', '172.31.255.254', '192.168.0.1') | ForEach-Object { New-AuroraAddress $_ 'IPv4' }
    $facts = Get-AuroraNetworkFacts -Addresses $addresses -Routes @((New-AuroraRoute 'IPv4'))
    Assert-AuroraTest ($facts.ipv4PrivateCount -eq 4 -and $facts.ipv4OtherCount -eq 0 -and $facts.ipv4DefaultRoute)
}
Test-AuroraCase {
    $addresses = @('100.64.0.1', '100.127.255.254', '127.0.0.1', '169.254.2.1', '192.0.2.1', '198.51.100.1',
        '203.0.113.1', '224.0.0.1', '240.0.0.1', '0.0.0.0', '255.255.255.255', '172.15.255.254', '172.32.0.1') |
        ForEach-Object { New-AuroraAddress $_ 'IPv4' }
    $facts = Get-AuroraNetworkFacts -Addresses $addresses -Routes @()
    Assert-AuroraTest ($facts.ipv4PrivateCount -eq 0 -and $facts.ipv4OtherCount -eq 13)
    Assert-AuroraTest (-not ($facts.Keys -match 'public|reachable'))
}
Test-AuroraCase {
    $addresses = @('2000::1', '3fff:ffff::1', '2606:4700::1') | ForEach-Object { New-AuroraAddress $_ 'IPv6' }
    $facts = Get-AuroraNetworkFacts -Addresses $addresses -Routes @((New-AuroraRoute 'IPv6'))
    Assert-AuroraTest ($facts.ipv6GlobalCandidateCount -eq 3 -and $facts.ipv6GlobalCandidate -and
        $facts.ipv6DefaultRoute -and $facts.ipv6SameInterfaceCandidate)
}
Test-AuroraCase {
    $addresses = @('2001:db8::1', '2001:db8:ffff:ffff::1', 'fe80::1', 'fd00::1', 'fc00::1', 'ff02::1', '::1',
        '::', '::ffff:192.0.2.1', '1fff:ffff::1', '4000::1') | ForEach-Object { New-AuroraAddress $_ 'IPv6' }
    $facts = Get-AuroraNetworkFacts -Addresses $addresses -Routes @((New-AuroraRoute 'IPv6'))
    Assert-AuroraTest ($facts.ipv6PreferredCount -eq 11 -and $facts.ipv6GlobalCandidateCount -eq 0 -and
        -not $facts.ipv6GlobalCandidate -and $facts.ipv6DefaultRoute -and -not $facts.ipv6SameInterfaceCandidate)
}
Test-AuroraCase {
    $facts = Get-AuroraNetworkFacts -Addresses @((New-AuroraAddress '2606:4700::1' 'IPv6' 8)) -Routes @((New-AuroraRoute 'IPv6' 9))
    Assert-AuroraTest ($facts.ipv6GlobalCandidate -and $facts.ipv6DefaultRoute -and -not $facts.ipv6SameInterfaceCandidate)
}
foreach ($boundary in @(
    @{ Address = '3fff::'; Candidate = $false },
    @{ Address = '3fff::1'; Candidate = $false },
    @{ Address = '3fff:fff:ffff:ffff:ffff:ffff:ffff:ffff'; Candidate = $false },
    @{ Address = '3fff:1000::'; Candidate = $true },
    @{ Address = '3ffe:ffff:ffff:ffff:ffff:ffff:ffff:ffff'; Candidate = $true })) {
    Test-AuroraCase {
        $facts = Get-AuroraNetworkFacts -Addresses @((New-AuroraAddress $boundary.Address 'IPv6')) -Routes @((New-AuroraRoute 'IPv6'))
        Assert-AuroraTest ($facts.ipv6PreferredCount -eq 1 -and $facts.ipv6DefaultRoute)
        Assert-AuroraTest ($facts.ipv6GlobalCandidate -eq $boundary.Candidate -and
            $facts.ipv6SameInterfaceCandidate -eq $boundary.Candidate -and
            $facts.ipv6GlobalCandidateCount -eq [int]$boundary.Candidate)
    }
}
Test-AuroraCase {
    $addresses = @('Deprecated', 'Tentative', 'Duplicate', 'Invalid') | ForEach-Object { New-AuroraAddress '2606:4700::1' 'IPv6' 7 $_ }
    $facts = Get-AuroraNetworkFacts -Addresses $addresses -Routes @((New-AuroraRoute 'IPv6'))
    Assert-AuroraTest ($facts.ipv6PreferredCount -eq 0 -and -not $facts.ipv6SameInterfaceCandidate)
}
Test-AuroraCase {
    $facts = Get-AuroraNetworkFacts -Addresses @((New-AuroraAddress '2606:4700::1' 'IPv6')) -Routes @(
        (New-AuroraRoute 'IPv6' 7 'Dead'), (New-AuroraRoute 'IPv6' 7 'Probe'))
    Assert-AuroraTest (-not $facts.ipv6DefaultRoute -and -not $facts.ipv6SameInterfaceCandidate)
}
foreach ($invalid in @(
    (New-AuroraAddress 'INVALID_FIXTURE' 'IPv6'), (New-AuroraAddress '192.0.2.1' 'IPv6'),
    (New-AuroraAddress '2606:4700::1' 'IPv6' 0), (New-AuroraAddress '2606:4700::1' 'IPv6' 7 'Unexpected'))) {
    Test-AuroraCase {
        $thrown = $false
        try { $null = Get-AuroraNetworkFacts -Addresses @($invalid) -Routes @() }
        catch { $thrown = $true }
        Assert-AuroraTest $thrown
    }
}
Test-AuroraCase {
    $addresses = 1..2048 | ForEach-Object { New-AuroraAddress '192.0.2.1' 'IPv4' }
    $facts = Get-AuroraNetworkFacts -Addresses $addresses -Routes @()
    Assert-AuroraTest ($facts.ipv4PreferredCount -eq 2048)
    $thrown = $false
    try { $null = Get-AuroraNetworkFacts -Addresses @($addresses + $addresses[0]) -Routes @() }
    catch { $thrown = $true }
    Assert-AuroraTest $thrown
}
Test-AuroraCase {
    $routes = 1..4096 | ForEach-Object { New-AuroraRoute 'IPv6' }
    $facts = Get-AuroraNetworkFacts -Addresses @() -Routes $routes
    Assert-AuroraTest $facts.ipv6DefaultRoute
    $thrown = $false
    try { $null = Get-AuroraNetworkFacts -Addresses @() -Routes @($routes + $routes[0]) }
    catch { $thrown = $true }
    Assert-AuroraTest $thrown
}

# These cmdlet fixtures live only in the child test scope. The normal script
# exposes no inventory, path, clock or authorization override parameter.
foreach ($scenario in @('normal', 'quiet_streams', 'missing_runtime', 'denied_runtime', 'address_failed', 'route_failed',
    'address_partial_failure', 'route_partial_failure',
    'address_limit', 'route_limit', 'invalid_inventory')) {
    Test-AuroraCase {
        $ErrorActionPreference = 'Continue'
        $VerbosePreference = 'Continue'
        $DebugPreference = 'Continue'
        $InformationPreference = 'Continue'
        $WarningPreference = 'Continue'
        function Get-Item {
            param($LiteralPath, [switch]$Force, $ErrorAction, $WarningAction)
            if ($scenario -eq 'missing_runtime') { throw (New-Object System.Management.Automation.ItemNotFoundException 'PRIVATE_FIXTURE') }
            if ($scenario -eq 'denied_runtime') { throw (New-Object System.UnauthorizedAccessException 'PRIVATE_FIXTURE') }
            $expectedSystem = if ([Environment]::Is64BitProcess) { 'System32' } else { 'Sysnative' }
            Assert-AuroraTest ($LiteralPath -match ('[\\/]' + $expectedSystem + '[\\/]VCRUNTIME140\.dll$'))
            [pscustomobject]@{ PSIsContainer = $false }
        }
        function Get-NetIPAddress {
            param($AddressFamily, $ErrorAction, $WarningAction)
            if ($scenario -eq 'quiet_streams') {
                Write-Verbose 'PRIVATE_FIXTURE'
                Write-Debug 'PRIVATE_FIXTURE'
                Write-Information 'PRIVATE_FIXTURE'
                Write-Warning 'PRIVATE_FIXTURE'
            }
            if ($scenario -eq 'address_failed') { throw 'PRIVATE_FIXTURE' }
            if ($scenario -eq 'invalid_inventory') { New-AuroraAddress 'INVALID_FIXTURE' 'IPv6'; return }
            $count = if ($scenario -eq 'address_limit') { 2049 } else { 1 }
            for ($i = 0; $i -lt $count; $i++) { New-AuroraAddress '2606:4700::1' 'IPv6' }
            if ($scenario -eq 'address_partial_failure') { Write-Error 'PRIVATE_FIXTURE' -ErrorAction Stop }
        }
        function Get-NetRoute {
            param($AddressFamily, $ErrorAction, $WarningAction)
            if ($scenario -eq 'route_failed') { throw 'PRIVATE_FIXTURE' }
            $count = if ($scenario -eq 'route_limit') { 4097 } else { 1 }
            for ($i = 0; $i -lt $count; $i++) { New-AuroraRoute 'IPv6' }
            if ($scenario -eq 'route_partial_failure') { Write-Error 'PRIVATE_FIXTURE' -ErrorAction Stop }
        }
        $captured = @(Invoke-AuroraNetworkPreflight *>&1)
        Assert-AuroraTest ($captured.Count -eq 1)
        $report = $captured[0]
        Assert-AuroraTest ($ErrorActionPreference -eq 'Continue')
        Assert-AuroraTest ($VerbosePreference -eq 'Continue' -and $DebugPreference -eq 'Continue' -and
            $InformationPreference -eq 'Continue' -and $WarningPreference -eq 'Continue')
        $json = $report | ConvertTo-Json -Compress
        Assert-AuroraTest ($json -notmatch 'PRIVATE_FIXTURE|2606|4700|192\.0|VCRUNTIME140\.dll|System32|Sysnative|INVALID_FIXTURE')
        foreach ($key in $report.Keys) {
            if ($key -in @('status', 'code')) { continue }
            Assert-AuroraTest ($null -eq $report[$key] -or $report[$key] -is [bool] -or $report[$key] -is [int])
        }
        Assert-AuroraTest (-not $report.networkReachabilityVerified -and -not $report.udpReachabilityVerified)
        if ($scenario -in @('normal', 'quiet_streams', 'missing_runtime')) {
            Assert-AuroraTest ($report.code -eq 'network_preflight_observed' -and $report.ipv6SameInterfaceCandidate)
            Assert-AuroraTest ($report.vcRuntime140FilePresent -eq ($scenario -ne 'missing_runtime'))
        }
        else {
            $expected = switch ($scenario) {
                'denied_runtime' { 'network_preflight_runtime_query_failed' }
                'address_failed' { 'network_preflight_address_query_failed' }
                'route_failed' { 'network_preflight_route_query_failed' }
                'address_partial_failure' { 'network_preflight_address_query_failed' }
                'route_partial_failure' { 'network_preflight_route_query_failed' }
                'address_limit' { 'network_preflight_inventory_limit' }
                'route_limit' { 'network_preflight_inventory_limit' }
                'invalid_inventory' { 'network_preflight_inventory_invalid' }
            }
            Assert-AuroraTest ($report.code -eq $expected -and $report.status -eq 'NAO_VERIFICADO')
            Assert-AuroraTest ($null -eq $report.ipv6PreferredCount -and $null -eq $report.ipv6SameInterfaceCandidate)
        }
    }
}

Test-AuroraCase {
    function Get-Item { throw 'UNEXPECTED_FIXTURE_IO' }
    function Get-NetIPAddress { throw 'UNEXPECTED_FIXTURE_IO' }
    function Get-NetRoute { throw 'UNEXPECTED_FIXTURE_IO' }
    $output = & (Join-Path $PSScriptRoot 'connect-network-preflight.ps1') '--unexpected'
    $report = $output | ConvertFrom-Json
    Assert-AuroraTest ($LASTEXITCODE -eq 1 -and $report.code -eq 'network_preflight_arguments')
    Assert-AuroraTest ($null -eq $report.windowsX64 -and $null -eq $report.ipv6GlobalCandidate)
}

[pscustomobject]@{ schema = 1; code = $(if ($script:failed -eq 0) { 'network_preflight_tests_pass' } else { 'network_preflight_tests_failed' });
    passed = $script:passed; failed = $script:failed } | ConvertTo-Json -Compress
if ($script:failed -ne 0) { exit 1 }

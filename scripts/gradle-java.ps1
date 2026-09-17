# Select the Gradle runner JVM independently from project compiler toolchains.
function Resolve-AuroraJdk {
    param([Parameter(Mandatory = $true)][ValidateSet(17, 21)][int]$Major)

    $candidates = @($env:JAVA_HOME, [Environment]::GetEnvironmentVariable("JAVA_HOME_${Major}_X64", 'Process'))
    $candidates += @(Get-ItemProperty -Path 'HKLM:\SOFTWARE\JavaSoft\JDK\*' -ErrorAction SilentlyContinue |
        ForEach-Object { if ($_.PSObject.Properties['JavaHome']) { $_.JavaHome } })
    $candidates += @(Get-ItemProperty -Path 'HKLM:\SOFTWARE\Eclipse Adoptium\JDK\*\hotspot\MSI' -ErrorAction SilentlyContinue |
        ForEach-Object { if ($_.PSObject.Properties['Path']) { $_.Path } })
    foreach ($programDirectory in @($env:ProgramW6432, $env:ProgramFiles) | Select-Object -Unique) {
        if ([string]::IsNullOrWhiteSpace($programDirectory)) { continue }
        foreach ($vendor in @('Eclipse Adoptium', 'Java')) {
            $vendorDirectory = Join-Path $programDirectory $vendor
            if (Test-Path -LiteralPath $vendorDirectory -PathType Container) {
                $candidates += @(Get-ChildItem -LiteralPath $vendorDirectory -Directory |
                    Sort-Object Name -Descending | ForEach-Object { $_.FullName })
            }
        }
    }
    foreach ($candidate in $candidates | Select-Object -Unique) {
        if ([string]::IsNullOrWhiteSpace($candidate)) { continue }
        $jdkDirectory = $candidate.Trim().Trim('"')
        $releaseFile = Join-Path $jdkDirectory 'release'
        if (-not (Test-Path -LiteralPath $releaseFile -PathType Leaf) -or
            -not (Test-Path -LiteralPath (Join-Path $jdkDirectory 'bin/java.exe') -PathType Leaf) -or
            -not (Test-Path -LiteralPath (Join-Path $jdkDirectory 'bin/javac.exe') -PathType Leaf)) { continue }
        $release = Get-Content -Raw -LiteralPath $releaseFile
        if ($release -match ('(?m)^JAVA_VERSION="' + $Major + '[.+"-]')) {
            return (Resolve-Path -LiteralPath $jdkDirectory).Path
        }
    }
    throw "JDK $Major nao encontrado. Instale um JDK $Major e defina JAVA_HOME_${Major}_X64. Nenhum download automatico foi realizado."
}

function Invoke-AuroraGradleWithJava {
    param(
        [Parameter(Mandatory = $true)][ValidateSet(17, 21)][int]$JavaVersion,
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][string[]]$Arguments
    )

    $selectedHome = Resolve-AuroraJdk $JavaVersion
    if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) { throw "Wrapper Gradle ausente: $Executable" }
    $previousHome = [Environment]::GetEnvironmentVariable('JAVA_HOME', 'Process')
    $previousPreference = $ErrorActionPreference
    $nativeExit = $null
    try {
        $env:JAVA_HOME = $selectedHome
        Write-Host "[Aurora Gradle] JVM $JavaVersion | $selectedHome"
        # Native stderr is not itself a failure in Windows PowerShell.
        $ErrorActionPreference = 'Continue'
        $global:LASTEXITCODE = 0
        & $Executable @Arguments
        $completed = $?
        $nativeExit = $LASTEXITCODE
        if (-not $completed -and $nativeExit -eq 0) { $nativeExit = 1 }
    } finally {
        [Environment]::SetEnvironmentVariable('JAVA_HOME', $previousHome, 'Process')
        $ErrorActionPreference = $previousPreference
        if ($null -ne $nativeExit) { $global:LASTEXITCODE = $nativeExit }
    }
}

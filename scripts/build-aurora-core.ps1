[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$RepositoryRoot = Split-Path -Parent $PSScriptRoot
$CoreRoot = Join-Path $RepositoryRoot "apps\aurora-core"
$SharedGradle = Join-Path $RepositoryRoot "apps\companion-mod\gradlew.bat"
$ModernGradle = Join-Path $CoreRoot "minecraft\1.21.1\gradlew.bat"

function Invoke-AuroraGradle {
    param(
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][string]$ProjectDirectory,
        [Parameter(Mandatory = $true)][string[]]$Tasks
    )

    Write-Host "[Aurora Core] $ProjectDirectory -> $($Tasks -join ' ')"
    & $Executable -p $ProjectDirectory @Tasks --no-daemon
    if ($LASTEXITCODE -ne 0) {
        throw "Gradle falhou em $ProjectDirectory com código $LASTEXITCODE."
    }
}

Invoke-AuroraGradle $SharedGradle $CoreRoot @("clean", "check")
Invoke-AuroraGradle $SharedGradle (Join-Path $CoreRoot "minecraft\legacy\forge-1.12.2") @("clean", "build")
Invoke-AuroraGradle $SharedGradle (Join-Path $CoreRoot "minecraft\1.16.5") @("clean", "build")
Invoke-AuroraGradle $SharedGradle (Join-Path $CoreRoot "minecraft\1.19.2") @("clean", "build")
Invoke-AuroraGradle $SharedGradle (Join-Path $CoreRoot "minecraft\1.20.1") @("clean", "build")
Invoke-AuroraGradle $ModernGradle (Join-Path $CoreRoot "minecraft\1.21.1") @("clean", "build")

Write-Host "[Aurora Core] Matriz de build concluída. Releases assinadas são imutáveis e não são sobrescritas por este script."

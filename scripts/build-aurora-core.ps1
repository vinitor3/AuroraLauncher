[CmdletBinding()]
param(
    [string]$StageDirectory = ".aurora\build-matrix\core\1.1.0"
)

$ErrorActionPreference = "Stop"
$RepositoryRoot = Split-Path -Parent $PSScriptRoot
$CoreRoot = Join-Path $RepositoryRoot "apps\aurora-core"
$SharedGradle = Join-Path $RepositoryRoot "apps\companion-mod\gradlew.bat"
$ModernGradle = Join-Path $CoreRoot "minecraft\1.21.1\gradlew.bat"
$Version = "1.1.0"
$StageRoot = if ([IO.Path]::IsPathRooted($StageDirectory)) {
    [IO.Path]::GetFullPath($StageDirectory)
} else {
    [IO.Path]::GetFullPath((Join-Path $RepositoryRoot $StageDirectory))
}

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

function Copy-StagedArtifact {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$Minecraft,
        [Parameter(Mandatory = $true)][string]$Loader
    )
    if (-not (Test-Path -LiteralPath $Source -PathType Leaf)) {
        throw "Artefato do Aurora Core ausente: $Source"
    }
    $destinationDirectory = Join-Path (Join-Path $StageRoot $Minecraft) $Loader
    New-Item -ItemType Directory -Force -Path $destinationDirectory | Out-Null
    $destination = Join-Path $destinationDirectory "aurora-core-$Loader-$Minecraft-$Version.jar"
    Copy-Item -LiteralPath $Source -Destination $destination -Force
}

Invoke-AuroraGradle $SharedGradle $CoreRoot @("clean", "check")
Invoke-AuroraGradle $SharedGradle (Join-Path $CoreRoot "minecraft\legacy\forge-1.12.2") @("clean", "build")
Invoke-AuroraGradle $SharedGradle (Join-Path $CoreRoot "minecraft\1.16.5") @("clean", "build")
Invoke-AuroraGradle $SharedGradle (Join-Path $CoreRoot "minecraft\1.19.2") @("clean", "build")
Invoke-AuroraGradle $SharedGradle (Join-Path $CoreRoot "minecraft\1.20.1") @("clean", "build")
Invoke-AuroraGradle $ModernGradle (Join-Path $CoreRoot "minecraft\1.21.1") @("clean", "build")

Copy-StagedArtifact (Join-Path $CoreRoot "minecraft\legacy\forge-1.12.2\build\libs\aurora-core-forge-1.12.2-$Version.jar") "1.12.2" "forge"
foreach ($minecraft in @("1.16.5", "1.19.2", "1.20.1", "1.21.1")) {
    foreach ($loader in @("fabric", "forge")) {
        Copy-StagedArtifact (Join-Path $CoreRoot "minecraft\$minecraft\$loader\build\libs\aurora-core-$Version.jar") $minecraft $loader
    }
}

Write-Host "[Aurora Core] Matriz de build concluída. Releases assinadas são imutáveis e não são sobrescritas por este script."
Write-Host "[Aurora Core] Artefatos de conferência: $StageRoot"

[CmdletBinding()]
param(
    [string]$StageDirectory = ".aurora\build-matrix\skins\0.2.4"
)

$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot 'gradle-java.ps1')
$RepositoryRoot = Split-Path -Parent $PSScriptRoot
$SkinsRoot = Join-Path $RepositoryRoot "apps\aurora-skins"
$SharedGradle = Join-Path $RepositoryRoot "apps\companion-mod\gradlew.bat"
$ModernGradle = Join-Path $RepositoryRoot "apps\aurora-core\minecraft\1.21.1\gradlew.bat"
$Version = "0.2.4"
$StageRoot = if ([IO.Path]::IsPathRooted($StageDirectory)) {
    [IO.Path]::GetFullPath($StageDirectory)
} else {
    [IO.Path]::GetFullPath((Join-Path $RepositoryRoot $StageDirectory))
}

function Invoke-AuroraGradle {
    param(
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][string]$ProjectDirectory,
        [Parameter(Mandatory = $true)][string[]]$Tasks,
        [ValidateSet(17, 21)][int]$JavaVersion = 17
    )

    Write-Host "[Aurora Skins] $ProjectDirectory -> $($Tasks -join ' ')"
    Invoke-AuroraGradleWithJava -JavaVersion $JavaVersion -Executable $Executable -Arguments (@('-p', $ProjectDirectory) + $Tasks + @('--no-daemon'))
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
        throw "Artefato do Aurora Skins ausente: $Source"
    }
    $destinationDirectory = Join-Path (Join-Path $StageRoot $Minecraft) $Loader
    New-Item -ItemType Directory -Force -Path $destinationDirectory | Out-Null
    $destination = Join-Path $destinationDirectory "aurora-skins-$Loader-$Minecraft-$Version.jar"
    Copy-Item -LiteralPath $Source -Destination $destination -Force
}

Invoke-AuroraGradle $SharedGradle $SkinsRoot @("clean", "build")
Invoke-AuroraGradle $SharedGradle (Join-Path $SkinsRoot "minecraft\legacy\forge-1.12.2") @("clean", "build")
Invoke-AuroraGradle $SharedGradle (Join-Path $SkinsRoot "minecraft\1.16.5") @("clean", "build")
Invoke-AuroraGradle $SharedGradle (Join-Path $SkinsRoot "minecraft\1.19.2") @("clean", "build")
Invoke-AuroraGradle $SharedGradle (Join-Path $SkinsRoot "minecraft\1.20.1") @("clean", "build")
Invoke-AuroraGradle $ModernGradle (Join-Path $SkinsRoot "minecraft\1.21.1") @("clean", "build") -JavaVersion 21

Copy-StagedArtifact (Join-Path $SkinsRoot "minecraft\legacy\forge-1.12.2\build\libs\aurora-skins-forge-1.12.2-$Version.jar") "1.12.2" "forge"
foreach ($minecraft in @("1.16.5", "1.19.2", "1.20.1", "1.21.1")) {
    foreach ($loader in @("fabric", "forge")) {
        Copy-StagedArtifact (Join-Path $SkinsRoot "minecraft\$minecraft\$loader\build\libs\aurora-skins-$Version.jar") $minecraft $loader
    }
}

Write-Host "[Aurora Skins] Matriz concluída: 1.12.2 Forge e Fabric/Forge de 1.16.5 a 1.21.1."
Write-Host "[Aurora Skins] Artefatos de conferência: $StageRoot"
Write-Host "[Aurora Skins] O script não sobrescreve artefatos versionados em releases."

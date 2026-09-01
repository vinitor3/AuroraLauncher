[CmdletBinding()]
param(
    [string]$StageDirectory = ".aurora\build-matrix\companion\0.2.2"
)

$ErrorActionPreference = "Stop"
$RepositoryRoot = Split-Path -Parent $PSScriptRoot
$CompanionRoot = Join-Path $RepositoryRoot "apps\companion-mod"
$Gradle = Join-Path $CompanionRoot "gradlew.bat"
$Version = "0.2.2"
$StageRoot = if ([IO.Path]::IsPathRooted($StageDirectory)) {
    [IO.Path]::GetFullPath($StageDirectory)
} else {
    [IO.Path]::GetFullPath((Join-Path $RepositoryRoot $StageDirectory))
}

function Invoke-CompanionGradle {
    param(
        [Parameter(Mandatory = $true)][string]$ProjectDirectory,
        [string[]]$Properties = @()
    )

    Write-Host "[Aurora Companion] $ProjectDirectory $($Properties -join ' ')"
    & $Gradle -p $ProjectDirectory clean build --no-daemon @Properties
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
        throw "Artefato do Companion ausente: $Source"
    }
    $destinationDirectory = Join-Path (Join-Path $StageRoot $Minecraft) $Loader
    New-Item -ItemType Directory -Force -Path $destinationDirectory | Out-Null
    $destination = Join-Path $destinationDirectory "aurora-companion-$Loader-$Minecraft-$Version.jar"
    Copy-Item -LiteralPath $Source -Destination $destination -Force
    Write-Host "[Aurora Companion] staged $destination"
}

$modernMatrix = @(
    @{
        Minecraft = "1.16.5"; Java = "8"; Forge = "1.16.5-36.2.42";
        FabricRange = "~1.16.5"; ForgeRange = "[1.16.5,1.17)"; ForgeLoaderRange = "[36,)"
    },
    @{
        Minecraft = "1.19.2"; Java = "17"; Forge = "1.19.2-43.4.16";
        FabricRange = "~1.19.2"; ForgeRange = "[1.19.2,1.20)"; ForgeLoaderRange = "[43,)"
    },
    @{
        Minecraft = "1.20.1"; Java = "17"; Forge = "1.20.1-47.4.23";
        FabricRange = "~1.20.1"; ForgeRange = "[1.20.1,1.21)"; ForgeLoaderRange = "[47,)"
    }
)

foreach ($entry in $modernMatrix) {
    $properties = @(
        "-Pminecraft_version=$($entry.Minecraft)",
        "-Pfabric_minecraft_range=$($entry.FabricRange)",
        "-Pforge_minecraft_range=$($entry.ForgeRange)",
        "-Pforge_loader_range=$($entry.ForgeLoaderRange)",
        "-Pjava_version=$($entry.Java)",
        "-Pcompile_java_version=$($entry.Java)",
        "-Pfabric_loader_version=0.16.14",
        "-Pforge_version=$($entry.Forge)"
    )
    Invoke-CompanionGradle $CompanionRoot $properties
    Copy-StagedArtifact (Join-Path $CompanionRoot "fabric\build\libs\aurora-companion-mod-$Version.jar") $entry.Minecraft "fabric"
    Copy-StagedArtifact (Join-Path $CompanionRoot "forge\build\libs\aurora-companion-mod-$Version.jar") $entry.Minecraft "forge"
}

Invoke-CompanionGradle (Join-Path $CompanionRoot "legacy\forge-1.12.2") @()
Copy-StagedArtifact (Join-Path $CompanionRoot "legacy\forge-1.12.2\build\libs\aurora-companion-forge-1.12.2-$Version.jar") "1.12.2" "forge"

Invoke-CompanionGradle (Join-Path $CompanionRoot "modern\1.21.1") @()
Copy-StagedArtifact (Join-Path $CompanionRoot "modern\1.21.1\build\libs\aurora-companion-fabric-1.21.1-$Version.jar") "1.21.1" "fabric"
Copy-StagedArtifact (Join-Path $CompanionRoot "modern\1.21.1\build\libs\aurora-companion-forge-1.21.1-$Version.jar") "1.21.1" "forge"

Write-Host "[Aurora Companion] Nove builds compiladas e isoladas em $StageRoot"
Write-Host "[Aurora Companion] Nenhum arquivo versionado em releases foi sobrescrito."

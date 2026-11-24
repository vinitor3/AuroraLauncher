[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][ValidateSet('core', 'skins')][string]$Module,
    [string]$StageDirectory,
    [string[]]$Minecraft
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'gradle-java.ps1')
$root = Split-Path -Parent $PSScriptRoot
$moduleRoot = Join-Path $root "apps/aurora-$Module"
$matrix = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'active-mod-matrix.json') | ConvertFrom-Json
$version = ($matrix.modules | Where-Object name -eq $Module).version
if (!$StageDirectory) { $StageDirectory = ".aurora/build-matrix/$Module/$version" }
$stage = if ([IO.Path]::IsPathRooted($StageDirectory)) { $StageDirectory } else { Join-Path $root $StageDirectory }
$standardWrapper = Join-Path $root 'tools/gradle/8.4/gradlew.bat'

function Invoke-ModuleGradle([string]$wrapper, [int]$jdk, [string]$directory, [string]$task) {
    Write-Host "[Aurora $Module] $directory ($task)"
    Invoke-AuroraGradleWithJava -JavaVersion $jdk -Executable $wrapper -Arguments @(
        '-p', $directory, 'clean', $task, '--no-daemon', '--no-parallel', '--max-workers=1'
    )
    if ($LASTEXITCODE -ne 0) { throw "Gradle falhou em $directory (exit $LASTEXITCODE)." }
}

# The shared API/runtime must be assembled before version-specific subprojects.
$sharedTask = if ($Module -eq 'core') { 'check' } else { 'build' }
Invoke-ModuleGradle $standardWrapper 17 $moduleRoot $sharedTask
$targets = @($matrix.targets)
foreach ($requested in $Minecraft) {
    if ($requested -notin $targets.minecraft) { throw "Minecraft fora da matriz: $requested" }
}
if ($Minecraft) { $targets = @($targets | Where-Object { $_.minecraft -in $Minecraft }) }
foreach ($group in ($targets | Group-Object { "$($_.minecraft)/$($_.buildProject)" })) {
    $profile = $group.Group[0]
    $gameVersion = $profile.minecraft
    $jdk = @($group.Group.buildJdk | Select-Object -Unique)
    if ($jdk.Count -ne 1) { throw "Alvos de $gameVersion precisam compartilhar o JDK de build." }
    $project = Join-Path $moduleRoot "minecraft/$gameVersion"
    $legacy = !$profile.buildProject -and !(Test-Path -LiteralPath $project -PathType Container)
    $task = 'build'
    if ($profile.buildProject) {
        $project = Join-Path $root $profile.buildProject
        $task = $profile.buildTasks.$Module
    } elseif ($legacy) {
        if ($group.Count -ne 1) { throw "Projeto de $gameVersion ausente: $project" }
        $project = Join-Path $moduleRoot "minecraft/legacy/$($profile.loader)-$gameVersion"
    }
    if (!(Test-Path -LiteralPath (Join-Path $project 'settings.gradle') -PathType Leaf)) {
        throw "Projeto Gradle ausente: $project"
    }
    if ($profile.gradleWrapper) {
        $wrapper = Join-Path $root $profile.gradleWrapper
    } else {
        $wrapper = Join-Path $root "apps/aurora-core/minecraft/$gameVersion/gradlew.bat"
        if (!(Test-Path -LiteralPath $wrapper -PathType Leaf)) { $wrapper = $standardWrapper }
    }
    Invoke-ModuleGradle $wrapper $jdk[0] $project $task
    foreach ($target in $group.Group) {
        $loader = $target.loader
        $filename = "aurora-$Module-$loader-$gameVersion-$version.jar"
        $source = if ($legacy) { Join-Path $project "build/libs/$filename" }
                  else { Join-Path $moduleRoot "minecraft/$gameVersion/$loader/build/libs/aurora-$Module-$version.jar" }
        $destination = Join-Path $stage "$gameVersion/$loader"
        New-Item -ItemType Directory -Force -Path $destination | Out-Null
        Copy-Item -LiteralPath $source -Destination (Join-Path $destination $filename) -Force
    }
}
Write-Host "[Aurora $Module] $($targets.Count) artefatos em $stage."

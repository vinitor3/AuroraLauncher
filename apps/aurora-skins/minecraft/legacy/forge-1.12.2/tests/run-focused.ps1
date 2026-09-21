param(
    [Parameter(Mandatory = $true)][string]$ForgeJar,
    [Parameter(Mandatory = $true)][string]$CoreJar,
    [string]$Java = 'java',
    [string]$Javac = 'javac'
)
$ErrorActionPreference = 'Stop'
$legacyRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$repoRoot = (Resolve-Path (Join-Path $legacyRoot '../../../../..')).Path
$testRoot = Join-Path $legacyRoot ('build/focused-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $testRoot -Force | Out-Null
# Copy read-only inputs locally: Java's ZIP provider may request write access to JARs.
Copy-Item -LiteralPath $ForgeJar -Destination (Join-Path $testRoot 'forge.jar')
Copy-Item -LiteralPath $CoreJar -Destination (Join-Path $testRoot 'core.jar')
$dependencies = (Join-Path $testRoot 'forge.jar') + ';' + (Join-Path $testRoot 'core.jar')
$production = @(Get-ChildItem -LiteralPath (Join-Path $legacyRoot 'src/main/java') -Recurse -Filter '*.java' | ForEach-Object FullName)
$shared = @(Get-ChildItem -LiteralPath (Join-Path $repoRoot 'apps/aurora-skins/core/src/main/java') -Recurse -Filter '*.java' | ForEach-Object FullName)
$coreUi = Join-Path $repoRoot 'apps/aurora-core/minecraft/legacy/forge-1.12.2/src/main/java/com/aurora/core/minecraft/legacy/LegacyAuroraUi.java'
& $Javac --release 8 -encoding UTF-8 -cp $dependencies -d (Join-Path $testRoot 'production') @production @shared $coreUi
if ($LASTEXITCODE -ne 0) { throw 'Compilation against the real Forge/Core JARs failed.' }
$fixtures = @(Get-ChildItem -LiteralPath (Join-Path $PSScriptRoot 'java') -Recurse -Filter '*.java' | ForEach-Object FullName)
$tested = @($production | Where-Object { $_ -notmatch 'AuroraSkins(LegacyModule|ForgeLegacy)\.java$' })
$corePlatform = Join-Path $repoRoot 'apps/aurora-core/minecraft/legacy/forge-1.12.2/src/main/java/com/aurora/core/minecraft/legacy/LegacyMinecraftPlatform.java'
& $Javac --release 8 -encoding UTF-8 -cp $dependencies -d (Join-Path $testRoot 'tests') @tested @shared $coreUi $corePlatform @fixtures
if ($LASTEXITCODE -ne 0) { throw 'Fixture compilation failed.' }
$testClasspath = (Join-Path $testRoot 'tests') + ';' + $dependencies
& $Java '-Djava.awt.headless=true' -cp $testClasspath com.aurora.skins.legacy.LegacyRegressionTest (Join-Path $testRoot 'store')
if ($LASTEXITCODE -ne 0) { throw 'Legacy regression checks failed.' }
& $Java '-Djava.awt.headless=true' -cp $testClasspath com.aurora.skins.legacy.LegacyCoreOwnershipTest (Join-Path $testRoot 'ownership-store')
if ($LASTEXITCODE -ne 0) { throw 'Core/Skins ownership regression checks failed.' }
& $Java '-Djava.awt.headless=true' -cp $testClasspath com.aurora.skins.legacy.LegacyCatalogGridTest (Join-Path $testRoot 'catalog-fixtures')
if ($LASTEXITCODE -ne 0) { throw 'Legacy catalog grid checks failed.' }
Write-Output "Focused classes and evidence: $testRoot (no Gradle, no release JAR)."

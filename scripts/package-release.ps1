[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$runId = (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [Guid]::NewGuid().ToString('N').Substring(0, 8)
$runDirectory = Join-Path $repositoryRoot ".aurora\release-runs\$runId"
New-Item -ItemType Directory -Path $runDirectory | Out-Null
$logPath = Join-Path $runDirectory 'build.log'

function Invoke-Logged {
    param([string]$Label, [string]$Executable, [string[]]$Arguments)
    Write-Host "[Aurora release] $Label"
    $previousPreference = $ErrorActionPreference
    try {
        # Windows PowerShell reports native stderr as ErrorRecord even on success.
        $ErrorActionPreference = 'Continue'
        & $Executable @Arguments >> $logPath 2>&1
        $code = $LASTEXITCODE
    } finally { $ErrorActionPreference = $previousPreference }
    if ($code -ne 0) {
        throw "$Label falhou (codigo $code). Log local: $logPath"
    }
}

function Read-Git {
    param([string[]]$Arguments)
    $output = & git -C $repositoryRoot @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) { throw 'Nao foi possivel consultar o estado Git.' }
    return @($output)
}

function Write-NewUtf8 {
    param([string]$Path, [string]$Content)
    $stream = [IO.File]::Open($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    $writer = New-Object IO.StreamWriter($stream, (New-Object Text.UTF8Encoding($false)))
    try { $writer.Write($Content) } finally { $writer.Dispose() }
}

function Get-AuroraFileSha256 {
    param([Parameter(Mandatory = $true)][string]$Path)
    # Available even when Windows PowerShell cannot auto-load Get-FileHash.
    $stream = [IO.File]::OpenRead($Path)
    $sha256 = [Security.Cryptography.SHA256]::Create()
    try {
        return [BitConverter]::ToString($sha256.ComputeHash($stream)).Replace('-', '').ToLowerInvariant()
    } finally {
        $sha256.Dispose()
        $stream.Dispose()
    }
}

Push-Location -LiteralPath $repositoryRoot
try {
    # Fail before the official build recompiles the full 27-JAR matrix.
    Invoke-Logged 'Preflight inicial' 'npm.cmd' @('run', 'release:preflight')
    $config = Get-Content -Raw -LiteralPath 'apps/desktop/src-tauri/tauri.conf.json' | ConvertFrom-Json
    $version = [string]$config.version
    $productName = [string]$config.productName
    if ($version -notmatch '^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$') { throw 'Versao de release invalida.' }
    if ($productName.IndexOfAny([IO.Path]::GetInvalidFileNameChars()) -ge 0) { throw 'Nome do produto invalido para artefato.' }
    $artifactName = "${productName}_${version}_x64-setup.exe"
    $releaseDirectory = Join-Path $repositoryRoot 'releases'
    $outputNames = @($artifactName, "$artifactName.sha256", "$artifactName.manifest.json")
    foreach ($name in $outputNames) {
        if (Test-Path -LiteralPath (Join-Path $releaseDirectory $name)) {
            throw "Release imutavel ja existe: $name. Use uma nova versao."
        }
    }
    $commit = (Read-Git @('rev-parse', 'HEAD') | Select-Object -First 1).Trim()
    $branch = (Read-Git @('branch', '--show-current') | Select-Object -First 1).Trim()

    # Cargo resolves CARGO_TARGET_DIR and local/global Cargo configuration itself.
    Push-Location -LiteralPath (Join-Path $repositoryRoot 'apps/desktop/src-tauri')
    try {
        $metadataPath = Join-Path $runDirectory 'cargo-metadata.json'
        $previousPreference = $ErrorActionPreference
        try {
            $ErrorActionPreference = 'Continue'
            & cargo metadata --format-version 1 --no-deps --locked 1> $metadataPath 2>> $logPath
            $metadataExit = $LASTEXITCODE
        } finally { $ErrorActionPreference = $previousPreference }
        if ($metadataExit -ne 0) { throw "Falha ao localizar o target Cargo. Log: $logPath" }
        $targetDirectory = (Get-Content -Raw -LiteralPath $metadataPath | ConvertFrom-Json).target_directory
    } finally { Pop-Location }

    $buildStartedUtc = [DateTime]::UtcNow
    Invoke-Logged 'Build oficial (27 mods e instalador; log em arquivo)' 'npm.cmd' @('run', 'desktop:build')
    Invoke-Logged 'Preflight final' 'npm.cmd' @('run', 'release:preflight')
    $currentCommit = (Read-Git @('rev-parse', 'HEAD') | Select-Object -First 1).Trim()
    $currentBranch = (Read-Git @('branch', '--show-current') | Select-Object -First 1).Trim()
    if ($currentCommit -ne $commit -or $currentBranch -ne $branch) { throw 'O checkout mudou durante o build; artefato nao publicado.' }

    # Native and explicitly targeted builds use different subdirectories.
    $bundleRoots = @(Join-Path $targetDirectory 'release/bundle/nsis')
    if (Test-Path -LiteralPath $targetDirectory -PathType Container) {
        $bundleRoots += @(Get-ChildItem -LiteralPath $targetDirectory -Directory |
            ForEach-Object { Join-Path $_.FullName 'release/bundle/nsis' })
    }
    $installers = @($bundleRoots | Select-Object -Unique | ForEach-Object {
        $candidate = Join-Path $_ $artifactName
        if (Test-Path -LiteralPath $candidate -PathType Leaf) {
            $item = Get-Item -LiteralPath $candidate
            if ($item.LastWriteTimeUtc -ge $buildStartedUtc) { $item }
        }
    })
    if ($installers.Count -ne 1) { throw "Esperado um instalador novo de v$version; encontrados $($installers.Count). Target: $targetDirectory" }
    $installer = $installers[0]
    if ($installer.Length -eq 0 -or $installer.VersionInfo.ProductVersion -ne $version -or $installer.VersionInfo.ProductName -ne $productName) {
        throw 'O instalador nao corresponde ao produto/versao configurados; artefato nao publicado.'
    }

    $modVersions = [ordered]@{}
    $modArtifacts = @()
    foreach ($mod in @(
        @{ Name = 'core'; Script = 'build-aurora-core.ps1'; VersionedRelease = $true },
        @{ Name = 'skins'; Script = 'build-aurora-skins.ps1'; VersionedRelease = $true },
        @{ Name = 'companion'; Script = 'build-companion-matrix.ps1'; VersionedRelease = $false }
    )) {
        $scriptText = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot $mod.Script)
        $match = [regex]::Match($scriptText, '(?m)^\$Version\s*=\s*"([0-9A-Za-z.+-]+)"\s*$')
        if (-not $match.Success) { throw "Versao do mod nao encontrada: $($mod.Name)" }
        $modVersion = $match.Groups[1].Value
        $modVersions[$mod.Name] = $modVersion
        $stageRoot = Join-Path $repositoryRoot ".aurora/build-matrix/$($mod.Name)/$modVersion"
        $jars = @(Get-ChildItem -LiteralPath $stageRoot -Recurse -File -Filter '*.jar')
        if ($jars.Count -ne 9) { throw "Matriz de $($mod.Name) incompleta para o manifesto." }
        $modReleaseRoot = Join-Path $releaseDirectory $mod.Name
        if ($mod.VersionedRelease) { $modReleaseRoot = Join-Path $modReleaseRoot $modVersion }
        foreach ($jar in $jars | Sort-Object FullName) {
            $relative = $jar.FullName.Substring($stageRoot.Length).TrimStart('\', '/')
            $publishedJar = Join-Path $modReleaseRoot $relative
            $modArtifacts += [ordered]@{
                mod = $mod.Name
                version = $modVersion
                path = $publishedJar.Substring($repositoryRoot.Length + 1).Replace('\', '/')
                sha256 = Get-AuroraFileSha256 $publishedJar
            }
        }
    }

    $stagedInstaller = Join-Path $runDirectory $artifactName
    [IO.File]::Copy($installer.FullName, $stagedInstaller, $false)
    $hash = Get-AuroraFileSha256 $stagedInstaller
    $manifest = [ordered]@{
        schemaVersion = 1
        product = $productName
        version = $version
        artifact = $artifactName
        sizeBytes = (Get-Item -LiteralPath $stagedInstaller).Length
        sha256 = $hash
        createdAtUtc = [DateTime]::UtcNow.ToString('o')
        source = [ordered]@{ commit = $commit; branch = $branch; clean = $true }
        buildCommand = 'npm run desktop:build'
        validation = [ordered]@{ releasePreflight = 'passed'; modSourceMatrix = '27/27'; runtime = 'NAO VERIFICADO'; codeSigning = 'NAO VERIFICADO' }
        modVersions = $modVersions
        modArtifacts = $modArtifacts
    }
    Write-NewUtf8 (Join-Path $runDirectory "$artifactName.sha256") "$hash  $artifactName`n"
    Write-NewUtf8 (Join-Path $runDirectory "$artifactName.manifest.json") (($manifest | ConvertTo-Json -Depth 8) + "`n")

    # CreateNew semantics: never overwrite any previous release, even in a race.
    foreach ($name in $outputNames) {
        [IO.File]::Copy((Join-Path $runDirectory $name), (Join-Path $releaseDirectory $name), $false)
    }
    Write-Host "[Aurora release] Instalador: $(Join-Path $releaseDirectory $artifactName)"
    Write-Host "[Aurora release] SHA-256: $hash"
    Write-Host '[Aurora release] Manifesto gerado. Runtime: NAO VERIFICADO.'
    Write-Host "[Aurora release] Log local: $logPath"
} catch {
    if (Test-Path -LiteralPath $logPath -PathType Leaf) {
        Get-Content -LiteralPath $logPath -Tail 45 | ForEach-Object { Write-Host $_ }
    }
    Write-Host "Empacotamento interrompido: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
} finally { Pop-Location }

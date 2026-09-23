[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName System.IO.Compression.FileSystem
Add-Type -AssemblyName System.IO.Compression
$repository = Split-Path -Parent $PSScriptRoot
$fixtureRoot = Join-Path $repository ('.aurora/matrix-gate-tests/' + [Guid]::NewGuid().ToString('N'))
$utf8 = New-Object Text.UTF8Encoding($false)

function Write-FixtureJar {
    param([string]$Path, [hashtable]$Entries)
    New-Item -ItemType Directory -Force -Path ([IO.Path]::GetDirectoryName($Path)) | Out-Null
    $archive = [IO.Compression.ZipFile]::Open($Path, [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($name in @($Entries.Keys | Sort-Object)) {
            $entry = $archive.CreateEntry($name)
            $stream = $entry.Open()
            try {
                $bytes = $utf8.GetBytes($Entries[$name])
                $stream.Write($bytes, 0, $bytes.Length)
            } finally { $stream.Dispose() }
        }
    } finally { $archive.Dispose() }
}

foreach ($scenario in @('forbidden-companion', 'source-mismatch', 'unexpected-target', 'extra-companion')) {
    $root = Join-Path $fixtureRoot $scenario
    $scripts = Join-Path $root 'scripts'
    New-Item -ItemType Directory -Force -Path $scripts | Out-Null
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'verify-mod-release-matrix.ps1') -Destination $scripts
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'skins-feature-contract.ps1') -Destination $scripts
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'skins-feature-contract.json') -Destination $scripts
    $matrix = @{
        schemaVersion = 1
        modules = @(@{ name = 'core'; version = 'fixture'; forbiddenEntryPrefixes = @('com/aurora/mod/') })
        targets = @(@{ minecraft = '1.20.1'; loader = 'fabric' })
    }
    [IO.File]::WriteAllText((Join-Path $scripts 'active-mod-matrix.json'), ($matrix | ConvertTo-Json -Depth 5), $utf8)
    $relative = '1.20.1/fabric/aurora-core-fabric-1.20.1-fixture.jar'
    $stage = Join-Path $root ".aurora/build-matrix/core/fixture/$relative"
    $release = Join-Path $root "releases/core/fixture/$relative"
    $entries = @{ 'pack.mcmeta' = '{"pack":{"pack_format":15,"description":"QA fixture"}}'; 'com/aurora/core/Marker.class' = 'same source' }
    $expected = switch ($scenario) {
        'forbidden-companion' {
            $entries['com/aurora/mod/AuroraCompanion.class'] = 'retired mod fixture'
            'pacote proibido'
        }
        'source-mismatch' { 'divergente' }
        'unexpected-target' { 'Alvo ativo ausente' }
        'extra-companion' { 'esperado: 1' }
    }
    Write-FixtureJar $release $entries
    if ($scenario -eq 'source-mismatch') { $entries['com/aurora/core/Marker.class'] = 'changed source' }
    if ($scenario -eq 'unexpected-target') { $stage = Join-Path ([IO.Path]::GetDirectoryName($stage)) 'wrong-target.jar' }
    Write-FixtureJar $stage $entries
    if ($scenario -eq 'extra-companion') {
        Write-FixtureJar (Join-Path ([IO.Path]::GetDirectoryName($stage)) 'aurora-companion.jar') $entries
    }
    $log = Join-Path $root 'gate.log'
    $previousPreference = $ErrorActionPreference
    try {
        # Each fixture intentionally fails; native stderr is expected evidence.
        $ErrorActionPreference = 'Continue'
        & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $scripts 'verify-mod-release-matrix.ps1') -SkipBuild *> $log
        $exit = $LASTEXITCODE
    } finally { $ErrorActionPreference = $previousPreference }
    $output = Get-Content -Raw -LiteralPath $log
    if ($exit -eq 0 -or $output -notmatch [regex]::Escape($expected)) {
        throw "Gate accepted or misclassified scenario '$scenario' (exit $exit). See $log"
    }
    Write-Host "PASS: $scenario rejected by the unchanged source digest / active artifact gate."
}
Write-Host '4 isolated matrix-gate fixtures passed; no versioned artifact was modified.'

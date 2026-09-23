[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$RepositoryRoot = Split-Path -Parent $PSScriptRoot
Add-Type -AssemblyName System.IO.Compression.FileSystem
. (Join-Path $PSScriptRoot "skins-feature-contract.ps1")

function Assert-RejectedSkinsArchive {
    param([IO.Compression.ZipArchive]$Archive, [string]$Expected)
    $failure = $null
    try { Assert-AuroraSkinsFeatures -Archive $Archive -Minecraft "1.20.1" }
    catch { $failure = $_.Exception.Message }
    if ($null -eq $failure -or -not $failure.Contains($Expected)) {
        throw "Expected missing feature '$Expected'; received '$failure'."
    }
}

foreach ($fixture in @(
    @{ Version = "0.2.5"; Missing = "SkinSyncProtocol.class" },
    @{ Version = "0.3.0"; Missing = "CatalogGridState.class" }
)) {
    $path = Join-Path $RepositoryRoot "releases/skins/$($fixture.Version)/1.20.1/forge/aurora-skins-forge-1.20.1-$($fixture.Version).jar"
    $archive = [IO.Compression.ZipFile]::OpenRead($path)
    try { Assert-RejectedSkinsArchive -Archive $archive -Expected $fixture.Missing }
    finally { $archive.Dispose() }
}

$matrix = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot "active-mod-matrix.json") | ConvertFrom-Json
$version = ($matrix.modules | Where-Object name -eq "skins").version
foreach ($target in $matrix.targets) {
    $path = Join-Path $RepositoryRoot "releases/skins/$version/$($target.minecraft)/$($target.loader)/aurora-skins-$($target.loader)-$($target.minecraft)-$version.jar"
    $archive = [IO.Compression.ZipFile]::OpenRead($path)
    try {
        try { Assert-AuroraSkinsFeatures -Archive $archive -Minecraft $target.minecraft }
        catch { throw "$($target.minecraft)/$($target.loader): $($_.Exception.Message)" }
    }
    finally { $archive.Dispose() }
}

# Removing a caller while retaining feature classes must also be rejected.
# Copy into a writable memory ZIP; immutable release fixtures are never modified.
$current = Join-Path $RepositoryRoot "releases/skins/$version/1.20.1/forge/aurora-skins-forge-1.20.1-$version.jar"
foreach ($mutation in @(
    @{ Resource = "com/aurora/skins/minecraft/ui/AuroraSkinsScreen.class"; Marker = "renderCard" },
    @{ Resource = "com/aurora/skins/catalog/AuroraCatalogProvider.class"; Marker = "previewCache" },
    @{ Resource = "aurora_skins.mixins.json"; Marker = "PlayerInfoMixin" }
)) {
    $memory = New-Object IO.MemoryStream
    [byte[]]$original = [IO.File]::ReadAllBytes($current)
    $memory.Write($original, 0, $original.Length)
    $memory.Position = 0
    $archive = New-Object IO.Compression.ZipArchive($memory, [IO.Compression.ZipArchiveMode]::Update, $false)
    try {
        $entry = $archive.GetEntry($mutation.Resource)
        $content = New-Object IO.MemoryStream
        $stream = $entry.Open()
        try { $stream.CopyTo($content); [byte[]]$bytes = $content.ToArray() }
        finally { $stream.Dispose(); $content.Dispose() }
        [byte[]]$marker = [Text.Encoding]::UTF8.GetBytes($mutation.Marker)
        $matches = 0
        for ($offset = 0; $offset -le $bytes.Length - $marker.Length; $offset++) {
            $same = $true
            for ($index = 0; $index -lt $marker.Length; $index++) {
                if ($bytes[$offset + $index] -ne $marker[$index]) { $same = $false; break }
            }
            if ($same) {
                $matches++
                for ($index = 0; $index -lt $marker.Length; $index++) { $bytes[$offset + $index] = 95 }
            }
        }
        if ($matches -eq 0) { throw "Fixture marker missing: $($mutation.Marker)" }
        $entry.Delete()
        $replacement = $archive.CreateEntry($mutation.Resource)
        $stream = $replacement.Open()
        try { $stream.Write($bytes, 0, $bytes.Length) }
        finally { $stream.Dispose() }
        Assert-RejectedSkinsArchive -Archive $archive -Expected $mutation.Marker
    } finally {
        $archive.Dispose()
        $memory.Dispose()
    }
}
Write-Host "[Aurora Skins] Feature contract: 9 active JARs accepted; 2 historical regressions and 3 disconnected features rejected."

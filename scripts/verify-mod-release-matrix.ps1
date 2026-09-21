[CmdletBinding()]
param(
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$RepositoryRoot = Split-Path -Parent $PSScriptRoot
$Matrix = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot "active-mod-matrix.json") | ConvertFrom-Json
Add-Type -AssemblyName System.IO.Compression.FileSystem
function Get-AuroraSha256Bytes {
    param([Parameter(Mandatory = $true)][byte[]]$Bytes)

    $sha256 = [Security.Cryptography.SHA256]::Create()
    try {
        return [BitConverter]::ToString($sha256.ComputeHash($Bytes)).Replace("-", "")
    } finally {
        $sha256.Dispose()
    }
}

function Normalize-AuroraByteSequence {
    param(
        [Parameter(Mandatory = $true)][byte[]]$Bytes,
        [Parameter(Mandatory = $true)][byte[]]$Sequence
    )

    if ($Sequence.Length -eq 0 -or $Bytes.Length -lt $Sequence.Length) { return }
    for ($offset = 0; $offset -le $Bytes.Length - $Sequence.Length; $offset++) {
        $matches = $true
        for ($index = 0; $index -lt $Sequence.Length; $index++) {
            if ($Bytes[$offset + $index] -ne $Sequence[$index]) {
                $matches = $false
                break
            }
        }
        if ($matches) {
            for ($index = 0; $index -lt $Sequence.Length; $index++) {
                $Bytes[$offset + $index] = 35
            }
            $offset += $Sequence.Length - 1
        }
    }
}

function Get-AuroraJarContentDigest {
    param([Parameter(Mandatory = $true)][string]$Path)

    $archive = [IO.Compression.ZipFile]::OpenRead($Path)
    try {
        $injectDirectories = @($archive.Entries |
            Where-Object { $_.FullName -match '^architectury_inject_[^/]+/$' } |
            ForEach-Object { $_.FullName.TrimEnd('/') })
        $records = foreach ($entry in ($archive.Entries | Sort-Object FullName)) {
            if ($entry.FullName.EndsWith('/')) { continue }
            $memory = New-Object IO.MemoryStream
            $stream = $entry.Open()
            try {
                $stream.CopyTo($memory)
                [byte[]]$bytes = $memory.ToArray()
            } finally {
                $stream.Dispose()
                $memory.Dispose()
            }
            foreach ($directory in $injectDirectories) {
                Normalize-AuroraByteSequence $bytes ([Text.Encoding]::UTF8.GetBytes($directory))
            }
            $normalizedName = $entry.FullName -replace '^architectury_inject_[^/]+/', 'architectury_inject_normalized/'
            "$normalizedName`t$($bytes.Length)`t$(Get-AuroraSha256Bytes $bytes)"
        }
        [byte[]]$recordBytes = [Text.Encoding]::UTF8.GetBytes(($records -join "`n"))
        return Get-AuroraSha256Bytes $recordBytes
    } finally {
        $archive.Dispose()
    }
}

if (-not $SkipBuild) {
    # Serial builds share the Loom cache and must never run concurrently.
    foreach ($module in $Matrix.modules) {
        & (Join-Path $PSScriptRoot $module.buildScript)
        if ($LASTEXITCODE -ne 0) { throw "A matriz de $($module.name) falhou." }
    }
}

function Compare-AuroraMatrix {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$StageDirectory,
        [Parameter(Mandatory = $true)][string]$ReleaseDirectory,
        [Parameter(Mandatory = $true)][string]$Module,
        [Parameter(Mandatory = $true)][string]$Version,
        [string[]]$ForbiddenEntryPrefixes = @()
    )

    $stageRoot = [IO.Path]::GetFullPath((Join-Path $RepositoryRoot $StageDirectory)).TrimEnd('\')
    $releaseRoot = [IO.Path]::GetFullPath((Join-Path $RepositoryRoot $ReleaseDirectory)).TrimEnd('\')
    $staged = @(Get-ChildItem -LiteralPath $stageRoot -Recurse -File -Filter *.jar)
    $expectedCount = @($Matrix.targets).Count
    if ($staged.Count -ne $expectedCount) { throw "$Name possui $($staged.Count) builds em staging; esperado: $expectedCount." }

    foreach ($target in $Matrix.targets) {
        $expected = Join-Path $stageRoot "$($target.minecraft)/$($target.loader)/aurora-$Module-$($target.loader)-$($target.minecraft)-$Version.jar"
        if (-not (Test-Path -LiteralPath $expected -PathType Leaf)) { throw "Alvo ativo ausente: $expected" }
    }

    foreach ($artifact in $staged) {
        $relative = $artifact.FullName.Substring($stageRoot.Length).TrimStart('\')
        $release = Join-Path $releaseRoot $relative
        if (-not (Test-Path -LiteralPath $release -PathType Leaf)) {
            throw "$Name não publicado para $relative"
        }
        # Architectury gera um namespace de injeção aleatório a cada build. A
        # comparação normaliza somente esse identificador e ignora metadados ZIP;
        # nomes, tamanhos e SHA-256 de todos os conteúdos restantes continuam
        # fazendo parte do digest.
        $stagedHash = Get-AuroraJarContentDigest $artifact.FullName
        $releaseHash = Get-AuroraJarContentDigest $release
        if ($stagedHash -ne $releaseHash) {
            throw "$Name divergente em $relative. A release não contém o código-fonte compilado atual."
        }

        $archive = [IO.Compression.ZipFile]::OpenRead($artifact.FullName)
        try {
            $entryNames = @($archive.Entries | ForEach-Object { $_.FullName.Replace('\', '/') })
            if ($entryNames -notcontains "pack.mcmeta") {
                throw "$Name não contém pack.mcmeta em ${relative}; o Forge descartaria seus recursos."
            }
            if ($ForbiddenEntryPrefixes.Count -gt 0) {
                foreach ($entryName in $entryNames) {
                    foreach ($prefix in $ForbiddenEntryPrefixes) {
                        if ($entryName.StartsWith($prefix, [StringComparison]::Ordinal)) {
                            throw "$Name contém pacote proibido em ${relative}: $entryName"
                        }
                    }
                }
            }
        }
        finally {
            $archive.Dispose()
        }
    }
    Write-Host "[Aurora release matrix] ${Name}: $expectedCount/$expectedCount artefatos correspondem ao código atual."
}

foreach ($module in $Matrix.modules) {
    Compare-AuroraMatrix -Name "Aurora $($module.name)" -Module $module.name -Version $module.version `
        -StageDirectory ".aurora/build-matrix/$($module.name)/$($module.version)" `
        -ReleaseDirectory "releases/$($module.name)/$($module.version)" `
        -ForbiddenEntryPrefixes $module.forbiddenEntryPrefixes
}
& node (Join-Path $PSScriptRoot "verify-core-release-signatures.mjs")
if ($LASTEXITCODE -ne 0) { throw "As assinaturas do Aurora Core falharam." }
$total = @($Matrix.modules).Count * @($Matrix.targets).Count
Write-Host "[Aurora release matrix] Matriz ativa: $total/$total JARs."

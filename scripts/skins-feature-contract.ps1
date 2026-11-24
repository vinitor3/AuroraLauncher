$AuroraSkinsFeatureContract = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot "skins-feature-contract.json") | ConvertFrom-Json
if ($AuroraSkinsFeatureContract.schemaVersion -ne 1 -or @($AuroraSkinsFeatureContract.requirements).Count -eq 0) {
    throw "Contrato de funcionalidades do Aurora Skins ausente ou invalido."
}

function Read-AuroraSkinsFeatureEntry {
    param([IO.Compression.ZipArchiveEntry]$Entry)
    $memory = New-Object IO.MemoryStream
    $stream = $Entry.Open()
    try { $stream.CopyTo($memory); return ,$memory.ToArray() }
    finally { $stream.Dispose(); $memory.Dispose() }
}

function Read-AuroraSkinsFeatureResources {
    param([IO.Compression.ZipArchive]$Archive, [hashtable]$Resources)
    if ($null -ne $Archive.GetEntry("com/aurora/skins/catalog/BuiltinSkinCatalog.class")) {
        throw "O JAR contém o antigo catálogo de skins Aurora Studio."
    }
    foreach ($requirement in $AuroraSkinsFeatureContract.requirements) {
        $entry = $Archive.GetEntry($requirement.resource)
        if ($null -ne $entry -and -not $Resources.ContainsKey($requirement.resource)) {
            $Resources[$requirement.resource] = Read-AuroraSkinsFeatureEntry $entry
        }
    }
    # 1.21 keeps skins-core in a loader-declared nested JAR. Undeclared nested
    # files must not satisfy a runtime dependency or hide an incomplete build.
    $metadata = $Archive.GetEntry("fabric.mod.json")
    $pathKey = "file"
    if ($null -eq $metadata) {
        $metadata = $Archive.GetEntry("META-INF/jarjar/metadata.json")
        $pathKey = "path"
    }
    if ($null -eq $metadata) { return }
    $document = [Text.Encoding]::UTF8.GetString((Read-AuroraSkinsFeatureEntry $metadata)) | ConvertFrom-Json
    foreach ($nested in $document.jars) {
        $entry = $Archive.GetEntry($nested.$pathKey)
        if ($null -eq $entry) { throw "JAR declarado ausente: $($nested.$pathKey)" }
        [byte[]]$bytes = Read-AuroraSkinsFeatureEntry $entry
        $memory = New-Object IO.MemoryStream
        $memory.Write($bytes, 0, $bytes.Length)
        $memory.Position = 0
        $child = New-Object IO.Compression.ZipArchive($memory, [IO.Compression.ZipArchiveMode]::Read, $false)
        try { Read-AuroraSkinsFeatureResources -Archive $child -Resources $Resources }
        finally { $child.Dispose(); $memory.Dispose() }
    }
}

function Assert-AuroraSkinsFeatures {
    param(
        [Parameter(Mandatory = $true)][IO.Compression.ZipArchive]$Archive,
        [Parameter(Mandatory = $true)][string]$Minecraft
    )

    $scope = if ($Minecraft -eq "1.12.2") { "legacy" } else { "modern" }
    $resources = @{}
    Read-AuroraSkinsFeatureResources -Archive $Archive -Resources $resources
    foreach ($requirement in $AuroraSkinsFeatureContract.requirements) {
        if ($requirement.scope -notin @("all", "modern", "legacy")) { throw "Escopo de funcionalidade invalido: $($requirement.scope)" }
        if ($requirement.scope -ne "all" -and $requirement.scope -ne $scope) { continue }
        if (-not $resources.ContainsKey($requirement.resource)) { throw "Funcionalidade Aurora Skins ausente: $($requirement.resource)" }
        if (@($requirement.markers).Count -eq 0) { continue }
        # JVM constant-pool names are preserved across Minecraft remapping.
        # These markers also require the UI/cache/network callers to be linked,
        # rather than accepting unused feature classes accidentally left in a JAR.
        $contents = [Text.Encoding]::UTF8.GetString($resources[$requirement.resource])
        foreach ($marker in $requirement.markers) {
            if (-not $contents.Contains($marker)) {
                throw "Ligacao de funcionalidade Aurora Skins ausente: $($requirement.resource): $marker"
            }
        }
    }
}

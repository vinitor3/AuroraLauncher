[CmdletBinding()]
param(
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$RepositoryRoot = Split-Path -Parent $PSScriptRoot

if (-not $SkipBuild) {
    & (Join-Path $PSScriptRoot "build-aurora-core.ps1")
    if ($LASTEXITCODE -ne 0) { throw "A matriz do Aurora Core falhou." }
    & (Join-Path $PSScriptRoot "build-aurora-skins.ps1")
    if ($LASTEXITCODE -ne 0) { throw "A matriz do Aurora Skins falhou." }
    & (Join-Path $PSScriptRoot "build-companion-matrix.ps1")
    if ($LASTEXITCODE -ne 0) { throw "A matriz do Aurora Companion falhou." }
}

function Compare-AuroraMatrix {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$StageDirectory,
        [Parameter(Mandatory = $true)][string]$ReleaseDirectory
    )

    $stageRoot = [IO.Path]::GetFullPath((Join-Path $RepositoryRoot $StageDirectory)).TrimEnd('\')
    $releaseRoot = [IO.Path]::GetFullPath((Join-Path $RepositoryRoot $ReleaseDirectory)).TrimEnd('\')
    $staged = @(Get-ChildItem -LiteralPath $stageRoot -Recurse -File -Filter *.jar)
    if ($staged.Count -ne 9) { throw "$Name possui $($staged.Count) builds em staging; esperado: 9." }

    foreach ($artifact in $staged) {
        $relative = $artifact.FullName.Substring($stageRoot.Length).TrimStart('\')
        $release = Join-Path $releaseRoot $relative
        if (-not (Test-Path -LiteralPath $release -PathType Leaf)) {
            throw "$Name não publicado para $relative"
        }
        $stagedHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $artifact.FullName).Hash
        $releaseHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $release).Hash
        if ($stagedHash -ne $releaseHash) {
            throw "$Name divergente em $relative. A release não contém o código-fonte compilado atual."
        }
    }
    Write-Host "[Aurora release matrix] ${Name}: 9/9 artefatos correspondem ao código atual."
}

Compare-AuroraMatrix "Aurora Core" ".aurora\build-matrix\core" "releases\core\1.1.0"
Compare-AuroraMatrix "Aurora Skins" ".aurora\build-matrix\skins" "releases\skins\0.1.0"
Compare-AuroraMatrix "Aurora Companion" ".aurora\build-matrix\companion" "releases\companion"

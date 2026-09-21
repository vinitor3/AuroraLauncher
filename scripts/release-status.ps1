[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repositoryRoot = Split-Path -Parent $PSScriptRoot

function Read-Git {
    param([string[]]$Arguments)
    $output = & git -C $repositoryRoot @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) { throw 'Nao foi possivel consultar o estado Git.' }
    return @($output)
}

$currentHead = (Read-Git @('rev-parse', 'HEAD') | Select-Object -First 1).Trim()
$currentBranch = (Read-Git @('branch', '--show-current') | Select-Object -First 1).Trim()
$worktreePaths = @(Read-Git @('-c', 'core.quotepath=false', 'worktree', 'list', '--porcelain') |
    Where-Object { $_ -like 'worktree *' } |
    ForEach-Object { $_.Substring(9) })
$sourcePaths = @('apps/desktop', 'apps/aurora-core', 'apps/aurora-skins', 'tools/gradle',
    'apps/edge-proxy', 'firebase', 'functions', 'scripts', 'package.json')
# Historical sibling worktrees may still contain the retired module. Keep their
# pending work visible without making that module part of the active build.
$sourcePaths += 'apps/companion-mod'

Write-Host "Aurora release status | $currentBranch | $($currentHead.Substring(0, 12))"
$pending = 0
foreach ($path in $worktreePaths) {
    if (-not (Test-Path -LiteralPath $path -PathType Container)) {
        Write-Host "INDISPONIVEL | $path"
        $pending++
        continue
    }
    $branch = (Read-Git @('-C', $path, 'branch', '--show-current') | Select-Object -First 1)
    if ([string]::IsNullOrWhiteSpace($branch)) { $branch = '(HEAD destacado)' }
    $head = (Read-Git @('-C', $path, 'rev-parse', 'HEAD') | Select-Object -First 1).Trim()
    $dirty = @(Read-Git @('-C', $path, 'status', '--porcelain', '--untracked-files=all'))
    $productDirty = @(Read-Git (@('-C', $path, 'status', '--porcelain', '--untracked-files=all', '--') + $sourcePaths))
    & git -C $repositoryRoot merge-base --is-ancestor $head $currentHead 2>$null
    $ancestorExit = $LASTEXITCODE
    if ($ancestorExit -notin @(0, 1)) { throw "Falha ao comparar o historico de $path" }
    $integrated = if ($ancestorExit -eq 0) { 'INTEGRADO' } else { 'PENDENTE DE INTEGRACAO' }
    $versionPath = Join-Path $path 'package.json'
    $version = if (Test-Path -LiteralPath $versionPath -PathType Leaf) {
        (Get-Content -Raw -LiteralPath $versionPath | ConvertFrom-Json).version
    } else { '?' }
    $isCurrent = [IO.Path]::GetFullPath($path).TrimEnd('\') -eq $repositoryRoot.TrimEnd('\')
    $marker = if ($isCurrent) { 'ATUAL' } else { 'OUTRO' }
    if ($ancestorExit -eq 1 -or $productDirty.Count -gt 0 -or ($isCurrent -and $dirty.Count -gt 0)) { $pending++ }
    Write-Host "$marker | $branch | v$version | dirty=$($dirty.Count) produto=$($productDirty.Count) | $integrated"
    Write-Host "  $path"
}
Write-Host "Worktrees: $($worktreePaths.Count) | com pendencias de integracao/commit: $pending"
Write-Host 'Consulta somente leitura. O preflight completo continua obrigatorio antes do build.'

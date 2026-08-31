[CmdletBinding()]
param(
    [switch]$AllowDirtyCurrent,
    [switch]$AllowDirtySiblingWorktrees,
    [switch]$AllowUnmergedSiblingBranches,
    [switch]$AllowExistingRelease
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Stop-Preflight {
    param([Parameter(Mandatory)][string]$Message)
    throw "[Aurora release preflight] $Message"
}

function Invoke-GitText {
    param([Parameter(Mandatory)][string[]]$Arguments)
    $output = & git @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) {
        Stop-Preflight "git $($Arguments -join ' ') falhou: $($output -join [Environment]::NewLine)"
    }
    return @($output)
}

$repoRoot = (Invoke-GitText -Arguments @("rev-parse", "--show-toplevel") | Select-Object -First 1).Trim()
if (-not $repoRoot) {
    Stop-Preflight "não foi possível localizar a raiz do repositório"
}

Push-Location -LiteralPath $repoRoot
try {
    $branch = (Invoke-GitText -Arguments @("branch", "--show-current") | Select-Object -First 1).Trim()
    if (-not $branch) {
        Stop-Preflight "HEAD destacado; use um branch de integração nomeado"
    }
    if ($branch -in @("main", "master")) {
        Stop-Preflight "releases não podem ser montados diretamente em '$branch'"
    }

    $conflicts = & git grep -n -E "^(<<<<<<<|=======|>>>>>>>)" -- . 2>$null
    if ($LASTEXITCODE -eq 0) {
        Stop-Preflight "marcadores de conflito encontrados:$([Environment]::NewLine)$($conflicts -join [Environment]::NewLine)"
    }
    if ($LASTEXITCODE -ne 1) {
        Stop-Preflight "não foi possível procurar marcadores de conflito"
    }

    $diffCheck = & git diff --check 2>&1
    if ($LASTEXITCODE -ne 0) {
        Stop-Preflight "git diff --check falhou:$([Environment]::NewLine)$($diffCheck -join [Environment]::NewLine)"
    }

    if (-not $AllowDirtyCurrent) {
        $currentStatus = @(Invoke-GitText -Arguments @("status", "--porcelain", "--untracked-files=all"))
        if ($currentStatus.Count -gt 0) {
            Stop-Preflight "o checkout atual possui alterações sem commit. Faça commit antes de empacotar."
        }
    }

    $requiredFiles = @(
        "apps/aurora-core/compatibility-manifest.json",
        "apps/desktop/src-tauri/src/engine/core.rs",
        "apps/desktop/src/App.tsx",
        "apps/desktop/src/styles.css"
    )
    foreach ($requiredFile in $requiredFiles) {
        if (-not (Test-Path -LiteralPath $requiredFile -PathType Leaf)) {
            Stop-Preflight "arquivo obrigatório ausente: $requiredFile"
        }
    }

    $rootVersion = (Get-Content -Raw -LiteralPath "package.json" | ConvertFrom-Json).version
    $desktopVersion = (Get-Content -Raw -LiteralPath "apps/desktop/package.json" | ConvertFrom-Json).version
    $tauriVersion = (Get-Content -Raw -LiteralPath "apps/desktop/src-tauri/tauri.conf.json" | ConvertFrom-Json).version
    $cargoVersionMatch = Select-String -LiteralPath "apps/desktop/src-tauri/Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
    if (-not $cargoVersionMatch) {
        Stop-Preflight "versão não encontrada em apps/desktop/src-tauri/Cargo.toml"
    }
    $cargoVersion = $cargoVersionMatch.Matches[0].Groups[1].Value
    $versions = @(@($rootVersion, $desktopVersion, $tauriVersion, $cargoVersion) | Select-Object -Unique)
    if ($versions.Count -ne 1) {
        Stop-Preflight "versões divergentes: root=$rootVersion desktop=$desktopVersion tauri=$tauriVersion cargo=$cargoVersion"
    }
    $version = $versions[0]

    $releasePath = Join-Path "releases" "Aurora Smart Launcher_${version}_x64-setup.exe"
    if ((Test-Path -LiteralPath $releasePath) -and -not $AllowExistingRelease) {
        Stop-Preflight "o release imutável já existe: $releasePath. Incremente a versão antes de recompilar."
    }

    $sourcePaths = @(
        "apps/desktop",
        "apps/aurora-core",
        "apps/companion-mod",
        "apps/edge-proxy",
        "firebase",
        "functions",
        "scripts",
        "package.json"
    )
    $currentRoot = (Resolve-Path -LiteralPath $repoRoot).Path.TrimEnd('\')
    $worktreeLines = Invoke-GitText -Arguments @("worktree", "list", "--porcelain")
    $worktreePaths = @($worktreeLines |
        Where-Object { $_ -like "worktree *" } |
        ForEach-Object { $_.Substring("worktree ".Length) })

    foreach ($worktreePath in $worktreePaths) {
        $resolvedWorktree = (Resolve-Path -LiteralPath $worktreePath).Path.TrimEnd('\')
        if ($resolvedWorktree -eq $currentRoot) {
            continue
        }

        if (-not $AllowDirtySiblingWorktrees) {
            $siblingStatus = & git -C $resolvedWorktree status --porcelain --untracked-files=all -- @sourcePaths 2>&1
            if ($LASTEXITCODE -ne 0) {
                Stop-Preflight "não foi possível verificar o worktree $resolvedWorktree"
            }
            if (@($siblingStatus).Count -gt 0) {
                Stop-Preflight "há alterações de produto sem commit em outro worktree: $resolvedWorktree"
            }
        }

        if (-not $AllowUnmergedSiblingBranches) {
            $siblingHead = (Invoke-GitText -Arguments @("-C", $resolvedWorktree, "rev-parse", "HEAD") | Select-Object -First 1).Trim()
            & git merge-base --is-ancestor $siblingHead HEAD 2>$null
            if ($LASTEXITCODE -eq 1) {
                $siblingBranch = (Invoke-GitText -Arguments @("-C", $resolvedWorktree, "branch", "--show-current") | Select-Object -First 1).Trim()
                Stop-Preflight "o branch '$siblingBranch' de $resolvedWorktree ainda não está integrado em $branch"
            }
            if ($LASTEXITCODE -ne 0) {
                Stop-Preflight "não foi possível comparar o histórico de $resolvedWorktree"
            }
        }
    }

    Write-Host "[Aurora release preflight] OK"
    Write-Host "  Branch: $branch"
    Write-Host "  Versão: $version"
    Write-Host "  Worktrees verificados: $($worktreePaths.Count)"
}
finally {
    Pop-Location
}

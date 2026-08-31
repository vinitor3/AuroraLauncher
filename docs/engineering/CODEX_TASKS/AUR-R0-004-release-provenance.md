# AUR-R0-004 — Proveniência, versão e documentação pública

- **Fase/prioridade:** R0 / P0.
- **Objetivo:** impedir release mutável/inconsistente e alinhar documentação ao estado real.
- **Contexto/problema:** manifests dizem `0.1.0`; tag `v0.1.0-alpha`; instalador rastreado e asset publicado sob a mesma versão têm SHA-256 diferentes. README ainda chama o alpha público de privado e `docs/module-a.md` está desatualizado.
- **Resultado esperado:** checker de versão/proveniência, manifesto de artefatos e docs honestas; nenhuma alteração retroativa em tag/binário.
- **Dependências:** D-002 para promessa de release pública; licença pode integrar antes/depois, com ownership respeitado.

## Escopo e ownership

- **Pode/deve tocar:** checker em `scripts/release/**`, manifests de versão, `README.md` fora da seção legal, `docs/module-a.md`, docs de release/changelog.
- **Não tocar:** `releases/Aurora Smart Launcher_0.1.0_x64-setup.exe`, tag/release/assets existentes, LICENSE/notices, código funcional.
- **Interfaces:** root/package, desktop/package, Cargo/Tauri e Companion version metadata.

## Passos

1. Registrar tamanhos/hashes históricos sem tratar um como canônico nem substituir qualquer um.
2. Implementar checker que compare todas as fontes de versão, formato SemVer/tag e colisão de release.
3. Definir próximo candidato `0.2.0-alpha.1`; o bump real ocorre somente quando a release for autorizada e o changelog estiver pronto.
4. Gerar/validar manifesto JSON e `SHA256SUMS` para novos artefatos, com caminho, tamanho, hash, plataforma e versão.
5. Atualizar README/module-a/status: alpha público, limitações, HUD Swing, runtimes comprovados, disclaimer não oficial.
6. Documentar SBOM, attestation, Tauri updater e SignPath como gate futuro, sem fingir assinatura atual.

## Testes/comandos

```powershell
<checker-de-versao-e-proveniencia> --check
Get-FileHash -Algorithm SHA256 'releases\Aurora Smart Launcher_0.1.0_x64-setup.exe'
git diff --check
```

Fixture com versão divergente e checksum errado deve falhar. Caminho com espaço precisa funcionar. Nenhum teste sobrescreve asset.

## Aceitação e Definition of Done

- Fontes de versão coerentes ou divergência histórica explicitamente catalogada.
- Próxima release inédita; tag/assets antigos intactos.
- Manifesto/hash reproduzíveis; docs não prometem HUD/runtime/auth/multiplayer inexistentes.
- Checklist de release inclui build limpo, SBOM, provenance, smoke NSIS e rollback por nova versão.

## Riscos e rollback

Bump prematuro pode criar outra identidade pública inconsistente. Checker começa read-only; rollback remove somente scripts/docs novos, nunca arquivos históricos.

## Git/PR

- Branch/worktree: `codex/aur-r0-004-release-provenance` / `AuroraLauncher-wt-r0-004`.
- Commit: `chore(release): enforce immutable provenance [AUR-R0-004]`.
- PR depende de: integração final após R0-001/002/003.
- Execução paralela: **sim**, `package.json` raiz exclusivo nesta wave.


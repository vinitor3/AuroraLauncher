# AUR-R0-002 — Licença e avisos do monorepo

- **Fase/prioridade:** R0 / P0.
- **Objetivo:** tornar inequívoco o que é open source, sob qual licença e quais partes são de terceiros.
- **Contexto/problema:** `LICENSE.md` resume GPL apenas para código identificado pelo manifesto Rust; TypeScript, Java, docs e assets ficam ambíguos e o texto integral não está no repositório.
- **Resultado esperado:** licença raiz integral, escopo autoral explícito, SPDX coerente e inventário de terceiros/proveniência suficiente para distribuição.
- **Dependência bloqueante:** decisão D-001 do responsável; preparar diff é permitido, merge/publicação não.

## Escopo e ownership

- **Pode/deve tocar:** `LICENSE*`, `NOTICE*`, `THIRD_PARTY_NOTICES.md`, `README.md` apenas na seção de licença, metadados de licença nos manifests.
- **Não tocar:** código funcional, assets/binários de terceiros, lockfiles, marca Minecraft, Git history.
- **Interfaces:** Cargo/npm/Gradle/Tauri devem declarar a mesma política ou exceção documentada.

## Passos

1. Inventariar código autoral, docs, logos/assets e binários; classificar “Aurora”, “terceiro”, “não redistribuível” ou “origem incerta”.
2. Após D-001, incluir texto integral oficial e arquivo de escopo/exceções; usar identificador SPDX exato.
3. Atualizar manifests e notices sem relicenciar dependência/asset alheio.
4. Registrar origem, versão/commit, licença e link de cada componente incorporado/material.
5. Adicionar checagem simples que falhe em licença vazia/metadata inconsistente, se couber sem tomar ownership da CI.

## Testes/comandos

```powershell
rg -n 'license|GPL|SPDX' Cargo.toml package.json apps companion LICENSE* THIRD_PARTY_NOTICES.md
git diff --check
```

Validar texto integral contra a fonte oficial; `npm`/Cargo/Gradle continuam parseando; busca por assets/origens sem classificação retorna vazia ou exceção explícita.

## Aceitação e Definition of Done

- Todo conteúdo distribuído tem owner/origem/licença/permissão ou é removido da distribuição por tarefa separada.
- Texto integral, escopo, exceções, marca/disclaimer e contribuição estão coerentes.
- Nenhuma alegação de licença sobre Minecraft/Mojang, mods ou assets que Aurora não controla.
- D-001 registrada no PR; revisão jurídica continua recomendada para distribuição ampla.

## Riscos e rollback

Risco de atribuir direitos inexistentes ou tornar contribuição incompatível. Se origem ficar incerta, bloquear distribuição daquele item; rollback reverte somente metadata/texto, nunca apaga evidência de proveniência.

## Git/PR

- Branch/worktree: `codex/aur-r0-002-license` / `AuroraLauncher-wt-r0-002`.
- Commit: `docs(legal): define repository licensing [AUR-R0-002]`.
- PR depende de: D-001 antes do merge.
- Execução paralela: **sim**, arquivos legais exclusivos.


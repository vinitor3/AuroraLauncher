# AUR-R0-003 — CI completa e determinística

- **Fase/prioridade:** R0 / P0.
- **Objetivo:** fazer cada regressão relevante bloquear merge.
- **Contexto/problema:** CI atual cobre build frontend/Rust test/Worker/Functions, mas não `fmt`, `clippy`, Companion, regras Firestore, versão/proveniência; referências e toolchains ainda são flutuantes.
- **Resultado esperado:** workflow com permissões mínimas, concorrência/cancelamento, caches seguros e checks separados/legíveis.
- **Dependências:** comandos de AUR-R0-001/R0-004 podem chegar depois; usar integração coordenada, não duplicar scripts.

## Escopo e ownership

- **Pode/deve tocar:** `.github/workflows/**`, scripts novos em `scripts/ci/**`.
- **Pode propor patch separado:** aliases no root `package.json`.
- **Não tocar:** código de produto, regras, licença, binários/JARs, versão.
- **Interfaces:** Node/Rust/Gradle/Firebase CLI; lockfiles são autoridade.

## Passos

1. Separar jobs frontend, Rust, Worker/Functions, Companion e security/governance.
2. Adicionar `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
3. Build Companion atual e validar artefato esperado; registrar que não é runtime 9/9.
4. Ligar suíte Firestore e checker de versão quando integrados.
5. Fixar versões/toolchains e Actions conforme política de supply chain; `permissions: contents: read` por padrão.
6. Configurar `concurrency` por PR/ref e caches derivados de lockfiles, sem cache de secrets/output publicado.

## Testes/comandos

Executar localmente todos os comandos do [`../CODEX_INTEGRATION_PLAN.md`](../CODEX_INTEGRATION_PLAN.md). Validar YAML com ferramenta disponível e revisar `actionlint` se adotado. Introduzir falha temporária controlada em cada classe ou demonstrar fixture que deixe job vermelho; remover antes do commit.

## Aceitação e Definition of Done

- Todos os checks obrigatórios aparecem isoladamente e ficam verdes na baseline.
- Uma falha de format/clippy/test/build/rule/versão resulta em job vermelho.
- Nenhuma Action mutável não justificada, permissão de escrita ou secret exposto a PR não confiável.
- Tempo/caches documentados; Companion moderno não é rotulado como matriz runtime completa.

## Riscos e rollback

Pins podem quebrar compatibilidade e Firebase/Gradle podem elevar tempo. Preferir versões fixas e jobs paralelos; rollback reverte o workflow, mantendo comandos locais documentados.

## Git/PR

- Branch/worktree: `codex/aur-r0-003-ci-baseline` / `AuroraLauncher-wt-r0-003`.
- Commit: `ci: enforce full baseline gates [AUR-R0-003]`.
- PR depende de: interfaces finais de R0-001/R0-004 na integração.
- Execução paralela: **sim**, workflows exclusivos.


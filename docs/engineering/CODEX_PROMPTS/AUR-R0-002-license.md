# Prompt AUR-R0-002

Você é o Licensing Implementer do AuroraLauncher. Trabalhe somente na task **AUR-R0-002 — Licença e avisos do monorepo**, a partir do baseline `5aa5fe8`, em `codex/aur-r0-002-license` e worktree dedicado. Leia `AGENTS.md`, `LICENSE.md`, `THIRD_PARTY_NOTICES.md`, todos os manifests e `docs/engineering/CODEX_TASKS/AUR-R0-002-license.md`.

O responsável ainda precisa confirmar D-001: recomendação GPL-3.0-only para todo o código autoral Rust/TypeScript/Java/scripts e documentação indicada, com exceções explícitas para assets/marcas e terceiros. Você pode preparar o diff nessa hipótese claramente marcada, mas não deve declarar a decisão tomada nem integrar/publicar antes da confirmação.

Ownership: `LICENSE*`, `NOTICE*`, `THIRD_PARTY_NOTICES.md`, seção de licença do README e metadados `license` dos manifests. Não altere comportamento, lockfiles, binários/JARs, assets, história Git ou arquivos de outro agente. Nunca relicencie Minecraft/Mojang, mods, logo/asset alheio ou código de origem incerta.

Inventarie o material distribuído por categoria: autoral Aurora, terceiro com origem/licença, excluído/não redistribuível ou origem incerta. Inclua texto integral oficial da licença decidida, escopo/exceções, identificadores SPDX coerentes, contribuição e avisos. Para cada componente incorporado relevante registre nome, versão/commit, origem, licença e notas de redistribuição. Se faltar origem, reporte bloqueador em vez de inventar.

Valide que Cargo/npm/Gradle/Tauri continuam parseando e busque inconsistências de `license`, GPL e SPDX. Rode `git diff --check`. Faça um commit lógico `docs(legal): define repository licensing [AUR-R0-002]` somente se a hipótese estiver isolada e revisável; marque o PR como bloqueado por D-001 até confirmação. Handoff: ID, branch/SHA, inventário, arquivos, validações, incertezas jurídicas, decisão pendente e rollback. Não faça push/merge.


# AUR-R1-002 — Harness de evidência Core/Skins e Assistente desktop

- **Fase/prioridade:** R1 / P0.
- **Objetivo:** substituir “JAR existe” por evidência reproduzível da matriz 9/9.
- **Contexto/problema:** as evidências de versões antigas do Companion são históricas. A matriz Core/Skins exige nove roteiros próprios; chat/voz do launcher exigem runtime desktop sem jogo.
- **Resultado esperado:** schema/runner/checklist de evidência, fixtures redigidas e estados `UNBUILT`, `BUILT`, `LAUNCHED`, `IPC`, `UI`, `VERIFIED_RUNTIME`.
- **Dependências:** R0 verde; matriz atual é autoridade inicial.

## Escopo e ownership

- **Pode/deve tocar:** fixtures/scripts de QA Core/Skins e desktop, fixtures/scripts e docs/matriz de evidência.
- **Não tocar:** implementação UI/HUD/adapters, JARs versionados, launcher `App.tsx`.
- **Interfaces:** coleta versão/loader/JAR/hash/Java/OS, launch, conexão/sessão IPC, menu/preview/Skins e fechamento; separadamente botão/atalho desktop, chat, microfone/TTS e logs redigidos.

## Passos

1. Definir schema JSON e critérios objetivos por nível.
2. Criar runner read-only/assistido que verifique hashes, arquivos e capture timestamps/resultados.
3. Produzir fixture de sucesso/falha sem credencial/nome pessoal.
4. Documentar roteiro manual de máquina limpa para eventos in-game não automatizáveis.
5. Atualizar matriz somente com evidência anexada e revisável.

## Testes/comandos

Build Core/Skins via wrappers preservados; validar schema/fixtures; executar dry-run sem Minecraft; fixture adulterada/hash divergente deve falhar. Não marcar runtime por build.

## Aceitação/DoD

Um terceiro consegue reproduzir o checklist; toda célula tem estado+evidência+data ou `UNVERIFIED`; nenhum token/log pessoal; harness não altera instância.

## Riscos/rollback

Automação visual pode ser frágil. Separar prova automática e atestado manual. Rollback remove harness, preserva evidências já coletadas.

## Git/PR

- Branch/worktree: `codex/aur-r1-002-runtime-harness` / `AuroraLauncher-wt-r1-002`.
- Commit: `test(core-skins): add runtime evidence harness [AUR-R1-002]`.
- PR depende de R0; não autoriza um novo Assistente in-game.
- Execução paralela: **sim**.

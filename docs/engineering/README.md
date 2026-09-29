# Engenharia Aurora

Revisão atual de documentação: **[Aurora Connect V2 — sete entregáveis](../task-aur-r4-connect-v2-review.md)**, 29/09/2026, reconciliada com launcher alpha.14. Inclui roadmap completo, especificação TCP/UDP/voz, proposta de ADR, backlog/matriz, dependências zero-spend e prompt executável. Connect continua **NÃO IMPLEMENTADO**; recovery/Doctor/Tools/cópias existentes foram preservados. Estados de entregas anteriores abaixo são registros datados, não validação repetida nesta revisão.

Escopo vigente em 21/09/2026: **O Assistente permanece no Aurora Launcher; o mod Companion e sua integração in-game foram retirados.** Consulte o [mapa de consumidores e validação](../task-remove-companion.md), a [ADR-018](ADR/ADR-018-remove-companion-keep-desktop-assistant.md) e o [IPC do Core](../core-ipc.md). Auditorias de agosto/setembro conservam resultados históricos; não são autorização para restaurar o mod.

## Evidência histórica de segurança da revisão inicial

O registro da revisão inicial documentou regras Firestore locais bloqueando autoatribuição de `ADMIN` e payloads iniciais incoerentes, com 7/7 testes da suíte [`../../firebase/tests/firestore.rules.test.mjs`](../../firebase/tests/firestore.rules.test.mjs) no Emulator. Esta branch Connect V2 altera documentação e não repetiu esses testes nem consultou o estado de produção. O gate continua exigindo evidência de auditoria dos perfis existentes e implantação das regras; não presumir conclusão por esse resultado histórico.

## Ordem de leitura

1. [Resumo executivo](EXECUTIVE_SUMMARY.md)
2. [Pesquisa externa](RESEARCH_REPORT.md)
3. [Revisão de arquitetura](ARCHITECTURE_REVIEW.md)
4. [Roadmap mestre](MASTER_ROADMAP.md)
5. [Grafo de dependências](DEPENDENCY_GRAPH.md)
6. [Registro de riscos](RISK_REGISTER.md)
7. [Estratégia de testes](TEST_STRATEGY.md)
8. [Estratégia de releases](RELEASE_STRATEGY.md)
9. [Matriz de compatibilidade](COMPATIBILITY_MATRIX.md)
10. [Dossiê histórico do Aurora Companion](COMPANION_AUDIT.md)
11. [Plano paralelo Codex](CODEX_PARALLEL_PLAN.md)
12. [Plano de integração](CODEX_INTEGRATION_PLAN.md)
13. [Decisões do responsável](DECISIONS_REQUIRED.md)
14. [ADRs](ADR/README.md)
15. [Tarefas Codex](CODEX_TASKS/README.md)
16. [Prompts Codex](CODEX_PROMPTS/README.md)

O registro específico da Fase 1.3 está em
[`../task-content-updates.md`](../task-content-updates.md).

O contexto permanente de agentes está em [`../../AGENTS.md`](../../AGENTS.md). O threat model normativo permanece no [`../../SECURITY.md`](../../SECURITY.md).

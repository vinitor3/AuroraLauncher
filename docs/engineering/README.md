# Engenharia Aurora

Revisão documental de 29/09/2026: [Aurora Connect V2 — diagnóstico e sete entregáveis em ordem](AURORA_CONNECT_REVIEW.md). O [Master Roadmap](MASTER_ROADMAP.md) permanece canônico e completo; Connect não está implementado/homologado por esta revisão.

Escopo vigente em 21/09/2026: **O Assistente permanece no Aurora Launcher; o mod Companion e sua integração in-game foram retirados.** Consulte o [mapa de consumidores e validação](../task-remove-companion.md), a [ADR-018](ADR/ADR-018-remove-companion-keep-desktop-assistant.md) e o [IPC do Core](../core-ipc.md). Auditorias de agosto/setembro conservam resultados históricos; não são autorização para restaurar o mod.

## Mudança de segurança preparada nesta branch

As regras Firestore locais agora bloqueiam autoatribuição de `ADMIN` e payloads iniciais incoerentes. A suíte em [`../../firebase/tests/firestore.rules.test.mjs`](../../firebase/tests/firestore.rules.test.mjs) passou 7/7 no Emulator. Isso **não** equivale a proteção em produção: ainda faltam auditar perfis existentes e implantar as regras no projeto Firebase.

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

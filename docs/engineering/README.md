# Engenharia Aurora

Baseline geral auditado em 2026-08-30; Companion reauditado em 2026-08-31.

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
10. [Dossiê do Aurora Companion](COMPANION_AUDIT.md)
11. [Plano paralelo Codex](CODEX_PARALLEL_PLAN.md)
12. [Plano de integração](CODEX_INTEGRATION_PLAN.md)
13. [Decisões do responsável](DECISIONS_REQUIRED.md)
14. [ADRs](ADR/README.md)
15. [Tarefas Codex](CODEX_TASKS/README.md)
16. [Prompts Codex](CODEX_PROMPTS/README.md)

O contexto permanente de agentes está em [`../../AGENTS.md`](../../AGENTS.md). O threat model normativo permanece no [`../../SECURITY.md`](../../SECURITY.md).

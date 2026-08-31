# Backlog executável Codex

Cada arquivo abaixo é um contrato de implementação. O agente deve respeitar ownership, dependências e “não tocar”. Prompts prontos da Wave 0 ficam em [`../CODEX_PROMPTS/README.md`](../CODEX_PROMPTS/README.md).

| ID | Fase | Prioridade | Título | Paralelo |
| --- | --- | --- | --- | --- |
| [AUR-R0-001](AUR-R0-001-firestore-rules.md) | R0 | P0 | Fechar autoatribuição de role e testar regras | sim |
| [AUR-R0-002](AUR-R0-002-license.md) | R0 | P0 | Licenciar o monorepo e inventariar terceiros | sim; merge depende de D-001 |
| [AUR-R0-003](AUR-R0-003-ci.md) | R0 | P0 | Tornar CI um gate completo e determinístico | sim |
| [AUR-R0-004](AUR-R0-004-release-provenance.md) | R0 | P0 | Versionamento, proveniência e docs coerentes | sim |
| [AUR-R1-001](AUR-R1-001-network-limits.md) | R1 | P0 | Limites de HTTP e autenticação IPC | após R0 |
| [AUR-R1-002](AUR-R1-002-runtime-harness.md) | R1 | P0 | Harness de evidência Companion 9/9 | após R0 |
| [AUR-R1-003](AUR-R1-003-app-decomposition.md) | R1 | P1 | Primeira decomposição do `App.tsx` | após R0 |
| [AUR-R1-004](AUR-R1-004-atomic-files.md) | R1 | P0 | Substituição transacional e rollback de arquivo | após R0 |

Branch padrão: `codex/<id-em-minusculo>-<slug>`. Um commit lógico por tarefa, mensagem `feat|fix|chore(scope): resumo [ID]`. PR deve incluir “Tests”, “Risk”, “Rollback” e “Docs”.


# Architecture Decision Records

| ADR | Decisão | Estado |
| --- | --- | --- |
| [001](ADR-001-prism-reference.md) | Prism como referência, não componente | Aceita |
| [002](ADR-002-firebase-functions.md) | Deprecar Firebase Functions | Proposta |
| [003](ADR-003-identity-appearance-storage.md) | Firebase identidade + storage de aparência limitado | Aceita com revisão |
| [004](ADR-004-companion-architecture.md) | Core compartilhado + adaptadores | Substituída pela ADR-018 |
| [005](ADR-005-native-hud.md) | Screen/HUD nativos | Substituída pela ADR-018 |
| [006](ADR-006-local-cas.md) | CAS local SHA-256 + SQLite | Proposta |
| [007](ADR-007-content-updater.md) | updater planificado e reversível | Aceita |
| [008](ADR-008-gemini-tools.md) | broker tipado níveis 0–2 | Proposta |
| [009](ADR-009-master-panel.md) | Master estático e adiado | IMPLEMENTAR FUTURAMENTE |
| [010](ADR-010-multiplayer.md) | integrar/medir antes de rede própria | Decisão: sim |
| [011](ADR-011-legacy-1122.md) | 1.12.2 como tier legado | Aceita |
| [012](ADR-012-creator-mvp.md) | recipes/datapack antes de suites fechadas | Proposta |
| [013](ADR-013-distribution.md) | GitHub Releases imutáveis | Proposta |
| [014](ADR-014-microsoft-auth.md) | entitlement antes de distribuição pública | Decisão: não por enquanto |
| [015](ADR-015-aurora-core.md) | Core modular, IPC único e módulos isolados | Aceita |
| [017](ADR-017-external-content-downloads.md) | Download externo por SHA-1 oficial e fila correlacionada | IMPLEMENTADO MAS NÃO HOMOLOGADO |

| [018](ADR-018-remove-companion-keep-desktop-assistant.md) | Retirar mod Companion; preservar Assistente desktop, Core/Skins e IPC | Aceita |
| [022](ADR-022-skins-server-relay.md) | Retransmitir skins pela conexão Minecraft, com integração da grade preservada | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| [023](ADR-023-independent-launcher-sessions.md) | Isolar autenticação das janelas mantendo login principal e instâncias protegidas | IMPLEMENTADO MAS NÃO HOMOLOGADO |

Estados mudam somente por novo ADR que substitua o anterior ou por revisão explícita deste arquivo.

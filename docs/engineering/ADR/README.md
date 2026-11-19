# Architecture Decision Records

Decisão experimental no caminho existente: [Aurora Connect V2](PROPOSAL-aurora-connect.md), Iroh 1.3.0 em laboratório independente com endereços diretos manuais. Produto/relay permanecem **HIPÓTESE**, sem homologação. A política permite relay gratuito verificado, prefere direto e veda gasto obrigatório/automático. ADR-010 conserva o registro histórico. [Reconciliação das propostas](../AURORA_CONNECT_REVIEW.md); conferir numeração/ownership antes de criar ADR de integração/distribuição.

| ADR | Decisão | Estado |
| --- | --- | --- |
| [033](ADR-033-social-desktop-join.md) | Entrada social no mesmo ConnectManager, broker autenticado local e barreira atômica de conta | PROTÓTIPO local; evidências e limites na tarefa89 |
| [032](ADR-032-social-join-context.md) | Contexto social assinado resolvido pela sessão, destinatário e política atuais antes de ingresso | PROTÓTIPO local testado; integração gráfica/produção pendentes |
| [031](ADR-031-social-local-admission.md) | Consentimento host, grant social separado e continuidade assinada, dois clientes TCP locais | PROTÓTIPO local testado; integração gráfica/produção pendentes |
| [030](ADR-030-social-account-key-binding.md) | Prova Ed25519 entre conta autenticada, chave RAM e preparo original, sem autorização de transporte | PROTÓTIPO local testado; ingresso social pendente |
| [029](ADR-029-connect-social-local.md) | Relações Aurora privadas e catálogo opaco com projeções autorizadas, emuladores e UI social default-off | PROTÓTIPO local; ingresso social pendente |
| [001](ADR-001-prism-reference.md) | Prism como referência, não componente | Aceita |
| [002](ADR-002-firebase-functions.md) | Deprecar Firebase Functions | Proposta |
| [003](ADR-003-identity-appearance-storage.md) | Firebase identidade + storage de aparência limitado | Aceita com revisão |
| [004](ADR-004-companion-architecture.md) | Core compartilhado + adaptadores | Substituída pela ADR-018 |
| [005](ADR-005-native-hud.md) | Screen/HUD nativos | Substituída pela ADR-018 |
| [006](ADR-006-local-cas.md) | CAS local SHA-256 + SQLite | PARCIAL: adoção limitada na ADR-027; sistemas anteriores preservados |
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
| [024](ADR-024-doctor-gemini-guidance.md) | Explicar o Doctor por consulta Gemini opcional com contrato reduzido e passos manuais permitidos | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| [025](ADR-025-confirmed-local-ai-tools.md) | Compartilhar Gemini e broker local de ações estreitas com revisão, confirmação, histórico canônico e restauração | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| [026](ADR-026-bounded-gemini-failover.md) | Limitar failover de chaves Gemini e oferecer Flash-Lite opcional dentro do prazo e orçamento compartilhados | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| [027](ADR-027-manual-instance-recovery.md) | Pontos manuais de instância em CAS, prévia confirmada e restauração reversível | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| [028](ADR-028-doctor-recovery-lab.md) | Evidência verificável do Doctor e cópias de diagnóstico com aplicação seletiva transacional | IMPLEMENTADO MAS NÃO HOMOLOGADO |

| [034](ADR-034-instance-java-engine-settings.md) | Configurar RAM e JVM por instância, com resolução nativa, persistência protegida e painel independente de temas | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| [035](ADR-035-declarative-launcher-themes.md) | Interpretar temas locais sem código executável, com associação por instância e recuperação nativa | IMPLEMENTADO MAS NÃO HOMOLOGADO |

Estados mudam somente por novo ADR que substitua o anterior ou por revisão explícita deste arquivo.

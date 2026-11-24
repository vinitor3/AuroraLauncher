# Risk Register

Revisão documental: 29/09/2026, alpha.14 (`6a107b8`). IDs e achados históricos
são preservados. Os estados abaixo distinguem evidências registradas nas entregas
de verificações atuais; esta revisão não consultou produção nem repetiu testes
runtime. [Recuperação](../instance-recovery.md), [Tools](https://github.com/vinitor3/AuroraLauncher/blob/088bf5b435b1b63e842c11ff1c01648b5e38bced/docs/aurora-ai-tools.md),
[cópias de diagnóstico](../diagnostic-lab.md) e [validação alpha.14](https://github.com/vinitor3/AuroraLauncher/blob/088bf5b435b1b63e842c11ff1c01648b5e38bced/docs/task-aur-1167-doctor-lab.md).

| ID | Prioridade | Risco | Evidência / estado | Mitigação e gate |
| --- | --- | --- | --- | --- |
| R-001 | P0 | criação de perfil escolhia `role` | Achado histórico; correção local e sete testes Emulator registrados. Estado atual de documentos/deploy em produção NÃO VERIFICADO nesta revisão | revisar usuários existentes, comprovar implantação controlada e smoke; Master mantém seu gate |
| R-019 | P0 | troca de arquivo pode apagar destino válido | Achado original de remoção antes de rename; updater, Tools e recuperação agora têm proteção própria. Cobertura de todas as mutações NÃO VERIFICADO | AUR-R1-004: inventariar lacunas, preservar staging/backup/journals atuais e provar rollback por domínio, sem refazer os já entregues |
| R-002 | P0 | artefato `0.1.0-alpha` sem proveniência única | Git: 5.029.160 B / `95C270…`; GitHub: 5.028.702 B / `582E5B…` | preservar ambos como histórico, publicar próxima versão com manifest/SHA/SBOM |
| R-003 | P0 | matriz Core/Skins sem runtime completo | JAR válido e histórico Companion não provam Core/launch/mixin/UI atuais | harness + evidência 9/9; não anunciar suporte completo |
| R-004 | P0 | modo offline pode contornar entitlement | launcher cria identidade offline sem Microsoft | decisão do responsável e revisão jurídica; distribuição pública bloqueada |
| R-005 | P0 | licença do monorepo ambígua/incompleta | `LICENSE.md` fala apenas no manifesto Rust e aponta para texto externo | escolher licença de todo código, incluir texto integral e notices/SBOM |
| R-006 | P1 | branch `main` sem proteção | Achado da auditoria anterior; configuração remota atual NÃO VERIFICADO nesta revisão | required checks, PR, bloquear force-push/delete |
| R-007 | P1 | acoplamento em `App.tsx`/`commands.rs`/Worker | Contagens antigas não representam alpha.14; features/domínios posteriores devem ser preservados | ownership exclusivo, inventário atual e decomposição por domínio sem regressão |
| R-008 | P1 | frontend pode esperar rede indefinidamente | MITIGADO no catálogo: Worker/desktop e chamadas Modrinth agora têm timeout; outros fetches ainda precisam de inventário | manter wrapper cancelável e cobrir demais operações |
| R-009 | P1 | rate limit Worker não distribuído | `Map` por isolate | quotas por UID/IP e binding distribuído apenas se couber no free tier |
| R-010 | P1 | supply chain/update malicioso | sem updater assinado/SBOM/pinning completo | updater fail-closed, hashes, assinatura, dependency review |
| R-011 | P1 | prompt injection/excessive agency | Tools estreitas implementadas com broker, IDs de inventário, plano, confirmação e journal; dados/metadados e prosa de IA continuam não confiáveis | preservar ADR-025, contexto canônico, least privilege e testes negativos; sem shell ou ação automática, homologação nativa pendente |
| R-012 | P1 | toolchains Java/Loom/Gradle divergentes | Warnings originais são históricos. Fluxo atual usa wrapper 8.4 compartilhado e 9.7.1 na geração 1.21.1; não repetir diagnóstico antigo sem execução | seguir [release-workflow](../release-workflow.md), JDK por etapa, builds sequenciais e matriz ativa; upgrades isolados com homologação |
| R-013 | P1 | confundir software livre ou relay tecnicamente disponível com infraestrutura gratuita autorizada | Aurora Connect é proposta; operador, quotas, termos, regiões, TCP/UDP e autorização de produção precisam de verificação por dependência | Direto preferido; relay gratuito permitido sob política V2. Sem cobrança obrigatória/automática, VPS obrigatória ou créditos temporários; alerta/contador não bloqueiam cobrança. Relay de laboratório não aprova distribuição; indisponibilidade/quota têm fallback explícito e nenhum desvio pago. Infraestrutura própria obrigatória adiada; [especificação](AURORA_CONNECT_SPEC.md) |
| R-014 | P2 | bucket de aparência público | URL vazada é acessível; 1 GB/egress limitado | retenção, remoção, chaves aleatórias e migração opcional R2 |
| R-015 | P2 | nonce IPC observável no argv | propriedade JVM é visível a processo local | single-client, TTL, vínculo ao processo e canal herdado futuro |
| R-016 | P2 | Edge TTS não oficial/instável | teste depende de serviço externo e está ignored | fallback sem voz, aviso de privacidade e adaptador substituível |
| R-017 | P2 | coleta CAS apaga blob vivo ou retenção esgota disco | CAS SHA-256/SQLite limitado à Recuperação já implementado; exclusão conserva blobs sem referências e GC não foi entregue | manter referências/identidade e validação; limites/espaço antes de captura, GC futuro com raízes explícitas, dry-run, transação e testes de interrupção; não apagar blobs manualmente |
| R-018 | P2 | conteúdo CurseForge sem autorização | alguns arquivos não têm URL direta e o CDN passou a exigir `x-api-key` | página oficial/manual quando bloqueado; tickets HMAC e gateway restrito a `forgecdn.net` quando autorizado |
| R-023 | P0 mitigado | CONCLUÍDO: credencial CurseForge renovada | `/health/curseforge` HTTP 200, 1 resultado e 52 categorias em 17/09/2026 após deploy `d9669ec0` | manter chave somente no Worker e verificar health antes de release; instalações reais ainda precisam de homologação |

## Riscos históricos do mod retirado

As linhas abaixo conservam a auditoria anterior. A [ADR-018](ADR/ADR-018-remove-companion-keep-desktop-assistant.md) cancela sua evolução como produto; não são instruções para reinstalar ou publicar Companion. O risco atual é regressão do Assistente desktop/IPC compartilhado ou remoção indevida na migração, cobertos por testes de consumidores e recuperação.

| ID | Prioridade histórica | Risco histórico | Evidência histórica | Mitigação antiga, substituída pela retirada |
| --- | --- | --- | --- | --- |
| R-020 | P0 | Companion 0.2.0 não se anexa ao Core nas linhas Fabric observadas | logs 1.16.5/1.19.2/1.20.1/1.21.1 mostram entrypoint antes do Core; módulo/IPC ficam ausentes | anexação tardia corrigida na fonte; publicar versão inédita e testar 4 linhas |
| R-021 | P0 | JARs Forge 1.16.5/1.19.2 exigem Forge 47 | `mods.toml` dos artefatos imutáveis usa `[47,)` em vez de 36/43 | template corrigido na fonte; não sobrescrever 0.2.0, reconstruir sob nova versão |
| R-022 | P1 | pedido do Assistente pode travar após falha/oversize do IPC | retorno booleano era ignorado, não havia timeout e JPEG não tinha teto alinhado ao Core | fonte limita captura, trata recusa e aplica timeouts; falta runtime e release |

## Política

P0 impede merge/release da capacidade afetada. Risco aceito precisa de responsável, prazo/revisão e fallback; “gratuito” não justifica esconder indisponibilidade ou remover controles.

R4 também exige ameaças de convite/replay, destino arbitrário, amplificação UDP,
isolamento de respostas e exposição de IP. Controles e testes permanecem na
[especificação Connect](AURORA_CONNECT_SPEC.md); aprovação TCP não reduz os gates
de UDP, voz, relay ou configuração reversível. A falta de homologação não apaga
a implementação alpha.14, e a existência dessa implementação não encerra o risco.

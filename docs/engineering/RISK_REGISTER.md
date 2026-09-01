# Risk Register

| ID | Prioridade | Risco | Evidência / estado | Mitigação e gate |
| --- | --- | --- | --- | --- |
| R-001 | P0 | criação de perfil escolhia `role` | CONFIRMADO; correção local força `PLAYER` e passou 7 testes Emulator, ainda sem auditoria/deploy | revisar usuários existentes, deploy controlado e smoke; Master bloqueado |
| R-019 | P0 | troca de arquivo pode apagar destino válido | fluxo remove destino antes de rename; falha intermediária perde a versão anterior | AUR-R1-004: staging no mesmo volume, backup/journal e rollback bit a bit |
| R-002 | P0 | artefato `0.1.0-alpha` sem proveniência única | Git: 5.029.160 B / `95C270…`; GitHub: 5.028.702 B / `582E5B…` | preservar ambos como histórico, publicar próxima versão com manifest/SHA/SBOM |
| R-003 | P0 | matriz Companion 0.2.0 sem runtime completo | JAR válido e evidência histórica 0.1.0 não provam Core/launch/mixin/UI atuais | harness + evidência 9/9; não anunciar suporte completo |
| R-020 | P0 | Companion 0.2.0 não se anexa ao Core nas linhas Fabric observadas | logs 1.16.5/1.19.2/1.20.1/1.21.1 mostram entrypoint antes do Core; módulo/IPC ficam ausentes | anexação tardia corrigida na fonte; publicar versão inédita e testar 4 linhas |
| R-021 | P0 | JARs Forge 1.16.5/1.19.2 exigem Forge 47 | `mods.toml` dos artefatos imutáveis usa `[47,)` em vez de 36/43 | template corrigido na fonte; não sobrescrever 0.2.0, reconstruir sob nova versão |
| R-022 | P1 | pedido do Assistente pode travar após falha/oversize do IPC | retorno booleano era ignorado, não havia timeout e JPEG não tinha teto alinhado ao Core | fonte limita captura, trata recusa e aplica timeouts; falta runtime e release |
| R-004 | P0 | modo offline pode contornar entitlement | launcher cria identidade offline sem Microsoft | decisão do responsável e revisão jurídica; distribuição pública bloqueada |
| R-005 | P0 | licença do monorepo ambígua/incompleta | `LICENSE.md` fala apenas no manifesto Rust e aponta para texto externo | escolher licença de todo código, incluir texto integral e notices/SBOM |
| R-006 | P1 | branch `main` sem proteção | CONFIRMADO via configuração pública do GitHub | required checks, PR, bloquear force-push/delete |
| R-007 | P1 | `App.tsx`/`commands.rs`/Worker monolíticos | 2.293/1.582/456 linhas | ownership exclusivo e decomposição por domínio |
| R-008 | P1 | frontend pode esperar rede indefinidamente | `edge.ts` e fetches Modrinth sem timeout | wrapper cancelável, timeout por operação e testes |
| R-009 | P1 | rate limit Worker não distribuído | `Map` por isolate | quotas por UID/IP e binding distribuído apenas se couber no free tier |
| R-010 | P1 | supply chain/update malicioso | sem updater assinado/SBOM/pinning completo | updater fail-closed, hashes, assinatura, dependency review |
| R-011 | P1 | prompt injection/excessive agency | chat recebe logs/web; Tools planejadas | broker tipado, least privilege, confirmação, auditoria e red-team |
| R-012 | P1 | Loom/Gradle obsoletos | warning de Loom 1.3 e Gradle 9 | upgrade isolado; reconstruir e testar cada linha |
| R-013 | P1 | relay P2P próprio viola custo zero | relay consome tráfego/abuso/operação | PoC e4mc/World Host; sem SLA e sem relay Aurora agora |
| R-014 | P2 | bucket de aparência público | URL vazada é acessível; 1 GB/egress limitado | retenção, remoção, chaves aleatórias e migração opcional R2 |
| R-015 | P2 | nonce IPC observável no argv | propriedade JVM é visível a processo local | single-client, TTL, vínculo ao processo e canal herdado futuro |
| R-016 | P2 | Edge TTS não oficial/instável | teste depende de serviço externo e está ignored | fallback sem voz, aviso de privacidade e adaptador substituível |
| R-017 | P2 | CAS GC apaga blob vivo | subsistema ainda ausente | raízes explícitas, GC dry-run, transação SQLite e fixtures de crash |
| R-018 | P2 | conteúdo CurseForge sem autorização | alguns arquivos não têm URL direta | manter página oficial/manual; nunca contornar escolha do autor |

## Política

P0 impede merge/release da capacidade afetada. Risco aceito precisa de responsável, prazo/revisão e fallback; “gratuito” não justifica esconder indisponibilidade ou remover controles.

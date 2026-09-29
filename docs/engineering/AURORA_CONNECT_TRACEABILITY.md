# Aurora Connect V2 — rastreabilidade e registro de alterações

Data: 29/09/2026. Base original: `dc2ec5ac36a88f54b1130bac4789f8ecd56641d2`. [Diagnóstico e autoridade](AURORA_CONNECT_REVIEW.md), [roadmap completo](MASTER_ROADMAP.md), [task file da revisão](../task-aurora-connect-v2-docs.md). Este arquivo registra requisitos e mudanças documentais; não é relatório de teste de transporte.

## Convenções de evidência

- `REQ-V2-*`: agrupamentos dos requisitos explícitos do solicitante, criados nesta revisão sem substituir IDs de tarefa existentes.
- `EST-01..12`: leituras estáticas realizadas e descritas no [backlog](AURORA_CONNECT_BACKLOG.md). Não são testes.
- `DOC-01`: leitura integral do roadmap canônico e comparação do diff de preservação. `DOC-02`: conferência documental de links, nove targets/IDs e escopo de arquivos. `DOC-03`: revisão independente de consistência e correções registradas no task file. Esses IDs apontam ao processo documental desta entrega, não a executáveis/hashes de runtime.
- `DEP-01..17`: dependências pesquisadas em [zero-spend](AURORA_CONNECT_DEPENDENCIES.md), com fontes/datas e lacunas explícitas. Pesquisa externa não prova implementação.
- `EV-R4-*`: resultados futuros exigidos no backlog, **sem arquivo ou aprovação produzidos**. `EV-R4-006-SVC` e `EV-R4-006-PV` devem ser independentes.

## Requisito → documento → tarefa → evidência

| Requisito e origem no pedido | Documento/contrato responsável | Tarefa | Evidência existente / prova ainda necessária |
| --- | --- | --- | --- |
| REQ-V2-01 — §1: preservar organização, R0–R5, prioridades, IDs, gates, ownership e nove pares | Master Roadmap; diagnóstico; backlog; plano paralelo | todas; gates AUR-R0/AUR-R1 preservados | DOC-01/02, EST-01/02; gates anteriores não reexecutados nem aprovados aqui |
| REQ-V2-02 — §§1/10: distinguir original, novo requisito, proposta, código/teste e pendência | diagnóstico; ADR proposta; task file | 001 e todos relatórios | inventário EST-01..12; sem teste executado de Connect |
| REQ-V2-03 — §2: público, privado cooperativo, dedicado e LAN; transporte/porta/LAN/auto-join/sync separados | spec, modos e fluxos; backlog | 001/005/007 | EV-R4-005: cenário real e evidência específica por capacidade |
| REQ-V2-04 — §2: sem VPN/driver/admin permanente/portas manuais no fluxo normal; host disponível, sem hosting 24h | roadmap; spec; prompt | 004/005/007 | EV-R4-005/007; PoC explícita não é UX final |
| REQ-V2-05 — §3: direto preferido, relay gratuito permitido; sem proibição geral nos testes | roadmap; ADR; dependências; D-003; prompt | 001/004/007 | DEP-02/03/07/09..11; EV-R4-004 inclui sucesso por relay autorizado e falha sem caminho permitido |
| REQ-V2-06 — §3: zero cobrança obrigatória/automática; software ≠ infraestrutura; teste ≠ produção | dependências e ADR | 001/004/007 | nenhum relay produção aprovado; conta/plano/bloqueio financeiro/termos atuais antes de habilitar |
| REQ-V2-07 — §§3/6: fluxo relay inicial/direto, NAT/descoberta/sinalização distintos e UDP externo bloqueado | spec; ADR; dependências | 004 | EV-R4-004: caminho real, continuidade, impacto de retransmissão e ausência de caminho; sem promessa universal |
| REQ-V2-08 — §4: camadas pequenas, serviço autorizado, host aprova destino; nada de proxy aberto/RCON/LAN geral | spec, registro/autorização; prompt | 001/002/003 | EV-R4-002/003: negar destino arbitrário antes de IO, separar TCP/UDP e sessões |
| REQ-V2-09 — §4.1: TCP byte stream íntegro, bidirecional, backpressure, conexões isoladas, fechamento/cancelamento | spec, TCP; backlog 002 | 002 | EV-R4-002: payload exato, status+jogo/conexões concorrentes, leitura lenta, half-close, negativas |
| REQ-V2-10 — §4.2: UDP limites/conteúdo preservados e retorno isolado | spec, associações/datagramas; backlog 003 | 003 | EV-R4-003: múltiplos peers/serviços e respostas corretas, associações vencidas e tardias |
| REQ-V2-11 — §4.2: DATAGRAM MTU/overhead, filas/frequência/expiração/congestionamento e fragmentação limitada | spec, limites e oversize; ADR | 003/006 | EV-R4-003/006: tamanho real, MTU/mudança de caminho, saturação, nenhum truncamento; fragmentação não implementada |
| REQ-V2-12 — §4.2: voz fora do stream do jogo, políticas por serviço, contenção medida | spec, escalonamento e perfis | 003/006/007 | EV-R4-003/006: TCP+UDP simultâneos e jitter/perda/idade; compartilhamento de conexão ainda hipótese |
| REQ-V2-13 — §5: SVC/PV existentes, versões/loaders/APIs reais, sem voz própria/coexistência obrigatória | spec, adaptadores; backlog matriz; DEP-15/16 | 001/006 | referências oficiais datadas; artefatos não selecionados/homologados; EV-R4-006-SVC/PV separados |
| REQ-V2-14 — §5: escuta/anúncio/ponte, voice_host/[host.public], misto e múltiplas instâncias | spec, endereçamento dos mods | 005/006 | EV-R4-006: anúncio recebido por cliente, porta efetiva, API ou restrição; modo misto não anunciado sem prova |
| REQ-V2-15 — §5: API/add-on/config reversível, hook Core mínimo, sem Companion/reescrever Minecraft cifrado | spec; ADR-018 preservada; prompt | 006 | contrato/API efetiva e matriz impactada; regressões Core/Skins quando houver hook; PoC 002 independe de Core |
| REQ-V2-16 — §5: config com plano/diff/snapshot/recuperação, concorrência e licença/updater | spec, transações; backlog 006; DEP-17 | 005/006 | EST-08/09 são padrões lidos, não suporte de voz; EV-R4-006 exige apply/restore/crash/divergência |
| REQ-V2-17 — §6: stack real, Iroh primeiro, Quinn e Go condicionais, versões fixadas | ADR; dependências; inventário | 001 | EST-03/04; DEP-01/05/06; build/API final futura, nenhum novo runtime nesta entrega |
| REQ-V2-18 — §6: e4mc/World Host comparados por mérito; relay próprio adiado; ICE+QUIC não automático | ADR proposta e nota ADR-010 | 001/004 | DEP-09/10/11 e fontes oficiais; ensaio integração/voz/custo antes de escolha |
| REQ-V2-19 — §7: chave-peer-sessão-convite-permissões, E2E, expiração/revogação/replay e limites | spec, segurança; prompt | 001/002/003/005 | EV-R4-002/005: identidade errada, sessão cruzada, expirado/revogado/replay, zero IO indevido |
| REQ-V2-20 — §7: tokens/segredos fora de peers/relay/JVM/logs; mods/online-mode preservados | spec; ADR; dependências | todas | revisão de diff/diagnósticos; EV-R4-006/007; sem áudio registrado/analisado |
| REQ-V2-21 — §7: sinalização mínima, regras por participante, retenção, sem jogo/áudio/JAR no Firestore | spec; DEP-12/13/14; prompt | 005 | EST-11 mostra contrato ausente; futuras regras/emulador, quota, expiração lógica e limpeza sem billing |
| REQ-V2-22 — §§7/8: IP não anônimo, logs redigidos, estado backend e evidência por rótulo | spec, UX/privacidade | 005/007 | EV-R4-005: túnel/status/mundo/voz distintos; timeout não diagnostica CGNAT; falha voz mantém jogo |
| REQ-V2-23 — §9: incrementos pequenos ajustáveis; não refazer o que funciona; UDP/voz concretos | roadmap; backlog; prompt | 001..007 | inventário focal; cada tarefa tem dependências, risco, teste, aceite, rollback, estado/evidência |
| REQ-V2-24 — §9: redes reais, mod individual dedicado/LAN, Brasil–Brasil/EUA, carga/métricas/falhas | spec testes; backlog cenários e células | 003/004/006/007 | EV-R4-* futuros; sem percentuais/ganhos inventados; loopback/LAN não provam NAT/áudio |
| REQ-V2-25 — §§9/10: gate TCP não aprova UDP/voz/relay/R4; experimental desativado; sem custo externo | todos contratos Connect e prompt | todas | nenhuma homologação/runtime nesta entrega; liberação por capability/mod/caminho e gates anteriores |
| REQ-V2-26 — §10: prompt autossuficiente, leitura Git/manifests/módulos, mudanças preservadas, incremento real e relatório | CODEX_PROMPTS/AUR-R4-connect-v2.md | próxima execução 001 + menor incremento seguro | DOC-03; comandos confirmados EST-12, nenhum build executado agora |
| REQ-V2-27 — §11: URLs oficiais e data, API da versão selecionada, não confiar em latest | spec fontes; ADR; dependências | 001 e toda mudança de dependência | consulta 29/09/2026, candidato Iroh 1.1.0; mods são referências, pins de artefato pendentes |

## Registro de alterações e justificativa

| ID desta revisão | Arquivos afetados | Alteração e justificativa |
| --- | --- | --- |
| CHG-01 | MASTER_ROADMAP.md | Preserva arquivo inteiro, escala, tabela R0–R5, waves e gates; amplia somente R4 e requisitos transversais necessários. Insere origem/estado para impedir leitura de planejamento como homologação. |
| CHG-02 | AURORA_CONNECT_REVIEW.md | Diagnóstico e índice em sete entregáveis; registra que proibição universal de relay não foi encontrada na base e que a versão canônica substitui o anexo ausente. Não inventa conflito textual. |
| CHG-03 | AURORA_CONNECT_SPEC.md | Contrato completo TCP/UDP, autorização, endereçamento de mods, UX, limites e provas. Fecha a lacuna entre “túnel” e serviços efetivamente utilizáveis. |
| CHG-04 | ADR/PROPOSED-aurora-connect-transport.md; ADR/ADR-010-multiplayer.md; ADR/README.md | Nova proposta sem número inventado; candidato Iroh condicionado a evidência; ADR-010 preservada com precedência explícita V2, alternativas continuam comparadas. |
| CHG-05 | AURORA_CONNECT_BACKLOG.md; CODEX_TASKS/README.md; COMPATIBILITY_MATRIX.md | Acrescenta IDs 001..007 sem colisão encontrada e matriz de investigação de voz, conserva nove targets/histórico, sem certificar compatibilidade. |
| CHG-06 | AURORA_CONNECT_DEPENDENCIES.md; DECISIONS_REQUIRED.md D-003; RISK_REGISTER.md R-013 | Política corrigida de relay/zero-spend, teste separado de distribuição, dependências desconhecidas desabilitadas e custo baseado em evidência do provedor. IDs antigos conservados. |
| CHG-07 | ARCHITECTURE_REVIEW.md; RESEARCH_REPORT.md | Atualiza resumo multiplayer e vincula pesquisa histórica à revisão V2; não reescreve resultados antigos nem exclui voz como futuro indefinido. |
| CHG-08 | CODEX_PARALLEL_PLAN.md | Remove instrução residual de iniciar HUD, por conflito com ADR-018, e marca baseline histórico. Preserva ownership, waves e integração. |
| CHG-09 | CODEX_PROMPTS/AUR-R4-connect-v2.md; CODEX_PROMPTS/README.md | Prompt completo para execução posterior: inventário, refinamento e implementação efetiva mínima, sem gates falsos, push/publicação/billing/reset. |
| CHG-10 | AURORA_CONNECT_TRACEABILITY.md; README.md; ../task-aurora-connect-v2-docs.md | Liga requisito, contrato, tarefa e prova, organiza leitura e registra arquivos/validação/rollback sem criar outra cópia de roadmap. |
| CHG-11 | CODEX_PROMPTS/AUR-R4-connect-v2.md; este registro; ../task-aurora-connect-v2-docs.md | Pedido posterior do usuário: consolidar o anexo de implementação e orientar execução autônoma por incrementos consecutivos, com ações humanas mínimas e reproduzíveis. Reconsulta oficial em 29/09/2026 e descoberta de revisão paralela sobre alpha.14 exigem reconciliação de base, IDs e schemas antes de implementar. Não fixa Iroh 1.1.0 por antiguidade, não aprova 1.3.0 por `latest` e não modifica o código ou integra branches nesta revisão. |

## Pendências que não viraram “concluídas”

Seleção final de biblioteca/toolchain/API; autorização de relay para testes e, separadamente, distribuição; defaults de lookup/NAT/portmapper; plano/capacidade das contas existentes; identidade Aurora vinculada a peer; sinalização e limpeza limitada; tamanho real de UDP/MTU em cada mod; compatibilidade da API por artefato/loader; LAN e modo misto; perfis de carga/limites calibrados; entrada real no mundo e voz validada; gate R4 e todos os anteriores aplicáveis. Os documentos não criam aprovação por ausência de evidência.

## Regra para revisões futuras

Antes de alterar requisito/estado, registre fonte, motivação e escopo; preserve IDs e histórico de evidências. Um resultado só preenche a célula exata de mod/versão/loader/host/caminho testada. Revalide termos e conta antes de habilitar infraestrutura e não apresente contador local como garantia financeira. Se nova implementação já resolver uma tarefa, audite-a e atualize a evidência em vez de impor retrabalho pela sequência deste backlog.

Adendo de 29/09/2026: o prompt consolidado substitui a limitação operacional de terminar após um único incremento. Os estados da entrega documental e os IDs locais acima continuam históricos, sem aprovação de runtime. A revisão paralela `codex/aur-r4-connect-v2-review` usa outra divisão de `006/007/008` e outro contrato proposto; o prompt exige mapeamento semântico explícito antes de reunir evidências. Nenhuma tarefa foi renumerada nem marcada concluída por esse adendo. Requisitos adicionais de continuidade, reconciliação e intervenção humana ficam rastreados em CHG-11 e na seção “Consolidação posterior” do task file.

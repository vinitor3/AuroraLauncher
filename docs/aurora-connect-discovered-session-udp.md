# Porta de jogo descoberta na sessão TCP+UDP — incremento64

Data: 14/10/2026. Base `778187a`, branch `codex/aur-r4-connect`. **PROTÓTIPO**, sob `connect-lab` desligada por padrão. Reutiliza a [descoberta48](aurora-connect-desktop.md#descoberta-de-porta-da-instância--incremento48) no [perfil62](aurora-connect-desktop-fragmented-session.md). A descoberta continua restrita ao processo filho vivo do launcher no Windows.

## Bloqueio resolvido e contrato

O perfil `sessionFragmentedGameUdp` podia usar somente destinos manuais, mesmo quando o launcher já possuía uma seleção de jogo verificada. Agora o anfitrião pode escolher a porta TCP descoberta do jogo e informar separadamente o destino UDP loopback autorizado. UDP não é descoberto, identificado como mod ou autorizado pela descoberta TCP.

O comando novo `connect_prepare_session_fragmented_host_discovered` recebe `{selectionId, request:{mode,bindAddress,udpTarget,admissionSeconds,sessionSeconds}}`. O DTO recusa campos desconhecidos e não aceita `gameTarget`, PID, instância, execução ou fingerprint fornecido pelo frontend. A janela principal é o proprietário. A seleção opaca de uso único vem da revisão e consulta STATUS48; é consumida e revalidada antes de preparar o anfitrião. Expiração, mudança do processo ou listener exigem nova seleção.

A capability opcional `sessionFragmentedUdpInvitations.ownedGameTcpDiscovery` permite ao painel oferecer essa combinação somente quando o nativo a suporta. Ausência conserva compatibilidade com62/63. O perfil curto57 continua manual. Seleção de processo não amplia prazo, serviço ou versão de Minecraft: protocolo STATUS767 e `worldState=notVerified` permanecem.

A revisão mostra a origem de game/TCP como `ownedInstance`, com a instância e a execução observadas, e o endpoint aux/UDP como manual. O digest inclui a origem privada já vinculada ao handle/criação do processo e ambos os destinos. Somente a confirmação abre o transporte. Chaves privadas, fingerprint e destino privado do host não são enviados ao convidado.

## Vida da sessão

O guard original é mantido na sessão, revalidado antes de abrir o transporte e pelo monitor existente a cada1s. Perda da origem encerra TCP e UDP próprios com `connect_source_changed`, aguardando liberação; não encerra Java nem altera configurações. Uma porta reaberta por outro processo não transfere a autorização.

Admissão até600s, sessão absoluta até3600s, recuperação UDP manual63 e consumo único continuam inalterados. A validação periódica não torna atômicos observar o listener e conectar: a corrida local já documentada permanece. Não existe prova de mundo aberto, áudio ou conectividade entre computadores por associação.

## Evidências e limites

Foco nativo: **4 PASS/0 falhas/0 ignorados**,6,04s, incluindo duas origensUDP4096 após ingresso, processo encerrado/porta reaberta por outro processo, seleção vencida/pedido inválido, origem perdida após revisão e UDP ocupado sem derrubar TCP. Interface: **81 PASS/0 falhas/0 ignorados**, com escolha explícita, consumo da seleção, resposta perdida/atrasada e revisão de origens distintas. Build passou com aviso anterior de chunk grande. QA independente sem bloqueador identificado.

Regressões completas: **266 PASS/5 ignorados padrão** e **357 PASS/42 ignorados combinado**, zero falhas,44,62s/168,77s somados. Clippy de todos os targets padrão/Connect/combinado, fmt de ambos os crates e diff passaram. Logs privados `discovered-session-64-*` e `session-udp-discovery-64-frontend-*`; comandos exatos e escopo compilado no [task64](task-aur-r4-connect.md#incremento64--porta-tcp-descoberta-na-sessão-tcpudp). A biblioteca65 foi compilada em paralelo sem alterar as APIs loopback exercitadas64. Locks e matriz permaneceram iguais.

Fixtures nativas usam processo filho próprio, listeners reais, STATUS sintético e dois agentes autenticados; a interface usa DOM/IPC controlado. Nenhum desses resultados equivale a Minecraft LAN real, janela Tauri ou áudio. SVC e Plasmo continuam separados; nenhum JAR, configuração pessoal, firewall, roteador ou relay é alterado.

Rollback operacional: Encerrar Connect e aguardar `cleanupState=complete`; o jogo permanece independente. Rollback de código: revert local revisado deste incremento. Nenhum push, deploy, instalador, mudança de matriz ou custo é autorizado por este contrato.

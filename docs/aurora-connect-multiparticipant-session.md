# Sessão com vários participantes — incremento71

Estado: **PROTÓTIPO**, implementado com testes locais reais e regressões completas da biblioteca aprovadas. Complementa o [schema3/UDP fragmentado61](aurora-connect-fragmented-session.md) e os [modos diretos65](aurora-connect-direct-ipv4.md), preservando os convites e APIs anteriores. Permanecem normativos a [especificação](engineering/AURORA_CONNECT_SPEC.md), o [prompt](engineering/CODEX_PROMPTS/AUR-R4-CONNECT-V2.md) e o [roadmap](engineering/MASTER_ROADMAP.md).

## Escopo e autorização

Um único Host recebe explicitamente uma lista estática de dois a oito participantes, também limitada por `Limits.max_peers`. Cada participante recebe seu próprio `SessionInvitation` schema3, assinado para seu peer e com ingresso de uso único. Não há convite coletivo, descoberta de contatos, adição de participantes durante a execução, renovação, persistência de identidades ou autorização implícita.

As APIs novas são `Host::start_invited_fragmented_session_group`, `start_invited_fragmented_session_group_external_ipv6` e `start_invited_fragmented_session_group_direct_ipv4`. Recebem `HostConfig` e `&[SessionInvitation]`. O modo de endereço é selecionado pela API; não deriva do JSON. Os Guests usam as APIs schema3 existentes. Bytes canônicos, assinatura, perfil, serviço e envelope UDP permanecem iguais; nenhum campo novo passa a ser aceito em APIs antigas.

Antes de abrir o endpoint, o Host exige uma bijeção completa entre convites e leases, sem IDs ou peers duplicados. Todos devem concordar com a chave do Host, `sessionId`, bind inicial e registro local. A validação de cada participante reutiliza o validador estrito existente sobre a projeção de exatamente seu lease; esta composição não amplia o contrato antigo que exige um único lease. Falta, duplicata ou autoridade adicional recusam o conjunto inteiro.

O registro continua exatamente `game`/TCP reliableStream revisão1 e `aux`/UDP fragmentedDatagram revisão1, original4096B, frame1264B e retorno autorizado16384B/s/20frames/s. Destinos são literais loopback aprovados no Host e não viajam nos envelopes. Orçamentos agregados e por participante existentes não aumentam automaticamente com o grupo.

## Ciclo de cada participante

Cada lease retém o objeto de convite original, inclusive clocks civil e monotônico. Ingresso e sessão podem terminar em instantes diferentes para cada participante; são os instantes assinados e nunca são substituídos pelo maior prazo do grupo. Após a abertura assíncrona do endpoint, todos os ingressos são rechecados antes de publicar o Host. Se qualquer um tiver vencido, o endpoint é fechado e nenhum grupo é retornado.

O Host autentica peer/digest/capacidades e consome somente o convite correspondente sob o mesmo lock de admissão/revogação existente. Falha de ACK não devolve consumo. `invitation_consumed(lease_id)` conserva a decisão individual; `revoke(lease_id)` cancela somente aquele participante e continua linearizado com ingresso. Expirar, desconectar ou revogar A não concede, renova ou cancela a autorização de B. Convite consumido não permite reconectar; outro teste requer novos dados explicitamente preparados.

O endpoint do Host permanece sob o ciclo explícito `shutdown` já existente; o caller é responsável por encerrá-lo. Não se introduz timer coletivo que finalize B quando A terminar. `shutdown` cancela e aguarda todos os recursos próprios, sem encerrar o jogo.

## Isolamento e limites

TCP mantém conexão, limites e cancelamento por participante, além do teto compartilhado. O mapa de associações UDP continua privado a cada conexão autenticada; revisão/serviço/associação não são suficientes para alcançar recursos de outro peer. Remontagem, orçamento por peer, retorno conectado ao endpoint local e rechecagens de autoridade conservam seus escopos originais. A mesma aplicação local pode receber pacotes de sockets distintos; respostas voltam somente pelo socket/associação correspondente.

Não há alteração de anúncio de voz, bind de mod, UI desktop, launcher, Minecraft, Core/Skins ou matriz. Transporte com dois Guests sintéticos não comprova dois jogadores simultâneos, contas mistas, identidade interna de mod ou áudio. Homologação de Simple Voice Chat e Plasmo Voice continua separada.

## Prova local exigida

- Um Host e dois Guests reais simultâneos, TCP distinto e quatro origens UDP com datagramas4096B marcados, respostas atrasadas e ausência de retorno cruzado.
- Revogar A fecha seus TCP/UDP e recusa replay; B mantém tráfego e abre outra associação com sua autoridade original.
- Ingresso e sessão expiram individualmente, sem chegada tardia ao serviço e sem encerrar o outro participante autorizado.
- Consumo e tentativa duplicada de ingresso independentes; limites globais e por peer preservados.
- Conjunto vazio/unitário/excessivo, IDs/peers repetidos, lease faltante/excedente, host/session/bind/grants divergentes, schema2 e modo incorreto recusados antes de bind; APIs antigas continuam recusando múltiplos leases.
- Encerramento aguardado e rebind dos listeners/sockets de todas as fixtures. Nenhum teste em outro PC é inferido.

## Evidência local — 14/10/2026

Quatro testes unitários passaram: atribuição por ID com ordem invertida, grupos/registry/modos inválidos, schema2/vencimento e retenção do relógio original de cada participante. O último avança apenas o relógio privado de A depois de formar o mapa; A vence e B permanece válida, sem alteração do relógio do PC.

Seis testes de integração passaram com endpoint/Host/Guests reais em loopback. São verificados quatro datagramas4096B de origens distintas, retorno marcado e atrasado, TCP simultâneo, continuidade de B após revogação/expiração de A, recusa de ingresso tardio correlacionada ao Host, peer/prova/capacidades/replay sem consumo alheio, limites por participante/globais e17 configurações inválidas antes de abrir endpoint. Barreiras causais exigem que TCP já tenha transferido bytes e ainda não tenha terminado quando os retornos UDP completam; a prova não depende da velocidade do computador. Cleanup aguarda os recursos e comprova rebind das portas próprias.

Cada cenário limita o payload UDP de aplicação a menos de64KiB nos dois sentidos. O teste é de integridade/isolamento, sem saturação prolongada, perda de rede real ou áudio. Os modos IPv4/IPv6 do grupo têm validação negativa local; a execução positiva desta etapa foi somente loopback. Provas65/66 de endereço próprio continuam distintas. Comandos, falhas corrigidas e resultados amplos estão no [registro71](task-aur-r4-connect.md#incremento71--um-anfitrião-com-convites-independentes-por-participante).

Regressão padrão da biblioteca:246 PASS/7 opt-ins ignorados; com relay próprio local:256 PASS/8 opt-ins ignorados. Nenhuma falha nessas rodadas, que incluem os testes focais e não devem ser somadas a eles. A regressão de integração desktop encontrou uma corrida anterior no encerramento; sua reprodução e correção são separadas no incremento71a, sem atribuir aprovação à rodada que falhou.

## Rollback

Desligar o novo caller e usar APIs antigas em uma execução nova; nunca reinterpretar um grupo como lease manual. Encerrar primeiro o Host existente e todos os Guests. Não há migração em disco, custo, relay público, firewall, configuração pessoal, deploy ou instalador neste incremento.

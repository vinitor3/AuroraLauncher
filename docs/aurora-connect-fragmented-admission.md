# Admissão por convite de UDP fragmentado — incremento 51b

Data: 08/10/2026, integração em09/10. Estado: **PROTÓTIPO** de admissão na biblioteca. As quatro entradas explícitas Host/Guest foram implementadas e sete testes reais locais passaram, sem falhas ou casos ignorados. A execução descrita aqui usa portas efêmeras próprias em loopback; não é homologação externa, de voz, jogo ou interface. A [CLI/probe fragmentados](aurora-connect-fragmented-cli.md) tem execução local e orçamento próprios; kit externo permanece separado e pendente.

## Dependências reais e separação de versões

O [incremento 49](aurora-connect-fragmented-udp.md) já oferece `Guest::open_udp_fragmented`, `udp_bridge_fragmented` e remontagem autenticada real sob leases manuais. O perfil de plano/convite do incremento 51 acrescenta `load_fragmented_plan`, `Invitation::issue_fragmented`/`verify_fragmented` e variantes explícitas `_external_ipv6`. O perfil exige exatamente `game`/TCP/revisão1/`reliableStream` e `aux`/UDP/revisão1/`fragmentedDatagram`, com teto original de 1–4096 B e política de retorno assinada.

O envelope do convite continua schema1, domínio `aurora/connect-lab/invitation/v1\0`, limite de 4096 B, geração1 e TTL máximo de 600 s para a sessão inteira. A versão2 aqui é a do envelope UDP. Não selecionar o `SessionInvitation` separado do incremento 52 nem mudar o prazo atual de ingresso/sessão.

As entradas `Host::start_invited` e `Guest::connect_invited`, inclusive suas variantes externas, continuam recusando um objeto com grants fragmentados já verificado. As novas entradas são `Host::start_invited_fragmented(config, &invitation)` e `Guest::connect_invited_fragmented(secret_key, &invitation, limits)`, com variantes de mesmo formato terminadas em `_external_ipv6`. A seleção do perfil é explícita; não depende de inferência a partir de um convite recebido. A entrada fragmentada também recusa um convite legado válido. O positivo executado seleciona loopback; não foi feito um dial externo por essas novas APIs.

## Contrato da integração na biblioteca

O emissor e o receptor devem selecionar de forma independente os dois eixos: perfil fragmentado e endereço inicial loopback ou IPv6 externo. Não pode haver fallback automático para V1, UDP simples, relay ou endereço alternativo por falha de validação.

No host, validar o perfil, o modo de endereço e os prazos antes de criar endpoint. Exigir uma única lease e correspondência exata entre chave host, chave destinatária, sessão, nonce/leaseId, geração, bind, vencimento e descritores dos dois serviços assinados. O destino local aprovado continua fora do convite, literal loopback, com porta não zero. Alterar o teto ou a política de retorno exige outro plano/digest aprovado.

No guest, conferir a chave privada correspondente ao destinatário, o pin independente do host, o perfil e o modo antes do endpoint. Derivar capacidades/grants do convite verificado; não aceitar uma lista adicional fornecida pelo chamador. Listeners de aplicação permanecem loopback. O bind externo de transporte continua sendo uma escolha explícita distinta do endereço de serviço.

A admissão deve reutilizar o fluxo existente: TLS completo, Hello com prova de digest/geração, capacidades exatas, autorização da lease, lock comum a admissão/revogação, consumo único antes do ACK e ausência de devolução do uso em erro/desconexão. Não duplicar uma implementação paralela de replay. `authorize`, watcher, clocks e tokens originais continuam limitando toda a sessão. Não se afirma que o upstream desative globalmente 0-RTT.

Depois da admissão, `openFragmented` só pode abrir `aux` autorizado na revisão assinada. Esperar `fragmentVersion=2` e teto negociado do datagrama original; `maxPayload` não é MTU física. A chamada V1 para esse serviço deve falhar. Autorização, associação e escopo precisam preceder reservas de remontagem e I/O local. Preservar header32B, frame completo até1264B, até8 fragmentos, prazo250ms e quotas já documentadas no incremento49.

## Matriz de testes e critérios

| Caso | Ação real e oráculo obrigatório |
| --- | --- |
| Entrada positiva | Emitir/verificar convite, iniciar Host/Guest pelas novas entradas explícitas e obter admissão real. Um TCP de 1 MiB por direção, half-close e comparação exata; duas origens UDP simultaneamente vivas. |
| Grants e modo | Recusar perfil errado, ausência/alteração de game ou aux, revisão, capability, teto/política divergentes, chave destinatária errada e modo externo recebido por entrada loopback. Antes do endpoint sempre que o erro for local. |
| Defaults preservados | Entradas legadas recusam um `Invitation` fragmentado já válido; reservar destino próprio como canário e exigir zero pacotes/bytes, sem usar uma porta alheia como fixture. |
| Isolamento e fronteiras | Entregar datagramas completos de 0/1427/2108/4096 B; confirmar associação e origem corretas nos dois sentidos, sem entrega parcial ou cruzada. Buffer de recepção deve detectar bytes extras, sem truncar silenciosamente 4097 B. |
| Teto de4097 B | `FragmentedUdpAssociation::send` recusa antes de emitir frames; comparar contadores e canário do destino. Em caso separado, enviar retorno local de4097 B, observar emissão real pelo endpoint e ausência de qualquer prefixo no guest. |
| Retorno autorizado | Convite/plano com `AuthorizedDownstream` explícito permite retorno maior que uma solicitação pequena somente depois do primeiro ingresso válido. A mesma expansão sob `ReplyBudget` é recusada. Zero bytes sintéticos são tratados como datagrama, sem inventar crédito de payload. |
| Replay | Desconectar após consumo, manter host e lease vigentes e tentar de novo com a mesma chave/mesmo convite. Exigir `Replay`, aumento de `denied` e nenhum novo destino, associação ou byte. Não revogar antes desse oráculo. |
| Revogação | Revogar sessão ativa, observar fechamento real e cleanup. Tentar o mesmo convite ainda não expirado, com host retido; correlacionar recusa nativa e `denied`, sem confundir vencimento local com resposta remota. |
| Expiração | Sessão admitida deve fechar pelo vencimento original; nenhuma atividade ou retorno renova a lease. Teste próprio curto e observado, separado de inatividade do transporte. |
| Cancelamento | Fechamento com conexão TCP aberta pela ponte, listeners TCP/UDP e associação UDP aguarda tarefas e solta handles antes do rebind. O teste exige EOF/reset nos dois sockets TCP de aplicação. As regressões internas49 continuam sendo a prova específica de cancelar antes de limpar e de autoridade original revogada; métricas públicas não substituem esses oráculos. |
| 0-RTT | Usar Endpoint real retido com ticket TLS; exigir stream emissor early-data e `ZeroRttStatus::Accepted`. Enfileirar Hello inválido/replay, pedidos TCP e `openFragmented` e um frame V2 antes da decisão. Recusar sem acesso aos canários. Ausência de ticket ou early data rejeitada falham o teste, sem fallback que pareça prova. |

O teste de 0-RTT pode reaproveitar a estrutura de `tests/invitation_resumption.rs`, mantendo exatamente o perfil novo no Hello e a operação UDP `openFragmented`. Aquecer somente a sessão TLS com prova inválida não pode consumir o convite legítimo; um positivo posterior comprova isso. Mesmo com o 0-RTT aceito pelo transporte, o teste não é medição temporal direta de um intervalo pré-TLS: essa barreira depende também da inspeção do caminho Host existente.

Negativos de configuração não devem criar destinos ou fazer dial para endereços públicos de exemplo. Testes positivos usam portas efêmeras próprias e registram todos os handles. Se uma porta reservada precisar ser liberada para o bind Host, qualquer ocupação concorrente deve causar recusa; não procurar outra porta, trocar identidade ou refazer silenciosamente a autorização.

## Orçamento do ensaio sintético

Para o caso positivo principal, distribuir os quatro tamanhos entre as duas origens: A envia 0 e 2108 B; B envia 1427 e 4096 B. Ambas permanecem abertas durante a janela inteira. O total de payload original enviado é 7631 B e o mesmo total retornado é 7631 B: **15262 B, abaixo de 16 KiB somando as duas direções**. Não repetir os quatro tamanhos em cada origem, o que duplicaria esse orçamento.

Esse limite se refere aos datagramas opacos de aplicação, separado do TCP de 1 MiB por direção. Ele não é limite de bytes UDP físicos da rede: o próprio TCP atravessa QUIC/UDP e ainda há headers, criptografia, handshake e controle. Os contadores de frames QUIC e datagramas lógicos devem ser registrados separadamente. Com capacidade completa de 1232 B por fragmento, os quatro tamanhos requerem 1/2/2/4 frames em cada direção; comparar a observação real, sem presumir capacidade ou afirmar medição de PMTU.

Os casos de retorno excessivo de 4097 B e expansão autorizada ficam em fixtures locais separadas, cada uma com teto explícito de 16 KiB de payload tentado nos dois sentidos. Somar essas emissões ao positivo principal excederia o teto conservador. Uma tentativa local `send(4097)` recusada antes do transporte pode acompanhar o positivo sem gerar UDP, mas seu tamanho e recusa precisam constar separadamente, sem contabilizá-la como pacote entregue. A ampliação do roteiro externo para outras emissões negativas depende de revisão do orçamento correspondente; não é autorizada implicitamente por este documento.

O probe CLI preparado separadamente tem outro recorte, revisado explicitamente: teto de **64 KiB de payload UDP de aplicação**, com 46912 B programados. São 30524 B válidos somando ida/volta (os quatro tamanhos em cada origem), duas tentativas de ingresso de 4097 B e dois retornos de 4097 B emitidos pelo endpoint próprio. Esse recorte CLI não muda o orçamento de 15262 B do teste positivo da API e não constitui resultado externo ou aprovação de kit. Seus testes e comandos devem ser registrados separadamente quando concluídos.

TCP e UDP devem se sobrepor de fato: registrar progresso TCP entre o primeiro e o último datagrama, usando envio TCP em blocos e prazo limitado, sem benchmark ou loops de volume ilimitado. O orçamento é consumido uma única vez; perdas ou recusas produzem resultado incompleto, sem retransmissão do fixture para obter um PASS.

## Roteiro externo preparatório

Este roteiro ainda não dispõe de kit externo fragmentado validado. A CLI nova possui comandos próprios com positivo local, documentados separadamente; não usar o runner47b, cujo perfil é game/TCP apenas, nem substituir seu executável histórico. Preparar binário/kit próprios, hash, plano com game+aux, orçamento, revisão integral e retorno aprovado; executar primeiro o próprio IPv6 autorizado do mesmo PC, com bind anterior ao dial, sem chamar essa amostra de internet/NAT.

No futuro ensaio entre redes, o coordenador controla o anfitrião e prepara seus endpoints sintéticos. O convidado usa um processo e uma identidade em RAM durante entrada/replay/revogação. Troca do convite exige cópia canônica e assinatura/digest conferidos antes de encaminhar, pin independente e verificação nativa no guest; não transportar escape de terminal ou reconstruir texto removendo apenas ANSI. Desvios de relógio devem ser resolvidos pelo fluxo de configuração aprovado, sem ampliar tolerância de validade.

Anfitrião: revalidar o endereço próprio, criar plano/hash novo, iniciar somente endpoints autorizados e registrar admissão/bytes/negações. Convidado: conferir o kit, manter o mesmo processo vivo para convite/aprovação, executar a fixture limitada, devolver somente resumo redigido e encerrar explicitamente. Não há comandos executáveis nesta fase preparatória; o procedimento final deverá separar os dois computadores e incluir cancelamento e recuperação.

O positivo UDP/TCP, replay, revogação e expiração são resultados separados. Um caso positivo externo não aprova automaticamente os negativos. Ao terminar ou falhar, aguardar encerramento de pontes, Host/Guest e ecos próprios; soltar handles/guards antes do rebind de todos os listeners, sockets de transporte, origens e destinos. Não matar Java ou processos pessoais por nome.

## Evidência e limites

Registrar versões/hashes, topologia declarada, prazos, payloads por direção, frames, duas origens/associações, progresso TCP durante UDP, recusas, negações, encerramento e rebind. Manter resultados locais e relatos remotos distintos. Não registrar convites, nonce, peers, IPs, nomes, chaves ou conteúdo de payload no Git ou em resumo compartilhável.

Primeira execução de `tests/fragmented_invitation.rs`: **7 PASS, 0 falhas, 0 ignorados**, em 8,05 s. Log privado `.aurora/connect-validation/connect-fragmented-invitation-fifty-one-b-1.log`. Foram executados:

- positivo assinado, verificado e admitido por dois agentes: TCP 1 MiB por direção, UDP 15262 B somando os dois sentidos, duas origens/associações e 9 frames QUIC DATAGRAM observados por direção; 8192 B de progresso TCP entre o primeiro e o último grupo UDP;
- recusas de perfil, modo, chave destinatária e nove divergências de registry/lease antes de I/O; canário de transporte próprio permaneceu sem pacotes e os destinos sem acesso. As variantes externas receberam um objeto loopback e o recusaram; não houve positivo externo;
- revisão de serviço, downgrade V1, grant desconhecido e protocolo incorreto sem abertura de destinos;
- emissão local de 4097 B recusada antes de qualquer frame; retorno de 4097 B realmente emitido pelo destino sem entrega parcial; retorno de 2108 B após solicitação de 1 B somente com a política assinada `AuthorizedDownstream`;
- replay antes de revogar e posterior recusa de revogação com convite ainda válido: contador `denied` 0 → 1 → 2 e nenhum destino novo;
- fechamento real de sessão ativa por revogação e, em outra fixture, pelo vencimento original de 2 s, com listeners TCP/UDP, associação UDP e rebind após soltar os handles;
- três casos de 0-RTT efetivamente aceito: replay, prova inválida e capacidades divergentes. Pedidos TCP, `openFragmented` e frame V2 antecipados não alcançaram destinos. Os dois erros que não consomem o convite foram seguidos de admissão legítima e retorno UDP real.

Depois da revisão, o caso de revogação/expiração foi reforçado para manter também uma conexão TCP aberta pela ponte: 4 B chegaram ao destino antes do fechamento, e os dois sockets de aplicação precisaram observar EOF/reset após `QUIC closed`. Esse delta passou em foco próprio: **1 PASS, 0 falhas, 0 ignorados, 6 filtrados**, em 2,15 s, log `.aurora/connect-validation/connect-fragmented-invitation-fifty-one-b-active-tcp.log`. Os outros seis casos não foram alterados. Clippy focal final passou novamente e a revisão independente não encontrou bloqueador. A suíte completa posterior fica sob coordenação do integrador; não somar este foco como um oitavo teste distinto.

Comando executado, a partir da worktree: `cargo test --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --target-dir .aurora/connect-validation/relay50-target --test fragmented_invitation -- --test-threads=1 --nocapture`. O foco final usou o mesmo comando até o nome do teste de integração, seguido de `active_revocation_and_original_expiry_close_bridges_and_release_all_sockets -- --exact --nocapture`. Clippy focal usou `cargo clippy` com os mesmos manifest/target/jobs, `--test fragmented_invitation -- -D warnings`. A primeira compilação encontrou somente um erro na fixture (`BTreeSet::pop`), corrigido para `pop_first` antes da execução; não foi uma falha do transporte. Os testes não recompilaram o target `apps/connect-lab/target/debug` nem modificaram os kits históricos.

Os novos casos de 0-RTT não repetem revogação/expiração em early data: essas condições permanecem cobertas pelas regressões legadas do caminho compartilhado, cuja execução completa deverá ser registrada pelo integrador. A expiração curta não altera o teto de 600 s. Não houve medição física de MTU, perda, NAT ou travessia entre duas internets. Continuam separados os gates de relay público/NAT/CGNAT, interface desktop, Minecraft, anúncio e áudio real de Simple Voice Chat e Plasmo Voice, cada mod com sua homologação. Preservar os nove pares Core/Skins e todas as fases do roadmap.

Rollback: não selecionar as novas APIs e conservar APIs V1, pins e kits históricos. Reverter o incremento de código somente por commit isolado revisado; o teste e este domínio não alteram configurações pessoais. [Especificação](engineering/AURORA_CONNECT_SPEC.md), [contrato de voz](aurora-connect-voice.md), [convites externos TCP atuais](aurora-connect-internet-invitations.md).

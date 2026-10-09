# Aurora Connect — CLI e probe de UDP fragmentado

Data: 09/10/2026. Incremento 51b. Estado: **PROTÓTIPO**, com quatro testes de integração reais locais e três testes unitários novos aprovados. Complementa a [admissão fragmentada](aurora-connect-fragmented-admission.md), o [perfil do convite](aurora-connect-fragmented-invitations.md) e o [transporte V2](aurora-connect-fragmented-udp.md). Não constitui kit externo, homologação de voz, Minecraft ou interface.

O trabalho foi concluído na branch `codex/aur-r4-connect`, sobre HEAD `78df138b78a4bc07b280f7aa880422f3492e48a9`, preservando os demais fontes 51b e a preparação separada 52. Nenhum manifest, lockfile, matriz Core/Skins, pin de caller ou kit histórico mudou nesta parte do incremento. A dependência de transporte continua Iroh 1.3.0 fixada. Não houve alteração de configuração pessoal, Java, firewall ou relógio.

## Seleção explícita e aprovação

Os sete comandos novos pertencem somente ao binário experimental `aurora-connect-lab`:

| Comando | Finalidade e argumentos |
| --- | --- |
| `plan-fragmented` | `<plano.json>`: valida o perfil V2 e apresenta plano/hash |
| `host-invite-plan-fragmented` | `<plano.json> <sha256-aprovado>`: host com endereço de transporte loopback |
| `host-invite-plan-fragmented-external-ipv6` | Mesmos argumentos; endereço inicial IPv6 externo explicitamente selecionado e validado |
| `guest-invite-fragmented` | `<host-public-id-fixado> <tcp-local> <udp-local> <segundos>`: requer convite V2 com endereço loopback |
| `guest-invite-fragmented-external-ipv6` | Mesmos argumentos; requer perfil de endereço IPv6 externo |
| `echo-fragmented` | `<tcp-loopback> <udp-loopback> <segundos-10-a-600>`: endpoint sintético próprio e limitado |
| `probe-fragmented` | `<ponte-tcp-loopback> <ponte-udp-loopback>`: uma execução limitada contra esse endpoint |

Todos os serviços de aplicação continuam em loopback. O perfil exige exatamente `game`/TCP/`reliableStream` e `aux`/UDP/`fragmentedDatagram`, revisão 1, com limites e política de retorno assinados. Não há opção TCP-only fragmentada, inferência do perfil por conteúdo, fallback V1 ou relay. O plano/hash aprovado continua sendo a autorização do destino local no host; a identidade pública do convidado é inserida no mesmo processo antes de emitir o convite. O convidado recebe o pin do host por canal autorizado, verifica a assinatura, apresenta o resumo e exige `approve <digest>` exato antes de abrir transporte ou listeners de aplicação.

Somente os novos comandos V2 têm preparação de até 600 s por etapa interativa, também limitada pelos prazos restantes aplicáveis. Convites continuam schema 1, no máximo 4096 B e validade de até 600 s para a sessão inteira. A aprovação não renova essa validade. Os comandos anteriores conservam preparação de 60 s. O leitor comum agora confere o prazo monotônico antes e depois de esperar input; EOF sem fim de linha continua sendo recusa, inclusive se o texto parcial era um digest correto. A fase inicial V2 também verifica seu prazo civil e a aprovação revalida o vencimento do convite antes da conexão.

Enviar uma linha e pressionar Enter, aguardar o evento correspondente e manter o mesmo processo. Não colar JSON ou `approve` no prompt `PS ...>` depois de o programa terminar. `stop` seguido de Enter, Ctrl+C, prazo ou fechamento da conexão encerram a ponte. Chaves e convites ficam em RAM; uma thread bloqueada na leitura do terminal pode permanecer até a saída do processo, sem bloquear o runtime Tokio.

`guest_ready` distingue `approvedOriginalDatagramBytes` do teto de frame completo, `maxFragmentFrameBytes=1264`, e declara `associationCeilingNegotiated=false`: o listener criado ainda não equivale a uma associação UDP negociada. A abertura ocorre para cada origem local conforme o tráfego e os limites existentes. Ao encerrar, `fragmented_transport_snapshot` fornece os deltas observados de frames QUIC DATAGRAM; não os chama de pacotes físicos ou áudio entregue.

## Fixture limitada e oráculos

A fixture usa protocolo de controle próprio sobre o TCP autorizado, somente para ler seus contadores. Não é protocolo de Minecraft, mod de voz ou extensão do wire Connect. O controle tem framing fixo, até 32 consultas; a fixture aceita no máximo quatro conexões TCP e uma carga TCP. Todos os endereços são literais loopback, sem DNS, scopeId ou flowinfo. Bind efêmero zero é permitido somente na criação de listeners próprios.

O probe mantém duas origens UDP abertas. Cada origem envia os tamanhos 1427, 0, 2108 e 4096 B, uma vez cada; o primeiro pacote não vazio permite identificar a origem antes do datagrama vazio. O retorno aplica XOR ao padrão sintético para que o oráculo compare bytes distintos, sem decodificar qualquer conteúdo de aplicação real. O probe observa ambos os sockets, recusando duplicatas, prefixos, retorno à origem errada ou mensagem inesperada. As origens e os destinos nunca vêm do payload.

Antes do último datagrama válido de cada origem, o probe tenta enviar 4097 B, observa silêncio por 500 ms e exige zero chegadas desse tamanho no endpoint. Para cada retorno válido final, o endpoint também emite primeiro 4097 B; o probe exige que esse retorno não apareça nem como prefixo. O datagrama válido de 4096 B posterior demonstra continuidade após a recusa. Os contadores do endpoint são lidos por TCP; emissão local não é prova de entrega nem identifica isoladamente a causa de um descarte.

O orçamento é **64 KiB de payload UDP de aplicação**, incluindo as tentativas acima do teto: 15262 B válidos por direção, 30524 B válidos somando ida/volta, mais quatro emissões tentadas de 4097 B, total **46912 B programados**. Esse recorte difere do teste positivo menor da API, documentado separadamente. Não conta headers, TLS, controle ou UDP físico do QUIC; também não inclui a carga TCP adicional de 1 MiB por direção. Não há retransmissão da fixture para fabricar sucesso.

A carga TCP é cadenciada em blocos de 32768 B a cada 200 ms, com comparação exata e half-close. Seus últimos três blocos aguardam a conclusão UDP. Contadores antes/depois do grupo UDP precisam mostrar progresso TCP real sem a carga já ter terminado. O contador `tcpBytesForwardedDuringUdp` mede bytes recebidos e validados no endpoint próprio, explicitado por `tcpProgressMeasurement=validated_bytes_received_at_owned_endpoint`; a comparação integral no probe ainda exige retorno byte a byte de 1 MiB. A publicação ocorre antes do eco do bloco, impedindo que seu recebimento acorde o probe antes de o snapshot enxergar o progresso já ocorrido. Isso demonstra sobreposição limitada, não saturação, fairness ou desempenho homologado. O probe tem prazo total de 60 s e cancela com Ctrl+C. O echo limita cada tarefa TCP a 60 s e sua vida total ao prazo explicitamente escolhido, até 600 s. Encerramento aborta e aguarda somente as tarefas próprias antes de liberar os listeners.

O positivo executado usou `ReplyBudget`, pois os retornos válidos têm exatamente o tamanho da solicitação; o retorno de 4097 B deve ser recusado pelo teto antes de obter entrega. Não anuncia aprovação da expansão `AuthorizedDownstream`, coberta por outra fixture da biblioteca. Os buffers de recepção de 65536 B detectam qualquer datagrama UDP local completo acima do teto de 4096 B sem truncamento silencioso; são alocados no heap com tamanho fixo.

## Execução e evidência

Diretório de execução: `C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-r4-connect`. Todos os builds usaram `.aurora/connect-validation/relay50-target`, modo offline/locked e dois jobs. `apps/connect-lab/target/debug` e os executáveis históricos com pins/firewall permaneceram intactos.

Comandos finais executados:

```powershell
cargo test --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --target-dir .aurora/connect-validation/relay50-target --test fragmented_cli -- --test-threads=1 --nocapture
cargo test --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --target-dir .aurora/connect-validation/relay50-target --bin aurora-connect-lab fragment_probe::tests -- --nocapture
cargo clippy --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --target-dir .aurora/connect-validation/relay50-target --bin aurora-connect-lab --test fragmented_cli -- -D warnings
rustfmt --edition 2024 apps/connect-lab/src/main.rs apps/connect-lab/src/fragment_probe.rs apps/connect-lab/tests/fragmented_cli.rs
cargo fmt --manifest-path apps/connect-lab/Cargo.toml --all -- --check
```

| Evidência | Resultado |
| --- | --- |
| `fragmented-cli-51b-focused.log` | Preparação de 08/10: 1 PASS, 2 FAIL, 7,19 s; saída dos filhos sem evento. Registro preservado, sem promoção a sucesso. |
| `connect-fragmented-cli-fifty-one-b-1.log` | Primeira rodada desta retomada: 1 PASS, 2 FAIL. O probe sofria stack overflow no Windows devido a buffers grandes embutidos em futures, inclusive antes de validar argumentos. A fixture detectou a saída sem evento. |
| `connect-fragmented-cli-fifty-one-b-2.log` | Após mover os buffers limitados para heap: 3 PASS, zero falhas/ignorados, 7,63 s. |
| `connect-fragmented-cli-fifty-one-b-final.log` | Snapshot anterior à última revisão: 4 PASS, zero falhas/ignorados, 9,71 s. Inclui a correção de publicação de `tcp_completed` antes do EOF e o negativo de caminho UDP sem ponte. |
| `connect-fragmented-cli-unit-final.log` | Três testes novos PASS, cinco outros filtrados; 0,00 s. |
| `connect-fragmented-cli-clippy-final.log` | Binário e teste de integração passaram com `-D warnings`, 3,70 s. |
| `connect-51b-full-default.log` | Suíte completa executada pelo integrador antes do último ajuste de ordem do contador: 171 PASS, quatro opt-ins ignorados, exit 0, 90,87 s agregados. |
| `connect-51b-full-feature.log` | Suíte completa com `local-relay-lab`, também anterior ao último ajuste do contador: 181 PASS, cinco ignorados, exit 0, 96,88 s agregados. |
| `connect-fragmented-cli-counter-order-final.log` | Último snapshot: 4 PASS, zero falhas/ignorados, 9,67 s; inclui o contador publicado após validação e antes do eco. |
| `connect-fragmented-cli-counter-order-unit.log` | Último snapshot: três unitários PASS, cinco filtrados; 0,00 s. |
| `connect-fragmented-cli-counter-order-clippy.log` | Último snapshot: Clippy binário/teste com `-D warnings` PASS, 1,65 s. |
| `connect-fragmented-cli-counter-order-fmt.log` | Formatação de toda a crate com `--check` PASS, exit 0. |

Os logs são privados em `.aurora/connect-validation/`; não contêm cópias de convites ou dados de participantes reais. Não somar rodadas repetidas como testes distintos. As suítes completas incluem os focos anteriores; não foram repetidas depois da última alteração restrita à fixture. Essa alteração foi coberta pelos quatro testes CLI, três unitários, Clippy e formatação finais acima. A revisão independente e a integração são registradas pelo integrador no task file.

O positivo real cria quatro processos próprios: echo, Host, Guest e probe; a consulta de plano é um processo preparatório que termina antes. O teste emite/verifica convite, faz admissão autenticada, transporta os bytes TCP/UDP, exige pelo menos 18 frames QUIC DATAGRAM observados por direção e espera `guest_stopped`, `host_stopped` e `echo_fragmented_stopped`. Após soltar todos os handles, faz rebind dos listeners Guest, destinos echo e socket de transporte Host; o próprio probe também verifica rebind de suas origens UDP. O resumo compartilhável contém somente contadores, sem endereço, peer ou convite.

Os três negativos de integração são: argumentos/TTL/perfil legado/modo incompatível recusados antes de identidade; aprovação com EOF sem Enter recusada como `invalid_protocol`, com canário de transporte próprio sem tráfego; e probe ligado diretamente ao echo por TCP/UDP comum, que precisa falhar com `fragment_probe_oversize_delivered` quando 4097 B alcança o endpoint. Esse último oráculo impede chamar um caminho sem a recusa de fronteira de resultado aprovado.

## Limites e rollback

TCP e UDP V2 convidados estão verificados nesta amostra loopback com dois agentes. As variantes de CLI IPv6 externo foram implementadas e o modo errado foi recusado, mas o positivo externo dessas variantes permanece **PRECISA DE TESTE**. Não foi criado ou enviado kit novo. Esta CLI não substitui o runner TCP47b do teste externo em andamento, não aumenta TTL e não corrige por si só os timeouts de outra rede.

Replay/revogação/expiração e 0-RTT V2 foram testados pela API em fixtures separadas, conforme o domínio de admissão; este probe não implementa um runner externo de replay. Voz audível Simple Voice Chat e Plasmo Voice, seus anúncios e restauração de configuração, jogo gráfico, LAN, IPv4/NAT/CGNAT, relay público e UI continuam com gates próprios. Bytes sintéticos de tamanhos motivados por mods não equivalem a áudio real. Não houve custo, conexão a provedor externo, publicação ou mudança da matriz.

Rollback operacional: encerrar os quatro processos próprios por `stop`/Ctrl+C e aguardar saída; os testes de falha fazem reap apenas de seus filhos. Não há arquivo pessoal a restaurar. Rollback de código exige reverter o incremento isolado revisado; os comandos anteriores, pins e kits continuam disponíveis. O próximo teste externo exige binário/kit novo, hashes, plano/destinos aprovados, duas máquinas e procedimento separado para anfitrião/convidado com encerramento. Sua preparação e aprovação não são inferidas deste resultado local.

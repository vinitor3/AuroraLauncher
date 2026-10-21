# Desativação independente de serviço UDP no Host — contrato81

Data21/10/2026. Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**, focais, regressões e revisão independente aprovados. Escopo exclusivo da biblioteca `apps/connect-lab`. Base auditada da proposta: branch `codex/aur-r4-connect`, HEAD `a279447a68ec180d267bda45f0799545a4908487`; base de implementação após integrar80: `9213cc834206d838f139debb50e087c3a94bfed4`. Revisão integral de root e QA aprovou o contrato de SHA256 `1506b8ed1b3bdc24d35091e73c278dc57bc8549f203bcbd017069abbb0b13dfc`, antes destas atualizações de progresso. A integração80 tem provas próprias e não é validação81.

Autoridades: [especificação](engineering/AURORA_CONNECT_SPEC.md), [backlog canônico](engineering/CODEX_TASKS/AUR-R4-CONNECT-BACKLOG.md), [roadmap](engineering/MASTER_ROADMAP.md), [framing49](aurora-connect-fragmented-udp.md), [sessão61](aurora-connect-fragmented-session.md), [grupo71](aurora-connect-multiparticipant-session.md), [recuperação UDP63](aurora-connect-desktop-udp-recovery.md) e [ownership79](aurora-connect-guest-source-cleanup.md). O vínculo de voz continua não implementado;81 resolve somente uma de suas dependências de transporte, sem depender de uma proposta em andamento para definir sua autoridade.

## Bloqueio e menor resultado útil

`Host::revoke` cancela a lease inteira; o watcher de `serve_peer` fecha a conexão autenticada e seus streamsTCP. Isso é correto para revogar um participante, mas não satisfaz a invalidação exclusiva de voz/UDP com jogo saudável. O contrato de produto exige que falha de voz não encerre o jogo automaticamente.

O menor incremento é permitir ao proprietário nativo do Host retirar permanentemente a autorização UDP de uma combinação exata de lease, serviço e revisão. A mesma conexão continua autorizada para os demais serviços e conserva os relógios originais. O caso motivador é `aux` de um participante schema3, sem modificar `game`/TCP. A API também deve aplicar seus gates aos serviçosUDP V1 já existentes quando explicitamente invocada para eles; não deixar um envelope alternativo contornar a decisão.

Não acrescentar binding de configuração, descobertaUDP, Guest interno, supervisor dedicado, comando Tauri, painel, CLI, capability remota ou protocolo de voz neste incremento. Não modificar Core/Skins, Assistente, Recovery/CAS/Doctor, matriz, lockfiles, dependências, pins ou kits históricos. Nenhum relay público, serviço externo, firewall, Java, mod, microfone ou configuração pessoal é necessário. Os recursos desktop continuam experimentais e desligados por padrão; a API de biblioteca não habilita uma ação pública por consequência.

## API e escopo da autoridade

Novo método de `Host`, usando o `Result` e o enum `Error` já existentes:

```rust
pub async fn disable_udp_service(
    &self,
    lease_id: &str,
    service_id: &str,
    service_revision: u32,
) -> Result<()>;
```

Este método não estava disponível na base auditada. A implementação passou nas seis integrações públicas,23 provas privadas e regressões gerais registradas abaixo. O chamador já deve possuir a instância nativa de `Host`; não há entrada de rede que permita a um Guest desativar outro participante. O método não aceita endereço, peer alternativo, novo grant, prazo, orçamento ou token de voz.

| Entrada ou estado | Resultado exigido, antes de qualquer efeito novo |
| --- | --- |
| IDs que não passam pelo validador `valid_id` existente, ou revisão0 | `Error::Configuration`. |
| Lease desconhecida, serviço desconhecido, serviço não concedido à lease ou serviçoTCP | `Error::ServiceDenied`; não alterar outra autoridade. |
| Serviço concedidoUDP com revisão não zero diferente da revisão original | `Error::StaleRevision`; não desativar uma revisão diferente. |
| Combinação válida, ainda habilitada | Selar a desativação, cancelar seu trabalhoUDP e aguardar a liberação efetiva descrita abaixo. |
| Mesma combinação válida já desativada, ainda drenando | Aguardar o mesmo trabalho; não criar operação ou recursos novos. |
| Mesma combinação válida já desativada e sem recursos próprios | `Ok(())`, sem I/O ou nova autorização. |

As validações estáticas precedem a idempotência: um argumento divergente não recebe sucesso por encontrar algum serviço previamente desativado. Desativar uma combinação conhecida é permitido antes da admissão, depois da desconexão e depois da expiração/revogação da lease. Trata-se de retirar autoridade e concluir cleanup, não de autorizar tráfego com prazo vencido. Não depender de `invitation_active` ou do ingresso ainda aberto para permitir o encerramento.

A decisão permanece no Host até seu descarte, inclusive quando o serviço está sem associações, o peer se desconecta ou uma lease manual ainda admite reconexão. Convites continuam com seu consumo único original. Não há método de reativação, reset, alteração de revisão ou recuperação que contorne a desativação. Uma sessão futura com autoridade nova segue os fluxos de preparação e autorização próprios;81 não a cria.

Desativação por lease não é remoção global do serviço. Desativar `aux` de A não altera `aux` de B nem outroUDP autorizado de A. Um futuro caller que invalide a origemUDP de todo o grupo deverá desativar explicitamente todas as leases afetadas;81 não promete atomicidade coletiva nem inclui uma API de lote.

## Semântica aguardada e cancelamento do chamador

A future não executada não produz efeito. Depois que a chamada começou a executar e selou a decisão, abandonar sua future não reabilita o serviço nem abandona seus recursos. O estado de desativação e o trabalho de encerramento pertencem ao Host e aos workers já responsáveis pelos recursos, não à duração do waiter público.

`Ok(())` significa que a combinação continua desativada e que não resta setup, associação, socketUDP de destino, trabalho de envio/retorno ou reserva de remontagem **do Host** pertencente àquela combinação. Outros serviços, a conexãoQUIC, o Endpoint e TCP podem continuar vivos. Não é necessário encerrar o Host para obter essa garantia.

Cancelamento, mapa vazio, contador de métricas zerado, retirada de um `JoinHandle` ou solicitação de `abort` não bastam como prova de término. Workers terminados e todos os owners pertinentes devem ser recolhidos/liberados antes de concluir. Se o primeiro waiter for perdido, uma segunda chamada válida observa a mesma decisão e continua aguardando os recursos originais. Nenhum prazo de autoridade é renovado por essa espera.

Não acrescentar timeout que devolva sucesso enquanto trabalho continua vivo. Os prazos de I/O já existentes permanecem; cancelamento deve acordar as esperas pertinentes. Um gate privado de teste pode reter trabalho para comprovar que o método continua pendente. Não há promessa de duração máxima de wall-clock para agendamento/cleanup do sistema operacional.

## Gate de setup, publicação e encerramento

Introduzir estado privado por lease/serviço/revisão, limitado ao registro estático e aos grants já aprovados. Não criar entradas a partir de IDs desconhecidos recebidos pela rede. A implementação pode extrair um módulo estreito para esse estado; os nomes de structs internas ficam para a revisão de código.

As seguintes propriedades são obrigatórias:

1. A decisão de desativar e o registro de setup pendente usam o mesmo gate de sincronização. O setup obtém registro de ownership antes do primeiro bind/connect ou outro I/O de destino. Se a desativação já venceu essa disputa, nenhum setup novo é registrado ou autorizado.
2. Uma permissão de setup antiga não é autorização permanente para fases posteriores. Revalidar a decisão antes de iniciar I/O de destino, depois de cada espera relevante e antes de instalar/publicar a associação. Operação já iniciada antes da desativação pode completar enquanto se encerra; seu resultado continua pertencendo ao setup e deve ser liberado, sem publicação tardia válida.
3. A passagem de setup pendente para associação publicada não abre intervalo sem tracking. Gate de publicação e desativação devem decidir de modo consistente: associação que venceu antes do selo entra no conjunto cancelado; publicação posterior é recusada. ACK/resposta já iniciada não autoriza manter a associação viva depois de desativá-la.
4. Selar a decisão impede novos acessos autorizados antes de começar a aguardar cleanup. Cancelar associações e esperas antes de limpar remontagens, para não permitir que um receiver recrie estado depois da limpeza.
5. Registro, cancelamento e consulta de término não dependem do loop de controle do peer voltar de uma leitura retida. Uma requisição de controle incompleta não deve bloquear a decisão local de desativar ou manter recursosUDP já cancelados indefinidamente.
6. Nenhum mutex síncrono permanece retido durante `.await`, resposta de rede, bind/connect, espera de worker ou espera do método público. Definir ordem de locks e evitar espera circular entre mapa de associações, estado do serviço, reassembler e conclusão.

Para a fronteira de envio imediato, a decisão do serviço precisa ser linearizada com a última autorização para `try_send`/`try_send_to` ou enqueueQUIC. Uma leitura de token seguida de I/O sem `await` não torna duas threads atômicas. Um gate síncrono curto pode proteger a checagem e a operação imediata; nunca aguardar socket gravável sob esse lock. Registrar no código qual ponto autoriza a operação e qual ponto sela a desativação.

## Ownership real de socket, payload e quotas

A base mantém o mapaUDP dentro de `serve_peer`, e cada `Association` possui `Arc<UdpSocket>`. Recepção/retorno podem reter um clone da associação depois de removê-la do mapa. Consequentemente, contar somente entradas do mapa ou tarefas de retorno deixa uma janela de falso cleanup.

O tracking deve cobrir tanto setup ainda sem associação quanto a vida efetiva dos recursos publicados. Um guard RAII de recurso acompanha o último owner da associação/socket e não apenas sua entrada no mapa. Se existir clone de socket fora da associação, esse clone deve conservar o mesmo ownership; preferir não criar clones avulsos. Não anunciar conclusão antes de liberar o socket por ordem incorreta de drops dos campos/guards.

`fragment::Completed` conserva reservas de bytes/mensagens durante entrega. `Reassembler::clear` libera pendências, mas não libera antecipadamente uma mensagem completa mantida por outro owner. O work guard deve abranger a janela de envio que possui esse payload. Drop/cancelamento libera o payload e suas reservas; não mover seus bytes para uma fila sem contabilização nem zerar manualmente contadores para satisfazer uma asserção.

Retirar associações libera seus permits de capacidade quando os recursos correspondentes efetivamente terminarem. Isso não significa devolver crédito de tráfego, reinicializar buckets ou ampliar limites. Os limitadores por lease/serviço, o orçamento agregado e os pools por identidade permanecem compartilhados como antes. A desativação não cria um bucket novo para cada chamada ou associação e não afeta as reservas de outro participante.

Não mudar os tetos existentes: V2 até4096B por original, frame até1264B incluindo header32B, até8fragmentos, remontagem250ms,8mensagens/32KiB por associação,64KiB por peer e1MiB global. Para `aux` schema3, retorno autorizado16384B/s e20frames/s, contando headers, permanece. V1 conserva o teto dinâmico do serviço/transporte e sua política de retorno aprovada. Esses valores não são orçamento de áudio homologado.

## Autorização nos dois sentidos e compatibilidade

O serviço continua exigindo lease/peer corretos, grants, revisão, capability, clocks civil/monotônico e prazos originais. O novo estado apenas pode negar uma autorização antes válida; nunca a substitui ou prolonga. Não modificar Hello, assinatura, consumo, ALPN, schema1/2/3 dos convites ou envelopesUDP V1/V2.

| Fronteira | Exigência adicional |
| --- | --- |
| `validate_open` e setupUDP | Recusar nova associação desativada antes de acessar destino; registrar/revalidar conforme o gate acima. |
| IngressUDP V1 | Recusar associação desativada antes de enviar ao socket local e interromper envio pendente. |
| IngressUDP V2 | Conferir antes de alocar/remontar, dentro da região que publica a remontagem e antes da entrega local imediata; não completar mensagem antiga depois do selo. |
| RetornoUDP V1 | Conferir antes do enqueueQUIC; cancelamento acorda leitura/espera de retorno. |
| RetornoUDP V2 | Conferir antes de cada fragmento emitido, mantendo reserva e debitando conforme o contrato original. Prefixo já emitido não é retransmitido nem recebe crédito devolvido. |
| FechamentoUDP | Conservar `CloseUdp` e cleanup local existentes; eventual `ServiceDenied` para associação já retirada não deve impedir o Guest de limpar seus próprios recursos. |

Frames malformados, de escopo errado ou serviço desativado continuam submetidos aos limitadores de ingress existentes. Não tornar o descarte uma via gratuita de alocação/remontagem. Não alterar replay fences, revisão, associação, sequência ou geração para conseguir encaminhar novamente.

TCP estabelecido e novas aberturasTCP autorizadas permanecem independentes. Não cancelar o token da lease, o token do peer/Host, fecharQUIC ou consumir/reemitir convite para retirarUDP. Expiração/revogação global continuam prevalecendo e podem encerrar TCP por suas razões originais. Depois de retirar o único serviço ativo, o idle original da conexão ainda pode encerrá-la;81 não mantém sessão ociosa artificialmente.

As APIs antigas mantêm comportamento quando o método novo não é invocado. Nenhuma atualização implícita do caller desktop63: suas tentativasUDP continuam limitadas à mesma conexão/autoridade, sem poder reativar um serviço negado pelo Host.

## Limites explícitos do resultado remoto

`Ok(())` é uma garantia sobre autoridade e recursos do Host. Datagramas já aceitos pelo kernel ou enfileirados no QUIC antes da desativação podem chegar ao destino remoto depois. A API não recolhe bytes já entregues, esvazia filas internas remotas, comprova silêncio audível ou garante zero payload remoto instantâneo.

O protocolo atual não possui aviso específico Host→Guest de serviço desativado. O Guest remoto pode conservar associação e listener local, e envios locais podem manter seu relógio de atividade mesmo sem retorno. Portanto, nem timeout eventual da ponte nem mudança automática de `udpState` para failed fazem parte deste contrato. A rejeição de uma nova abertura não atribui por si só a causa a uma mudança de configuração de voz.

Aviso remoto confiável, distinção pública entre desativação e outras recusas ou fechamento coordenado das pontes remotas exigiriam contrato de protocolo/capability e testes próprios. Não acrescentar esses mecanismos incidentalmente em81. Um futuro Guest local pertencente ao mesmo caller pode ser encerrado por esse proprietário por ação interna explícita, mas não faz parte desta entrega.

## Plano de provas locais causais

Todos os testes usam bytes opacos, listeners próprios loopback e relógios privados quando necessário. Gates de fixture ficam em `cfg(test)` e não recebem endereços, executáveis ou poderes novos em produção. Root coordena Cargo depois do commit80; nenhum comando deste plano foi executado ao escrever este contrato.

### Fase1 — validação e baseline do contrato

- Confirmar recusa sem efeito para IDs/revisão inválidos, lease/serviço/grant desconhecidos e TCP. Método válido é idempotente antes da admissão, durante atividade e depois do término.
- Registrar a ausência da API/comportamento na base, sem inventar uma falha de teste executado. Quando houver fixture comparativa, preservar seu resultado real separado da correção.
- Conferir que chamadores antigos, bytes assinados, grants, clocks, default-off e limites não mudaram. Os cenáriosV1 eV2 devem ser identificados, sem atribuir cobertura de um ao outro.

### Fase2 — transporte real com isolamento e TCP em progresso

- Um Host de grupo schema3 e dois Guests reais. Cada um usa TCP e pelo menos duas origensUDP marcadas; verificar integridade e ausência de retorno cruzado antes da desativação.
- Manter TCP comprovadamente incompleto e transferindo enquanto desativa `aux` de A. Depois de `Ok(())`, A e B conservam TCP, inclusive aberturaTCP nova; B mantémUDP e abre outra associação. Nova aberturaUDP de A recebe recusa, sem bind/acesso ao destino do serviço desativado.
- Cobrir também serviçoV1 e dois serviçosUDP de uma lease no registro manual existente: retirar um não retira o outro. Não ampliar o schema3 para adicionar serviços.
- Desativar antes do ingresso não consome convite. A admissão legítima continua de uso único, com game permitido e aux recusado; replay não readmite o participante.
- Drenar/identificar o tráfego anterior antes dos oráculos negativos. Não interpretar um datagrama já enfileirado antes do selo como prova de novo encaminhamento posterior. Correlacionar os negativos com gates/contadores de destino próprios e payloads novos marcados.

### Fase3 — corridas de criação, publicação e perda do waiter

- Reter setup registrado antes de bind, depois de bind, depois de connect e antes de publicação/resposta. Desativação sela imediatamente, não registra setup novo e permanece pendente enquanto existir trabalho/recurso retido. Ao liberar a mesma tarefa, não surge associação válida tardia.
- Reter trabalho entre publicação e instalação do retorno. A associação fica coberta pelo tracking durante toda a passagem; resposta perdida/falha de ACK não gera owner órfão nem nova autorização.
- Abandonar o primeiro waiter depois do selo, repetir a mesma chamada e tentar abrirUDP. A recusa é permanente e a segunda espera acompanha o mesmo término; não reinicia limpeza nem conclui cedo.
- Uma leitura de controle incompleta do Guest não bloqueia o selo nem o fechamentoUDP ativo. Testar concorrência com `CloseUdp`, revogação, expiração original e `Host::shutdown`, sem deadlock ou liberação falsa.

### Fase4 — ownership, fragmentos e quotas

- Reter remontagem incompleta e, separadamente, `Completed` durante entrega. Cancelamento precede limpeza; nenhuma remontagem nova nasce depois. Contadores privados de reserva voltam ao baseline pertinente somente depois de soltar os holders reais.
- Reter `Arc<Association>` em trabalho real de envio/retorno e exigir cleanup pendente até seu término. Não usar apenas contagem deArc como oráculo final.
- Com Host e Guests ainda vivos, reabrir as portas exatas dos socketsUDP de destino liberados de A. O EndpointQUIC e os recursos de B permanecem ativos. Testar liberação de setup falho mesmo quando `udp_associations_opened` nunca incrementou.
- Provar recuperação dos permits de capacidade sem refund/reset dos buckets, dos créditos ou do orçamento agregado. Limites de B,4096/4097B, retorno autorizado eV1 permanecem iguais. Desativar/repetir/desconectar não cria orçamento novo.
- Observar liberação dos recursos de todas as fixtures no encerramento normal, sem Java, processos alheios ou portas externas.

### Fase5 — revisão e regressões proporcionais

Depois de implementar, root seleciona os testes reais criados antes de indicar filtros exatos. Executar focais e regressões da biblioteca, fmt/clippy pertinentes e checks desktop que compilem a dependência alterada; padrão desligado continua verificado. Não ativar opt-ins históricos, relay público, rede externa ou Java por seleção ampla. Resultados das features compartilhadas são conjuntos sobrepostos, não devem ser somados.

QA independente confere código, semântica de cancelamento, gates/locks, ordens de drop, oráculos e diffs. Falhas e correções ficam registradas separadamente. Somente depois de resultado/revisão reais atualizar este documento, task, SPEC/backlog e demais autoridades no escopo comprovado. Durante a execução,81 permaneceu **PARCIAL** até completar as provas e a regressão; resultados reais do plano aparecem abaixo.

## Checkpoint de implementação e primeiras evidências81

Produção inicial em `src/host.rs`, novo `src/host_udp_service.rs` e ajuste mínimo de construtor em `src/host_session_udp_tests.rs`; seis integrações públicas no novo `tests/udp_service_disable.rs`. Registro estático por grantUDP original; setup retém Work antes de I/O, recurso libera socket/permit antes do último Work, Delivery mantém Completed e suas reservas. Publicação usa ordem mapa→gate; desativação solta gate antes de remover mapa, limpar remontagem e soltar owners, sem depender da leitura de controle. Último envio imediato nos dois sentidos é linearizado com o selo. Não há novo aviso remoto ou comando desktop.

QA estática aprovou a produção inicial. Endurecimento preventivo: poison do gate de autorização sela/cancela e recusa nova concessão; disable/Drop podem recuperar somente para cleanup. Não foi reproduzida uma vulnerabilidade externa ou uma falha de teste relacionada a poison. No teste de capacidade, revisão corrigiu per-peer para3 e global para2, para que um teto por peer igual ao global não escondesse ampliação do limite agregado. Nenhum gate de produção foi relaxado.

Comandos realmente executados pelo root após freeze das fontes pertinentes:

```powershell
cargo check --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --all-features --lib
cargo test --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --all-features --test udp_service_disable -- --test-threads=1
```

Primeiro check **PASS**,4,00s (`.aurora/connect-validation/udp-service-disable-81-first-check.log`). Primeiro público **6 PASS/0 falhas/0 ignorados**,7,94s/compilação42,83s, exit0 (`udp-service-disable-81-public-first.log`). Validações sem efeito, dois Guests schema3 com TCP incompleto/avanço real enquanto UDP aguarda, quatro origens isoladas, portasHostA reutilizáveis após disable com Host/Guests/B vivos, gameA/B e auxB preservados, aberturas novasUDP A recusadas, admissão única/replay, dois serviçosV1/reconexão, capacidade2/per-peer3 e prazos/revogação/idempotência terminal. QA conferiu log e hash da integração congelada; conjuntos de transporte locais não são áudio ou rede externa.

Ainda pendentes: hooks/provas privadas de setup/publicação/retorno, Completed separado de remontagem pendente, controle retido, perda do waiter e últimas fronteiras concorrentes; regressão completa, fmt/clippy e dependentes desktop. Estas seis integrações não substituem aquelas provas. Estado continua **PARCIAL**, sem homologação ou commit81 neste checkpoint.

Checkpoint privado81-A: `cargo test --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --all-features --lib host::udp_service_tests -- --test-threads=1` **6 PASS/0 falhas/0 ignorados**,1,02s/compilação26,87s, exit0 (`udp-service-disable-81-private-first.log`). Host/Guest reais, gates antes do bind, após bind, após connect, antes de publicar, antes de ACK e antes de instalar retorno. Primeiro waiter abortado depois de observar o selo; repetição aguarda o mesmo Work, porta exata continua ocupada enquanto o recurso está retido e só fica reutilizável depois de liberar o gate/receber Ok. Após liberação, setup antigo não publica associação válida e novo UDP é recusado; TCP novo bidirecional continua. Na fase antes do retorno, ACK anterior ao selo pode ter sido recebido, sem autorização de tráfego novo depois. QA aprovou a causalidade por leitura; não há relógio do Windows ou autoridade temporal fabricada.

Essa compilação intermediária emitiu dois avisos de dead_code em helpers cfg(test) preparados para o lote seguinte (SendGate, install_send/seal_attempted). Não foram ocultados e precisam ser usados/removidos antes do Clippy final. Pending/Completed, retorno em voo, controle incompleto e gate final concorrente continuam pendentes; os seis testes de setup não os substituem.

Checkpoint81-B, mesmo comando focal: **6 PASS/5 falhas/0 ignorados**,1,82s/compilação22,84s, exit101 (`udp-service-disable-81-private-second.log`). Os cinco casos novos alcançaram seus oráculos principais de Pending/Completed, retorno, Notify e controle retido, mas falharam em `Fixture::finish`, `udp_fixture_released`. Fonte e QA confirmaram que o próprio teste conservava `FragmentedUdpAssociation`, cujo inner guarda clone forte de Guest, depois de chamar shutdown por referência. Esse owner ainda vivo retinha Endpoint durante o rebind final dos transportes da fixture; não era falha observada da desativação de recursoHost81.

Correção somente nos testes: descartar explicitamente a associação depois de seu shutdown e antes de descartar a fixture. O rebind específico do socketHost desativado, feito anteriormente com Host e Guest vivos, permanece inalterado. QA também retirou um confound do negativo de remontagem: o fragmento inicial manual usava message_id1 sem avançar o gerador Guest; o envio negativo seguinte poderia repetir1 e ser descartado por replay. O negativo agora usa explicitamente message_id2 novo, sem alterar replay fence ou gate de produção. Reteste será registrado separadamente. Essa compilação intermediária tinha três avisos cfg(test), incluindo o observador de Bucket preparado para o loteC; não ocultados.

Reteste completoB, mesmo comando: **10 PASS/1 falha/0 ignorados**,1,82s/compilação7,94s, exit101 (`udp-service-disable-81-private-third.log`). As quatro novas provas de reservas/retorno/Notify passaram; restou somente o rebind final da fixture de controle incompleto. Ela ainda conservava o próprio SendStream depois de reset. Descartar explicitamente SendStream e RecvStream antes de finish resolveu o caso, sem alterar produção ou mover o oráculoHost. Focal exato: `cargo test --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --all-features --lib host::udp_service_tests::incomplete_control_read_cannot_retain_active_udp_socket -- --exact --test-threads=1` **1 PASS/0 falhas/0 ignorados**,0,12s/compilação7,32s, exit0 (`udp-service-disable-81-control-retest.log`). QA confirmou ambas as rodadas. Elas não são uma execução11PASS; o próximo foco completo verificará todos no fonte final.

Checkpoint81-C, mesmo comando focal privado: **19 PASS/0 falhas/0 ignorados**,2,81s/compilação20,24s, exit0 (`udp-service-disable-81-private-fourth.log`), sem avisos. Inclui os11 casos anteriores no fonte reparado, quatro disputas reais do gate final V1/V2×destino/retorno, três recusas por poison e uma prova de budgets. A tentativa de selo é observada separadamente do token desativado; enquanto o envio síncrono detém o gate, a desativação ainda não venceu. Payload previamente autorizado é drenado antes de tentar marcador novo depois do selo. Não há promessa de recolher bytes já enfileirados.

Budgets: sete buckets realmente debitados e crédito são amostrados com helper `Bucket::test_snapshot` exclusivamente cfg(test), que devolve campos sem ler relógio/refill/take. Retorno real retido fornece a janela de observação; disable, repetição e desconexão conservam o estado contabilizado, enquanto permit é recuperado depois do último owner. Nenhum crédito foi artificialmente reposto ou clock de produção substituído. QA aprovou por leitura gates, ordens de locks/drops e oráculos; helpers antes sem uso agora têm consumidores. Próximo lote: concorrência com CloseUdp/revogação/shutdown/expiração original, antes das regressões gerais.

Checkpoint81-D, mesmo comando focal privado: **23 PASS/0 falhas/0 ignorados**,3,35s/compilação15,18s, exit0 (`udp-service-disable-81-private-final.log`), sem avisos. Acrescenta CloseUdp, revogação, shutdown e expiração concorrentes com a desativação. CloseUdp foi comprovadamente processado pelo Host enquanto o owner de retorno continua retido; `association.shutdown()->()` não expõe Reply e não prova recebimento de ACK. O comentário foi corrigido para essa precisão depois da rodada, sem lógica nova.

Nos casos globais, o selo e waiter pendente são observados antes de revogar/encerrar; conclusão ocorre no limite5s sem abrir o Gate de fixture cujo fallback é8s. Weak deixa de promover a associação, porta exata é reutilizável e permit recuperado depois do teardown aguardado. Expiração usa a mesma SessionInvitation schema3 original com relógio privado já existente, avançado somente até o deadline assinado; não muda scalar deadline, grant ou relógio Windows. QA conferiu fonte, limites temporais, oráculos e log. Fonte final recebeu apenas precisão de comentário e rustfmt focal, antes de congelar para regressões/fmt/clippy; nenhuma execução ignorada ou prova externa foi promovida.

## Critérios de aceite e recuperação

Regressão final da biblioteca: `cargo test --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --all-features --all-targets -- --test-threads=1` **295 PASS/0 falhas/9 ignorados**,31 targets,197,32s somados de testes/compilação5m46s, exit0 (`udp-service-disable-81-library-full.log`). Inclui as23 provas privadas e seis integrações públicas no fonte final; rodadas focais sobrepostas não são somadas. Opt-ins ignorados permanecem pendentes e não viram evidência por compilação. Sem nova topologia externa, Java ou áudio.

`cargo fmt --manifest-path apps/connect-lab/Cargo.toml --all -- --check` PASS. `cargo clippy --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --all-features --all-targets -- -D warnings` PASS11,54s; mesma chamada sem `--all-features` PASS11,04s. Logs `udp-service-disable-81-{fmt,clippy-all,clippy-default}.log`, sequência exit0.

Regressão do dependente: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-features --all-targets -- --test-threads=1` **495 PASS/0 falhas/63 ignorados**, oito targets,253,25s somados de testes/compilação2m59s (`udp-service-disable-81-desktop-full.log`). Recompila a biblioteca e alpha.14; inclui conexões, descoberta, abertura pronta, observação80, liberação79, Recovery/CAS/Doctor e outros fluxos nativos. Não repete nem atribui a81 a suíte padrão290 do incremento80: com transporte opcional ausente, esse runtime não mudou.

`cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --all -- --check` PASS. `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-features --all-targets -- -D warnings` PASS21,44s; sem `--all-features` PASS2,55s; substituindo por `--features connect-lab` PASS12,33s. Logs `udp-service-disable-81-{desktop-fmt,desktop-clippy-all,desktop-clippy-default,desktop-clippy-connect}.log`, todos exit0. Root único executor Cargo; fontes congeladas e revisão independente sem bloqueador. Locks, manifests, versões e matriz ativa conservam hashes anteriores. Nenhum frontend, CLI/Node caller ou configuração pessoal alterado; não houve novo build/UI80 atribuído a81.

Arquivos81: `apps/connect-lab/src/{host.rs,host_udp_service.rs,host_udp_service_tests.rs,host_session_udp_tests.rs,transport.rs}` e `apps/connect-lab/tests/udp_service_disable.rs`; domínio81, AGENTS, SPEC, ADR, backlog, matriz documental e task. O ajuste em transport.rs é exclusivamente o observador cfg(test). Propostas de binding/voz em andamento não integram este commit ou sua autoridade.

Estado por componente: TCP preservado, inclusive durante retiradaUDP; UDP bidirecional/isolado mantém fronteiras, com nova retirada por serviço e cleanup real. Direto mantém evidências anteriores, sem nova rede/topologia; relay público desligado, fixture local não equivale a uso público. Minecraft sem Java/mundo novo nesta etapa. Convites preservam consumo/relógios/replay; interface sem nova ação. Simple Voice Chat e Plasmo Voice, separadamente, continuam sem áudio novo ou binding automático. Nove pares, Assistente desktop, fases/gates e R3 preservados. Nenhum push, deploy, release, instalador ou custo.

Aceite81 exige API e validações acima, desativação permanente por lease/serviço/revisão, nenhuma ampliação de wire/grants/quotas/prazos, TCP preservado, V1/V2 recusados corretamente e cleanup causal com liberação de sockets e reservas enquanto o Host continua vivo. Perda do waiter, setup pendente, retorno e fechamento concorrentes precisam de prova própria. Resultado será de transporte experimental; não homologa Minecraft, SVC, Plasmo, áudio, GUI, uma hora de estabilidade ou outra rede.

Rollback operacional: a desativação não pode ser desfeita na mesma autoridade. O jogo continua conforme os serviços e prazos originais; quando desejado, encerrar normalmente os recursos Connect e aguardar os trabalhos próprios. Uma nova sessão exige o fluxo normal de autorização, sem reusar convite consumido. Não restaurar anúncio, matar Java ou modificar recibo por consequência da desativaçãoUDP.

Rollback de código: reversão local revisada somente81, preservando80,79 e as demais entregas, sem reset/clean ou alteração de kits. Não há migração de disco nem configuração pessoal a restaurar. Recibos58/60/67/68, estrita/reconciliada e guard de ausênciaJava permanecem com suas autoridades e aprovações próprias. Reverter o código não é reabilitação de uma sessão em andamento.

## O que você precisa fazer agora

Nenhuma intervenção humana é necessária para revisar, implementar e testar81 localmente no escopo autorizado. A revisão deste contrato é coordenação técnica entre agentes, não pedido adicional de aprovação ao usuário. Segundo computador, áudio consentido e integração de dedicado continuam etapas independentes; nenhum push, deploy, release, instalador ou custo faz parte desta entrega.

# Vínculo nativo de observação de voz ao Host — contrato86

Data de elaboração: 21/10/2026. Aprovação integral root/QA e canonicalização: 23/10/2026. **CONTRATO APROVADO; implementação86 NÃO IMPLEMENTADO / PRECISA DE TESTE.** Os gates85C1 e85D **e**85E foram fechados com PASS/revisão/commits próprios, sem promover áudio ou homologação. Este contrato autoriza os incrementos Rust privados abaixo dentro do escopo já autorizado pelo usuário. Alterar seus limites ou reduzir perfis exige revisão explícita; não há fallback silencioso.

Base efetiva aprovada: `dbca5b251217f28a2f2177851e4238d35444598d`, na mesma worktree de Connect;85C1 em `75526959c565e9740d7fd0f2e3c0cae1f143320c` e85D/E em `dbca5b2`. O draft integral tecnicamente aprovado por QA e root tinha SHA-256 `31f9ec542ca4ff1ffb4544569577b82ed04088e0a091e8fbd1a79f8f7fd8bb53`; cópia byte-exata preservada em evidência privada `native-voice-86-reviewed-draft.md`. A canonicalização altera estado/proveniência, sem mudar o desenho técnico aprovado. O histórico75 permanece referência de riscos, não autoridade.

Autoridades de domínio: [observação e produtor nativos85](aurora-connect-native-voice-runtime.md), [dedicado84](aurora-connect-dedicated-status.md), [owner83](aurora-connect-native-owned-run.md), [UDP82](aurora-connect-owned-udp-binding.md), [retirada de serviço81](aurora-connect-udp-service-disable.md), [ownership79](aurora-connect-guest-source-cleanup.md), [encerramento do grupo77](aurora-connect-group-source-shutdown.md), [revisão76](aurora-connect-owned-source-review.md), [origem do grupo74](aurora-connect-discovered-group.md), [grupo72](aurora-connect-multiparticipant-desktop.md), [grupo71](aurora-connect-multiparticipant-session.md), [sessão fragmentada61](aurora-connect-fragmented-session.md) e [relógios52](aurora-connect-session-lifetime.md). Preservar a [especificação](engineering/AURORA_CONNECT_SPEC.md), o [backlog](engineering/CODEX_TASKS/AUR-R4-CONNECT-BACKLOG.md), o [roadmap](engineering/MASTER_ROADMAP.md) e o `AGENTS.md` ativo.

## 1. Bloqueio real e menor resultado útil

85 observa configuração limitada e bindingUDP do processo original; sua amostra redigida não autoriza transporte. O desktop atual ainda admite `aux/UDP` manual em64/74. `TargetGuard` conserva origemTCP manual ou descoberta e os atores76/77 encerram o Host inteiro quando essa origem falha. Nenhum desses caminhos recebe o vínculo nativo85 ou usa81 para uma falha exclusivamente de voz.

81 já permite ao proprietário de um `Host` retirar permanentemente uma combinação exata de lease, serviço e revisão. Falta um caller nativo que vincule a origem original, a revisão concreta e o conjunto estático de autoridades, mantenha as consultas/retiradas sob ownership e trate voz separadamente de jogo.

O menor resultado86 é **transporte local de bytes opacos por um Host e GuestsQUIC reais**, com origem derivada de um único `DedicatedRun`, aprovação Rust própria e retirada terminal de `aux/revisão1` quando a observação exigida deixa de ser favorável. O TCP saudável continua autorizado. Não chamar esse resultado `VoiceReady`, voz funcional ou voz autenticada.

## 2. Escopo fechado e condições de entrada

Todo código novo de86 ficará sob Windows + `connect-lab` + `connect-voice-lab`, ambas default-off, em entradas Rust privadas. Não acrescentar comandoTauri, DTO/JSON de seleção da origem, UI, CLI, configuração pessoal, convite de outro schema, envelopeUDP, protocolo remoto ou seleção de executável. APIs manuais/descobertas antigas continuam com seus formatos, preimagens e comportamentos; não ganharão autoridade85 por usar as mesmas portas.

Não alterar Core/Skins, os nove pares ativos, Assistente, Companion removido, Recovery/CAS/Doctor, journalsEULA, pins, dependências, lockfiles, matriz, kit, relay, firewall ou política de gastos.86 não inicia Java: recebe por empréstimo o `DedicatedRun` já criado pela receita/revisão85 e não assume seu controle ou completion. Os testes deste recorte usam o filho próprio de fixture e transporte real; os ensaiosJava85D/E são pré-requisitos distintos, não testes86 repetidos nem prova de áudio.

Perfil e destinos são compilados e fechados: Minecraft1.21.1/Fabric0.16.14/protocolo767, `game/TCP` em127.0.0.1:43001 e `aux/UDP` em127.0.0.1:43002 para SVC2.6.22 ou127.0.0.1:43003 para Plasmo2.1.17. Exigir a política85 literal qualificada e anúncio `DefaultUnchanged`. Não aceitar target, porta, PID, path, perfil ou nonce como substituto da origem nativa.

O primeiro recorte seleciona explicitamente transporte loopback nas APIs de grupo71. Nenhuma falha tenta IPv4 direto, IPv6 externo, relay, outro endereço, outra porta ou outro perfil. Perfis externos existentes permanecem disponíveis somente pelos fluxos antigos; sua homologação não é herdada por86.

## 3. Três fronteiras que não se substituem

| Fronteira | Evidência e autoridade permitidas |
| --- | --- |
| Origem nativa | O mesmo estado proprietário83/84/85 e seu contexto imutável observam processo, TCP, configuraçãoA/B e linhaUDP literal. A amostra não prova continuidade após o último gate nem qual JAR abriu o socket. |
| Transporte Connect | Revisão concreta, convite original, pin, chave destinatária, TLS/Hello, consumo, grants, serviço/revisão e clocks autorizam os bytes entre Host e Guest. Porta ou digest isolado não concede essa autoridade. |
| Mod e anúncio | Credencial do mod, anúncio recebido pelo cliente, clientePLAY, retorno autenticado do mod e áudio exigem provas próprias. Autenticar o peerQUIC não autentica o emissorMinecraft nem uma credencial SVC/Plasmo. |

85 exige `voice_host` vazio ou `host.public` ausente. A ponte desktop existente é35554.86 não altera o anúncio, não demonstra que clientes encontram35554 e não remove esse bloqueio de endereçamento. Recibos58/67/68, claimEULA, arquivo, hash, log, endpoint e observaçãoOS não substituem uma dessas fronteiras. Não chamar guard de mutação antigo com Java vivo para contornar a separação.

## 4. Origem opaca do mesmo estado

Proposta de interface mínima, com nomes internos sujeitos à revisão de código: um `NativeVoiceHostBinding` só pode nascer de método privado sobre `&DedicatedRun`. O construtor extrai TCP e voz do **mesmo** `Shared`, confirma perfilV2/contexto e conserva a identidade nativa original, inclusive nonce e prazo já ancorados. Não aceitar dois sources independentes e tentar conciliá-los depois por comparação de amostras.

O binding conserva somente referências fracas e valores imutáveis necessários ao vínculo. Não retém `DedicatedRun`, controle de stop, `Child`, proof, completion, recursos, guard, pins, handle de arquivo ou closure. A identidade nativa não é desserializável nem fabricável a partir de UUID/perfil. Nonce e digest entram somente na conferência/preimagem privadas; não vão para logs, snapshots compartilháveis ou frontend. Um clone continua apontando para a execução original; não é uma seleção reutilizável de outra cópia.

Sem constructor público por campos e sem conversão de `VoiceRuntimeSample`, JSON, recibo, `ApprovedTarget`TCP ou `SocketAddr` em binding. V1, fonte fechada, perfil divergente e origem substituída são recusados. O sample85 continua dados redigidos sem ID/path/hash/serde/autoridade; não o ampliar para facilitar a admissão.

### Reserva antes de dispatch

`DedicatedVoiceSource::observe` atual é síncrono e reserva84 dentro da execução. Para o caller assíncrono86, extrair uma entrada privada fechada de reserva/execução que mova o **mesmo QueryJob84 já reservado** ao worker. O wrapper síncrono existente pode chamar as duas fases; todos os resultados reservados continuam no mesmo gate final85, inclusive erros. Não criar segundo slot, novo owner83, provider, callback livre ou fila de queries.

O Attempt registra seu trabalho antes do dispatch; a reserva84 também precisa existir antes de enfileirar a parte bloqueante. Só guardar um `JoinHandle` e reservar84 depois que o worker começar não satisfaz o contrato85. Falha de dispatch libera o job realmente reservado sem publicação. Perda do waiter conserva worker/job até término; tarefa ainda não executada continua coberta pela barreira de cleanup. Aplicar o mesmo princípio às consultasTCP usadas por esse caminho, sem alterar os callers antigos.

## 5. Preparação, revisão concreta e mesma autoridade

O caller Rust confiável escolhe um binding opaco e um roster de dois a oito destinatários distintos, também limitado por `Limits.max_peers`. Host não aparece como jogador e não ganha Guest interno. Roster, modo e serviços ficam estáticos antes da revisão; participante novo, substituição de origem ou mudança de perfil exige encerrar a operação e preparar outra, sem promover um convite antigo.

Usar o slot/geração/Attempt e a barreira de cleanup existentes. A revisão nova é uma estrutura Rust privada distinta; não rotular a origem nativa como `manual`/`owned` nos DTOs antigos. Antes de emitir os convites, consultar a origem fora do mutex e revalidar proprietário, operação, geração, fase, cancelamento, shutdown e prazos. Emitir cada `SessionInvitation`schema3 uma única vez, reter os objetos originais e conferir novamente esses gates antes de publicar a revisão. Falha parcial não publica roster incompleto.

A revisão deve mostrar: execução/cópia privada original, perfil e versões fixados, TCP43001, UDP do perfil, política literal e anúncio padrão/ausente, endereço inicial do Host, todos os destinatários, `game` e `aux` com revisão/capability/orçamentos, ingressos e sessões originais e a retirada definitiva deUDP sem promessa de áudio. Não copiar texto de revisãoEULA que diz não executarJava para descrever o dedicado já em execução. Não mostrar nonce, chave privada, claim ou conteúdo de configuração.

Preimagem nova proposta, fechada e exclusiva do caller Rust:

```text
("aurora.connect.desktop.native-voice-static-group/v1",
 owner,
 (native_nonce, profile_label),
 native_review_data)
```

`profile_label` vem da seleção compilada (`svc` ou `plasmo`), nunca de texto recebido. `native_review_data` tem ordem fixa: `operation_id`, `generation`, `review_id`, `host_public_id`, `mode`, `direct_address`, `game_target`, `udp_target`, `announcement_policy`, `services`, `participants`, `expires_at_ms`. `participants` conserva a ordem revisada, IDs opacos próprios, destinatários, digest de cada envelope original e seus dois prazos. `services` conserva os descritores compilados de71. O digest é SHA256 dos bytes compactos dessa estrutura; o campo digest não entra na própria preimagem. Este domínio não valida uma revisão manual/descoberta nem vice-versa.

Essa preimagem não altera bytes, domínio, digest ou assinatura de nenhum convite ou revisão anterior. O estado privado liga o digest ao binding opaco **ainda retido**; verificar/recriar os bytes de um digest não reconstrói esse estado nem uma autoridade temporal. Fixar a ordem exata dos tipos/preimagem e suas provas diferenciais antes de integrar o código86; mudança no texto acima exige nova revisão deste draft.

Preparo tem no máximo600s desde a reserva original e não ultrapassa o limite nativo já ancorado. Revisão tem no máximo60s e é limitada pelo restante original do preparo, ingresso de cada integrante e limite nativo. Ingresso1–600s e sessão1–3600s, sessão≥ingresso, permanecem como71/72. Conservar as amostras/deadlines civis e monotônicas originais; polling, fila, IO, consulta favorável, retry, exportação, ACK perdido e retirada81 não reancoram relógios.

Confirmação consome a revisão exata uma única vez e confere digest, identidade nativa/perfil, proprietário, UUID/geração, fase e todos os gates anteriores. Nenhum socket de transporte86 é aberto só por preparar/revisar. Confirmar abre o Host pelas APIs71 existentes, com a bijeção exata entre leases e convites retidos. Revalidar origem e autoridade antes do bind e depois dele, inclusive dentro do gate de publicação. Não publicar listening/exportação se stop/expiração/troca venceu; fechar owners já criados e aguardar o trabalho original.

## 6. Admissão e limites preservados

Reutilizar TLS, pin independente, chave destinatária, prova Hello, digest/geração, capabilities, lock de admissão/revogação e consumo antes do ACK existentes. Falha de ACK/desconexão não devolve consumo. Exportar usa o envelope original; não reemitir para renovar ingresso, reconectar ou contornar retirada. Vencimento de ingresso já consumido não fecha uma sessão ainda válida; sessão/revogação de A não redefine a autoridade de B.

Registro exato71: `game/TCP/revisão1/reliableStream` e `aux/UDP/revisão1/fragmentedDatagram`, original até4096B, frame até1264B incluindo header32B, até8fragmentos, remontagem250ms e retorno autorizado16384B/s/20frames/s. Manter pools/quotas existentes:8mensagens/32KiB por associação,64KiB por peer,1MiB global e orçamentos agregados. Esses limites não são orçamento de áudio homologado. Não aumentar nem reinicializar tokens/quotas na observação, retirada, desconexão ou nova associação.

Os destinos locais derivados do binding não viajam em convites/envelopes. Capacidade de serviço não identifica mod; binding literal não autoriza wildcard/mapped. Não mudar82/83/85 para aceitar outra representação por causa de um ensaio86.

## 7. Monitor e política finita de indisponibilidade

O ator86 serializa suas consultasTCP e voz sobre o slot84 existente. Não iniciar um monitorTCP concorrente ao monitor de voz e depois tratar o Busy causado pelo próprio desenho como mudança da configuração. Cada ciclo de voz usa a sequência85 completa TCP→configuraçãoA→UDP85→configuraçãoB→TCP, contexto antes/depois e gate final. Não guardar o resultado como lease de processo nem evitar a próxima consulta por igualdade de amostras.

Política proposta deste DRAFT: **zero retries automáticos de Busy**. Antes da publicação, Busy/NotReady/Closed ou observação recusada impedem o vínculo e não emitem/publicam autoridade utilizável. Com Host publicado, qualquer falha de observação exigida retira `aux` das leases vinculadas. Busy/NotReady são `ObservationUnavailable`, nunca `ConfigurationChanged`. Uma nova preparação exige ação Rust explícita e todos os gates próprios; não há backoff, fila de amostras, pedido automático de revisão ou renovação deTTL.

Cadência proposta: uma consulta por segundo, sem catch-up de ticks acumulados; no máximo uma consulta desse ator pendente. Janela de observação proposta:2s desde o dispatch, também limitada pelos gates originais pertinentes. Vencer essa janela **nega/retira UDP**, mas não cancela por drop nem considera terminado o worker bloqueante. Reter o job original até retorno/gate/drop reais. Esses números são decisões de revisão do DRAFT, não medições ou garantia de wall-clock; não os ampliar para conseguirPASS.

Enquanto a query estiver retida, observar em paralelo cancelamento, shutdown, geração/operação, clocks e a janela de observação. Não manter UDP indefinidamente aguardando uma syscall. Não prometer detectar alteração antes do próximo ciclo ou continuidade após o último sample. Os timers dependem de escalonamento; não são garantia de silêncio imediato.

Com Host já publicado e jogo saudável, erro somente de voz jamais chama `DedicatedRun::request_stop`, `Host::revoke`, cancelamento da lease inteira, fechamentoQUIC ou `Host::shutdown` como atalho. Antes da publicação, uma validação recusada fecha o Host ainda não publicado e seus owners, sem deixar transporte utilizável. RetirarUDP não encerra Java nem restaura EULA. O guardTCP nativo independente conserva a política de falha do jogo, com a classificação tipada abaixo. Erro genérico de voz não prova por si falhaTCP.

Depois de retirarUDP por janela vencida, não iniciar consultaTCP enquanto o job de voz desse ator continuar retido. Busy também não é prova de falhaTCP: uma disputa externa pelo slot não pode ser convertida em `SourceChanged` para encerrar o jogo. Nesse intervalo, continuar observando cancelamento, autoridade dos participantes e limite/selo nativo originais, sem prolongá-los; retomar as consultas serializadas quando o job realmente terminar. Nenhuma dessas observações substitui uma nova provaTCP ou promete disponibilidade contínua.

### 7.1. Classificação tipada das fontes atuais

Fixar o despacho por `VoiceRuntimeError`, `DedicatedError`, `OwnedRunError` e `DiscoveryError` atuais. As razões da coluna central são enums privados propostos de86, não códigos novos de protocolo/UI. Não classificar por texto, `Debug`, `Display`, substring ou um `SourceChanged` genérico. Match de uma camada não pode apagar a diferença Busy da camada seguinte. Uma variante futura exige revisão da tabela e match explícito, não fallback que ganhe autoridade.

Na consulta de voz, `Source(OwnedRunError)` pode ter vindo do primeiroTCP, UDP ou últimoTCP: o tipo atual não informa essa fase. Portanto, **nenhum erro de voz abaixo é provaTCP autônoma**. Antes da publicação, qualquer um recusa o vínculo. Depois da publicação, todos iniciam retirada terminal de `aux`; o latch continua selado mesmo se a consultaTCP distinta posterior for favorável.

| Resultado da consulta de voz85 | Classificação privada86 | Consequência para jogo/TCP |
| --- | --- | --- |
| `Ok(VoiceRuntimeSample)` com perfil/valores compilados esperados | `VoiceObservationAccepted` | Autoriza somente continuar o caminho de revisão/transporte já permitido por seus gates; não renovaTTL nem reativaUDP retirado. |
| `VoiceRuntimeError::Busy` ou `NotReady` | `ObservationUnavailable` | Nenhuma inferência de mudança; zero retries de voz. RetirarUDP e seguir a políticaTCP serializada. |
| `VoiceRuntimeError::Closed` | `VoiceSourceClosed` | RetirarUDP. A razão não substitui a observação do selo/prazo nativo original ou uma consultaTCP separada para decidir encerramento global. |
| `ConfigurationMissing`, `ConfigurationUnsafe`, `ConfigurationLimit`, `ConfigurationIncompatible`, `ConfigurationIo` | `VoiceConfigurationRejected` com subtipo fechado `Missing`, `Unsafe`, `Limit`, `Incompatible` ou `Io` | RetirarUDP; TCP não foi diagnosticado por esses erros. |
| `ConfigurationChanged` | `VoiceConfigurationChanged` | RetirarUDP; não promover essa diferençaA/B em mudança do listenerTCP. |
| `Context(Error)` | `VoiceContextRejected` | Manter o erro tipado apenas internamente; relatório usa a razão fechada sem formatar o payload. Não provaTCP. |
| `UnsupportedBinding` | `VoiceBindingRejected` | RetirarUDP; sem normalização, fallback82 ou inferência de falhaTCP. |
| `Source(OwnedRunError::Busy)` ou `Source(OwnedRunError::Discovery(DiscoveryError::Busy))` | `ObservationUnavailable` | Tratar Busy aninhado como disputa de slot, nunca como configuração/listener alterado. |
| `Source(OwnedRunError::Closed)`, `Source(Cancelled)` ou `Source(Deadline)` | `VoiceSourceUnavailable` | O owner83 pode ter selado, mas a consulta de voz não identifica qual fase originou a recusa. Conferir a autoridade nativa original/consultaTCP distinta antes de falha global. |
| `Source(Discovery(ProcessChanged))` ou `Source(Discovery(ListenerChanged))` | `VoiceSourceRejected` | A recusa pode ser da etapaUDP; retirarUDP e comprovarTCP separadamente, sem transformar diretamente em `SourceChanged` do jogo. |
| `Source(Discovery(Unavailable))`, `Source(Discovery(Limit))` ou `Source(Discovery(Internal))` | `ObservationUnavailable` | Não é diagnóstico de configuração nemTCP. RetirarUDP sem retry de voz. |
| `Source(InvalidRequest)`, `Source(Spawn)`, `Source(Initialization)`, `Source(Internal)`, ou `Source(Discovery(InvalidRequest\|Stale\|Owner\|Digest\|StatusFailed))` | `VoiceObserverRejected` | Erro de observação/invariante tipado, sem autorização nova e sem deduzir falhaTCP. Nenhum Debug bruto. |
| Janela86 de voz vencida, sem resultado85 ainda decidido | `ObservationWindowElapsed` | RetirarUDP antes de o job terminar; é prazo local de observação, não `OwnedRunError::Deadline` nem completion. |

Nas duas tabelas, `Source(X)` abrevia `DedicatedError::Source(OwnedRunError::X)` ou `VoiceRuntimeError::Source(OwnedRunError::X)` conforme a coluna de origem; `Discovery(Y)` abrevia a variante aninhada `OwnedRunError::Discovery(DiscoveryError::Y)`. As alternativas com `\|` são padrões enumerados, não parsing de texto. Não há wildcard para novos erros de source.

Depois que a query de voz terminou **realmente**, usar uma consultaTCP distinta da mesma origem opaca e perfil imutável: a entrada reservada/serializada de `DedicatedSource::observe_tcp`. Não criar outra seleção pelo endpoint ou obter source de outroRun. Se o slot estiver ocupado por outra query normal, não forçar acesso paralelo nem cancelar o trabalho alheio.

| Resultado da consultaTCP distinta | Classificação privada86 | Decisão com Host já publicado |
| --- | --- | --- |
| `Ok(())` | `GameBindingObserved` | ManterTCP sob sua autoridade original. UDP continua retirado; não voltar a observar voz para tentar reativá-lo. |
| `DedicatedError::Busy`, `Source(Busy)` ou `Source(Discovery(Busy))` | `GameObservationUnavailable` | ManterTCP eUDP retirado. Não é `SourceChanged`, não encerra jogo/Host e não cria fila. Observar latches/deadlines originais até a próxima oportunidade periódica finita. |
| `DedicatedError::NotReady` | `GameObservationUnavailable` | Antes da publicação, recusar; depois, não inventar mudança de listener. Observar os gates originais e próxima oportunidade serializada. Não promover a execução paraReady por essa via. |
| `DedicatedError::Closed` ou `Cancelled` retornado pelo gate original; `Source(Closed\|Cancelled\|Deadline)` retornado pela consultaTCP real | `GameAuthorityClosed` | Encerrar o transporte dessa operação conforme sua autoridade original. A origem foi consultada pela viaTCP ou seu próprio gate, não inferida de um erro de voz. Cancelar um waiter não pode fabricar essas variantes. |
| `Source(Discovery(ProcessChanged\|ListenerChanged))` | `GameBindingChanged` | Esta consulta é especificamenteTCP e comprova a recusa desse vínculo. Pode usar o encerramento global de origem; não altera o proprietário do dedicado. |
| `Source(Discovery(Unavailable\|Limit\|Internal\|StatusFailed))` | `GameObservationRejected` | O guardTCP falhou de modo conservador; pode encerrar o transporte, mas não afirmar que o listener mudou. Não classificar como `SourceChanged` por texto nem pedir stopJava. |
| `Source(InvalidRequest\|Spawn\|Initialization\|Internal)`, `Source(Discovery(InvalidRequest\|Stale\|Owner\|Digest))`, `DedicatedError::Preparation(Error)`, `Thread` ou `Internal`cfg(test) | `GameObservationIntegrationFailure` | Não são prova de alteraçãoTCP. Recusar publicação ou encerrar conservadoramente apenas o transporte da operação com erro interno redigido. São falhas da entrada fechada/invariante, sem retry, fallback, reconstrução de fonte ou autoridade nova. |

`Preparation`, `Thread`, `Internal`cfg(test) e alguns erros83 não são resultados normais do `observe_tcp` síncrono atual. A tabela os cobre explicitamente para a extração privada de dispatch/execução, sem tornar alcançável uma operação nova de processo. Falha/pânico do worker é classificado como falha de integração86, não como um erroTCP forjado. Em nenhuma linha se chama `DedicatedRun::request_stop` ou se restauraEULA; encerramento global significa somente o transporteConnect dessa operação.

Próxima oportunidadeTCP significa no máximo uma tentativa por ciclo de1s, sem catch-up, loop imediato, fila ou reaproveitamento do resultado tardio de voz. Essa periodicidade não é retry de voz e não recria relógios. Uma sequência de Busy pode durar somente enquanto a autoridade original ainda existir; continuar observando cancelamento, latches e limites nativos/dos participantes e encerrar quando esses gates próprios vencerem. Não prometer amostraTCP favorável ou duração máxima de cleanup.

### 7.2. Janela de voz e empate entre resultado e prazo

A janela2s vale para a decisão de observação de **voz**, não como prova emprestada de falhaTCP. Armazenar um deadline monotônico fixo antes do dispatch e conferir `now >= deadline` também depois do retorno do worker e no gate final de publicação/aceitação. Se timer e resultado ficarem prontos juntos, a ordem de `select`, `biased` ou polling não pode aceitar um resultado favorável tardio. Prazo vencido prevalece com retirada terminalUDP mesmo que o timer ainda não tenha sido consumido.

Se a janela vencer com job84 retido, iniciar todos os retiros81 imediatamente, sem esperar esse job cair. Manter o mesmo job/worker registrado e não consultarTCP por esse ator enquanto ele continuar pendente. Quando o término/gate/drop reais ocorrerem, descartar o resultado de voz para fins de admissão/reativação e seguir com consultaTCP distinta, serializada, ou com os latches originais já vencidos. Retirada81 e término84 são dois owners/trabalhos diferentes; um não serve de completion do outro.

## 8. Retirada terminal por lease e outstanding owners

O estado da operação sela uma decisão terminal para o vínculo/revisão/serviço original antes de qualquer espera de drenagem. Uma origem comum inválida afeta **todas as leases originalmente vinculadas a ela**, incluindo ainda não admitidas, desconectadas, expiradas ou revogadas. Não confundir essa decisão com uma revogação de participantes. Não retirar serviços/leases de outra origem por varredura global.

Registrar todo o trabalho de retirada no mesmo Attempt, com IDs/serviços/revisões vindos exclusivamente do registro estático aprovado. Iniciar/conduzir todas as chamadas `Host::disable_udp_service(lease_id, "aux", 1)` antes de aguardar a conclusão da primeira. Criar uma lista de futures sem executá-las não sela nada. A execução precisa conduzir cada chamada até seu gate81 mesmo quando a primeira está retida; não processar uma lease de cada vez até cleanup completo.

81 é linearizado individualmente e não oferece atomicidade coletiva. O estado `Retiring` não significa que todos os gates81 já selaram nem que todos os recursos terminaram. O estado `Retired` só pode ser publicado após todas as chamadas válidas retornarem `Ok(())` e o trabalho de retirada pertencer à operação até seu término. Se houver falha interna, continuar responsável pelos retiros já registrados, negar reuso do vínculo e reportar erro redigido; não transformar falha parcial em sucesso ou reconstruir Host habilitado.

Perda/cancelamento do waiter de revisão, confirmação, retirada ou stop não libera o slot, os handles ou o latch. Guardar o trabalho real e sua cauda separadamente do ator/transporte; nenhum handle sobrescreve outro. Uma segunda observação/espera acompanha a mesma decisão. Sem `mem::forget`, tarefas destacadas, contadores zerados manualmente ou timeout que anuncie cleanup completo.

O Host conserva o selo81 até seu descarte. A operação conserva adicionalmente a decisão terminal, impedindo que recriação automática do Host, nova conexão ou nova amostra positiva recupere a mesma revisão. Não existe `enable`, reset, aumento de revisão, reemissão ou fluxo63 que contorne o selo. Uma execução futura com autoridade nova fica nos contratos próprios;86 não a inicia automaticamente.

Concluir retirada espera setup, associações, sockets de destino, envio/retorno, payloads completos e reservas reais81. O restante do Host/TCP pode continuar vivo. Preservar os gates de imediato envio/enqueue, cancelamento antes de limpar reassembly e últimos owners de socket/payload. Não segurar mutexConnect ou lock síncrono durante IO/await/join.

Stop/logout/shutdown da operação fecha primeiro o transporte pertencente a ela, impede admissão/publicação nova e conduz seus retiros/owners registrados. Depois aguarda a mesma query retida e as caudas de retirada antes de liberar o Attempt ou anunciar cleanup completo. A origem dedicada fica com o proprietário85 original; encerrar Connect não a encerra por consequência. Não aceitar `DedicatedReport`, evento ou completion externo como autorização de restore.

## 9. Observações redigidas e limites remotos

Estados privados propostos: `Preparing`, `Reviewed`, `Starting`, `Bound`, `Retiring`, `Retired`, `Closing`, `Closed`, `Failed`. `Bound` significa somente transporte associado à origem revisada; não `VoiceReady`. Separar estado de transporteTCP, decisãoUDP e cleanup. Uma revisão falha não é um Host ativo; `Retiring`/`Retired` não significam encerramento do jogo.

Razões privadas fechadas distinguem cancelamento/expiração, origem incompatível, observação indisponível, configuração recusada/alterada e falha de transporte/cleanup. Usar enums tipados redigidos, sem encadear Debug/Display de erro bruto, path, conteúdo de arquivo, hash/ID físico, nonce, peer, convite, credencial, servidor ou payload nos relatórios. A classificação específica85 continua disponível internamente para oráculos; não transformar todo erro em `Changed`.

Não criar projeçãoTauri/UI por consequência. Se houver recibo Rust de observação, ele contém apenas fase, perfil fixado, razão redigida, contagens limitadas ao roster e pendência/conclusão real. Ele não é confirmação, endpoint, seleção da origem, digest consumível ou autorização restaurável.

`Ok` das retiradas81 garante autoridade e recursos **do Host**. Bytes aceitos pelo kernel ou enfileirados emQUIC antes do selo podem chegar depois. O Guest remoto não recebe mensagem específica de serviço desativado pelo protocolo atual e pode conservar listener/associação local. Não atribuir mute, notificação instantânea, fechamento remoto imediato ou zero payload já enviado a86. Não acrescentar protocolo/capability incidentalmente.

## 10. Provas causais locais exigidas

Todos os casos são futuros. Fixtures próprias têm RAII de release desde o spawn, protocolo fechado/nonce-bound, fallback limitado ao próprio filho e limites já revisados; nenhum provider falso substitui fonte85, tabelaOS ou admissãoQUIC. Root coordena Cargo serial/offline/locked depois do freeze e de revisão independente. Nenhum Java adicional é necessário a86.

| Grupo de prova | Oráculo obrigatório |
| --- | --- |
| Origem e revisão | Construtor positivo de cada perfil, seguido de caminho real de revisão/confirmação. RecusarV1, fonte fechada, cross-profile/cross-run e digest/operação/geração divergentes sem bind. Não fabricar binding por campos, amostra ou JSON. Verificar controles positivos das mesmas APIs e preimagem86; revisões antigas e seus bytes permanecem exatos. |
| Reserva antes de dispatch | Reter execução antes de iniciar worker, depois de reservar o job real84. Stop sela a origem, mas completion85 continua pendente até o job terminar. Perder waiter não permite segunda reserva/publicação; falha de dispatch solta a reserva real. Teste atravessa o método de produção, sem contador desconectado. |
| Transporte positivo | Filho próprio85 com TCP/STATUS767 eUDP reais; Host e dois GuestsQUIC autenticados, chaves/convites individuais. TCP comprovadamente em progresso enquanto datagramas marcados de duas origens por Guest atravessam `aux`, com integridade e retorno isolado. Testar SVC/Plasmo em rodadas próprias. |
| Configuração e retirada | Com TCP incompleto e saudável, causar rewrite semântico inválido, rename/substituição detectada, remoção/limite/unsafe e perda realUDP nos casos pertinentes. Exigir erro85 correspondente e retirada81 de todas as leases vinculadas, inclusive não admitida. TCP das duas leases continua e nova aberturaTCP válida funciona. Remover condições da fixture antes de recolher recursos. |
| Não reativação | Depois da retirada, restaurar configuração/UDP e obter amostra85 realmente favorável. Exigir que `aux` continue negado na mesma revisão, sem novo destino/associação/bucket/convite, enquantoTCP segue. Exercitar tentativa explícita do fluxoGuest63 na mesma autoridade; não reconstruir Host ou convite para obter sucesso. |
| Busy e janela | Ocupar o mesmo slot84 com query real retida e chamar o caminho86. Antes de bind, recusa sem publicação; ativo, retirada por `ObservationUnavailable` sem retry ouTTL novo. Separadamente reter query além da janela: retirarUDP antes de liberá-la, conservar job/cleanup pendentes e ignorar resultado favorável tardio. O ator não cria Busy por consultas próprias concorrentes. |
| Voz versusTCP | Perder o UDP próprio mantendoTCP real saudável: a consulta de voz pode retornar `Source(Discovery(ListenerChanged))`, mas a consultaTCP distinta favorável mantém jogo/Host eUDP retirado. Em caso separado, retirar o listenerTCP próprio com filho ainda vivo e exigir a recusaTCP correspondente antes do encerramento global. BusyTCP real retido por outro job preserva jogo, latches e prazos até o próximo ciclo; não converterBusy aninhado em `SourceChanged`. Matches puros da tabela não contam como execuçãoOS desses erros. |
| Resultado e deadline simultâneos | Reter a consulta, ultrapassar a janela monotônica e tornar resultado e timer prontos antes de devolver execução ao ator. Exigir retirada terminal mesmo se o branch de resultado vencer o select; TCP só é consultado depois do término real do job. Sem relógio do sistema alterado e sem timeout que solte owners. |
| Todas as leases antes da primeira drenagem | Reter owner81 real da primeira lease. Provar que a segunda já foi selada, recusa nova aberturaUDP e não acessa destino enquanto a primeira retirada ainda aguarda. A primeira permanece pendente até liberar seu último socket/payload/permit real; sem alegação de selo atômico do grupo. |
| Perda de waiter e corridas | Abandonar waiters após registro e durante retirada. Segunda espera acompanha os mesmos owners. Gates antes/depois de bind e no gate final cobrem stop, expiry original, troca de conta/geração, pânico controlado e resultado tardio; nenhum listening, convite ou associação válidos aparecem depois. |
| Clocks e admissão | Ingresso consumido vencido preservaTCP até sessão original; lease não admitida vencida não entra. Avançar somente clocks privados retidos nos objetos reais de teste; nenhuma revalidação por bytes reancora deadline. Conferir digest/consumo/grants/revisões/quotas antes/depois da retirada. |
| Ownership e isolamento | Reter query85 e, separadamente, setup/associação/Completed81. Cleanup não termina por mapa vazio/cancelamento. Rebind das portas de associaçãoUDP do Host só depois do último owner; Endpoint eTCP continuam ativos durante retirada. StopConnect permite rebind do transporte antes de liberar query retida quando os owners pertinentes terminaram, mas cleanup geral segue pendente. |

O childUDP43002/43003 continua legitimamente vinculado durante retirada81; não exigir rebind da porta do mod enquanto a origem vive. O oráculo de retirada usa as portas exatas dos sockets de associação pertencentes ao Host. Só o encerramento85 original pode fornecer completion/reap/readers e liberar as portas do filho; essa prova fica separada de encerrar Connect.

Drenar/identificar tráfego anterior antes dos negativos e enviar payloads novos marcados depois do selo observado. Contador de destino/ausência de resposta isolados não distinguem tráfego anterior, perda e negativa; combinar ACK/gates reais, recusa correlacionada e inspeção dos owners próprios. Não retransmitir o fixture automaticamente para obterPASS. Nenhuma afirmação de áudio, MTU física, saturação, internet ou dois computadores.

Orçamento proposto por caso de transporte: no máximo64KiB de payloadUDP de aplicação somando tentativas nos dois sentidos e até1MiBTCP por direção/Guest, com sobreposição causal e sem benchmark. Testes usam métodos da bibliotecaGuest e listeners próprios efêmeros; não alterar `LOCAL_UDP_BIND=35554` do desktop para acomodar dois Guests no mesmo PC. A ponte desktop35554 e a entrega ao cliente do mod continuam fora desta prova.

## 11. Caminhos e divisão mínima de implementação futura

| Área | Menor alteração proposta, somente após aprovação/gates |
| --- | --- |
| `apps/desktop/src-tauri/src/engine/connect_dedicated/producer.rs` e `producer_voice.rs` | Construtor privado opaco a partir do mesmoRun e extração fechada reserve→execute do job original; preservar fontesWeak, recipes/preimagensV1/V2 e todos os gates85. Não alterar83 nem a políticaUDP. |
| `apps/desktop/src-tauri/src/engine/connect/` | Novo módulo privado de binding/revisão/monitor/retirada e seus testes. Integrar ao slot/cleanup existentes, sem ampliar DTOs/comandos. Preferir módulo próprio a refactor amplo de `live.rs`. |
| `connect/session_group.rs`, `group_source.rs`, `group_actor_source.rs` | Somente os pontos estreitos necessários para o novo caminho Rust distinto, roster/autoridades originais e tracking separado. Manual/descoberto e sua políticaTCP continuam iguais. A retirada exclusivamenteUDP não chama `ActorSource::interrupt` global como atalho. |
| `apps/connect-lab/src/host.rs`, `host_udp_service.rs` e Guests | Consumir as APIs71/81 eGuest existentes. Não há necessidade demonstrada de alterar protocolo/biblioteca; qualquer lacuna real exige revisão própria antes de ampliar esse escopo. Gates privados adicionais devem reter owners reais, sem callback genérico de produção. |

Incremento86A proposto: vínculo/revisão/gates e positivos de bytes com origem/Host/Guests reais, ainda sem monitor publicamente selecionável.86B: retirada terminal, Busy/janela e todas as corridas/owners. Esses nomes organizam implementação futura;86A isolado não libera uso que continue encaminhando depois de invalidação. O aceite86 exige os dois e as regressões pertinentes.

Não implementar ponte do Host jogador, convite local secreto, participante oculto, nona vaga, anúncio35554, mutação de configuração, autenticação SVC/Plasmo, áudio, UI ou rede externa neste recorte. A proposta histórica75 informa esses riscos, mas sua normalização mapped, ausência histórica de produtor e ausência histórica de81 não descrevem a base atual. Ela é somente referência textual histórica, não autoridade ou dependência versionada deste contrato. A etapa do Host jogador exigirá novo contrato com participante explícito entre as oito vagas e conflito/retorno35554 próprios.

## 12. Aceite, riscos, rollback e estado permitido

Aceite exige aprovação integral deste contrato por root/QA, gates85 concluídos, implementação real de86A/B, focais causais, regressões pertinentes/default/features, compatibilidade de bytes/digests antigos, fmt/clippy e revisão independente. Registrar comandos, fontes/hashes, falhas, correções, limites, contagens de pais distintos e cleanup no task quando ocorrerem; filtros sobrepostos não somam novos testes. Não declararPASS por compilação, enum sintético, tabela mock, sample favorável isolado ou contador artificial.

Riscos explícitos: a política sem retry pode retirarUDP por disputa legítima do slot; a janela2s pode recusar um sistema lento. Isso é recusa conservadora de disponibilidade, não diagnóstico de configuração nem motivo para aumentar limites durante um ensaio. ConsultasOS podem ultrapassar a janela e ainda precisam ser recolhidas. Retirada coletiva é composta de selos individuais, e filasQUIC anteriores não podem ser recolhidas. O anúncio padrão não encaminha automaticamente clientes à ponte35554. Mesmo com transporte positivo, não se prova qual mod consumiu bytes ou que áudio é autenticado/audível.

Rollback operacional encerra a operação86 e aguarda seus owners/queries/retiradas reais; o proprietário85 decide separadamente o encerramento do dedicado e seu cleanupEULA. Não reativar a revisão antiga para recuperar o serviço. Rollback de código é local/revisado, preserva journals/formatos/evidências anteriores e não publica release/instalador. Sem download, gasto, push, firewall, configuração pessoal ou segundo computador obrigatório para este recorte.

Estado máximo futuro, somente depois do aceite: **PROTÓTIPO de transporte local de bytes com vínculo nativo e retirada terminalUDP por lease**. Anúncio funcional, Host jogador, autenticação do mod, voz audível, GUI e doisPCs continuam **NÃO IMPLEMENTADO / PRECISA DE TESTE** conforme seus gates próprios.86 é continuação parcial dos IDs existentes AUR-R4-003/005/006/007/008 e dos itens de voz/lifecycle já mapeados em85§10; não criar IDs, matriz ou homologação por consequência.

## 13. Registro de aprovação e implementação incremental

Na elaboração original houve somente leitura e criação do draft, sem Rust ou execução de86/85D/E pelo autor. Depois, root concluiu/revisou/commitou85C1 e85D/E, e QA independente leu e aprovou integralmente o draft acima, inclusive retirada81 e classificação de erros. Root o leu integralmente, aprovou o mesmo desenho e registrou esta canonicalização antes de Rust86. Não é aprovação humana nova nem promoção de estado de produto.

Primeira subdivisão86A0: extrair reserva→execução do mesmo QueryJob84 e binding opaco derivado exclusivamente do mesmoRun, com testes do processo próprio, fontes reais, Busy, gates e completion original. Esse fundamento não abre Host, emite convite, altera anúncio ou libera uso de86A/86B incompletos.86A completo e86B continuam exigindo todas as provas das seções10–12. Cada incremento registrará arquivos/comandos/falhas/cleanup no task antes do seguinte.

# Ciclo de preparação das origens descobertas — incremento75

Data: 15/10/2026. Estado: **PROTÓTIPO**, implementado com provas nativas e DOM/IPC locais; não homologado para usuários. Base74: `5c9f73526ecc9580bad74b5b4164b90239e961a5`. Resultados integrais e falhas preservados no [task75](task-aur-r4-connect.md#incremento75--preparação-individual-descoberta-com-trabalho-próprio). Contratos preservados: [descoberta48](aurora-connect-desktop.md#descoberta-de-porta-da-instância--incremento48), [sessão TCP56](aurora-connect-desktop-session-lifetime.md), [origem TCP na sessão TCP+UDP64](aurora-connect-discovered-session-udp.md) e reserva de trabalho próprio do [grupo74](aurora-connect-discovered-group.md).

## Bloqueio observado e limite do incremento

Antes de75, os três callers de descoberta48/56/64 consultavam `DiscoveryManager::take_target` antes de reservar uma operação Connect. A ativação e as novas consultas de processo/listener também precediam a reserva no manager. Esses comandos Tauri eram síncronos. Enquanto uma consulta demorava, um encerramento de conta podia observar um slot ocioso ou já terminado, concluir sua barreira e depois receber uma preparação antiga publicada na mesma janela proprietária.

A correção reserva e mantém o trabalho próprio desde antes da primeira consulta. A preparação permanece observável e cancelável enquanto a consulta ocorre fora do mutex Connect. O resultado público dos comandos continua `ConnectPreparation`; não substituir APIs existentes pelo retorno inicial de snapshot usado exclusivamente no grupo74.

Escopo: preparo descoberto dos perfis curto TCP48, sessão TCP56 e sessão TCP+UDP64. Preservar comandos manuais, convidados, grupo72/74, assinatura dos convites, limites e gates existentes. Não iniciar Java, abrir mundo LAN, instalar arquivos, mudar configurações, descobrir UDP ou habilitar voz/relay. `connect-lab` permanece experimental e desligada por padrão. Nove pares Core/Skins, Assistente desktop, recovery/CAS/Doctor e roadmap permanecem cumulativos.

## APIs preservadas

Os comandos mantêm nome, parâmetros JSON e retorno de sucesso; passam a executar como comandos assíncronos. O handler chama sincronamente o manager para reservar a operação e instalar seu worker antes do primeiro `await` ou dispatch da consulta bloqueante. O manager devolve um recibo interno com `oneshot::Receiver` para o resultado, que só então é aguardado pelo caller. O `Sender` pertence ao worker; descartar o `Receiver` não cancela, libera ou destaca o trabalho. As consultas síncronas de processo/listener ficam em trabalho bloqueante pertencente à operação, executado pelo único worker bloqueante retido no manager. Colocar a própria reserva dentro de `spawn_blocking(move || manager.reserve(...))` não atende ao contrato: a fila abriria uma janela em que logout ainda poderia observar o slot ocioso.

| Comando | Pedido existente | Perfil do retorno |
| --- | --- | --- |
| `connect_prepare_discovered_host` | `{selectionId,request:{mode,bindAddress,gameTarget,lifetimeSeconds}}` | `invitationProfile` omitido, conforme48. |
| `connect_prepare_session_host_discovered` | `{selectionId,request:{mode,bindAddress,gameTarget,admissionSeconds,sessionSeconds}}` | `invitationProfile:"sessionGameTcp"`, conforme56. |
| `connect_prepare_session_fragmented_host_discovered` | `{selectionId,request:{mode,bindAddress,udpTarget,admissionSeconds,sessionSeconds}}` | `invitationProfile:"sessionFragmentedGameUdp"`, conforme64. |

O retorno permanece `{operationId,generation,publicId,expiresAtMs,invitationProfile?}`. APIs internas da biblioteca podem ser adaptadas ou acrescentadas para separar a reserva síncrona da espera pelo resultado; o JSON público continua igual. A auditoria de callers deve remover ou migrar APIs Rust antigas de preparo descoberto que executem IO antes da reserva, incluindo seus testes internos. Não manter esse caminho vulnerável como compatibilidade interna nem como atalho supostamente confiável. Não acrescentar campos de grupo, seleção, `sessionTiming`, PID, fingerprint ou prova de processo ao retorno. Os DTOs continuam recusando campos desconhecidos. Em48/56, `gameTarget` continua sendo uma igualdade exigida com a origem confiável descoberta; não vira autoridade sobre o destino. Em64, continua proibido fornecê-lo: TCP deriva exclusivamente do guard nativo. O destino UDP64 é loopback manual, autorizado separadamente.

Não haverá capability nova para ativar outra funcionalidade. Os gates existentes `ownedInstanceDiscovery` e, para64, `sessionFragmentedUdpInvitations.ownedGameTcpDiscovery` continuam exigidos. Build normal/não elegível conserva `discovery_unavailable` nesses comandos, sem reserva de transporte.

## Reserva e snapshot intermediário

Antes do primeiro `await` ou dispatch de consulta no handler, e antes de `take_target`, `activate` ou qualquer consulta de processo/listener, validar somente dados puros, proprietário e disponibilidade do slot. Sob o mesmo mutex, reservar UUID, geração, proprietário, token de cancelamento, prazo original e armazenamento do worker próprio. Instalar o handle antes de liberar o mutex; não publicar operação sem o trabalho que ela deve aguardar. A consulta pode aguardar execução na fila de trabalho bloqueante somente depois dessa reserva observável.

Projeção aditiva acordada para a implementação:

```ts
type PendingHostSource = {
  kind: "ownedGameTcp";
  mode: "loopback" | "externalIpv6" | "directIpv4";
  profile: "legacy" | "sessionGameTcp" | "sessionFragmentedGameUdp";
};
```

O novo campo opcional `pendingHostSource` existe somente em `phase:"hostPreparing"`, para estes três callers. Nesse estado:

- `operationId` é o UUID reservado, `generation` é monotônica e positiva, `role:"host"`.
- `transportState:"notStarted"`, `statusState:"notChecked"`, `cleanupState:"pending"`.
- `expiresAtMs` é o instante da reserva mais60s; o prazo monotônico é ancorado na mesma reserva.
- `preparation`, `review`, `tcpAddress`, `statusSummary` e `errorCode` são nulos. Ainda não há chave, peer, convite ou socket Connect.
- `invitationProfile` é omitido no legado; nos perfis56/64 corresponde ao `profile` de `pendingHostSource`. O literal `legacy` existe somente nesse metadado intermediário, sem mudar os aliases antigos.
- Não há `hostGroup`, `pendingGroupSource`, `sessionTiming`, contrato/estado/endereço/recuperação UDP ou destino privado nessa projeção. As afirmações de mundo/voz/caminho permanecem não verificadas.

Os parsers distinguem `pendingHostSource` de `pendingGroupSource` por forma exata. Não aceitar ambos, um marcador em fase diferente, perfil/mode contraditório ou uma identidade fictícia para preencher `ConnectPreparation`. A ausência dos dois marcadores em `hostPreparing` é inválida. O marcador75 é omitido em todas as respostas dos caminhos anteriores que não estão nessa fase.

O worker usa a seleção original de uso único, vinculada à janela, sem reconstruir uma busca ou devolver o token ao estoque. Reconferir operação, fase, cancelamento e prazos entre consultas e antes de publicar a preparação. A seleção conserva seus30s originais; a reserva60s não permite consumir ou ativar uma seleção vencida. Mudança de processo/listener, proprietário errado ou consumo anterior falham na operação reservada.

Após a validação, criar a identidade e publicar `hostPrepared` com a `preparation` comum, removendo `pendingHostSource`. Nessa fase, `snapshot.expiresAtMs` volta a ser nulo como no contrato anterior; `snapshot.preparation.expiresAtMs` conserva o vencimento capturado na reserva. O retorno IPC de sucesso deriva dessa preparação publicada, com o mesmo UUID, geração e prazo. O deadline não recomeça depois do IO. A revisão, confirmação, exportação e transporte existentes são reutilizados; não abrir socket no worker de preparação.

## Cancelamento, conclusão e perda do waiter

O trabalho pertence ao `Attempt`, inclusive antes de existir uma identidade. Descartar ou perder o future IPC e seu `oneshot::Receiver` não cancela, desanexa, libera ou transfere o worker para o frontend. O worker conserva o `Sender`, publica o resultado na operação e tenta entregá-lo ao waiter uma única vez; se o receptor já saiu, o snapshot continua sendo a fonte de observação. Não repetir consulta, preparo ou entrega como nova operação. O slot permanece ocupado enquanto a tarefa própria ainda termina ou coleta suas consultas bloqueantes. Não sobrescrever seu handle quando o ator Host for criado; manter a cauda da preparação separada do ator de transporte.

`connect_snapshot` permite observar a reserva pelo UUID durante o await do comando original. `connect_stop`, logout e shutdown cancelam essa mesma operação e aguardam os trabalhos próprios. Uma resposta antiga não cria um novo UUID nem publica chave/revisão depois do cancelamento. Shutdown continua impedindo toda reserva nova; stop não encerra Java nem o listener da instância.

Quando o preparo falha ou é cancelado antes da identidade, o snapshot pode terminar sem `preparation` e sem marcador pendente: `phase:"failed"|"closed"`, transporte fechado, prazo nulo e erro controlado quando aplicável. `cleanupState:"pending"` é obrigatório enquanto ainda existir trabalho próprio. Somente o término real do worker bloqueante retido, observado pelo manager, permite `complete` e reutilização do slot. O worker faz consulta, captura de pânico, publicação e entrega pelo oneshot no mesmo trabalho; não há supervisor assíncrono adicional nem promessa de um join que não tenha sido executado. Prazo vencido remove autoridade imediatamente, mas não significa consulta já coletada.

O retorno assíncrono ainda usa os códigos de erro públicos existentes. Um código sozinho pode corresponder a rejeição antes da reserva ou falha da operação já reservada; o frontend deve reconciliar o snapshot antes de liberar novo preparo quando houver dúvida. Não inventar uma preparação para transformar falha em sucesso. Erros desconhecidos, resposta malformada ou perda de resposta nunca autorizam reenviar `selectionId`.

Um snapshot ocioso/terminal anterior ao início do pedido não comprova rejeição da reserva. Guardar geração/UUID anteriores e aceitar como reconciliação uma nova operação coerente ou a prova específica de rejeição anterior à reserva descrita abaixo. Se não houver evidência suficiente, manter resultado desconhecido e permitir atualização explícita. Uma conta nova só aparece após a barreira do App observar e aguardar os trabalhos próprios.

Depois de receber um erro **nativo, controlado, catalogado e definitivo** daquele pedido, a interface pode iniciar uma nova chamada `connect_snapshot`. Somente uma consulta iniciada estritamente depois desse reply pode provar rejeição anterior à reserva: o UUID e a geração devem ser iguais aos da observação anterior ao pedido, e o estado deve ser `idle` sem trabalho ou terminal com `cleanupState:"complete"`. O manager garante que toda reserva cria UUID novo e incrementa a geração; depois da reserva, não remove o `Attempt`, não volta para `None` e não restaura o UUID/geração anteriores. Portanto, essa igualdade observada após o erro definitivo prova que aquele pedido não reservou trabalho. O formulário pode voltar a ser oferecido, exigindo **nova seleção**.

Registrar explicitamente esse contexto no frontend, por exemplo em `controlledRejectionSettled`: pedido/epoch corrente, erro nativo catalogado recebido, baseline imutável e chamada de snapshot iniciada após a resposta. Não reutilizar cache, snapshot anterior, poll já iniciado antes do reply ou resposta de outro pedido como essa prova. Se a consulta nova falhar ou devolver dados inválidos, manter o bloqueio; uma atualização explícita posterior pode tentar obter a prova usando o mesmo contexto definitivo.

Igualdade com baseline ativo não libera novo preparo: permite apenas observar e encerrar a operação existente. Erro de IPC desconhecido, resposta malformada ou perda de receptor/resultado sem erro definitivo continuam incertos; mesmo um snapshot ocioso igual não os transforma em rejeição. Em todos os casos a seleção visual já foi consumida e não será reenviada.

Após erro definitivo e snapshot novo com o mesmo token de contexto, uma operação nativa de UUID e geração novos também pode ser observada e encerrada, mesmo que pertença a outro perfil, Guest ou grupo. Isso não atribui essa operação ao preparo recusado nem transfere consentimento, seleção ou convite. Sem a prova de erro definitivo, a reconciliação conserva a correspondência de perfil/modo exigida para o pedido cujo resultado continua desconhecido.

## Interface

Reutilizar o seletor48 e as ações explícitas de inventário, revisão e STATUS. Preparar consome a seleção visual uma vez nos três perfis, antes do await. Conferir validade no clique; resposta tardia do seletor após mudança de papel/perfil não repõe a seleção. Mudar de perfil/origem ou encerrar exige escolha nova.

Durante o preparo, mostrar uma mensagem simples de conferência do jogo. Permitir consultar o snapshot mesmo com o pedido inicial pendente; assim que houver UUID observado, permitir Encerrar Connect. Não aguardar indefinidamente o primeiro IPC para oferecer cancelamento. O campo TCP manual antigo não substitui a seleção descoberta; UDP64 continua explícito e separado.

Após `hostPrepared`, apresentar a identidade e as ações de revisão existentes. Não aprovar, exportar ou iniciar automaticamente. Resposta inicial que chega depois de um stop/novo contexto não restaura preparação, consentimento ou formulário. Observação de terminal com cleanup pendente mantém o encerramento disponível e bloqueia novo preparo. Falha ao obter capabilities não remove observação ou limpeza da operação já existente.

A integração75 deve conservar o retorno `ConnectPreparation` inclusive para clientes existentes; a interface atualizada usa os novos snapshots intermediários para segurança e clareza. Não inferir conectividade, servidor disponível, jogador no mundo ou áudio de uma preparação concluída.

## Critérios de prova

A lista abaixo define as provas exigidas; somente as execuções explicitamente registradas na seção seguinte contam como realizadas. As provas74 não contam como execução destes callers.

- Regressão de formato: JSON de entrada e saída dos três comandos, perfis e omissões originais, comandos manuais e convidados inalterados; default-off sem reserva nem consulta.
- Gate causal antes de `take_target`: snapshot intermediário sem peer/socket; stop e logout observam UUID e permanecem pendentes até coleta. Descartar o waiter não libera slot; consulta liberada após cancelamento não publica preparação.
- Gate durante `activate`/recheck e após publicação: consultas fora do mutex, parada observável, nenhuma troca de handle por confirmação rápida, slot bloqueado até finalizar a cauda própria.
- Expiração civil/monotônica injetada: seleção30s e preparo60s contados de suas âncoras originais, inclusive após IO lento; sem alterar relógio do PC, sem renovar prazo ou usar600s do grupo.
- Erros e autoridade: seleção de outra janela, consumo repetido, mudança de processo/listener, divergência de `gameTarget`48/56 e campo extra64 rejeitados sem abrir transporte. Um token consumido não volta a ficar disponível após falha.
- Transporte de regressão local: cada perfil continua passando pelo respectivo contrato autenticado existente após preparo/revisão/consentimento, e perda da origem continua encerrando somente Connect. Não reinterpretar teste sintético como Minecraft LAN real.
- DOM/IPC controlado: três perfis consumindo uma única seleção; observação e stop durante await; erro/resposta perdida, idle antigo, remount, troca de contexto e resposta tardia; compatibilidade manual e rejeição de marcadores/perfis cruzados.
- Prova de rejeição anterior: erro catalogado seguido de consulta nova recupera formulário somente para o mesmo baseline idle/terminal com cleanup completo; exige nova seleção. Poll iniciado antes do erro, cache igual, erro desconhecido, UUID/geração diferentes, consulta inválida e baseline ativo não autorizam esse caminho. Uma atualização iniciada depois de falha da primeira consulta pode recuperar usando o contexto definitivo preservado, sem reenviar o preparo.
- Barreira de conta: preparação pendente75 aparece como operação própria para `stopConnectForAccountTransition`; nenhuma conta nova publicada antes de `cleanupState:"complete"` da mesma operação e geração não regressiva.

Registrar comandos, falhas preparatórias, correções, contagens e limites reais no task canônico somente após execução. Fmt/lint/build e fixtures não comprovam Tauri gráfico, internet, Minecraft, dois jogadores, contas mistas, Core/Skins ou áudio. Simple Voice Chat e Plasmo Voice mantêm homologação separada; IPv4/NAT/CGNAT e relay elegível continuam seus gates próprios.

## Evidência da interface75

Arquivos do checkpoint: `ConnectView.tsx`, `ConnectView.smoke.mjs`, `types.ts`, o parser/helper novo `host-source-types.ts` e `account-transition.smoke.mjs`, todos em `apps/desktop/src/features/connect`. A interface preserva o envio dos três DTOs, consome a seleção antes da espera, observa a reserva e mantém cancelamento sem aguardar o retorno inicial. O formulário exige uma primeira observação nativa válida para existir. O helper de rejeição só recebe o token capturado antes de cada consulta; uma consulta anterior ao erro não pode provar ausência de reserva.

Comandos realmente executados, com diretório de trabalho `apps/desktop`:

```powershell
node --test src/features/connect/ConnectView.smoke.mjs src/features/connect/fragmented-types.test.mjs src/features/connect-game-launch/ReadyGameLaunch.smoke.mjs src/features/connect/account-transition.smoke.mjs
npm run build
```

Primeira rodada: **206 PASS / 1 FAIL**,2808,1938ms. A falha foi o oráculo64 que procurava o botão de preparo após perda de IPC; o fluxo75 bloqueia todo o formulário enquanto o resultado é desconhecido. Corrigida a expectativa para formulário ausente e aviso de resultado desconhecido, preservando a prova de seleção consumida. Segunda rodada: **207 PASS / 0 FAIL**,2753,5373ms. Após os casos adicionais de baseline inicial ausente e rejeição definitiva com outra operação observada, rodada final: **212 PASS / 0 FAIL**,2963,0434ms, zero ignorados. As rodadas e suítes se sobrepõem; não somar suas contagens como provas distintas.

Build final passou: **371 módulos**,6463ms para TypeScript/Vite,513ms informados pelo Vite. Conserva o aviso anterior de chunk acima de500kB. Nenhuma dependência foi atualizada. Logs completos locais: `.aurora/connect-validation/discovery-lifecycle-75-frontend-{first,second,final}.log` e `discovery-lifecycle-75-build-{first,final}.log`. Esses logs contêm fixtures sintéticas e resultados, sem credenciais, peers externos ou dados pessoais.

As provas incluem três perfis, preservação dos DTOs, reserva sem peer, prazo original projetado, stop durante await, resposta atrasada, erro pós-reserva com cleanup pendente, resposta desconhecida/malformada, expiração da seleção, recuperação por nova consulta após rejeição, consulta iniciada antes do erro, falha da consulta de prova, baseline ativo recusado, outra operação observada sem consentimento, remount e barreira de troca de conta aguardando cleanup. Nenhum teste desta seção inicia transporte real, processo Java, conta Firebase, janela Tauri ou voz. Os resultados nativos e a revisão final serão registrados separadamente após execução.

## Evidência nativa75 e revisão

`host_source.rs` reserva o Attempt e instala o único worker bloqueante antes de soltar o mutex; o retorno interno só transporta o receptor. Os caminhos Rust antigos de IO pré-reserva foram removidos e os callers48/56/64 migrados. `live.rs`, `ready_game.rs` e inicializadores de grupo integram a propriedade do trabalho às barreiras. Nenhum arquivo de transporte da biblioteca, dependência, configuração pessoal ou matriz foi alterado.

Comando focal: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-features --lib host_source -- --test-threads=1`. A primeira compilação recusou E0308; corrigido binding local do MutexGuard, nova rodada passou9/9. Reforço posterior dos JSONs default/prepared não altera o contrato. Focais finais com filtros `host_source`, `connect_discovery`, `discovered_session`, `owned_fragmented_tests` e `stop_completion_tests`: respectivamente **9 PASS;6 PASS/1 ignorado;2 PASS;4 PASS;2 PASS**, sem falhas. As suites se sobrepõem às regressões integrais; não somar resultados. Comandos completos, tempos, logs e estados finais das suites maiores estão no task.

Os nove casos próprios exercitam três perfis, reserva anterior à consulta, perda de receptor/waiter de stop, pânico com erro entregue antes da cauda terminar, shutdown, consumo/owner/expiração/equivalência da seleção, substituição de origem, clocks privados e confirmação rápida conservando a cauda do preparo. Regressões48/56/64 usam processo/listeners de fixture e atores autenticados reais no próprio PC. Stop Connect preserva o filho do teste até seu encerramento próprio. Default omite o novo marcador; comandos sem feature conservam discovery_unavailable. Nenhum Java, cliente Minecraft, conta, áudio ou rede externa foi executado por estas provas.

Revisão independente confirmou o ownership e exigiu a prova causal de erro pós-reserva antes do fim do worker. Também corrigiu a observação de outra operação válida após erro definitivo sem transferir consentimento. Nenhum bloqueador identificado no checkpoint. `is_finished` observa término real do worker retido; este desenho não alega um supervisor separado ou join aguardado onde não existe.

## Decisões e limites remanescentes

O formato `pendingHostSource` acima, os três comandos e seus DTOs públicos estão acordados. A reserva síncrona cria `Attempt`, prazo60s e worker antes de retornar o recibo interno com `oneshot::Receiver`; somente então o handler aguarda o resultado. O worker conserva o `Sender` e o ownership do trabalho independe do receptor. Não haverá repetição automática. Erros públicos não ganharão um marcador novo de pré/pós-reserva: a interface reconcilia uma nova operação ou aplica exclusivamente a prova de erro catalogado mais snapshot novo descrita acima. Fora dessas condições, conserva o bloqueio.

Implementação nativa e auditoria de callers concluídas neste escopo; os resultados registrados acima e no task têm autoridade sobre as provas efetivamente executadas. O grupo conserva preparo600s e contrato próprio;75 mantém60s individuais. Preparação validada, STATUS sintético, processo de fixture e DOM têm escopos distintos e não homologam o produto.

A revisão síncrona de origem dos perfis individuais também existe fora deste preparo; não declarar todo o ciclo livre de consultas bloqueantes por esta proposta. Qualquer ampliação para revisão/monitor exige delimitação e testes próprios antes da implementação, preservando a autoridade e seus prazos.

## Rollback e ação do usuário

Rollback operacional: Encerrar Connect e aguardar recursos liberados. O jogo permanece independente. Não há configuração, snapshot pessoal ou recibo de voz alterado por este contrato. Rollback de código será um revert local revisado do incremento implementado, preservando os anteriores; não há push, publicação, instalador ou custo.

A implementação e as provas locais não precisam de outro computador ou intervenção do usuário. Procedimentos externos serão preparados separadamente quando os testes entre computadores forem retomados. Este incremento não representa homologação nem conclusão dessas etapas.

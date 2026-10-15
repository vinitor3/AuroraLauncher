# Porta de jogo descoberta no grupo de participantes — incremento74

Data: 15/10/2026. Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**, sob `connect-lab` desligada por padrão. O [grupo72](aurora-connect-multiparticipant-desktop.md) aceita agora a seleção opaca da [descoberta48](aurora-connect-desktop.md#descoberta-de-porta-da-instância--incremento48), conservando também sua origem manual. A [integração64](aurora-connect-discovered-session-udp.md) fornece o guard de origem reutilizado. Foco final74 com11 testes e interface com177 testes aprovados; a regressão integral tem registro próprio ao final. Não há nova homologação de Minecraft, GUI ou redes externas.

## Bloqueio e escopo

O launcher identifica a porta TCP do processo filho que ele iniciou, revisa uma consulta STATUS e devolve uma seleção opaca. O Host de grupo72 exigia `gameTarget` manual. Este incremento conecta a seleção existente ao grupo estático de dois a oito convidados, pela ação descoberta explicitamente escolhida.

O escopo é somente a escolha e a conservação da origem `game/TCP`. O destino `aux/UDP` permanece manual, loopback e autorizado separadamente na revisão. Descobrir TCP não identifica Simple Voice Chat ou Plasmo Voice, não descobre UDP e não autoriza alterar configuração de nenhum mod. Cada convidado continua usando seu convite individual schema3, os prazos e grants originais, e a recuperação UDP63 já existente.

Preservar os nove pares Core/Skins, o Assistente desktop, a matriz e todos os gates R0–R4. A feature `connect-lab` continua desligada por padrão. Não há instalação, execução de Java, abertura automática de mundo LAN, varredura de rede, broadcast, seleção de PID pela interface, sinalização social, renovação de convite ou inclusão dinâmica de participantes.

## Capability e pedido próprios

Extensão aditiva: o campo existente `sessionFragmentedHostGroups.ownedGameTcpDiscovery` anuncia `true` somente no Windows sob `connect-lab`. Os outros oito campos e os valores do contrato72 permanecem iguais. Backend antigo ou não elegível conserva `false`; ausência da capability de grupo mantém o painel anterior. O parser aceita os dois valores booleanos exatos, recusando valores ausentes, strings e capacidade contraditória com a descoberta nativa disponível.

Não interpretar convites ou a presença de uma seleção como capability. A ação nova usa comando e DTO próprios:

```ts
type GroupDiscoveredHostRequest = {
  mode: "loopback" | "externalIpv6" | "directIpv4";
  bindAddress: string;
  udpTarget: string;
};

connect_prepare_session_group_host_discovered({
  selectionId: string;
  request: GroupDiscoveredHostRequest;
}): ConnectSnapshot;

type PendingGroupSource = {
  kind: "staticSessionFragmentedHost";
  mode: "loopback" | "externalIpv6" | "directIpv4";
};
```

O pedido recusa campos desconhecidos. Não recebe `gameTarget`, PID, caminho de processo, instância, execução, fingerprint, protocolo ou identidade de jogo. A janela principal continua sendo a proprietária. O comando manual72 não passa a aceitar `selectionId` nem deriva descoberta de campos extras.

Antes de qualquer consulta ou consumo da seleção, o manager reserva um `Attempt` com UUID, proprietário, geração, cancelamento e trabalho pertencente à operação. A reserva e a instalação de `source_work` acontecem sob o mesmo mutex, antes de permitir observação ou stop; não deixar uma janela com operação publicada e trabalho ainda sem proprietário. O comando devolve esse snapshot inicial em `hostPreparing`; ainda não existem chave, peer ou socket de grupo. Essa reserva impede que logout ou troca de conta observem um slot ocioso enquanto uma consulta antiga continua e depois publica um grupo. Não manter o mutex Connect durante consulta de processo/listener.

O snapshot inicial tem `role:"host"`, `transportState:"notStarted"`, `cleanupState:"pending"`, `expiresAtMs` igual ao instante da reserva mais600s, e `pendingGroupSource` com os dois campos acima. `hostGroup` está ausente; `preparation` e `review` comuns permanecem nulos. `pendingGroupSource` é exclusivo de `hostPreparing`: sai quando a operação publica `hostPrepared`, entra em encerramento ou falha. Encerramento/falha antes de criar a identidade pode, portanto, não ter `hostGroup`. Não usar string vazia ou peer fictício para satisfazer o parser.

O worker próprio obtém a seleção pela API existente `DiscoveryManager::take_target`, confere o proprietário, ativa o `ApprovedTarget` original e deriva TCP exclusivamente de `recheck_owned`, antes de construir os serviços. `DiscoveryManager` agora conserva seu mesmo estoque privado em `Arc<Mutex<...>>`; seu clone retido pelo worker não duplica uma seleção nem permite fabricar um guard. Cancelamento, operação e prazos são reconferidos após cada consulta e imediatamente antes da publicação da chave e de `GroupPreparation`. Somente então aparece `hostGroup.preparation`, em `hostPrepared`, pela consulta de snapshot já existente. O prazo600s começa na reserva, não no fim do worker.

Uma seleção inválida, vencida, já consumida ou de outra janela falha no snapshot da operação reservada, antes da identidade/transporte de grupo. Se uma etapa posterior falhar depois do consumo, o usuário precisará de nova seleção; não devolver automaticamente o token ao estoque nem repetir o pedido após perda de resposta. Antes da reserva, somente validações puras e condições do slot podem rejeitar o comando: feature indisponível, proprietário/pedido inválidos, operação ocupada, shutdown ou erro interno de acesso ao estado. Falha de IPC sem resultado estruturado continua ambígua; um snapshot ocioso isolado não prova que o pedido foi rejeitado antes de reservar.

Os comandos de revisão, confirmação, exportação individual, revogação e stop72 são reutilizados. A revisão Tauri executa o trabalho síncrono em `spawn_blocking`, com `AppHandle` próprio; o registro RAII nasce na execução real. O grupo continua ocupando o mesmo slot; não há um segundo manager de descoberta ou transporte paralelo.

## Revisão e vinculação à origem

`GroupReview.targetSource` é uma união explícita `manual | ownedInstance`. A revisão descoberta acrescenta `instanceId` e `runId`, ambos não vazios e vindos do guard nativo. Esses campos são omitidos no grupo manual, preservando sua forma anterior. A projeção pública não contém PID, handle, data de criação, fingerprint privado ou linha de comando.

```ts
type GroupSourceReview =
  | { targetSource: "manual" }
  | { targetSource: "ownedInstance"; instanceId: string; runId: string };
```

Os demais campos de `GroupReview` são os de72: operação, geração, digest, kind, modo, peer do Host, endereço de transporte, gameTarget observado, política/endpoint UDP, participantes e prazo. A interface deve mostrar claramente que TCP veio do jogo próprio selecionado e que UDP foi informado manualmente. Uma revisão manual com campos de origem, ou descoberta sem instância/execução, deve ser recusada pelo parser.

O guard opaco original é retido pelo `GroupDraft` e pelo ator do grupo. O digest da revisão descoberta usa domínio próprio e inclui proprietário, operação, origem/instância/execução, fingerprint privado do processo, endereços, políticas, roster completo, digests individuais e prazos. O domínio/formato do grupo manual72 permanece igual.

A consulta de origem durante a revisão acontece fora do mutex, mas permanece pertencente ao `Attempt` por um registro RAII exclusivo. Esse guard vive na execução efetiva da consulta síncrona/bloqueante, não apenas no future IPC que a aguarda: descartar o waiter não libera uma consulta ainda em andamento. Outra revisão concorrente é recusada enquanto esse registro existir. Antes de publicar roster, convites ou digest, comparar novamente operação, geração, fase e cancelamento; o guard libera o registro em sucesso, erro ou pânico. Conferência apenas antes/depois da consulta não substitui sua propriedade: stop/logout e expiração devem aguardar o registro antes de liberar o slot.

A seleção não deve ser substituída por uma nova busca no momento da confirmação. Processo encerrado, origem divergente ou porta reaberta por outro processo invalidam a revisão; não autorizam nova origem nem preservam consentimento para um alvo diferente. Os convites dos convidados não expõem o destino privado ou a prova de processo do anfitrião.

## Prazos e vida do grupo

| Etapa | Limite e conservação |
| --- | --- |
| Inventário, consulta e seleção de origem | Prazos30s já existentes em48; consumir/ativar a seleção dentro de sua validade original. |
| Preparação do grupo | Até600s desde a reserva, incluindo o worker inicial e a coleta de identidades; não revalida seleção vencida nem prolonga seu prazo de consumo. |
| Revisão do grupo | Até60s, limitada também pela preparação e pelos ingressos individuais; mesmos relógios civil/monotônico de72. |
| Ingresso de cada convidado | Até600s; consumo único, sem renovação por exportação ou nova observação. |
| Sessão de cada convidado | Até3600s desde emissão, sessão≥ingresso; sem prazo coletivo que substitua as autoridades individuais. |

Depois de ativada, a seleção deixa de ser um token disponível: o ator conserva o guard do processo e observa continuamente a mesma origem. Isso não prolonga o prazo30s de consumo, não recria a seleção e não renova autoridade de transporte. Preparar, revisar e confirmar reconferem esse guard, inclusive depois de IO que possa consumir o prazo aplicável.

Antes de abrir o transporte, o worker reconfere origem e cancelamento. Em execução, o monitor verifica a origem a cada1s, com consulta Windows fora do mutex Connect. Stop/logout devem manter a tarefa pertencente à operação e aguardar também uma consulta pendente; perder a chamada IPC não transfere a propriedade nem permite trocar o slot antes do cleanup.

O trabalho de origem tem armazenamento próprio, separado do handle do ator Host. Publicar `hostPrepared` não significa que a cauda da preparação terminou; uma confirmação rápida não pode sobrescrever ou destacar esse handle. A preparação e o registro de revisão participam das mesmas barreiras de `terminal`, stop, logout, expiração e recuperação de snapshot. `cleanupState` continua `pending` enquanto algum desses trabalhos existir, inclusive após fase terminal publicada; somente o retorno efetivo permite `complete` e novo preparo. O monitor do Host também aguarda a consulta bloqueante que iniciou; cancelamento não abandona uma consulta Windows em execução.

Precisão da barreira: a consulta bloqueante é aguardada pelo worker; erro/pânico dessa tarefa vira erro controlado. O handle externo de preparação permanece no `Attempt`, e sua conclusão é observada por `JoinHandle::is_finished`, junto à flag RAII da revisão; o caminho não faz `await` do resultado desse handle externo. As provas causais cobrem caudas mantidas vivas, perda de waiter e liberação do slot somente após retorno. Isso não é uma promessa de interrupção instantânea de qualquer chamada Windows nem de recuperação automática de mutex envenenado.

Se a origem se perder, encerrar **todo o Host de grupo**, com `connect_source_changed`: todas as conexões TCP, associações UDP e recursos Connect daquele grupo são fechados e aguardados. Nenhum participante continua apontando para um servidor diferente na mesma porta. O estado final conserva as decisões históricas de consumo/revogação, projeta transporte inativo e só declara cleanup concluído quando os workers próprios tiverem terminado.

O launcher não encerra o jogo, não restaura arquivos de voz automaticamente e não reutiliza convites consumidos. Revogação individual mantém seu comportamento72; perder a origem compartilhada é um evento distinto que encerra o grupo inteiro. Falha de UDP isolada de um Guest continua sem derrubar seu TCP quando a origem do jogo permanece válida.

Observar processo/listener antes e depois de STATUS e periodicamente não torna atômica a observação com cada conexão TCP. Permanece a corrida local documentada48/64; não prometer sandbox contra um usuário local comprometido. STATUS767 é uma observação do servidor, não prova de mundo, jogador ou mod.

## Interface

No modo Host de grupo, oferecer a mesma escolha explícita de origem dos perfis anteriores: **Endereço manual** ou **Jogo aberto pelo Aurora · mundo LAN**, somente com capability disponível. Reutilizar o seletor existente e sua revisão/consulta de STATUS. O campo UDP deve permanecer visível nos dois modos.

Selecionar origem não inicia grupo, Java ou convite. Preparar grupo descoberto exige uma seleção atual e a consome uma única vez. Não mandar o valor antigo do campo TCP manual junto ao DTO descoberto. Mudar perfil/origem ou encerrar a operação limpa a seleção e o consentimento; resposta atrasada não restaura origem de outra operação. Revisar roster continua sendo uma segunda ação explícita, com todos os participantes e destinos apresentados antes da confirmação.

Após resposta de preparo perdida, consultar o snapshot do slot já existente; não preparar novamente automaticamente. A interface mostra preparo em andamento em `hostPreparing`, recupera `GroupPreparation` de `hostGroup.preparation` quando a operação chega a `hostPrepared`, e permite stop pelo UUID desde a reserva. Se não houver observação suficiente, manter resultado desconhecido e permitir atualização explícita. Uma conta nova só pode ser publicada depois da barreira de encerramento existente. Nenhum botão deve prometer jogador no mundo ou áudio disponível por conexão autenticada.

## Provas e limites da validação

Não somar provas64/72 como se fossem testes de74. Os casos abaixo estão implementados como ensaios próprios; os resultados efetivamente executados ficam no registro ao final desta seção.

- Contrato/default: capability desligada ou falsa não invoca descoberta; campos extras/PID/gameTarget recusados; modo/endereço continuam sob as políticas existentes. API manual e parsers antigos têm regressões próprias.
- Seleção: proprietário errado, consumo repetido, prazo30s vencido, fonte encerrada antes/depois de STATUS e entre preparo/revisão/confirmação falham sem abrir transporte. Não alterar o relógio do PC nos testes.
- Transporte real local: um Host e dois Guests autenticados, cada um com seu convite schema3, passam TCP e UDP4096B simultaneamente com retorno isolado. Esse ensaio usa processo/listener de fixture próprio e STATUS sintético, sem Minecraft pessoal.
- Origem perdida: encerrar o processo filho de fixture após os dois ingressos encerra o grupo inteiro; reabrir a mesma porta em outro processo não recupera autorização. Verificar ausência de novos destinos, ambos os Guests fechados e rebind dos recursos próprios depois do cleanup.
- Concorrência inicial: gate causal bloqueia o worker antes de consumir a seleção, com `hostPreparing` observável e sem chave/socket. Stop/logout e perda do primeiro waiter mantêm o slot ocupado até retorno e coleta; o worker antigo não cria identidade depois de cancelado.
- Cauda de publicação: gate causal segura o worker depois de publicar `hostPrepared`. Revisão/confirmação rápida, stop e perda de waiter não descartam o handle; o trabalho da origem permanece separado do ator Host e bloqueia reuse até terminar.
- Revisão e monitor: gate causal bloqueia a consulta fora do mutex; revisão duplicada é recusada e stop/expiração/logout aguardam o registro RAII ou monitor próprio. Operação, geração e fase antigas não publicam roster nem consentimento depois do cancelamento. Nenhum worker destacado, grupo iniciado depois de stop ou publicação terminal perdida.
- Isolamento/prazos: revogar um participante ainda preserva o outro enquanto a fonte vive; perder a fonte encerra ambos. Ingresso vencido não encurta sessão consumida válida, e descoberta não renova nenhum dos relógios72.
- DOM: escolha descoberta somente com capability; ausência da seleção desabilita preparo; pedido sem gameTarget; UDP manual preservado; revisão distingue origem; campos contraditórios, resposta antiga/perdida e tentativa duplicada recusados; fluxo manual continua disponível.

Registrar comandos realmente executados, resultados e primeiras falhas antes de atualizar este estado. Exigir fmt, lint/build pertinentes, testes focados e regressões necessárias após a implementação. Compilação não prova Tauri gráfico, Minecraft LAN, dois jogadores, contas mistas, áudio, NAT/CGNAT ou estabilidade prolongada; esses gates permanecem separados e pendentes.

Registro parcial15/10: primeira focal nativa **5 PASS**,1,56s, compilação36,52s, log privado `group-74-owned-first.log`; rodada ampliada **10 PASS**,3,04s, e contrato DTO **1 PASS**. A ampliação inicial não executou testes porque um teste tentou chamar `OwnedChildProof::recheck` privado (`E0624`); a verificação passou a usar a API pública de inventário, sem ampliar a API de produção. Esse primeiro erro fica preservado em `group-74-owned_group.log`. O teste adicional da fronteira entre review e confirm, e as regressões finais, serão registrados pelo executor responsável. `rustfmt --edition 2021 --config skip_children=true` nos arquivos nativos alterados e `git diff --check` foram executados. A interface e o relatório canônico terão contagem final própria; não somar rodadas.

A regressão72 encontrou um timeout reproduzível na rajada de quatro UDP4096B junto a dois fluxos TCP32KiB. A instrumentação preservada observou quatro requests no endpoint, Host com `udp_forwarded=4`, `udp_returned=3`, `udp_dropped=1`, e um Guest sem um retorno; não houve oversize. Isso localiza um descarte no retorno do Host, mas não identifica isoladamente qual gate interno o decidiu. Saturação do orçamento compartilhado é **HIPÓTESE** consistente com a reserva TCP16KiB e com o custo de requests/returns UDP incluindo headers; não atribuir retrospectivamente a falha ao monitor74.

O ensaio72 foi ajustado para testar isolamento com concorrência comprovada dentro do orçamento: mantém os quatro UDP4096B sem retries e os dois TCP32KiB exatos em ambos os sentidos. O endpoint segura o primeiro retorno UDP; depois da chegada, cada TCP transmite e confirma blocos1024B, com5ms entre blocos. Somente após ACK do primeiro bloco em ambos os fluxos o endpoint libera os retornos. Asserções exigem2 ACKs, pelo menos2048B de TCP enquanto UDP aguardava,32768B por fluxo e4 requests únicos. Quotas, TTL, prazo8s e biblioteca de transporte permanecem iguais. O gate só é ativado nesse teste; não representa um mecanismo do produto nem homologa entrega UDP infalível sob saturação. Logs das falhas: `group-74-final-session_group.log`, `group-74-manual-diagnostic.log` e `group-74-manual-metrics-repeat-1.log`.

Checkpoint revisado: `owned_group` **11 PASS/3,15s**, `group_profile` **1 PASS/0s**, `session_group` **10 PASS/7,18s**, `owned_fragmented_tests` **4 PASS/6,08s**, `stop_completion_tests` **2 PASS/0,07s**, `ready_game` **10 PASS/1 ignorado/6,65s**. O foco individual da fixture causal passou1/1 em0,68s; não somar a repetição presente no grupo10. Comando-base `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-features --lib <filtro> -- --test-threads=1`, logs privados `group-74-reviewed-*.log`. Interface final **177 PASS/0 falhas/0 ignorados**,2377,0277ms; TypeScript/Vite **PASS**,370 módulos. O [task74](task-aur-r4-connect.md#incremento74--origem-tcp-descoberta-no-grupo) registra comandos completos, falhas, correção da fixture e regressões integrais. Nenhuma quota/TTL/código da biblioteca de transporte foi alterada pela correção do ensaio.

Regressão integrada final: **460 PASS/63 ignorados** com ambas as features e **289 PASS/5 ignorados** no padrão, zero falhas; oito targets em cada execução. Tempos de testes somados206,96s/57,34s e compilação1m49s/53,41s. Clippy all-targets combinado/padrão passou em16,38s/10,04s; fmt e diff passaram. Logs `group-74-{desktop-full,default-full,clippy-all,clippy-default,fmt}.log`; comandos exatos e falhas históricas preservados no task74. Revisão independente sem bloqueador identificado; nenhuma prova nova de Java, cliente real, áudio ou rede externa.

## Rollback e dependências do usuário

Rollback operacional: encerrar Connect e aguardar cleanup completo. Se o jogo permanecer aberto, fica sob controle do usuário. Desabilitar a capability nova remove somente a escolha descoberta no grupo, preservando grupo manual e demais perfis. Reversão de código deve ser local e revisada após encerramento; não há mudança de configuração pessoal a desfazer neste incremento.

Implementação e provas locais com fixtures não dependem de outro computador nem de intervenção do usuário. A validação posterior de um mundo Minecraft aberto para LAN, janela Tauri, áudio ou redes externas precisará de procedimento próprio e evidência real; preparar um roteiro não satisfaz esses testes.

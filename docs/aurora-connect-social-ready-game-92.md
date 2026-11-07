# Connect92 — origem social viva para a abertura explícita de cópia pronta

Contrato local de implementação, extensão da ADR-033 e dos incrementos73/79/90.
Estado atual: **PROTÓTIPO / PRECISA DE TESTE**. O contrato
foi escrito antes do código; a implementação conserva o mesmo
ConnectManager/Attempt, slot, epoch e barreiras de encerramento.
Não cria cliente social paralelo, política nova, autoentrada sem consentimento,
conta real, instalação de jogo ou autoridade baseada no endereço da interface.

## Autoridade original

O ator convidado só publica uma GuestSource depois da admissão social verificada,
da conferência de grant/contexto/convite e da criação da própria ponte `game/TCP`.
Essa origem usa uma variante social privada distinta da autoridade manual. Uma
`SessionInvitation` isolada não autoriza abrir o jogo social: a conferência também
exige a mesma continuidade social, incluindo assinatura, grant/política, sequência,
deadline civil/monotônico e estado terminal originais.

O objeto `SocialContinuity` do convidado é movido integralmente para o guard
compartilhado depois do grant autenticado, antes de aguardar a entrega do convite,
sem reconstruí-lo a partir da lease mais recente
e sem refazer `start` com sequência arbitrária. O worker atualiza esse mesmo
objeto com as respostas assinadas existentes. O guard mantém o convite verificado
original e os limites originais da operação; nenhum update/poll/consulta reancora
o preparo, a sessão, o contexto ou o plano de abertura. Após admissão, expiração
do contexto ou do prazo de ingresso não encerra por si só a sessão válida.

O ator de transporte é o único dono forte de Guest/TcpBridge. A GuestSource
retém apenas referências fracas, inclusive ao guard social; bloqueio de preparo
ou spawn não conserva Endpoint. A falha/saída do worker de continuidade tem um
guard de conclusão: ele invalida a mesma autoridade sob o mesmo mutex,
independentemente do próximo poll do ator. A invalidação
terminal ocorre antes de aguardar teardown/recolhimento e não pode ser revertida
por uma resposta posterior. Stop/logout continuam cancelando a operação original
sob o mutex Connect antes de aguardar os trabalhos próprios.

A epoch original da reserva também é comparada ao estado atual no gate final.
Incrementar a epoch antes do cleanup já invalida o spawn, inclusive no intervalo
entre a invalidação da conta e o cancelamento do worker pela barreira existente.

## Gate de lançamento

Preparação, revisão, confirmação e backend continuam os existentes73. O guard
social faz apenas verificações limitadas em memória; nunca chama rede, consulta
processo, lê arquivo ou aguarda dentro do gate. A sequência de locks é a existente
Running → IPC → Connect, seguida pelo guard privado de continuidade. Updates e
invalidação do guard não mantêm esse guard ao tentar obter o mutex Connect.

No gate final, a conferência da autoridade social e a closure curta já existente
de spawn/registro são serializadas contra invalidação terminal pelo mesmo guard.
O caminho manual mantém sua autoridade anterior. Nenhum endereço TCP ou snapshot
do renderer constrói uma origem ou substitui o objeto original. Uma porta
reutilizada ou fonte de outra tentativa não satisfaz identidade de ponte/Attempt.

Este incremento não amplia o runtime73. O gate de arquivos/perfis/Java/Core/Skins
continua obrigatório; qualquer extensão de perfil, incluindo93 nesta wave, tem
contrato/testes separados. Selecionar sessão social não prepara nem lança Java
automaticamente. A interface reaproveita a ação explícita existente: selecionar
cópia pronta → revisar → consentir → iniciar e tentar entrar. Processo criado não
significa login, mundo, Core/Skins ou áudio homologados.

## Provas locais exigidas

Usar grant/lease/contexto reais do broker local Auth/Firestore e Host/Guest reais
quando exercitar a admissão. O backend de spawn de teste cria somente um filho
próprio do executável de teste; não simula o gate com endereço ou contador solto.
Casos: preparo/revisão sem spawn automático; confirmação única e continuidade
válida; expiração/terminalidade da lease impedindo o gate mesmo antes do poll do
ator; resposta assinada atrasada não reativa a origem; conta alterada/stop com
preparo ou gate retido; waiter perdido conserva worker/slot; sockets reais livres
antes de liberar trabalho retido; filho já registrado permanece vivo após stop.

Regressões73/79/89/90, default-off, build/format/Clippy e revisão independente são
necessárias. Os testes de fixture não substituem Minecraft real ou GUI. Registrar
comandos, falhas, contagens e limites somente após execução.

## Implementação e evidência parcial

`social_ready_game.rs` contém o guard privado. `social.rs` transfere a
continuidade original e publica a fonte apenas após obter o Guest/bridge reais;
`ready_game.rs` confere a epoch e mantém a guarda no gate final. Nenhum DTO de
entrada novo oferece porta, PID, grant ou autorização. A projeção opcional
`social.readyGameLaunchAvailable:true` só aparece com a fonte publicada e sai
antes do teardown. Clientes anteriores podem omitir o campo. ConnectView usa o
mesmo painel explícito73; disponibilidade retirada impede novos pedidos e ainda
permite observar/cancelar o preparo anteriormente observado.
Amigos e comunidade oferece `Abrir Minecraft` apenas ao convidado com a fonte
nativa publicada e sessão atual. A ação navega para o Connect existente; não
prepara nem confirma. O painel continua explicando contexto ausente ou cópia
incompatível antes de permitir uma revisão.

DOM/DTO executado com
`node --test apps/desktop/src/features/social/connect-controller.test.mjs apps/desktop/src/features/connect/ConnectView.smoke.mjs`:
**232/232 PASS**,3473ms, sem falhas/ignorados. Inclui fonte social ausente,
publicação positiva sem preparo automático, contexto do launcher encaminhado,
retirada da fonte e projeções contraditórias recusadas. São objetos sintéticos,
sem prova de transporte ou abertura de Minecraft.

Após ligar a navegação explícita, a rodada combinada
`node --test apps/desktop/src/features/social/SocialView.smoke.mjs apps/desktop/src/features/social/connect-controller.test.mjs apps/desktop/src/features/connect/ConnectView.smoke.mjs`
passou **275/275**,4224ms. Acrescenta ausência/publicação/retirada do botão,
navegação sem IPC de preparo/launch e expiração conferida no clique antes do
próximo timer. Essa contagem inclui a anterior; não somar os resultados.

O processo ignorado de integração usa `social_ready_game_fixture.rs`, backend
que passa pelo gate nativo real e cria somente um filho próprio do executável de
testes. O filho termina normalmente por EOF no cleanup do helper; encerrar
Connect preserva um filho já registrado. Gates limitados permitem reter o
preparo ou o instante anterior ao spawn. `readyStopDetached` perde o waiter
de stop real; `readyAdvanceEpoch` isola o passo inicial da transição de conta.
`epochRejectedBeforeSpawn` é um oracle `cfg(test)` marcado somente no ramo do
gate final que recusa epoch, depois dos checks de cancelamento, identidade e
prazo. Não integra a API de produto.

Integração real Auth/Firestore/broker executada pelo root com projeto
`demo-aurora-social`, configuração `firebase/social-emulators.json`,
`AURORA_SOCIAL_DESKTOP_INTEGRATION=1` e
`AURORA_SOCIAL_DESKTOP_TEST_BINARY` apontando ao helper local congelado:
`node --test --test-concurrency=1 firebase/tests/social-desktop.integration.test.mjs`.
Resultado **11/11 PASS** (dez casos e pai),22802ms, sem falhas/ignorados, em
`.aurora/connect-validation/resume92-social-desktop-integration-first.log`.
Essa rodada inclui89/90; não soma onze provas novas a92. Os quatro casos novos:

- Confirmação criou um filho nativo próprio; stop Connect preservou esse filho
  registrado e liberou a ponte para rebind.
- Alteração isolada de epoch observou `originalCancelled:false`, acionou o ramo
  exato `epochRejectedBeforeSpawn:true` e conservou zero spawns.
- Preparo retido perdeu o waiter real de stop; ponte foi liberada e nova reserva
  retornou `connect_busy` antes da liberação do trabalho original. Depois dele,
  cleanup completou sem spawn.
- Bloqueio real do convidado no Firestore retirou a fonte durante a espera
  anterior ao spawn; liberação posterior continuou com zero spawns e rebind.

Esses casos usam grant/continuidade/Host/Guest reais, mas filho de teste e fonte
echo próprios. Não provam Minecraft/Java, entrada no mundo ou áudio. A prova
isolada de expiração da lease original e de resposta assinada válida tardia
foi acrescentada na rodada final abaixo; não atribuir essa cobertura à retirada
por bloqueio da primeira rodada.

Para essa prova foi ligado um helper somente `cfg(test)` em
`social_ready_guard_fixture.rs`. Ele pausa assincronamente o ator depois da
publicação, antes de consumir novas leases, sem pausar o heartbeat original.
Conserva referências fracas a Guest/bridge e recebe o mesmo watch cujo escritor
verifica assinatura, grant, sequência e prazo antes de publicar. A pausa tem
limite10s e é liberada no cancelamento, quit, clear e Drop. A inspeção limitada a6s
aguarda a lease original vencer naturalmente, sem alterar relógio ou recriar
`SocialContinuity`. Exige grant/convite/Guest/bridge ainda válidos, token original
não cancelado e uma lease posterior validada pelo heartbeat ainda vigente;
então verifica que o guard não executa a closure e que update tardio não muda a
sequência nem retira o selo terminal. O helper não cria processo, não recebe
lease do RPC e não imprime seu conteúdo.

A rodada final com os mesmos flags e comando de integração acima passou
**12/12 PASS** (onze casos e pai),27995ms, sem falhas/ignorados, registrada em
`.aurora/connect-validation/resume92-social-desktop-integration-final.log`.
Helper congelado SHA-256:
`028fc068fc283d600c775a237fd3b6d13bf009a920d0c77a757f567f2823e603`.
Inclui a rodada anterior; não somar12+11. O quinto caso92 levou3475ms e confirmou
controle positivo inicial, expiração natural da continuidade original com
grant/convite/Guest/bridge ainda válidos e token não cancelado, lease posterior
validada e vigente, update recusado, sequência original preservada e selo
terminal. As duas closures posteriores à expiração tiveram zero chamadas.
Depois da liberação da mesma pausa, cleanup completou e a ponte aceitou rebind.
Não houve relógio alterado, autoridade reconstruída ou processo de jogo nesse
caso. Os emuladores foram encerrados de forma controlada pelo root.

GUI92 não foi executada nesta rodada. A interação de Computer Use foi
interrompida por Escape; não repetir nem atribuir a92 o ensaio gráfico anterior90.
DOM, build, integração nativa e GUI permanecem evidências distintas.

Rollback operacional: encerrar Connect e aguardar recursos próprios; o usuário
continua responsável por um jogo já iniciado. Sem push/deploy/instalador/billing,
mudança de matriz, configuração pessoal ou alteração de convite remoto.

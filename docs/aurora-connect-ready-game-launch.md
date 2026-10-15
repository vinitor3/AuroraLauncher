# Aurora Connect — entrada explícita usando uma cópia pronta

Estado: **PROTÓTIPO** do incremento73. Painel, caller nativo e controles de lançamento estão implementados, com testes locais e revisão independente registrados neste documento e no [relatório73](task-aur-r4-connect.md#incremento73--abertura-explícita-de-cópia-pronta-pela-ponte-do-guest). Não há nova homologação de Minecraft, GUI, rede externa ou voz.

## Escopo inicial

Uma ação separada abre uma **cópia diagnóstica pronta e verificada**, já instalada com Minecraft1.21.1, Fabric0.16.14, Java21, Core1.1.5 e Skins0.3.1, usando a ponte TCP própria do convidado Connect. A seleção é explícita, seguida de revisão e consentimento. Receber/confirmar convite, iniciar polling ou voltar à tela nunca inicia Java automaticamente.

O par já existe na matriz ativa; os outros oito pares permanecem disponíveis pelos fluxos atuais, sem nova declaração de compatibilidade ou remoção. Forge e instâncias de origem ficam fora desta primeira ação. As regras de [cópias diagnósticas](diagnostic-lab.md), [sessões independentes](engineering/ADR/ADR-023-independent-launcher-sessions.md), [Doctor](aurora-doctor.md), [Recuperação](instance-recovery.md) e [IPC Core](core-ipc.md) continuam aplicáveis.

Não há instalação, download, atualização de loader, substituição de Core/Skins, migração de Companion, alteração de preferências Java ou edição de `servers.dat`, opções, anúncio de voz ou configuração pessoal. Qualquer requisito ausente/divergente gera erro controlado orientando o fluxo normal separado. O caller não chama `App.launchMinecraft`, que contém reparação de Fabric, instalação de Java e persistência de preferência.

Iniciar um jogo gera os registros normais de execução/Doctor e logs na cópia. Depois do início, Minecraft/mods podem gravar seus arquivos normais nessa cópia e no cache de aparência do launcher. O caminho existente de Skins usa `data/skins/<uuid>`; a cópia não oferece um cache de aparência isolado. Isso não equivale a uma execução sem escrita ou a isolamento do sistema operacional. A origem não é executada nem modificada por esta ação; a reutilização do cache não homologa Skins em runtime.

## Base oficial e integração dos argumentos

QuickPlay é a interface oficial para solicitar entrada no servidor durante o início do cliente; informar o endereço não comprova que o mundo foi aberto. `quickPlayPath` controla um registro separado e não é necessário para esta ação. Os parâmetros antigos de servidor/porta não são o mecanismo escolhido. [Documentação oficial do Minecraft](https://www.minecraft.net/en-us/article/minecraft-snapshot-23w14a).

O [manifesto oficial1.21.1](https://piston-meta.mojang.com/v1/packages/499680d659b7a0ccb2fa9a809c345c5baf2ace7d/1.21.1.json) consultado na pesquisa da integração declara a feature `is_quick_play_multiplayer` e o argumento `--quickPlayMultiplayer`. O resolver atual descarta condições com feature true e remove placeholders não resolvidos. O helper separado recebe um destino tipado obtido da ponte e acrescenta o par de argumentos somente depois de verificar o perfil resolvido; não habilita features desconhecidas nem muda o resolver padrão.

O helper produz exatamente um par de argumentos de jogo: `--quickPlayMultiplayer` e o endereço `127.0.0.1:porta` obtido da ponte nativa. A confirmação não aceita esse endereço de volta, nem um vetor de argumentos. Outros modos QuickPlay, flags duplicadas/conflitantes e substituições incompletas são recusados. Sem shell, alteração do protocolo Core, mod novo, `--quickPlayPath` ou gravação de lista de servidores.

## Capability e entrada do caller

Campo opcional em `ConnectCapabilities`, somente no build experimental disponível:

```ts
readyGameLaunch?: {
  schemaVersion: 1;
  instanceScope: "diagnosticCopy";
  maxPlanSeconds: 60;
  minecraftVersion: "1.21.1";
  loader: "fabric";
  loaderVersion: "0.16.14";
  javaMajor: 21;
  coreVersion: "1.1.5";
  skinsVersion: "0.3.1";
};
```

Ausência mantém exatamente os fluxos anteriores e não oferece o botão. Ações novas exigem capability; observar/cancelar um trabalho já preparado não deve depender de uma consulta posterior de capability bem-sucedida. Nenhum campo entra no convite assinado, na capability remota ou no contrato TCP/UDP. O estado de execução usa um caller separado; `ConnectSnapshot` e os quatro perfis existentes conservam seu formato.

DTO de preparação, com `deny_unknown_fields` em todos os objetos:

```ts
type ReadyGameLaunchRequest = {
  operationId: string;
  instanceId: string; // somente aurora-test-<uuid> pronto, verificado nativamente
  sourceInstanceId: string; // origem selecionada; relação de proveniência verificada nativamente
  contextKey: string; // UUID efêmero da seleção/conta, sem dados privados embutidos
  javaExecutable: string; // Java existente, selecionado explicitamente
  nickname: string;
  appearance: {
    skinUrl: string | null;
    skinFile: string | null;
    capeUrl: string | null;
    skinModel: "classic" | "slim" | null;
  };
};
```

Não recebe `minecraftVersion`, `versionId`, loader, servidor, porta, URL de download, token, UUID de jogador ou argumento Java. O apelido passa pelo validador atual; UUID offline é derivado nativamente. A aparência usa exclusivamente os valores públicos e arquivo local já preparados pelo fluxo atual, com as mesmas allowlists de URLs/caminhos/modelos. Nenhuma preferência, configuração, identidade de Recovery ou arquivo de jogo é salvo durante a preparação. O mecanismo normal de exclusão mútua pode criar/abrir seu arquivo operacional de lock fora da cópia; isso não é uma operação de configuração. O nativo rejeita valores inválidos em vez de aprovar um valor e omiti-lo silenciosamente no lançamento.

Comandos próprios, todos proprietários da janela principal:

| Comando | Argumentos | Resultado |
| --- | --- | --- |
| `connect_prepare_ready_game_launch` | `{ request: ReadyGameLaunchRequest }` | snapshot inicial preparing; polling até reviewed |
| `connect_confirm_ready_game_launch` | `{ operationId, planId, digest }` | snapshot; nenhuma execução duplicada |
| `connect_cancel_ready_game_launch` | `{ operationId, planId }` | snapshot cancelado ou resultado já observado |
| `connect_ready_game_launch_snapshot` | `{ operationId }` | snapshot atual, sem ação de rede/launch |

`operationId` identifica a sessão Connect; `planId` é UUID de uma tentativa de lançamento, distinto da identidade de transporte. Há no máximo um trabalho preparando/revisado/iniciando por janela. O estado é separado no DTO, mas seu worker pertence ao mesmo Attempt Connect, para a barreira de stop/logout aguardar ambos. Perder uma resposta não recria plano nem reaplica confirmação automaticamente. A tentativa consumida conserva resultado/falha para consulta. Uma confirmação repetida do mesmo plano/digest pode devolver a observação já registrada, nunca iniciar outro processo. Um novo plano não reutiliza consentimento anterior.

## Projeção da revisão e estados

Projeção:

```ts
type ReadyGameLaunchSnapshot = {
  operationId: string;
  generation: number;
  planId: string | null;
  phase: "idle" | "preparing" | "reviewed" | "starting"
    | "started" | "failed" | "cancelled" | "expired";
  review: ReadyGameLaunchReview | null;
  result: ReadyGameLaunchResult | null;
  errorCode: string | null; // somente códigos da allowlist
};
type ReadyGameLaunchReview = {
  operationId: string;
  planId: string;
  digest: string;
  expiresAtMs: number;
  sessionExpiresAtMs: number;
  instanceId: string;
  sourceInstanceId: string;
  contextKey: string;
  minecraftVersion: "1.21.1";
  loader: "fabric";
  loaderVersion: "0.16.14";
  javaExecutable: string;
  javaMajor: 21;
  nickname: string;
  appearance: ReadyGameLaunchRequest["appearance"];
  gameAddress: string; // projeção loopback para revisão; não é autoridade de retorno
  coreVersion: "1.1.5";
  coreSha256: string;
  skinsVersion: "0.3.1";
  skinsSha256: string;
};
type ReadyGameLaunchResult = {
  runId: string;
  processId: number;
  instanceId: string;
  worldState: "notVerified";
};
```

Somente `reviewed` carrega `review`; somente `started` carrega `result`. Preparing/starting são trabalho ocupado; cancelled/expired não carregam revisão reaproveitável. `started` significa que o filho foi criado e registrado no acompanhamento existente, não que autenticou, entrou no mundo ou carregou Core/Skins. O resultado referencia o run do Doctor para consulta normal.

O digest, domínio novo e privado desse fluxo, vincula proprietário, operação e identidade original da ponte, plano, identidade física/proveniência da cópia e origem, chave efêmera do contexto, versão/perfis realmente lidos, fingerprints de arquivos relevantes, Java verificado, apelido/aparência normalizados e prazo. Não contém token nem transfere essas permissões a outra janela ou convidado. Preparação e revisão têm limites próprios de até60 segundos, ambos limitados pelo encerramento original da sessão Connect e por relógio monotônico além do civil. Polling, cancelamento, retomada de tela e repetição de IPC não renovam a autoridade.

## Pré-condições verificadas sem autocorreção

1. O slot Connect pertence à janela, é Guest autenticado com grant `game/TCP`, ponte local ainda própria e autoridade original válida. Estado de UDP/voz não autoriza ou bloqueia TCP por si só.
2. A cópia existe, tem proveniência diagnóstica íntegra e estado ready, identidade física consistente, nenhum jogo rastreado/órfão e nenhum journal pendente. Instância normal/origem, cópia incompleta/excluída/recriada e caminho com links/reparse são recusados. Não se cria uma identidade de recuperação para fazer uma cópia não verificada passar.
3. Versão e loader vêm dos metadados/perfis nativos confinados e herdados, não do nome da pasta ou declaração da UI. A cadeia deve resolver a Minecraft1.21.1 e Fabric0.16.14, sem ciclos, ambiguidades ou referências fora da cópia. Bibliotecas, cliente, assets/natives e arquivos necessários já existem; o launcher não tenta baixar faltantes.
4. Os JARs gerenciados Core/Skins presentes correspondem exatamente ao par atual: tamanho/hash e assinatura Core confiável, Skins igual aos bytes incorporados verificados para0.3.1. Ausência, bytes divergentes, duplicações conflitantes ou necessidade de migração Companion recusam esta ação. Preservar o scanner/allowlist de proveniência existentes quando reutilizáveis.
5. O Java escolhido existe, tem caminho/identidade estáveis e sua consulta limitada de versão confirma21. A preparação executa esse subprocesso de inspeção, sem iniciar Minecraft. A consulta `-version` tem timeout5s e saída limitada a16KiB em memória, sem arquivo temporário de saída; encerra/recolhe apenas seu próprio processo. Não consulta catálogos nem instala Java; o método antigo com `output()` ilimitado não basta como prova de cancelamento desse novo caller.
6. Preparação conserva somente fingerprints, hashes Core/Skins e dados do plano em RAM; libera os handles de arquivos e o bloqueio de mutação antes de esperar consentimento. Confirmação readquire o bloqueio, repete as verificações e mantém os handles verificados até criar o filho. Mudança da seleção, Java, perfis, Core/Skins, aparência local ou identidade invalida o plano; não produz um diff novo implicitamente aceito.

Fingerprints do cliente, bibliotecas e natives conferem que os arquivos preparados continuam iguais no momento da confirmação. Essa continuidade não é uma afirmação de autenticidade upstream de todo arquivo instalado. Core usa assinatura confiável e Skins usa os bytes incorporados fixados; os assets são conferidos por SHA-1 contra o índice instalado. A verificação de versão executa o Java selecionado: ela não fornece sandbox nem atesta o fornecedor desse executável.

O leitor tem limites de32.768 handles/arquivos,4GiB acumulados,30.000 objetos de assets e256 arquivos de natives/mods por diretório, além dos tetos por tipo. Falta de recurso ou limite excedido recusa a ação, sem baixar ou truncar. Uma última comparação dos nomes de mods/natives e do marcador de exclusão precede o gate. Ela detecta acréscimos posteriores ao hash, mas não transforma a cópia em sandbox contra alterações por outros processos após essa comparação. A prova de árvore intacta nos testes começa após adquirir o lock normal; seus bytes bloqueados pelo Windows são excluídos da leitura, e nomes/tamanhos são comparados. Recovery, configurações e demais arquivos continuam comparados integralmente.

A criação normal de cópia exclui módulos gerenciados: uma cópia nova sem Core/Skins não passa. O usuário deve prepará-la pelo fluxo normal já existente e fechá-la antes desta revisão; o botão novo não usa essa pendência como autorização para instalar os JARs.

## Fronteira interna e encerramento

`ConnectSnapshot.tcpAddress` é observação, não permissão para lançar. O worker Guest deve publicar uma referência opaca verificável à sua ponte real, incluindo identidade da operação, conexão autenticada, original `Authority` e cancelamento. Nenhum construtor público de permit aceita endereço recebido da UI. O permit não prolonga clocks, admissão ou sessão e não autoriza um socket substituto na mesma porta.

O preparo de argumentos/instância ocorre sem segurar o mutex Connect. Imediatamente antes de `spawn`, um helper interno curto revalida a ponte, a autoridade original, o cancelamento, a conexão e o mesmo plano; o gate se lineariza com stop/logout. Não segurar o mutex Connect durante leitura/hash/probe Java ou espera de I/O. Se stop vence antes do spawn, nenhum filho é iniciado. Se spawn vence, registrar o filho no acompanhamento normal antes de publicar sucesso; parar Connect depois não mata Java.

O caller reutiliza `RunningInstances`, mutation guard, detecção Windows de JVM órfã, Doctor e `IpcSessions`, com nonce por jogo e aparência existente. O caminho separado de verificação pronta não expõe `LaunchCommand` pela IPC. Ele chama helpers de verificação, nunca `install_embedded_*`, `migrate_managed_companion`, install/download de loader/Java ou gravação de preferência. O caminho normal preserva seu comportamento atual.

O Java selecionado nesta primeira via deve ser `java.exe`; `javaw.exe` não é substituído implicitamente. O probe mantém stdout/stderr em memória e consulta a disponibilidade dos pipes próprios com `PeekNamedPipe`, sem reader destacado nem espera pelo EOF de terceiros. Referência: [API Windows](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-peeknamedpipe). O prazo limita observação; encerramento/recolhimento do filho próprio continuam aguardados, sem promessa de tempo real do sistema operacional.

Confirmação consome o plano antes do executor. Cancelar/fechar durante preparing/starting sinaliza cancelamento e espera o worker alcançar um resultado registrado. Se Java ainda não nasceu, não nasce depois desse fechamento. Se já nasceu, a resposta preserva run/processo observados e a interface informa que o jogo continua sob controle do usuário. Logout/troca de conta invalida a revisão antes da transição; stop/saída Connect também invalida permits ainda pendentes. Não depender apenas de unmount React para cancelar um executor nativo.

A conexão pode cair ou vencer enquanto Minecraft inicia. Não renovar o convite, não trocar porta e não alegar ingresso garantido. Permissão válida no gate de spawn não demonstra que a rede permanecerá disponível até o cliente conectar; a prova final exige teste de jogo real. A identidade de jogo atual continua offline, sem implementar autenticação Microsoft, autenticação mista ou alterar `online-mode` do servidor.

## Interface

O módulo `features/connect-game-launch` é importado por `ConnectView`. O painel recebe contexto explícito da tela já autenticada: instância de origem selecionada, Java já escolhido, apelido/aparência prontos e uma chave efêmera da seleção/conta. Não descobre credenciais, não usa o Provider de voz como autenticação e não chama o lançamento automático de instalação. App gera uma nova chave opaca quando esses valores ou a conta mudam, sem incorporá-los no identificador.

Depois da primeira visita, App conserva ConnectView montado e oculta/inativa sua seção ao navegar. Assim o contexto continua observado quando Java, instância ou aparência são alterados em outra tela. Isso evita deixar um preparo antigo sem sinal de cancelamento durante a navegação. A barreira nativa continua responsável pelo resultado; unmount, cancelamento solicitado ou ocultar a tela não provam ausência de processo. Saída/troca de conta passa pelo encerramento Connect já aguardado.

O observador de autenticação também passa por um controlador permanente em App. A primeira observação atravessa a barreira de cleanup, inclusive após recarregar a WebView: o manager nativo pode conservar trabalho da apresentação anterior. Depois de uma conta pronta, estado loading/error/signed-out ou outra UID invalida a apresentação imediatamente e aguarda consulta/stop Connect. Eventos seguintes são acumulados e somente o mais recente pode ser publicado após cleanup completo; falha mantém a tela bloqueada até tentativa explícita. Recriar o observador não descarta esse trabalho. O encerramento consulta o snapshot mesmo sem capability disponível. O Provider de voz permanece montado, sem apagar recibos/restaurar configurações automaticamente. Isso não torna callback JavaScript e spawn nativo uma operação atômica: vale a ordem observada pelo gate nativo, e jogo já registrado permanece sob controle do usuário.

Um seletor reaproveita `diagnostic_lab_list({id: sourceInstanceId})`, mostrando somente cópias ready. A escolha é independente da identidade do convidado e revalidada nativamente. Não criar cópia ou instalar arquivos ao selecionar. Se não houver cópia compatível/pronta, informar a pendência e orientar a seção de cópias existente.

Fluxo: **Selecionar cópia → Revisar entrada no servidor → revisar destino/versões/Java/apelido/aparência/prazos → checkbox → Iniciar cópia e tentar entrar**. Novas ações exigem capability, Guest TCP autenticado, contexto pronto e observações atuais. Falha de UDP mantém a escolha TCP; nenhuma frase promete áudio. Edição de contexto, operação, revisão/digest, encerramento ou expiração limpa consentimento. A cópia permanece fixa durante o preparo; cancelar permite selecionar outra. Resposta atrasada não restaura outra operação ou copia destino antigo. Após IPC perdido, consulta-se snapshot, sem confirmar novamente automaticamente; observação antiga/inalterada não libera a barreira de resultado desconhecido.

O parser recusa campos e fases contraditórios, capacidades alteradas, destinatário diferente da operação, revisão com outra cópia selecionada, URLs/paths como endereço da ponte, estado de mundo diferente de `notVerified` e erros fora da allowlist. Capability omitida em backend antigo não adiciona chamadas ao caller novo. Falha de capability ao recuperar trabalho existente conserva consulta e cancelamento, sem habilitar nova confirmação. Contexto alterado solicita um cancelamento por plano; falha mantém a ação manual e não cria repetição automática sem fim.

Mensagens de erro devem distinguir: cópia não pronta/proveniência inválida, perfil incompatível, arquivos/Core/Skins ausentes ou divergentes, Java não disponível/incompatível, instância ocupada, journal pendente, revisão alterada/vencida, Connect encerrado e falha de criação do processo. Somente texto fixo controlado chega à UI; não exibir argv, nonce, stack trace ou linhas de saída arbitrárias do Java.

## Arquivos, provas locais e gates externos

- UI: `features/connect-game-launch/{types,ConnectGameLaunchPanel,ReadyCopyPicker}.tsx/ts` e smoke DOM. Ligação em ConnectView/App somente para dados atuais, sem migrar seu monolito ou chamar auto-instalação.
- Nativo novo: módulo de plano/estado + caller estreito; permit no live Connect; helper de argv/validação instalada; pequenas extrações em commands/Core/Skins/launch para compartilhar o pipeline sob política de verificação sem escrita.
- Testes puros: perfil herdado e versão derivada, placeholders/flags conflitantes, argv tipado exato, nenhum fallback; hashes/JARs/Core signature, cópia/proveniência/caminhos alterados, Java errado/faltante.
- Testes nativos: Host/Guest reais loopback, permit não construtível por endereço, revogação/stop/expiry antes do gate impedem criação; processo fixture controlado para comprovar o registro do actor, coleta do filho e preservação do transporte; concorrência, resposta IPC perdida e cancelamento com coleta do worker. Os helpers de argumentos e verificação instalada têm provas separadas. O caminho completo Desktop/AppHandle/Doctor com jogo real ainda exige validação; a fixture do actor não comprova essa integração. Nenhum Java/Minecraft pessoal para esse foco.
- DOM: seleção de cópia ready e contexto explícito, revisão/digest, expiração no clique, troca de conta/seleção/operação, nenhuma chamada automática, lost IPC e remount, cleanup sem capability e estado iniciado sem `worldJoined`.
- Homologação pendente: cliente Minecraft real1.21.1/Fabric0.16.14 pela ponte e observação de entrada/encerramento. Testes sintéticos, argumentos corretos ou processo criado não concluem isso, nem os testes separados dos dois mods de voz e contas mistas.

## Evidências locais da interface — 15/10/2026

Executados a partir de `apps/desktop`:

```text
node --test src/features/connect-game-launch/ReadyGameLaunch.smoke.mjs src/features/connect/ConnectView.smoke.mjs src/features/connect/fragmented-types.test.mjs
npm run build
```

- Suite final desta revisão UI: **146 PASS, 0 falhas, 0 ignorados**, duração2317.4455ms;27 casos do painel novo/seletor/contratos,108 de ConnectView e11 do contrato UDP anterior. Log local ignorado: `.aurora/connect-validation/ready-game-73-ui-third.log`.
- Build TypeScript/Vite: **PASS**,369 módulos. Log: `.aurora/connect-validation/ready-game-73-frontend-build-second.log`. Permanece o aviso preexistente de chunk maior que500kB. Esse build inclui a ligação App que conserva a tela montada/oculta depois da primeira visita.
- Primeira rodada isolada:24/24 novos e106/106 de ConnectView passaram. O primeiro build encontrou erros de tipagem em argumentos IPC/narrowing do parser; corrigidos antes dos builds verdes. Essas rodadas são anteriores e não se somam ao total146.
- Os casos usam DOM/IPC sintéticos, sem Java, jogo, conta real, transporte externo ou escrita em configuração pessoal. Exercitam preparo explícito, consentimento novo, expiração no clique, perda de resposta, retorno antigo, remount, mudança de conta/seleção durante preparo pendente, cancelamento concorrente a resultado iniciado, capability indisponível, erro redigido, flags inconsistentes e preservação de TCP quando UDP falha.
- Revisão independente encontrou que desmontar ConnectView ao navegar deixava o contexto sem observação. A ligação App foi corrigida para manter o painel montado e oculto/inativo, com contexto atual; saída/troca de conta conserva a barreira nativa. Validação gráfica manual e lançamento real permanecem pendentes.
- Ajuste posterior somente no texto do consentimento tornou explícito o cache de aparência compartilhado. O foco `node --test src/features/connect-game-launch/ReadyGameLaunch.smoke.mjs` passou novamente em27/27 casos,829.9623ms, sem alterar o total de cobertura146. Log: `.aurora/connect-validation/ready-game-73-ui-consent.log`.
- Gates e testes nativos são registrados separadamente no relatório do incremento. Esta evidência frontend não comprova que o processo entrou no mundo ou que o áudio funcionou.

Após a correção da invalidação espontânea de conta, os mesmos146 casos passaram em3253.0582ms (`ready-73-frontend-account.log`). O helper `node --test src/features/connect/account-transition.smoke.mjs` passou separadamente em **13/13**,597.2251ms (`ready-73-account-final.log`): transições, coalescência, resposta perdida, bloqueio e retry, sem depender de capability. A primeira chamada agregada indicava por engano o sufixo `.test.mjs` inexistente para esse helper; Node executou somente os146 arquivos/casos existentes, por isso os13 foram executados corretamente e registrados em separado. Não somar reruns da mesma cobertura. Build após ligação App: **PASS**,370 módulos, aviso anterior de chunk>500kB (`ready-73-frontend-build-account.log`). São provas sintéticas de estado/DTO/DOM, sem autenticação Firebase real.

Validação final após a barreira inicial: `node --test src/features/connect-game-launch/ReadyGameLaunch.smoke.mjs src/features/connect/ConnectView.smoke.mjs src/features/connect/fragmented-types.test.mjs src/features/connect/account-transition.smoke.mjs` passou **162 casos, zero falhas/ignorados**,4260.5452ms. São146 casos anteriores e16 do helper; as rodadas anteriores não são somadas. `npm run build` passou com370 módulos,979ms na etapa Vite. Logs `ready-73-frontend-final.log` e `ready-73-frontend-build-final.log`. Revisão independente do controlador e ligação App não encontrou bloqueador.

Rollback: cancelar plano e aguardar trabalho pendente, encerrar Connect normalmente e controlar Minecraft pelo usuário se já iniciado. Remover/ocultar capability impede novos lançamentos; não desfazer logs/Doctor nem matar processos. Nenhuma configuração pessoal precisa de restauração porque a integração não a edita; não reaproveitar essa frase para afirmar que o jogo nunca grava na cópia.

## Validação nativa final

Regressão integrada73: **448 PASS/0 falhas/63 ignorados** com todas as features/targets; padrão: **288 PASS/0 falhas/5 ignorados**. Clippy em ambas as combinações, formatação dos dois crates e diff passaram. Comandos exatos, hashes dos executáveis, opt-ins, falhas encontradas e corrigidas estão no [relatório73](task-aur-r4-connect.md#incremento73--abertura-explícita-de-cópia-pronta-pela-ponte-do-guest). A biblioteca de transporte também passou247 testes/7 ignorados. Não somar focos e regressões sobrepostos. Os processos executados foram fixtures próprias; não houve Java, Minecraft, autenticação Firebase real ou áudio nesta validação. A integração completa Desktop/AppHandle/Doctor com o cliente gráfico permanece **PRECISA DE TESTE**.

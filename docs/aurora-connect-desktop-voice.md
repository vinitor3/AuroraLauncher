# Aurora Connect — anúncio de voz no desktop

## Estado e escopo — incremento 60

**PROTÓTIPO.** O caller, o painel e o diálogo global estão implementados sob o contrato abaixo. A validação de interface usa fixtures DOM/IPC; os testes nativos de escrita/restauração usam cópias próprias e artefatos locais fixados. Compilação e testes de apresentação não homologam janela Tauri, transporte ou áudio. Resultados e limites de cada camada estão registrados separadamente abaixo.

O menor ciclo completo é: selecionar uma cópia diagnóstica pronta e um mod → inspecionar → revisar aplicação → confirmar → conservar recibo somente em RAM → revisar restauração → confirmar restauração. A via estrita e a via reconciliada explícita são ações separadas. A feature `connect-voice-lab` permanece desligada por padrão. O perfil de transporte57 e sua capability não habilitam este painel por consequência.

Antecedentes obrigatórios: [voz](aurora-connect-voice.md), [inspeção59](aurora-connect-voice-inspection.md), [controller58](aurora-connect-voice-restoration-controller.md), [desktop57](aurora-connect-desktop-udp.md), [Recuperação](instance-recovery.md), [cópias de diagnóstico](diagnostic-lab.md), [ADR-027](engineering/ADR/ADR-027-manual-instance-recovery.md) e [ADR-028](engineering/ADR/ADR-028-doctor-recovery-lab.md).

Este ciclo altera somente o anúncio na cópia selecionada. Não instala mod, inicia Java, escolhe endpoint UDP, abre transporte, ativa áudio, observa microfone ou aplica mudanças à origem. Não altera Assistente, nove pares Core/Skins, matriz, convites, prazos de rede ou política de custos. Simple Voice Chat e Plasmo Voice continuam com evidências e homologação separadas.

## Autoridade e seleção

Há um `VoiceAnnouncementManager` nativo por processo, com um único slot ativo. O proprietário deriva de `WebviewWindow.label() == "main"`; nenhum argumento pode escolher outra janela. Essa propriedade local não equivale a autenticação Microsoft/Firebase ou identidade de jogador.

O wrapper resolve o diretório conhecido `app.path().app_data_dir()` e abre `InstanceLayout::new(data_dir).open(InstanceId::parse(instanceId))`. Recebe apenas `InstanceId`, nunca caminho absoluto, raiz, PID, hash de identidade física, configuração completa ou guard. O formato de ID mantém os limites existentes: 1–64 caracteres ASCII alfanuméricos, `-`, `_` ou `.`, recusando `.` e `..`. A camada59/58 continua verificando identidade física e proveniência; o nome da pasta não é autoridade.

O painel vive em **Ambiente de teste**, associado ao `currentDetails.environment.id` escolhido explicitamente. A lista e seu estado `ready` são apresentação; o backend reconfere os gates reais. O mod é um enum fechado `simple_voice_chat | plasmo_voice`, traduzido ao enum existente, sem inferência pela porta, nome de arquivo ou perfil UDP. O slot conserva cópia/mod/operação; ações posteriores usam seu identificador opaco. Trocar seleção ou desmontar o painel não move nem descarta o recibo.

O backend fixa `DedicatedAuroraOnly` e porta convidada35554. O resultado previsto é `127.0.0.1:35554`; não há campo editável de porta/modo/destino na solicitação de aplicação. Isso é endereço anunciado aos participantes, inclusive ao cliente do próprio anfitrião, e não configuração do bind do servidor. Cada participante ainda precisa da ponte local apropriada. O transporte57 não prova que essa configuração pertence a um mod carregado.

## Contrato IPC

Todos os pedidos estruturados recusam campos desconhecidos. IDs de operação/plano/fechamento são UUIDs gerados no nativo, comparados ao slot dono; digests são SHA-256 hex de64 caracteres. Os valores do frontend não reconstruem planos ou recibos. A configuração selecionada nunca entra em eventos gerais, logs, diagnóstico compartilhado ou mensagens remotas.

Prefixo dos comandos: `voice_announcement_`. As respostas de operação são `VoiceAnnouncementSnapshot`; erros de protocolo/ownership usam somente códigos estáticos. Resultados de trabalho de arquivo, inclusive falhas, são incorporados ao snapshot antes de retornar ao caller.

`capabilities` e `snapshot` não recebem argumentos JavaScript. Todos os outros wrappers recebem **um único objeto `request`**, com `deny_unknown_fields` no DTO Rust. Exemplo: `invoke("voice_announcement_prepare_apply", { request: { operationId, generation } })`. A tabela lista os campos internos de `request`, não argumentos soltos. Janela, manager e AppHandle são fornecidos pelo Tauri. Após erro de comando, o frontend consulta snapshot antes de voltar a oferecer confirmação, pois uma tentativa pode ter consumido a revisão mesmo tendo sido recusada.

| Comando | Argumentos | Ação |
| --- | --- | --- |
| `capabilities` | nenhum | Contrato de disponibilidade; sem I/O de instância. |
| `snapshot` | nenhum | Recupera o slot da janela dona; sem renovar revisão ou ler arquivos. |
| `inspect` | `instanceId`, `voiceMod` | Cria/reutiliza seleção sem recibo pendente; executa59 com guard real. Nova operação tem ID novo. |
| `prepare_apply` | `operationId`, `generation` | Usa somente o hash da observação retida no slot; prepara plano original com modo/porta fixos, revalidando os gates nativos. Não recebe hash arbitrário da interface. |
| `confirm` | `operationId`, `planId`, `digest` | Consome a revisão atual de aplicação ou restauração; não recebe action, bytes, mod, instância ou substituição. |
| `prepare_restore` | `operationId` | Usa somente `controller.prepare_strict`, independentemente do JAR ainda existir. |
| `prepare_restore_reconciled` | `operationId` | Ação explícita distinta: obtém hash atual por leitura segura vinculada ao slot e chama `controller.prepare_reconciled`; jamais é fallback. |
| `cancel_review` | `operationId` | Descarta somente revisão não executada. Preserva recibo e estado de recuperação. Não cancela uma transação já iniciada. |
| `begin_close` | `operationId` ou `null` quando o slot está vazio, `intent: panel \| logout` | Fecha admissão de novas operações, aguarda trabalho em curso e cancela revisão não executada; pode exigir revisão explícita de saída. A intenção `exit` é exclusivamente nativa. |
| `cancel_close` | `closeId` | Revoga fechamento `waiting`/`reviewRequired` e sua revisão; reabre ações no estado estável, conservando recibo. Recusa qualquer fechamento `ready`. |
| `confirm_close` | `closeId`, `digest` | Consome a revisão de perda de autoridade RAM. Não restaura nem escreve arquivos. |
| `finish_close` | `closeId` | Libera a barreira de logout/painel depois da conclusão da ação exterior. Não substitui consentimento nem libera intenção nativa de exit. |

O DTO de apresentação usa o enum fechado `panel | logout | exit`; pedidos JavaScript de `begin_close` aceitam somente `panel | logout`. O nativo cria a intenção `exit` diretamente no evento do aplicativo, usando o slot atual sob lock; não depende de um `operationId` lido antes pelo JavaScript. Uma intenção concorrente distinta recebe `voice_close_pending`, sem sobrescrever a primeira. Repetir a mesma intenção em curso recupera o mesmo fechamento, sem novo prazo ou revisão.

O build padrão registra capability indisponível e wrappers de recusa estática, sem compilar/instanciar autoridade dos adaptadores. `available=true` requer Windows e `connect-voice-lab`; não requer nem implica `connect-lab`. O fechamento do app deve tratar build somente voz, somente transporte e ambos.

### Capability e apresentação

```ts
type VoiceMod = "simple_voice_chat" | "plasmo_voice";
type VoiceAnnouncementCapabilities = {
  available: boolean;
  experimental: true;
  diagnosticCopyOnly: true;
  mode: "dedicatedAuroraOnly";
  guestAddress: "127.0.0.1:35554";
  mods: VoiceMod[]; // os dois quando disponível; [] quando indisponível
  restoration: "strictAndExplicitReconciled";
  audioState: "notVerified";
  transportState: "notObserved";
};
```

`AnnouncementInspection` conserva o contrato59: versão/pin/tamanho esperados, caminho relativo fixo, hash/tamanho observado, campo configurado e limitações estáticas. SVC vazio, ausência de `host.public` e porta0 do Plasmo são estados distintos e não resolvidos. `ip` vazio do Plasmo continua recusado pela gramática. Não se apresenta um desses valores como bind ou rota efetiva.

A revisão do painel é uma projeção fechada de `AnnouncementReview`, com `action: "setAnnouncement" | "restoreOriginal" | "restoreReconciled"`, `voiceMod`, `planId`, `digest`, `configPath`, `before`, `after`, `sourceSha256`, `replacementSha256`, `expiresAtMs` e `warning`. Não expõe `receipt_id` interno. O digest permanece o produzido sobre o plano nativo original, incluindo identidade/fingerprint privados; o caller nunca o recalcula a partir do DTO. Strings de anúncio são limitadas a2048 bytes em `before`/`after` e1024 bytes em `warning`, apresentadas como texto e nunca HTML; recusa-se projeção excessiva em vez de truncar diff.

O frontend mostra cópia, mod/versão/pin, caminho relativo, antes/depois, hash e prazo; confirmação começa desmarcada e exige ação explícita. Alteração de ID/digest/contexto, expiração, cancelamento ou remount limpa o consentimento. Atualizar snapshot não recria prazo: aplicação e restauração usam os300s monotônicos originais do adaptador. O relógio civil serve somente à apresentação.

### Snapshot e estados

```ts
type VoiceAnnouncementPhase =
  | "idle" | "inspecting" | "observed" | "preparingApply" | "reviewApply"
  | "applying" | "applied" | "preparingRestore" | "reviewRestore"
  | "restoring" | "restored" | "needsRecovery" | "failed"
  | "closing" | "closeReview" | "closed";
type VoiceAnnouncementSnapshot = {
  generation: number;
  operationId: string | null;
  instanceId: string | null;
  voiceMod: VoiceMod | null;
  phase: VoiceAnnouncementPhase;
  busy: boolean;
  observation: AnnouncementInspection | null;
  review: VoiceAnnouncementReview | null;
  restorationState: "ready" | "needs_recovery" | "restored" | null;
  lastRecoveryResult: {
    restoredPointId: string;
    undoAvailable: boolean;
    recoveredInterrupted: boolean;
  } | null;
  lastErrorCode: VoiceAnnouncementErrorCode | null;
  close: VoiceAnnouncementClose | null;
};
```

`generation` é inteiro seguro monotônico por mudança de estado; respostas antigas não substituem snapshot novo. ID/cópia/mod aparecem juntos no slot e não mudam enquanto ele existir. `idle` não tem seleção, revisão, recibo ou fechamento. `closed` pode conservar metadados limitados do último contexto para informar o resultado; não conserva autoridade descartada. Parsers frontend recusam chaves extras, IDs/campos fora dos limites, perfil de mod divergente entre revisão/observação/slot, capability contraditória, deadline não seguro e combinações impossíveis de fase/autoridade. Não usam cast TypeScript como validação.

Somente as fases de trabalho de arquivo e `closing` têm `busy=true`. `reviewApply` exige revisão de aplicação; `reviewRestore` exige revisão de restauração e controller `ready`. Revisões não existem após confirmação/cancelamento/fechamento. Uma observação é uma leitura datada, não monitoramento; é removida após confirmação de aplicação e quando uma falha indica contexto/configuração obsoletos.

`applied` significa somente que a transação de anúncio retornou sucesso. Seu controller está `ready`; não significa servidor ou áudio disponível. `restored` conserva `restorationState=restored` até liberar o slot: na via estrita o arquivo anterior foi restaurado; na reconciliada somente as chaves próprias foram restauradas, preservando os demais bytes atuais. Uma recusa antes do executor na restauração consome revisão mas retorna a `applied` com controller `ready` e erro estático; erro posterior conserva `needs_recovery`.

Aplicação e restauração precisam distinguir validação prévia de início do executor. Não classificar qualquer erro como escrita parcial. Se a aplicação falhar antes do executor, consumir revisão e exigir nova inspeção/preparo, sem criar recibo. Se falhar após entrar no executor, `needsRecovery` pode existir sem recibo utilizável; o journal/recovery é a via segura. O manager real é filho privado de `connect_voice` e reutiliza `apply`/`on_executor_begin` sem callback/probe/construtor público que permita ao caller fabricar autoridade. A fachada sempre compilada fornece o contrato indisponível sem a feature. Se a aplicação retornar sucesso, instalar controller e resultado no manager **antes** de concluir o worker, inclusive quando a interface já pediu fechamento.

`lastRecoveryResult.restoredPointId` é o ponto aplicado segundo o executor, não um ID prometido do backup original. **Abrir Recuperação** navega para a cópia, sem selecionar/restaurar esse ponto automaticamente. `undoAvailable` registra o resultado daquela operação; não garante que o estado ainda é o mesmo após outra ação externa.

## Trabalho aguardado, cancelamento e concorrência

O manager mantém um slot exclusivo e autoridade opaca; snapshots são clones de apresentação, sem `PreparedAnnouncement`, controller ou guard serializados. Cada trabalho síncrono roda em `spawn_blocking`, recebe posse temporária do recurso necessário e publica o resultado no slot. O mutex de snapshot não fica retido durante inventários/CAS; chamadas concorrentes recebem estado/erro definido, sem segundo writer.

O trabalho adquire `VoiceMutationGuard` novo dentro do worker. Reutiliza os gates59/58 e solta o guard ao terminar, antes de qualquer execução posterior de jogo. Não chama `run_recovery` ou `run_diagnostic_lab`: esses wrappers fazem `recover_pending` automaticamente e não representam a consulta59 somente leitura. Journal pendente leva a erro/Recuperação separado, sem reparo implícito pela inspeção.

Cancelar ou sair não usa `abort` para fingir que o blocking terminou. A barreira bloqueia novas solicitações, aguarda o worker já iniciado, incorpora seu resultado e só então decide o próximo estado. Se a transação terminou enquanto se aguardava cancelamento, apresentar `applied`/`restored`/`needsRecovery` real; nunca "cancelado, nada mudou". Não renovar aprovação, restaurar automaticamente ou derrubar TCP quando UDP falha.

Navegação/remount conserva o slot nativo. O painel utiliza geração, ID e contexto de seleção para ignorar respostas tardias, mas continua oferecendo acesso ao slot anterior quando sua cópia difere da seleção atual. Uma segunda cópia/mod não toma o recibo anterior. Iniciar, excluir ou editar a cópia fica bloqueado na interface durante I/O; o lock nativo continua sendo a proteção de autoridade, inclusive contra outros fluxos.

### Restauração reconciliada explicitamente selecionada

O botão **Revisar restauração preservando outras edições** é uma ação distinta da restauração estrita. Em controller `ready`, o manager usa a instância e o mod imutáveis do recibo/slot, adquire guard novo e obtém o hash atual do caminho fixo por `with_existing_observation`. Esse reader não exige presença do JAR; não chama `inspect_announcement`59. O manager extrai somente a entrada inventariada do arquivo selecionado, sem enviar inventário/bytes/hash arbitrário pela interface.

O callback de leitura termina e libera seu handle SQLite somente leitura **antes** de abrir o executor/preparador seguinte; o `VoiceMutationGuard` permanece o mesmo. Então chama `controller.prepare_reconciled(instance, guard, hash_observado)`. A API58 reconfere cópia/UUID/runtime/hash, compara os campos atuais com os valores aplicados e prepara revisão nova. Mudança entre as leituras ou edição de uma chave própria é recusa; não adivinhar valor, aplicar restauração integral ou escolher outro arquivo. O original e os bytes aplicados continuam vindo apenas do recibo.

O resultado é `reviewRestore` com action `restoreReconciled`, digest/prazo novos e explicação de que outros bytes atuais serão preservados. Exige confirmação separada. Falha estrita não chama esse comando, marca checkbox, copia digest ou cria revisão automaticamente. Remover/desabilitar o JAR não impede essa restauração se os gates próprios do recibo continuarem válidos. Estado `needsRecovery` não permite nenhuma das duas preparações; abrir a Recuperação existente.

## Saída, logout e perda do recibo RAM

```ts
type VoiceAnnouncementClose = {
  closeId: string;
  intent: "panel" | "logout" | "exit";
  state: "waiting" | "reviewRequired" | "ready";
  review: {
    digest: string;
    expiresAtMs: number;
    instanceId: string;
    voiceMod: VoiceMod;
    state: "applied" | "needsRecovery";
    selectiveRestorationLost: true;
  } | null;
};
```

`begin_close` instala a barreira antes de aguardar I/O. Ao ficar estável, cancela qualquer revisão de arquivo não executada. Se nada foi aplicado, ou se a restauração já terminou, fecha o slot sem escrita e retorna `ready`. Se existe anúncio aplicado/estado de recuperação, produz `reviewRequired` com nonce interno, digest vinculado a dono/operação/geração/intenção/cópia/mod/estado/último resultado de recuperação e prazo de60s civil e monotônico. A revisão só é criada **depois** do quiesce. Repetir a consulta não renova esse prazo. Confirmar não cria nova vigência nem outro consentimento. Nenhum endereço configurado ou recibo entra no evento de atenção.

A revisão explica: a cópia continuará alterada; não há restauração automática; confirmar elimina a restauração seletiva disponível somente neste processo; o mecanismo durável de Recuperação cobre seu próprio escopo e transações, não recria o recibo. Não se promete que o hash integral original será recuperado automaticamente depois de reiniciar.

Opções apresentadas: **Continuar e revisar restauração**, **Abrir Recuperação** e **Sair mantendo alteração na cópia**. As duas primeiras chamam `cancel_close` e conservam a autoridade antes de navegar. A última exige confirmação explícita do digest vigente. Uma tentativa no `closeId` atual consome a revisão de fechamento, inclusive digest incorreto/vencido; não descarta o controller em falha e exige cancelar/pedir nova revisão. ID de outro fechamento nunca consome a autoridade atual. Cancelar o diálogo libera a barreira e deixa restauração utilizável, sem reativar revisão de arquivo cancelada. Se o cancelamento chegar durante `waiting`, aguarda o mesmo worker antes de publicar o estado estável; não solta a barreira com I/O pendente.

Depois de consumir uma revisão de saída recusada, `close.state=reviewRequired`, `close.review=null` e `lastErrorCode=voice_stale_close` indicam que o recibo continua protegido, mas aquela aprovação não pode ser repetida. Falha de relógio/limite ao gerar a revisão usa essa mesma projeção, conservando o recibo e a possibilidade de cancelamento. A interface oferece cancelar e iniciar nova revisão; não fabrica digest nem renova por polling. `waiting` e `ready` também têm `review=null`.

No logout, `App.tsx` começa esse fechamento antes de chamar o logout Firebase. Enquanto `reviewRequired`, mantém conta e app abertos. Após `ready`, aguarda cleanup Connect existente e então executa logout; em sucesso ou falha finaliza o ciclo com `finish_close(closeId)`. A barreira persiste durante essa ação exterior. Falha do logout não recupera um recibo cujo descarte foi explicitamente aprovado e não reabre revisão antiga; a interface informa o resultado e permite nova ação.

Se uma resposta IPC de confirmação se perder depois de o nativo aprovar o logout, o caller reconcilia somente um snapshot válido de geração posterior, com o mesmo `closeId`, intenção `logout` e estado `ready`; segue a ação exterior sem repetir confirmação, e continua aguardando seu término antes de `finish_close`. Se a resposta de cancelamento se perder, exige snapshot posterior da mesma operação, sem fechamento nem trabalho ocupado, antes de encerrar o pedido de logout sem executar a ação da conta. Nos dois casos confere também cópia/mod e espera as chamadas em andamento terminarem. A intenção pendente é registrada antes do comando e reconciliada uma única vez, inclusive quando as leituras imediatas falham e o primeiro snapshot válido chega por polling. Um erro sozinho não comprova nenhuma dessas transições.

Depois de remount ou abandono da ação exterior, uma revisão de logout não pode descartar o recibo por uma confirmação órfã: a interface pede cancelar e iniciar novo logout. `ready` de painel/logout pode ser retomado por **Retomar trabalho**, que chama somente `finish_close`, sem nova confirmação ou escrita de arquivo. Isso também recupera a resposta perdida de confirmação do painel. `cancel_close` recusa todo `ready`; no caso `exit`, o estado fica selado até a saída e nem `cancel_close` nem `finish_close` o liberam.

No exit efetivo, `RunEvent::ExitRequested` impede saída, mostra/restaura/foca a janela principal se estava escondida, inicia/recupera o fechamento nativo e aguarda o worker. Se precisa de consentimento, emite somente para essa janela `aurora-voice-close-attention` com `{closeId}`; se o início da saída falhar sem revisão disponível, emite `aurora-voice-close-blocked` com `{code}` limitado ao enum de erros. O provider global acima das telas de autenticação/editor recupera snapshot e apresenta diálogo ou erro estático mesmo fora do editor. Não depende de o painel estar montado. Cancelar antes de `ready` remove a intenção de exit, restaura ações e não deixa o estado global de saída permanentemente travado. Confirmar descarte ou não ter alteração permite aguardar também `ConnectManager.shutdown_all`, marcar saída nativa pronta e repetir `app.exit`.

`finish_close` não pode, por chamada arbitrária, desbloquear uma intenção `exit` ou dar autorização para sair. Fechar janela para bandeja preserva o comportamento atual de apenas esconder: não descarta slot nem recibo. Encerrar à força/crashar o processo continua fora da garantia de espera; o journal protege a recuperação existente, mas o recibo RAM desaparece. Nenhum caminho encerra Java do usuário.

## Erros fechados

| Código | Significado apresentado |
| --- | --- |
| `voice_unavailable` | Experimento indisponível neste build/plataforma. |
| `voice_owner_mismatch` | Janela sem autoridade para esta operação. |
| `voice_invalid_request` | Pedido fora do contrato. |
| `voice_busy` | Trabalho anterior ainda em execução. |
| `voice_context_active` | Finalize o contexto anterior antes de selecionar outra cópia/mod. |
| `voice_context_changed` | Cópia, mod ou identidade não corresponde mais à seleção. |
| `voice_stale_review` | Revisão consumida, vencida, divergente ou configuração alterada; preparar novamente. |
| `voice_java_running` | Fechar Java antes de ler/alterar por este laboratório. |
| `voice_process_unavailable` | Não foi possível verificar a ausência de Java. |
| `voice_instance_busy` | Lock da instância ocupado. |
| `voice_incompatible` | Runtime/artefato incompatível com o adaptador selecionado. |
| `voice_unsafe_path` | Configuração ausente ou caminho/link recusado. |
| `voice_integrity` | Configuração, armazenamento ou contexto não verificável. |
| `voice_limit` | Limite de arquivo, apresentação ou inventário excedido. |
| `voice_storage` | Falha estática de leitura/escrita/armazenamento/espaço. |
| `voice_needs_recovery` | Journal pendente ou executor interrompido; usar Recuperação. |
| `voice_close_pending` | Há intenção de fechamento em curso; cancelar ou concluir esse fluxo. |
| `voice_stale_close` | Revisão/ID de fechamento inválido, consumido ou vencido; recibo preservado. |
| `voice_internal` | Falha interna redigida; conferir snapshot antes de repetir ação. |

Não retornar `Display` bruto de I/O, stack, caminho absoluto, conteúdo de configuração, inventário ou comandos. O frontend faz allowlist dos códigos; código desconhecido vira mensagem genérica, sem interpolar causa. Erro e fase são separados: `voice_needs_recovery` não prova que a tentativa atual escreveu; um journal já pendente também exige a via separada.

## Integração e validação

Ownership: `session52_prepare` entrega engine/manager/DTOs/testes nativos e possui Cargo exclusivo; root entrega `voice_announcement_commands.rs`/`main.rs` e lifecycle nativo; `qa51_library` entrega `features/voice-announcement/VoiceAnnouncementPanel.tsx`, parsers/tipos/testes e integrações estreitas `DiagnosticLabView.tsx`/`App.tsx`, além deste domínio. Registrar manager e comandos em `main.rs`; módulo de engine e wrapper ficam separados dos monólitos existentes. Não refatorar account/session, discovery, transportes ou recovery para implementar esse ciclo.

O aceite exige testes nativos com escrita e restauração reais em cópias próprias de ambos os JARs fixados, sem Java: fonte intacta, guard/lock real com probe somente privado de teste, inspeção→plano→aplicação→recibo→restauração, snapshot/CAS, replay/digest errado/expiração, mudança de identidade/runtime/configuração, segundo contexto, aplicação concorrente, cancelamento antes/durante worker, resultado retido mesmo após pedido de saída, restauração sem JAR e erro pós-executor. Reconciliação explícita deve preservar edição alheia, recusar chave própria alterada, manter original intacta e exigir novo digest; falha estrita não a dispara. Saída precisa de provas de barreira, intenção concorrente, review vencida/incorreta preservando recibo, cancelamento reabilitando restore e espera até conclusão do worker. Opt-ins são selecionados pelo módulo exato, nunca `include-ignored` geral.

DOM cobre featureoff, seleção/mod/cópia, schema fechado, distinção configurado/efetivo, apresentação e autorização explícita, novo digest desmarcando consentimento, sourceguard, resposta tardia/remount, trabalho ocupado, estado real após cancelamento, ações estrita/reconciliada separadas sem fallback, drift/conflito e navegação Recuperação, diálogo global de exit, logout esperando native/Connect/Firebase e ausência de alegação de áudio. Fixture DOM não homologa janela Tauri nem rede.

Os arquivos de interface são `features/voice-announcement/{types.ts,VoiceAnnouncementProvider.tsx,VoiceAnnouncementPanel.tsx,VoiceAnnouncement.smoke.mjs}`, com integrações estreitas em `App.tsx` e `DiagnosticLabView.tsx`. O provider acima da autenticação conserva a apresentação global de saída; a autoridade continua exclusivamente nativa. Nenhum desses arquivos escreve configuração diretamente ou usa inspeção como autorização.

### Evidência de interface executada

Comandos executados na raiz da worktree; logs privados em `.aurora/connect-validation/`:

| Comando | Resultado observado | Log |
| --- | --- | --- |
| `node --test apps/desktop/src/features/voice-announcement/VoiceAnnouncement.smoke.mjs` | 33 testes PASS, zero falhas/skip,941,0356ms; cobre ambos os mods com IPC sintético, consentimento, schema/limites, remount, gerações, expiração/relógio recuado, restaurações separadas, barreiras, perda de respostas e recuperação por polling tardio. Inclui clocks restritos à revisão, sem atualizações periódicas de expiração no browser/padrão desligado. | `voice60-frontend-smoke-clock-final.log` |
| `node --test apps/desktop/src/features/connect/ConnectView.smoke.mjs apps/desktop/src/features/connect/fragmented-types.test.mjs` | 48 PASS, zero falhas/skip, 1335,1244ms. | `voice60-connect-regression.log` |
| `node apps/desktop/src/features/diagnostic-lab/DiagnosticLabView.smoke.mjs` | PASS nas 16 verificações nomeadas do roteiro; não são 16 casos contabilizados por `node --test`. | `voice60-diagnostic-regression.log` |
| `npm --prefix apps/desktop run build` | TypeScript/Vite PASS. Permanece aviso de chunk maior que500kB. | `voice60-frontend-build-clock-final.log` |
| `git diff --check -- apps/desktop/src/App.tsx apps/desktop/src/features/diagnostic-lab/DiagnosticLabView.tsx apps/desktop/src/features/voice-announcement docs/aurora-connect-desktop-voice.md` | PASS após correções. | saída do comando |

Falhas e correções preservadas: o primeiro build recusou cinco usos de `unknown` nos testes de pertencimento de fase (`TS2345`); a fase agora é estreitada somente depois de validar o enum. O primeiro focal DOM teve20 PASS/1 FAIL por atribuir diretamente o getter `select.value` do Linkedom; a fixture passou a selecionar a opção pelo DOM. As rodadas intermediárias22,25 e28 PASS permanecem nos logs `voice60-frontend-smoke-{second,third,fourth}.log`; builds segundo/terceiro/quarto também passaram. A revisão independente encontrou e orientou negativos para coerção de enum por array, consentimento reaproveitado depois de expiração civil, revisão de logout órfã e retomada de fechamento cuja resposta IPC se perdeu. O último delta cobre também as consultas imediatas indisponíveis, sem deixar logout pendente após recuperação do snapshot por polling. A QA independente final leu fontes e os30 resultados, sem bloqueador estático restante. Nenhuma dessas fixtures é evidência de execução gráfica Tauri ou de mutação real de arquivos.

Depois da rodada30, o coordenador identificou um timer de expiração que atualizava o provider mesmo com a feature desligada. Provider e painel agora iniciam seus clocks somente para revisões relevantes, leem o horário ao abri-las e limpam o intervalo ao sair; os listeners e o polling geral não são reinstalados por essa mudança. Três provas adicionais levaram ao33 final; QA independente conferiu o delta e os logs. As30 provas/979,2499ms e build anteriores continuam registrados como intermediários.

### Evidência nativa executada

Base `69601a0`, em09/10/2026. Os fixtures criam origem e cópia diagnóstica próprias, conferem tamanho/SHA dos dois JARs locais e usam CAS, locks e arquivos reais. O probe de processos e as barreiras causais são privados a `cfg(test)`; não existe injeção pública de guard/processo ou bypass de aprovação. Origem permanece intacta, e o cleanup confere caminho canônico/parent/prefixo/UUID antes de remover somente o diretório temporário próprio. Não foi iniciado Java.

Comandos executados na raiz da worktree:

```powershell
$env:AURORA_CONNECT_ARTIFACT_DIR=(Resolve-Path .aurora/connect-voice-artifacts).Path
cargo test --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --features connect-voice-lab --lib engine::connect_voice::desktop::tests -- --list
cargo test --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --features connect-voice-lab --lib engine::connect_voice::desktop::tests -- --include-ignored --test-threads=1
cargo test --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --lib engine::connect_voice_desktop::tests -- --test-threads=1
cargo test --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --features connect-voice-lab --lib engine::connect_voice_desktop::tests -- --test-threads=1
cargo test --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --features connect-voice-lab --bin aurora-launcher-core voice_announcement_commands::tests -- --test-threads=1
```

Foco inicial:10 PASS/0 falhas/0 ignorados,68,77s de testes e12,02s de compilação (`voice60-focused-1.log`). Foco final: **11 PASS/0 falhas/0 ignorados**,75,82s de testes e10,44s de compilação (`voice60-focused-final.log`), incluindo os três casos regulares e oito opt-ins selecionados somente neste módulo. A listagem restrita foi conferida antes de acrescentar os últimos casos; nenhum opt-in histórico/interativo foi ativado. Fachada:3 PASS padrão e2 PASS voz; wrapper:1 PASS. Logs `voice60-facade-{default,voice}.log` e `voice60-wrapper.log`, com compilação12,46s/0,95s/41,46s, respectivamente.

As provas incluem ciclo completo dos dois mods, aprovação de uso único, expiração original, digest/generation/contexto/UUID/runtime/hash divergentes, segunda cópia recusada, restauração reconciliada sem JAR preservando edições alheias e recusando mudanças de chave própria. Erros antes e depois do executor têm oráculos separados, inclusive restauração que exige `NeedsRecovery` depois do CAS. Barreiras causais de execução, sem depender de um sleep para ordenar a corrida, comprovam que abandonar IPC/cancelar fechamento aguarda o writer e conserva resultado/recibo; outra prova não publica revisão concluída sob barreira. Revisões de saída vinculam contexto/intenção, consomem aprovação e mantêm recibo quando recusadas. Relógios civil/monotônico, `Ready` e `Exit` selado são verificados separadamente.

Verificações estáticas usaram os comandos abaixo nas três configurações: sem flags adicionais (padrão), `--features connect-voice-lab` (voz) e `--all-features` (combinado), inseridos antes de `-- -D warnings` no Clippy:

```powershell
cargo check --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets
cargo clippy --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --all -- --check
```

Check/Clippy finais **PASS**: padrão8,31s/9,51s; voz5,96s/10,27s; combinado12,10s/13,58s. Logs `voice60-{check,clippy}-default.log`, `voice60-{check,clippy}-voice-final.log` e `voice60-{check,clippy}-combined-final.log`. Formatação passou em `voice60-fmt.log`. Primeiro check de biblioteca voz passou4,69s, e check voz de todos os targets intermediário passou9,60s. A primeira listagem recusou somente `let_underscore_lock` da fixture; a segunda passou. O Clippy voz inicial recusou dois tipos complexos dos hooks de teste; aliases privados resolveram sem alterar produção ou desabilitar lint. Esses logs de falha permanecem. Depois do foco11 houve somente essa extração de tipos em `cfg(test)`.

As regressões completas usam `cargo test --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml -- --test-threads=1`, sem feature, com `--features connect-voice-lab` e com `--all-features` antes do separador `--`, respectivamente:

| Configuração | Resultado | Tempo somado de testes / compilação | Log |
| --- | --- | --- | --- |
| Padrão | 265 PASS/5 ignorados/0 falhas;156 biblioteca,43 binário,66 integrações | 49,94s /51,99s | `voice60-full-default.log` |
| Voz | 307 PASS/41 ignorados/0 falhas;198 biblioteca,43 binário,66 integrações | 59,46s /1m45s | `voice60-full-voice.log` |
| Combinado | 338 PASS/42 ignorados/0 falhas;229 biblioteca,43 binário,66 integrações | 123,68s /2m11s | `voice60-full-combined.log` |

Cada execução acima contém nove grupos. Opt-ins já exercitados no foco não são somados novamente ao total das regressões, e ignorados históricos não recebem aprovação por esta execução. Esses tempos de testes são somas dos grupos, não tempo total de parede. As rodadas voz/combinado e foco11 precedem o último gate puro de candidato à cópia, descrito a seguir; o build padrão não compila esse ramo.

**Último delta e provas posteriores.** O caller agora repete o gate puro `is_lab_candidate` antes do hook/probe/guard, preservando o erro `voice_unsafe_path` da inspeção59. Uma origem comum é recusada sem consultar processos ou criar lock/DB; a proveniência completa da cópia continua obrigatória depois. Quatro casos regulares passaram no fonte final,0,72s, com oito opt-ins ignorados nessa rodada (`voice60-early-gate-regular.log`). O ciclo positivo dos dois mods passou separadamente,1 teste/27,08s,0 falhas/ignorados (`voice60-early-gate-positive.log`). Os comandos foram, respectivamente:

```powershell
cargo test --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --features connect-voice-lab --lib engine::connect_voice::desktop::tests -- --test-threads=1
cargo test --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --features connect-voice-lab --lib engine::connect_voice::desktop::tests::both_mods_complete_inspection_apply_and_strict_restore_with_owned_original_reviews -- --include-ignored --test-threads=1
```

Check/Clippy de todos os targets foram repetidos após esse delta: voz5,23s/6,11s e combinado6,87s/6,81s, todos PASS; logs `voice60-{check,clippy}-{voice,combined}-gate.log`. Formatação/diff também passaram. Não chamar essas rodadas de12/12, nem afirmar que todos os opt-ins/full voz/combinado foram repetidos: as evidências anteriores e posteriores ao ramo aditivo são separadas. O build padrão não compila esse gate.

QA independente e coordenador revisaram lifecycle nativo, perda de IPC, projeções e fronteiras do caller. Logs completos foram conferidos pelo coordenador; não restaram diretórios dos fixtures60 após cleanup. Locks e matriz ativa permanecem inalterados; helpers/pins imutáveis53 não foram substituídos. `voice60-source-freeze.json` registra os arquivos finais do incremento e pins preservados, em `.aurora/connect-validation/`. Ensaios gráficos pessoais, logout real, Java, áudio e dois PCs permanecem separados; não foi gerado instalador. O painel não declara voz disponível, transporte aberto ou configuração efetiva do mod.

## Rollback

Cancelar revisão antes de confirmar não altera disco. Depois de aplicação, preparar e confirmar restauração estrita enquanto o recibo estiver `ready`; se houve apenas edição alheia, a via reconciliada exige seleção e revisão próprias. Em conflito/`needsRecovery`, abrir Recuperação com o jogo fechado e revisar a ação apropriada. Nunca selecionar reconciliada automaticamente nem usar o inspector59 como gate de restore, pois o JAR pode ter sido removido depois da aplicação.

Antes de desativar/reverter o código ou fechar o app, informar estado real e tratar o recibo pelo fluxo acima. Rollback de código é revert local revisado do incremento60; não apaga cópias, CAS, banco, configuração pessoal, kits ou worktrees. O build padrão continua sem habilitar o experimento.

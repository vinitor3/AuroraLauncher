# Recibos de anúncio de voz no desktop — contrato do incremento68

Estado: **PROTÓTIPO / PRECISA DE TESTE**. O caller e o painel acrescentam a alternativa explícita de recibo durável sobre o [armazenamento67](aurora-connect-durable-voice-receipts.md). O caminho RAM do [60](aurora-connect-desktop-voice.md) permanece disponível. Testes DOM/parser e build passaram; isso não homologa janela Tauri, falha física de energia, áudio ou redes externas. A feature `connect-voice-lab` permanece desligada por padrão.

## Objetivo e limites

Permitir aplicar o anúncio em uma cópia diagnóstica com recibo salvo na mesma transação Recovery e, em outra execução do launcher, selecionar esse recibo para preparar uma restauração nova. Isso elimina a dependência da RAM para a restauração seletiva sem transformar arquivos persistidos em uma aprovação reutilizável.

Somente cópia diagnóstica verificada, modo dedicado Aurora, Fabric1.21.1, Simple Voice Chat2.6.22 ou Plasmo Voice2.1.17. A origem pessoal não é alterada. Um recibo salvo não comprova bind, transporte, mod carregado ou áudio. Não inicia Java, abre ponte, descobre UDP ou configura LAN/modo misto. Os dois mods conservam validação separada; Assistente e nove pares Core/Skins permanecem intactos.

## Escolha explícita antes da revisão

O caminho RAM existente permanece disponível com seus contratos e avisos. A alternativa durável começa por uma ação própria, **Revisar aplicação com recibo salvo**, depois da inspeção. O nativo fixa essa escolha no plano retido antes de apresentar o diff. O aviso e o digest da revisão precisam distinguir a persistência; não se permite escolher RAM/durável apenas na confirmação com a mesma revisão.

Confirmar continua exigindo operação, plano, digest e consentimento atuais. Imagens integrais de configuração, identidade física, token de recibo, hash de registro privado e permissão de restauração permanecem no nativo. O IPC apresenta somente a observação e o diff delimitados já previstos no60. O sucesso só é apresentado como salvo após commit comprovado e retenção do resultado nativo. Erro posterior ao início do executor conserva `NeedsRecovery`; não se deduz sucesso pela aparência atual do arquivo.

Preparar e cancelar continuam sem mutação de configuração ou recibo. A revisão é somente RAM, mantém seu prazo original de300s e nunca é restaurada depois de reiniciar. O recibo publicado usa índice/CAS67; não há JSON importado, sidecar, armazenamento no navegador, reconstrução de reciboRAM ou persistência de consentimento.

## Listagem e retomada

**Consultar recibos salvos** é uma ação explícita na cópia pronta selecionada. O nativo resolve `InstanceId` pelo layout existente e adquire `VoiceMutationGuard`, conservando verificação de ausência de Java, lock, identidade existente e proveniência. A consulta não executa Recovery, inicializa banco, repara identidade ou escreve configuração. Lock ocupado informa que a instância está em uso; journal pendente exige verificar a Recuperação.

O resumo tem no máximo64 registros, cada um com `receiptId`, `voiceMod`, `state` (`ready` ou `restored`) e `generation` (0 ou1). Somente `ready` pode ser selecionado. Os caminhos e imagens originais/aplicadas não aparecem na lista. Estado `restored` é histórico, não botão para reaplicar ou desfazer.

**Retomar restauração** envia somente a cópia explícita, ID do recibo e geração esperada. O nativo revalida índice, CAS, mod, identidade e estado antes de criar uma nova operação. Retomar não cria revisão nem escreve arquivo. O usuário escolhe então **Revisar restauração estrita** ou **Revisar restauração preservando outras edições**; cada uma mantém os gates58/59/67 e exige aprovação nova. Conflito não seleciona automaticamente a alternativa reconciliada. A retirada posterior do JAR não impede restaurar as próprias chaves.

## Contrato IPC implementado

Todos os pedidos usam o envelope `{request: ...}`. Identidade física, imagens integrais de configuração e autoridade de restauração permanecem no nativo; observação e diff delimitados continuam disponíveis para revisão:

| Operação | Pedido mínimo | Resultado esperado |
| --- | --- | --- |
| `voice_announcement_prepare_apply_durable` | `operationId`, `generation` | Snapshot com plano durável explicitamente identificado e digest próprio |
| `voice_announcement_list_receipts` | `instanceId` | Identificação da cópia consultada e lista limitada de resumos |
| `voice_announcement_resume_receipt` | `instanceId`, `receiptId`, `expectedGeneration` | Snapshot da operação nova, controlador carregado e nenhuma revisão |
| `voice_announcement_confirm` | `operationId`, `planId`, `digest` | Executa somente a política retida na preparação |

A capability opcional é `durableReceipts:{schemaVersion:1,maxReceipts:64}`, ausente no build sem a feature. Sua ausência preserva o painel60 e impede invocar os comandos novos. A lista retorna `{instanceId,receipts:[{receiptId,voiceMod,state,generation}]}`, recusa campos desconhecidos, IDs repetidos, mais de64 registros e mais de um `ready` por mod. Somente `ready/0` e `restored/1` são válidos. Pedidos não recebem path, configuração, PID ou token arbitrário.

O snapshot acrescenta opcionalmente `durableReceipt:{state,receiptId,generation}`. `planned` aparece durante preparo/revisão/aplicação e tem ID/geração nulos; não é prova de persistência. `saved` tem ID verificado e geração0 enquanto disponível ou1 após restauração. `uncertain` exige Recovery: ID/geração permanecem nulos quando a publicação ainda é incerta, ou conservam o recibo original quando a restauração foi interrompida. Cancelar um plano remove `planned`. O caminho RAM não serializa esse campo.

A retomada publica `resumingReceipt`, ocupada e sem contexto inventado, enquanto lê o registro. Sucesso cria operação nova em `applied`, com recibo `saved`, sem inspeção, revisão ou resultado de Recovery antigos. Falha sem controlador retorna `idle` e erro redigido. A revisão de fechamento inclui o mesmo metadado durável e digest próprio; `selectiveRestorationLost:false` só é permitido para `saved`. `uncertain` e RAM conservam o aviso de perda/indisponibilidade. Depois do fechamento o metadado pode permanecer como observação histórica até liberar a barreira, sem autoridade de restauração no frontend.

## Concorrência, fechamento e resultado incerto

Listagem/retomada participam da autoridade exclusiva e da barreira de saída60. Resposta de listagem fica ligada à cópia, seleção e época da chamada; mudar cópia/mod, desmontar ou encerrar invalida a apresentação atrasada. Uma operação ativa nunca é substituída silenciosamente por um recibo selecionado.

O provider mantém os resultados mutantes sob geração/operação nativa. Se uma resposta se perder, apenas observa o estado; não repete confirmação, retomada ou restauração automaticamente. Não conserva recibos ou aprovações em React/localStorage para recuperar uma operação.

Encerrar, logout e sair continuam aguardando o worker e conservando a barreira nativa. Não restauram arquivos, matam Java ou removem registros. ReciboRAM mantém o aviso e consentimento de perda existentes. Recibo durável comprovado permite explicar que o registro permanece para uma retomada explícita. Resultado incerto não pode usar o aviso de persistência comprovada: exige Recovery e revalidação. O fechamento deve vincular seu resultado ao próprio digest; o frontend não decide que uma alteração é durável pelo perfil de transporte ou por um nome de arquivo.

## Cenários DOM e de contrato

1. Capability ausente/desligada mantém60 exatamente: nenhum botão, IPC ou promessa de persistência nova.
2. Inspeção e preparação durável são separadas. Nenhum comando mutante antes da revisão/consentimento; confirmação manda somente operação/plano/digest.
3. Trocar a opção RAM/durável exige preparar outra revisão; consentimento antigo não vale para novo plano, prazo, digest, cópia ou mod.
4. Lista aceita no máximo64 resumos exatos, IDs únicos, mods conhecidos e combinações `ready/0` ou `restored/1`; recusa dados extras, bytes, paths e estados contraditórios.
5. Resposta atrasada de outra cópia ou de seleção anterior não fornece botão para retomar. Listar, mudar a seleção e listar novamente não reaproveita o primeiro resultado.
6. Retomar envia ID/geração previstos, não faz autoapply/restore e exige revisão nova. Recibo restaurado não pode ser retomado. Conflito ou geração obsoleta pede observação nova, sem repetição automática.
7. Reabrir/remontar não mantém consentimento nem revisão antiga. Snapshot retomado não afirma que o mod está instalado, o transporte disponível ou o áudio testado.
8. Restauração estrita e reconciliada usam botões separados; falha estrita não chama a reconciliada.
9. Perda de resposta após aplicar/restaurar mostra somente o último resultado nativo validado; polling não reenvia a mutação e snapshot inválido não libera confirmação.
10. Sair durante worker aguarda seu resultado. RAM exige aviso de perda; durável comprovado mostra retenção; `NeedsRecovery` incerto conserva advertência própria. Cancelar saída conserva o controlador.
11. Logout preserva a ordem: barreira de voz, cleanup Connect, ação de conta, liberação da barreira; resultado IPC desconhecido não confirma logout por inferência.
12. Os dois mods, o caminho RAM e as regressões60 permanecem cobertos. Os parsers não aceitam resumo de recibo como plano ou revisão como recibo publicado.

## Prova nativa e arquivos

Além das fixtures DOM, a integração deve executar em diretório temporário real, para cada mod: inspecionar/preparar/confirmar durável, fechar/destruir o manager, reabrir outro manager, listar/retomar, obter revisão nova e restaurar; conferir bytes e registro transacional. Exercitar retirada do JAR, edição alheia preservada, conflito próprio, geração/replay, fechamento durante worker, perda da futureIPC e journal pendente. Reabrir objetos prova independência da RAM; não homologa queda física de energia, janela Tauri, doisPCs ou áudio.

Arquivos: `engine/connect_voice_desktop{,_live,_durable,_durable_types}.rs`, `voice_announcement_commands.rs`, registro em `main.rs` e testes nativos próprios; adaptador67 mantém a política no digest e observa início do executor. No frontend: `features/voice-announcement/{types.ts,VoiceAnnouncementProvider.tsx,VoiceAnnouncementPanel.tsx,DurableReceipts.tsx,VoiceAnnouncement.smoke.mjs}`. Nenhuma mudança nos arquivos Connect66, no transporte, matriz, manifests ou locks.

## Evidência local do frontend

Comandos realmente executados na worktree, em14/10/2026:

- `node --test apps/desktop/src/features/voice-announcement/VoiceAnnouncement.smoke.mjs`: **46 PASS**, zero falhas/ignorados. Inclui os dois mods, aplicação durável escolhida antes do consentimento, fechar/remontar/listar/retomar com revisão nova, RAM sem capability, cancelamento, seleção/resposta tardia, histórico restaurado, erro/perda de resposta de aplicação e retomada sem repetição, estado incerto, saída durante retomada, lista que atravessa a barreira de fechamento e ordenação do logout. São fixtures DOM/IPC, não execução gráfica Tauri nem prova de gravação de arquivos.
- `npm.cmd run build`, em `apps/desktop`: **PASS**, TypeScript e Vite,362 módulos. Permanece o aviso anterior de chunk maior que500kB; build não comprova homologação.

Logs locais da rodada final: `.aurora/connect-validation/durable-voice-68-frontend-final.log` e `durable-voice-68-frontend-build-final.log`. A rodada anterior,44 testes, também passou; não é somada à contagem final. Evidência nativa de arquivos/transações deve ser registrada separadamente após os testes coordenados. Nenhum jogo, mod real, rede externa ou configuração pessoal foi iniciado/alterado por esses comandos.

## Rollback e intervenções

Evidência nativa68: seis testes comuns passaram no foco integrado67/68; cinco opt-ins de arquivo passaram em59,73s, com ambos os mods, retomada por manager novo, JAR retirado, restauração estrita/reconciliada, conflito e resultados abandonados/incertos. Clippy combinado passou. Regressão integrada all-features:379 PASS/54 ignorados, zero falhas,158,45s somados; os opt-ins pertinentes foram executados separadamente e estão discriminados no [task68](task-aur-r4-connect.md#incremento68--recibos-salvos-e-retomada-no-painel). Não equivalem a áudio/GUI ou falha física de energia. Um ajuste somente de sincronização de teste posterior ao binário completo será conferido no foco seguinte.

Rollback operacional: cancelar revisão não executada ou encerrar a operação, aguardando o worker; preservar CAS, índice, recibos e cópia. Rollback de código deve respeitar67: resolver journals2 com versão que os compreenda antes de downgrade. Não remover recibos para fazer uma versão antiga aceitar o armazenamento.

Implementação e testes locais descritos não exigem segundo computador nem ação do usuário. Validação gráfica, áudio real e redes continuam gates próprios. Recibo salvo não identifica participante, mod instalado ou serviço UDP disponível; nenhuma prontidão de voz é inferida desta funcionalidade.

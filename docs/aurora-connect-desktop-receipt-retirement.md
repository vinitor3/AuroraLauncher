# Aposentadoria do recibo pelo desktop — incremento70

Estado: **PROTÓTIPO / PRECISA DE TESTE**. Integração sobre a [aposentadoria69](aurora-connect-voice-receipt-retirement.md) e o [painel durável68](aurora-connect-desktop-durable-voice.md). Frontend implementado e testes DOM/parser/build executados conforme registro abaixo; prova nativa de índice e homologação gráfica são separadas. A feature `connect-voice-lab` permanece desligada por padrão.

## Ação e limite

Uma edição posterior ou undo genérico pode deixar um recibo disponível cuja restauração seletiva já não corresponde à configuração. Isso conserva a recusa segura da restauração e bloqueia outra aplicação durável do mesmo mod. O painel deve oferecer uma ação explícita para revisar a aposentadoria desse recibo; falha estrita nunca escolhe essa ação automaticamente.

A confirmação abandona somente a autoridade seletiva do recibo. O índice passa de `Ready/0` a `Retired/1`; a configuração permanece exatamente como está. Não restaurar ou editar anúncio, arquivo, mod, bind, ponte ou sessão; não apagar CAS, snapshots ou histórico. O estado aposentado deve ser apresentado separadamente de `Restored`, sem botão para retomar ou restaurar.

## Seleção, revisão e aprovação

A ação **Revisar aposentadoria do recibo** aparece somente quando a capability própria estiver disponível e houver um recibo `Ready/0` na lista explícita da cópia e mod atuais. O preparo exige que nenhuma operação esteja ativa: encerrar uma operação conserva o recibo salvo, permitindo consultar novamente seu histórico. O nativo continua conferindo identidade, proveniência, guard, índice, CAS e prazo. Um resumo de lista não é aprovação nem autoridade executável.

A revisão é separada do diff de configuração. Exibe cópia, mod, identificador do recibo, estado anterior e previsto, observação limitada de presença/hash/tamanho do arquivo e o aviso de perda da restauração seletiva. Arquivo ausente deve ser distinguido de vazio ou leitura recusada. Não recebe bytes arbitrários, caminho ou configuração pelo frontend.

Confirmar exige consentimento novo para o plano/digest atuais. A caixa começa desmarcada e é limpa por cancelamento, troca de revisão/cópia/mod, fechamento, desmontagem ou expiração observada. Voltar o relógio não reaproveita consentimento vencido. **Cancelar revisão e manter recibo** descarta o plano e conserva o controlador `Ready` carregado, permitindo a restauração explícita já existente. Outra aposentadoria exige encerrar essa operação e consultar a lista novamente. Preparar ou cancelar não escreve; a confirmação não reutiliza autorização de aplicar, restaurar, fechar ou outra aposentadoria.

## Concorrência e encerramento

Preservar o slot exclusivo e a barreira de saída do68. A resposta atrasada de uma lista ou preparação anterior não pode exibir ação na nova cópia. Uma vez iniciada a confirmação, o worker nativo conserva o resultado mesmo sem a future IPC ou painel. Resposta perdida é reconciliada por observação/listagem explícita; não repetir a mutação automaticamente e não inferir sucesso pelo arquivo.

Fechar durante preparo cancela somente revisão ainda não executada. Fechar durante confirmação aguarda o resultado real. Resultado incerto não afirma que o recibo foi aposentado nem oferece restauração a partir de autoridade possivelmente consumida. Logout continua aguardando a barreira de voz, cleanup Connect e ação da conta na ordem existente. Nada restaura automaticamente ao sair.

## Contrato IPC

Capability aninhada opcional: `durableReceipts.retirement:{schemaVersion:1}`. Sua ausência mantém o68 e bloqueia os comandos de aposentadoria. O caminho RAM não serializa novos metadados. A lista compreende `ready/0`, `restored/1` e `retired/1`, mantendo limite64 e um Ready por mod. `retired/0`, `ready/1`, campos desconhecidos e resumo usado como revisão recusam. O caller68 anterior, sem conhecer esse estado, conserva recusa em vez de converter ou apagar registros.

Todos os pedidos usam `{request: ...}`:

| Comando | Pedido | Resultado |
| --- | --- | --- |
| `voice_announcement_prepare_retirement` | `instanceId`, `receiptId`, `expectedGeneration:0` | Nova operação com revisão de aposentadoria própria; nenhuma escrita |
| `voice_announcement_confirm_retirement` | `operationId`, `planId`, `digest` | Confirma somente o plano retido; resultado comprovado `retired/1` |
| `voice_announcement_cancel_review` | `operationId` | Descarta a revisão, conserva o recibo salvo `Ready/0` |

O snapshot acrescenta `retirementReview` somente em `reviewRetirement`: `planId`, `receiptId`, `voiceMod`, `expectedGeneration:0`, `currentConfigSha256`, `currentConfigBytes`, `digest`, `expiresAtMs`, `warning`. Hash/tamanho são ambos nulos se ausente, ou hashSHA256 e inteiro0..262144. Não há diff ou conteúdo arbitrário nessa revisão. ID/mod/geração correspondem ao recibo `saved/0` da operação; `VoiceReview` de alteração/restauração não pode ser combinado a ela.

`preparingRetirement` é ocupado e sem contexto inventado enquanto verifica o registro; falha nessa carga retorna `idle`, sem autoridade. `reviewRetirement` conserva controlador `Ready/saved0`; `retiring` é ocupado e não tem revisão reutilizável. Sucesso publica fase `retired`, sem `restorationState`, com `durableReceipt:{state:'retired',receiptId,generation:1}`. Esse metadado pode permanecer como histórico durante `closing/closed`, com aviso distinto de recibo RAM perdido e de configuração restaurada. Falha da confirmação anterior ao executor conserva `Applied/saved0`; falha depois de iniciar o executor exige `NeedsRecovery/uncertain0`, sem conclusão por inferência. A confirmação não publica resultado de Recovery de arquivos, pois altera somente o índice.

Nenhuma mudança de transporte, matriz, Core/Skins ou autojoin integra este incremento.

## Evidência exigida

- Capability ausente mantém o comportamento68; nenhum novo IPC ou botão.
- Os dois mods apresentam revisão própria, diferença entre aposentadoria/restauração e consentimento inicialmente desmarcado; nenhuma confirmação antes da ação explícita.
- Cancelar não consome o recibo; revisão nova não herda consentimento. Expiração e retorno do relógio preservam a recusa.
- Trocar cópia/mod, desmontar ou iniciar fechamento invalida resultados antigos; clique duplo não inicia dois workers.
- Registro aposentado é histórico e nunca retomável. Configuração ausente/vazia/observada tem representação exata, sem inferências de restauração.
- Perda de resposta, geração antiga e fechamento/logout durante worker conservam resultado e barreira; nenhum replay automático.
- Testes nativos69/70 devem verificar o índice e bytes em armazenamento temporário real. Fixtures DOM não comprovam gravação física ou janela Tauri.

## Evidência local do frontend

Executado em14/10/2026 na worktree, sem Java ou configuração pessoal:

- `node --test apps/desktop/src/features/voice-announcement/VoiceAnnouncement.smoke.mjs`: **58 PASS**, zero falhas/ignorados,1353,3704ms. Inclui46 regressões60/68 e12 cenários70: ambos os mods, capability antiga, consentimento/cancelamento, expiração observada pelo timer ou clique e relógio retornando, outra cópia, ausência/vazio, históricos distintos inclusive após fechamento, perda de respostas de preparo/commit, executor incerto, duplo clique/logout durante worker e contratos incompatíveis. Não somar rodadas sobrepostas.
- `npm.cmd run build` em `apps/desktop`: **PASS**, TypeScript e Vite,364 módulos; aviso anterior de chunk acima de500kB permanece.

Logs finais: `.aurora/connect-validation/receipt-retirement-70-frontend-final.log` e `receipt-retirement-70-frontend-build-final.log`. A primeira rodada57 e seu build também passaram, mas não substituem a rodada final após ajustes de fechamento/prazo. Esses testes usam DOM/IPC sintéticos e não homologam a janela Tauri ou efeitos físicos do índice.

Prova nativa70: dois testes comuns e quatro opt-ins de arquivo passaram no foco combinado69/70. Ambos os mods mantiveram bytes/edições independentes e fonte intactos; cancelar permitiu outra revisão, histórico Retired recusou retomada/replay e resposta perdida durante saída reteve o resultado. A interrupção depois de entrar no executor conservou estado incerto até consulta posterior ao índice. O teste68 de liberação efetiva do lock também passou após seu ajuste. Regressão padrão278 PASS/5 ignorados, Clippy com voz e todos os targets PASS, rustfmt/diff PASS. [Comandos, durações e limites](task-aur-r4-connect.md#incremento70--aposentadoria-pelo-painel-com-barreira-de-saída).

## Rollback e intervenção

Cancelar a revisão não executada preserva a autoridade. Depois de uma aposentadoria confirmada, não prometer reativação do recibo; alterar configuração requer outro plano e aprovação, conservando os snapshots existentes. Rollback de código respeita os estados/journals do índice e nunca reescreve histórico para permitir downgrade.

Implementação e testes locais não exigem outro computador, Java, conta ou configuração pessoal. Áudio real, redes externas e experiência gráfica continuam validações separadas.

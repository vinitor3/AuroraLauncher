# Revisão e rechecagem da origem individual — incremento76

Data: 15/10/2026. Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**, com testes nativos locais, DOM e build aprovados. Base: commit75 `28f8018909d597df7379f648a6a14baa60c74beb`. Base contratual: [preparo individual75](aurora-connect-discovery-lifecycle.md), [grupo74](aurora-connect-discovered-group.md), [descoberta48](aurora-connect-desktop.md#descoberta-de-porta-da-instância--incremento48), [sessão56](aurora-connect-desktop-session-lifetime.md) e [origem64](aurora-connect-discovered-session-udp.md).

## Bloqueio observado

`live::Manager::review_host` mantém o mutex Connect durante `TargetGuard::recheck`. Para origem descoberta, isso consulta processo e tabela Windows de listeners. Enquanto essa consulta não retorna, snapshot e stop não conseguem observar ou cancelar a operação. O comando Tauri também é síncrono. Esse trecho não é corrigido pelo preparo75.

O ator individual `run` executa ainda três rechecagens síncronas de origem fora do mutex, mas na tarefa assíncrona: antes de abrir o Host, depois da abertura e no encerramento. O monitor periódico já despacha e aguarda uma consulta bloqueante. A análise estática não mede um travamento real do Windows; o aceite será uma prova causal com consulta própria retida, sem alterar o sistema ou fingir demora real.

## Escopo e compatibilidade

Preservar `connect_review_host({operationId,guestPublicId}) -> ConnectReview`, perfis, JSON, assinatura, destinatário, origem confiável, digest e deadlines existentes. Não introduzir outra revisão ou renovar o preparo60s/admissão/sessão por esperar IO. Grupo74, manual, Guest, descoberta de seleção e recibos de voz mantêm contratos próprios. Não abrir sockets, executar Java, alterar configuração ou consultar rede no worker de revisão.

Antes da consulta ainda não há convite emitido: como no contrato atual, ingresso e sessão nascem uma única vez após a rechecagem e os gates finais, no instante de emissão. Não antecipar essa âncora para o dispatch do worker. O preparo60s continua ancorado na reserva original e limita a emissão/revisão; depois de emitida, a autoridade não é reconstruída nem ganha prazo por atraso, erro ou perda do waiter. A espera pela consulta não cria direito de emitir depois do preparo expirado.

As APIs Rust internas podem mudar para garantir ownership e espera. Auditar/migrar todos os callers; não manter como atalho uma revisão descoberta que segura o mutex durante IO. Uma origem manual não precisa de consulta de processo, mas continua sujeita às mesmas validações e à compatibilidade de revisão.

Desenho acordado: `reserve_host_review` síncrono devolve recibo interno de resultado; manual pode resolver imediatamente, origem descoberta usa oneshot e worker retido separadamente em `host_review`. Apenas o guard de origem é clonado para consultar; o draft permanece no Attempt até o gate final, podendo ser removido por stop/expiração. Falha de origem/pânico após reserva encerra essa tentativa com erro controlado e exige novo preparo; isso substitui explicitamente o antigo retorno de erro que recolocava o draft em hostPrepared. Rejeições puras de campos/proprietário/destinatário conservam seus códigos e não criam worker.

## Trabalho de revisão

Antes do primeiro dispatch/await, validar campos puros, selecionar a operação e registrar o trabalho de revisão sob o mutex Connect. O trabalho pertence à mesma operação/geração e não ao waiter IPC. No máximo uma revisão de origem pendente; duplicação é erro controlado, sem emissão de outro convite.

Consultar processo/listener fora do mutex e em worker bloqueante próprio. Reter seu handle ou guarda de término real até a consulta e a publicação terminarem; descartar o waiter não libera o slot. O trabalho de preparo75, o trabalho de revisão76 e o ator Host não podem sobrescrever os handles uns dos outros. Snapshot e stop devem continuar acessíveis durante a consulta.

Antes de emitir/publicar a revisão, revalidar proprietário, UUID/geração, fase, cancelamento, shutdown, prazo civil/monotônico original e origem. Stop, troca de conta ou expiração ocorridos durante a consulta impedem publicação tardia e emissão de autoridade utilizável. Não retornar a um draft removido nem restaurar estado anterior após stop. Falha de origem conserva erro controlado e nenhum socket aberto; falha/pânico não pode deixar um marcador ocupado sem tarefa capaz de terminar.

Revalidar o prazo original também depois da emissão/assinatura e imediatamente antes de salvar a revisão. Se venceu durante esse trabalho, não publicar a autoridade nem renovar seu preparo. Término/expiração de prepared e reviewed, `terminal`, `finish`, `refresh_cleanup`, stop e reutilização manual devem considerar separadamente a revisão própria, inclusive sua cauda posterior à publicação.

`cleanupState:"complete"` exige término real dos trabalhos pertencentes à operação. O estado normal de preparo já publicado pode preservar sua projeção histórica; ao fechar, a consulta ainda em execução exige cleanup pendente. Preservar snapshot e preparo/revisão públicos; acrescentar metadados somente se uma necessidade concreta de observação não for atendida pelos existentes e após alinhamento entre parser e nativo.

## Interface e resposta incerta

O snapshot público continua em `hostPrepared`, com a preparação e geração existentes, durante a consulta de revisão. Não há novo DTO ou campo que represente esse trabalho privado. A UI permite **Atualizar estado** e **Encerrar Connect** enquanto aguarda `connect_review_host`. Atualizar substitui a observação anterior na interface, mas não cancela nem repete o trabalho nativo.

Depois de uma resposta perdida, malformada ou `connect_busy`, observar novamente o mesmo `hostPrepared` não prova que a consulta terminou. O campo do destinatário e o botão de revisão permanecem bloqueados até observar revisão publicada ou outra fase/operação. Uma revisão recuperada por snapshot exige consentimento novo; a resposta IPC antiga não pode restaurar outro digest ou autorização. Stop continua disponível e recursos só são apresentados como liberados com cleanup completo.

O código nativo exato `connect_invalid_request` é reservado a campos/draft inválidos recusados antes de criar o worker de revisão; somente ele libera a edição para um novo envio explícito. A UI também recusa o próprio código público do Host como destinatário antes de enviar o pedido. Não estender essa exceção a `connect_invitation_rejected`, `connect_busy`, falhas de canal, objetos de erro ou respostas sem schema válido. A validação nativa permanece obrigatória.

O contexto de pedido incerto pertence ao componente atual. Navegação no launcher mantém Connect montado, conforme73; troca de conta e inicialização de uma WebView passam pela barreira nativa de encerramento antes de publicar a conta. Após remontagem, o snapshot continua sendo a autoridade; a exclusão de uma segunda revisão durante um trabalho privado é garantida pelo nativo, não por um marcador inventado na UI. Nenhum desses caminhos reaplica consentimento antigo.

## Rechecagens do ator

Deslocar rechecagens bloqueantes de origem descoberta para consulta aguardada pelo ator existente; não destacar trabalho em caso de cancelamento ou usar timeout que perca a tarefa. O ator pertence ao manager e o stop já deve aguardá-lo. Origem manual retorna imediatamente sem criar consulta Windows.

Após a consulta anterior à abertura, revalidar cancelamento e autorização/prazo antes de chamar a abertura do Host. Após a consulta posterior, não anunciar listening/exportar se cancelou, expirou ou a origem mudou; fechar os recursos já abertos. O encerramento aguarda sua última consulta e os recursos existentes, sem matar o processo do jogo. Preservar classificação de erros e autoridade dos convites; timeout isolado não diagnostica NAT.

Antes da primeira abertura, preservar e verificar também `admission_deadline` monotônico já emitido; não depender somente do relógio civil do convite legado. Os gates de publicação listening devem ser repetidos dentro do mutex, pois cancelamento pode ocorrer após uma verificação externa. Nenhum resultado de consulta concede permissão para ignorar o estado atual.

Quando já houver Host aberto, a espera pela consulta não pode impedir observar cancelamento ou expiração. Manter a mesma tarefa de consulta retida e observar os gates de encerramento em paralelo: revogar/fechar o transporte primeiro, depois aguardar a consulta para concluir cleanup. Não cancelar por drop/destacar o worker nem continuar encaminhando indefinidamente só porque uma consulta Windows demora. Aplicar essa propriedade ao monitor periódico existente e à rechecagem posterior à abertura; após admissão consumida, conservar a regra de sessão original, sem encerrar pelo prazo de ingresso já consumido.

## Provas exigidas antes da integração

- Revisão retida fora do mutex: snapshot responde, stop muda a fase e permanece pendente; liberar consulta após cancelamento não publica revisão nem autoridade.
- Perda do waiter de revisão não libera trabalho; segunda revisão e novo preparo ficam recusados conforme fase, mesmo após erro ou publicação enquanto resta uma cauda própria.
- Revisão publicada com cauda ainda retida, seguida de confirmação rápida, conserva o handle de revisão separado do ator Host; stop fecha o transporte e só conclui cleanup após liberar a cauda.
- Expiração original durante a consulta, shutdown e geração alterada impedem resultado tardio. Clocks privados de teste; não alterar relógio do PC.
- Origem desaparecida/substituída é recusada. Processo/listener do jogo permanece vivo após simples stop Connect.
- Perfis48/56/64 e manual continuam com formato/digest e fluxo autenticado existentes. Sem ampliação de lifetime, capacidade UDP, destinatários ou transporte.
- Gates anteriores e posteriores à abertura comprovam cancelamento/expiração sem publicação listening e sem recurso perdido. Ator deve aguardar consultas mesmo se um waiter de stop for descartado.
- Com Host ativo e consulta do monitor retida, stop/expiração fecha as pontes e impede novos fluxos antes de liberar a consulta; cleanup permanece pendente até o worker terminar. Conferir ingresso consumido versus prazo de sessão sem renovar autoridade.
- Interface permite Stop durante revisão pendente e ignora revisão atrasada após encerrar/trocar contexto; nenhuma aprovação automática ou reapresentação de consentimento anterior.
- Build padrão, combinações de features relevantes, suites focadas, regressões, fmt/clippy/build, revisão independente e diff registrados. Provas locais não homologam GUI, internet, Minecraft, contas, Core/Skins, estabilidade ou áudio.

## Evidências executadas

### Nativas focadas

Implementação em `host_review.rs` com integração em `live.rs`, lifecycle e comando Tauri assíncrono; callers Rust usam reserva e recibo interno, sem mudar o JSON público. Manual resolve sem IO; origem descoberta tem worker separado do preparo e do ator Host. Após consulta, publicação revalida proprietário/operação/geração/fase/cancelamento e prazo original. Nova emissão só ocorre uma vez. O ator possui o Host e remove os owners do Endpoint antes de aguardar consulta pendente em encerramento; shutdown lógico sozinho não liberava a porta Windows.

`cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-features --lib host_review -- --test-threads=1`: primeiro **7 PASS/4 FAIL**,43,19s, compilação42,56s; diagnóstico do executável congelado confirmou rebind aguardando a porta ainda ocupada. Após correção de ownership, **11 PASS/0 falhas**,5,61s, compilação40,90s. Logs `owned-source-review-76-native-first.log`, `owned-source-review-76-after-bind-diagnostic.log` e `owned-source-review-76-native-second.log`. Detalhes e comando diagnóstico no task. A regressão integral fica pendente neste checkpoint.

Provas usam processo/listener de fixture próprio, transporte autenticado local e STATUS sintético, incluindo três perfis, perda do receptor, erro/pânico, origem substituída, prazos, revisão publicada com cauda retida, confirmação rápida, queries pré/pós-bind e monitor/final. Um observador privado comprova que o monitor avaliou ingresso vencido já consumido antes do segundo STATUS. Rebind e recusa de novas conexões acontecem antes de liberar a consulta retida. Não foi mantido payload TCP contínuo durante o corte neste teste; não inferir áudio, latência de corte UDP ou zero perda. O processo do jogo de fixture permanece independente do stop Connect.

### Interface

Arquivos: `apps/desktop/src/features/connect/ConnectView.tsx`, `ConnectView.smoke.mjs` e `account-transition.smoke.mjs`. Sem alteração de DTO, capability, App, estilos, Java ou configuração pessoal neste recorte.

- `node --test src/features/connect/ConnectView.smoke.mjs src/features/connect/fragmented-types.test.mjs src/features/connect-game-launch/ReadyGameLaunch.smoke.mjs src/features/connect/account-transition.smoke.mjs`, a partir de `apps/desktop`: **231 PASS**, zero falhas, `2843.4613 ms`. Log integral local: `.aurora/connect-validation/owned-source-review-76-frontend-first.log`.
- `npm run build`, a partir de `apps/desktop`: **PASS**, TypeScript + Vite,371 módulos; `6145 ms` totais medidos, Vite `431 ms`. Log integral local: `.aurora/connect-validation/owned-source-review-76-build-first.log`. Permanece o aviso de bundle acima de500 kB; sem instalador.
- `git diff --check -- apps/desktop/src/features/connect/ConnectView.tsx apps/desktop/src/features/connect/ConnectView.smoke.mjs apps/desktop/src/features/connect/account-transition.smoke.mjs`: **PASS**.

Os19 testes adicionais incluem revisão retida→atualização→Stop retido→resposta antiga ignorada→cleanup completo nos três perfis48/56/64; perda de resposta recuperada sem consentimento; origem inválida com cleanup pendente; `busy`, erro desconhecido e projeção malformada sem reenvio; correção explícita após rejeição pura; destinatário igual ao Host; digest mais novo preservado; remontagem; e barreira de conta aguardando o encerramento de cada perfil. Fixtures exercitam o componente e a barreira reais com respostas nativas sintéticas: não comprovam consultas Windows, mutex, worker Rust, sockets ou cancelamento real do sistema operacional. Esses resultados não substituem as provas nativas exigidas acima nem homologam GUI, Minecraft, mods de voz ou duas redes.

## Limites e rollback

Checkpoint frontend final76, após achado de QA no baseline manual expirado: a prova de rejeição do preparo descoberto aceita também terminal/cleanup complete/transport notStarted, somente com mesmo UUID, geração, fase, papel e transporte, sem preparo/revisão e com resposta definitiva seguida de nova consulta. Os gates contra IPC desconhecido e poll anterior permanecem. Acrescentados cinco casos. Mesma suite **236 PASS/0 falhas**,2845,0268ms; build **PASS**,371 módulos,6212ms total/435ms Vite. Logs `owned-source-review-76-frontend-final.log` e `owned-source-review-76-build-final.log`; `host-source-types.ts` também alterado. Não somar a rodada inicial. QA independente do frontend sem bloqueador restante.

Sem relay público, NAT automático, configuração pessoal, Java, voz própria ou mudança de matriz. Funcionalidades experimentais continuam desligadas por padrão. Simple Voice Chat e Plasmo Voice conservam gates separados. Nenhum push/deploy/instalador/gasto.

Validação integrada final: **480 PASS/63 ignorados** com todas as features e **289 PASS/5 ignorados** no padrão, zero falhas; oito targets por configuração. Fmt, clippy all/default/connect-lab, frontend236 e build passaram; revisão independente sem bloqueador restante. O primeiro padrão falhou E0433 porque Tokio opcional apareceu no recibo sem cfg; campos/espera condicionais corrigiram o padrão sem alterar o ramo experimental já testado. Todos os comandos, tempos e falhas permanecem no [task76](task-aur-r4-connect.md#incremento76--revisão-individual-e-consultas-de-origem-canceláveis). Essas provas não homologam o conjunto para usuários.

Rollback operacional encerra Connect e aguarda cleanup; não encerra o jogo. Reversão de código será local e revisada, preservando incrementos posteriores. Nenhuma intervenção humana ou segundo computador foi necessário para as provas locais. O monitor de grupo permanece escopo posterior, sem presumir corrigido por76.

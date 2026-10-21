# Encerramento do grupo durante consulta de origem — incremento77

Início15/10/2026, retomada21/10/2026: **IMPLEMENTADO MAS NÃO HOMOLOGADO**, com provas locais e revisão independente. Base de código76 `8c90a52a9c193175f99995e0d63f91ee58246847`, HEAD após revalidação documental de relay `d4328c3e40a45aba2dd5b080cbde870bdff35f91`. Complemento de [74](aurora-connect-discovered-group.md) e [76](aurora-connect-owned-source-review.md), sem atribuir ao grupo uma correção feita no ator individual.76 foi concluído e revisado; seus testes não contam como execução77.

## Bloqueio observado

O monitor de `session_group::run_group` aguarda `group_source::recheck` dentro de um ramo do select. A consulta pertence ao ator e cleanup corretamente espera por ela; entretanto, enquanto ela não retorna, o ator não observa stop nem a conclusão das autoridades individuais. `closing` muda a projeção, mas a revogação efetiva e `Host::shutdown` só ocorrem depois desse loop. É uma conclusão do código, não alegação de travamento real do Windows. O teste74 da consulta retida verifica propriedade do trabalho; não verifica que os fluxos cessam antes de liberar a consulta.

## Contrato implementado

Conservar uma única consulta bloqueante própria e aguardar seu término real. Enquanto ela estiver em andamento, observar cancelamento e as autoridades individuais. Stop/logout fecha o Host inteiro, revoga seus grants e encerra TCP/UDP antes de aguardar a consulta. O slot e cleanup permanecem pendentes até o retorno do mesmo trabalho; perder o waiter não destaca a consulta nem libera a operação. Não abortar a tarefa de consulta para simular cleanup.

Rechecagens anteriores e posteriores à abertura devem usar o mesmo modelo de propriedade. Nunca publicar listening após cancelamento ou autoridade vencida; repetir os gates sob mutex na publicação. A origem descoberta deve ser conferida depois da abertura e antes da publicação. Falha da origem encerra o grupo inteiro com erro controlado e preserva o processo do jogo.

Continuam valendo os convites schema3 individuais, respectivos relógios originais, consumo único e revogação individual. A expiração ou revogação de A não fecha B válido. Ingresso consumido não vira sessão encerrada quando seu prazo de ingresso termina. Somente fim de todas as autoridades, perda da origem compartilhada ou encerramento global fecha o grupo inteiro. Não substituir prazos por um timer coletivo, reemitir convites ou reconstruir autoridade a partir de projeções da interface.

Essa regra individual é do grupo já publicado em `hostListening`. Até a primeira publicação, preservar o gate estrito existente: todos os integrantes do roster revisado devem continuar com ingresso válido, antes de abrir e dentro do mutex ao publicar. Se A vencer durante a partida/consulta pós-abertura, encerrar a tentativa inteira sem publicar grupo reduzido. Isso não revoga B em um grupo previamente disponível; impede iniciar um roster diferente do revisado. Consulta de relógio precede leitura cumulativa de consumo ao decidir o fim de uma autoridade em execução.

Sem mudança nos comandos, DTOs, digest, formatos manuais, geração de convites, orçamento UDP, seleção30s, preparo600s, revisão60s ou matriz. Não ampliar76 durante sua validação. Qualquer necessidade de alterar esses contratos exige atualização explícita antes do código.

Fechar não significa apenas chamar shutdown e continuar guardando o Endpoint: liberar também os owners de Host/Endpoint antes de aguardar a consulta retida, comprovando reutilização da porta nesse intervalo. Antes de retirar `group.host`, conservar a leitura cumulativa de consumo de cada convite, depois da revogação global impedir novos ingressos. Preservar `member.revoked` como histórico de revogação individual explícita; o encerramento global não transforma esse campo em true para todos. A projeção final mantém consumo/revogação histórica, com validade e transporte inativos.

## Evidências requeridas

- Consulta do monitor retida com Host e dois Guests autenticados: stop fecha TCP e UDP e impede novos fluxos antes de liberar a consulta; snapshot responde e cleanup permanece pendente. Perda do primeiro waiter e tentativa de novo preparo não destacam trabalho.
- Consulta retida durante fim de todas as autoridades fecha o Host antes de terminar; ingresso consumido com sessão válida permanece utilizável. Revogação individual durante consulta preserva o outro convidado e seus prazos.
- Gates de consulta pré/pós-abertura cobrem cancelamento e expiração sem publicação tardia; erro/pânico não publica Host válido nem perde recursos.
- Fonte substituída/perdida fecha o conjunto; stop comum não encerra processo/listener do jogo próprio.
- Regressões dos grupos manual e descoberto, três agentes locais, retorno UDP isolado e contagens causais preservadas. Sem repetição automática para mascarar perda UDP.
- Fmt/clippy/testes proporcionais, revisão independente, comandos reais, falhas e resultados no task. Fixtures/DOM não homologam janela gráfica, Minecraft LAN, dois jogadores, redes externas, Core/Skins ou áudio.

## Primeira execução local — 21/10/2026

Retomada após interrupção, sem alterações intermediárias identificadas nas fontes77. O comando `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-features --lib group_actor -- --test-threads=1` passou **3/3**, zero ignorados,9,34s de testes e1m14s de compilação. Log privado `.aurora/connect-validation/group-source-shutdown-77-first.log`.

Dois Guests reais passam STATUS e quatro origens UDP4096B sem retry. Stop fecha o TCP já aberto em até2s, antes do timeout5s da fixture; os listeners fecham e a porta Host pode ser reutilizada antes de liberar a consulta retida. Cleanup permanece pendente mesmo após perda do primeiro waiter; novo preparo é recusado e o processo de origem permanece disponível. A matriz pré/pós-bind cobre stop e expiração original de um integrante antes de publicar o roster; erro de origem e pânico controlado não deixam Host válido. A prova de sessão consumida, revogação individual e término de todas as autoridades continua pendente neste checkpoint, assim como a regressão integral.

## Provas ampliadas e regressão

O mesmo focal, com os três casos adicionais, passou **6/6**, zero falhas/ignorados,16,34s/compilação21,67s (`group-source-shutdown-77-expanded.log`). Ingressos2s consumidos continuam servindo TCP/UDP; sessão de A4s termina sem fechar B6s. O fim de todas as autoridades libera a porta antes de soltar o gate10s da consulta. Revogar A fecha seu TCP ativo em até2s, mantém STATUS/UDP de B e preserva digest/deadlines; stop global conserva `revoked=true` somente para A. Erro e pânico do monitor fecham o conjunto e liberam a porta. No ramo de origem substituída, a fixture também encerra e pode causar o EOF; o ramo de pânico conserva a fixture viva e fornece a prova complementar de corte pelo transporte.

Regressão desktop all-features/all-targets: **486 PASS/0 falhas/63 ignorados**, oito targets,246,45s de testes/compilação1m38s. Padrão sem all-features: **289 PASS/0 falhas/5 ignorados**, oito targets,65,84s/compilação9,79s. Comandos, checagens estáticas e logs finais no [task77](task-aur-r4-connect.md#incremento77--encerramento-do-grupo-durante-consulta-de-origem). Não somar filtros sobrepostos. Nenhum frontend, Java ou mod foi alterado/executado neste incremento.

## Limites e rollback

Experimental e desligado por padrão; nenhuma configuração pessoal, Java, relay público, custo, release ou instalador. Rollback operacional encerra Connect e espera cleanup completo; jogo e recibos de voz ficam independentes. Reversão local revisada preserva incrementos posteriores. Os ensaios locais propostos não exigem outro computador nem ação humana.

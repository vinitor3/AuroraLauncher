# Liberação do transporte Guest e origem de abertura pronta — incremento79

Data21/10/2026. **IMPLEMENTADO MAS NÃO HOMOLOGADO**, correção, quatro testes causais, regressão e revisão independente aprovados. O contrato foi aprovado antes da primeira prova e do código. A observação de caminho78 é trabalho independente da biblioteca; a integração de interface antes proposta como79 foi renumerada80 antes de qualquer código. Bases e resultados no [task79](task-aur-r4-connect.md#incremento79--liberação-da-origem-de-abertura-pronta-do-guest). Ler também [73](aurora-connect-ready-game-launch.md), [76](aurora-connect-owned-source-review.md), [77](aurora-connect-group-source-shutdown.md) e [encerramento](aurora-connect-stop-completion.md).

## Bloqueio auditado

Antes da correção, `ready_game::GuestSource` guardava um clone forte de `lab::Guest`, que conserva o Endpoint. O ator publicava essa origem em `Attempt.guest_source`; `finish` limpava outros recursos, mas não a removia. Stop podia concluir e fechar listeners de aplicação enquanto esse clone permanecia até substituir a tentativa ou descartar o manager. A auditoria identificou a retenção e a prova causal reproduziu a falha antes da correção, registrada abaixo. Cancelamento e `address()` já recusavam nova autorização após encerramento; não houve demonstração de reabertura de acesso.

Uma simples remoção no estado é insuficiente para a prova mais forte: o worker de abertura pronta e seu gate também guardam `Arc<GuestSource>` enquanto executam preparação ou lançamento. Eles devem continuar pertencentes ao manager e manter cleanup pendente, mas não precisam conservar um socket de transporte depois que o ator Guest encerra. Não abortar nem destacar esses trabalhos para fazer cleanup parecer concluído.

## Decisão implementada e limites

O ator passa a ser o proprietário de `Arc<lab::Guest>`. A origem de abertura pronta conserva somente `Weak<lab::Guest>`, assim como já conserva Weak da ponte TCP. A autoridade, deadline e cancelamento originais permanecem retidos; não reconstruir convite nem estender prazo. `address()` pode promover a referência apenas durante a verificação síncrona curta e devolve erro se o ator desapareceu, a conexão encerrou, a autoridade venceu ou a ponte fechou. Nenhum Arc Guest promovido cruza `backend.prepare`, espera de aprovação, `launch`, gate de fixture ou trabalho bloqueante.

Remover `Attempt.guest_source` ao encerrar e no caminho terminal idempotente. Não guardar Arc Guest ou Endpoint em Attempt, ReadyWork, gate público, snapshot ou fila. A trava final de spawn continua verificando UUID/plano/proprietário/fase/cancelamento e a origem atual sob a ordem de locks existente. Uma origem antiga ou gate retido não pode iniciar um processo após stop. Não ampliar o gate de Java, modificar o launcher padrão ou matar jogo já registrado.

O mesmo ator fecha TCP/UDP, aguarda setup/teardown próprio, descarta bridges/Guest e conserva os demais workers na barreira de encerramento. Cleanup só se torna completo quando o trabalho real terminou. Perder o primeiro waiter não libera a operação. Desligamento/expiração/revogação remota têm a mesma propriedade de liberação; perfis anteriores e recuperação UDP mantêm seus gates.

Sem comandos, DTOs, digest, capacidade, feature, dependência, lockfile, matriz ou prazos novos. Não alterar configuração pessoal, processo alheio, Java, Minecraft, recibo de voz, relay ou rede externa. Não introduzir teste baseado apenas em contagem de Arc; a evidência principal deve ser reutilização das portas reais.

## Provas necessárias

1. Antes da correção: registrar sockets reais do Guest admitido, usar TCP e encerrar sem novo preparo nem drop do manager; exigir rebind com prazo curto. Preservar a falha observada e seu código/log.
2. Após correção: mesmos sockets reutilizáveis após stop com manager vivo, fonte removida e rebind TCP/UDP pertinentes; repetir nos perfis existentes proporcionalmente ao ramo compartilhado.
3. Preparação pronta bloqueada, espera de aprovação e gate de lançamento retidos: stop fecha transporte/libera porta antes de liberar o trabalho, mas cleanup continua pendente. Perder waiter, tentar novo preparo e liberar a mesma tarefa comprova ownership, recusa de spawn tardio e ausência de renovação.
4. Jogo fixture já registrado permanece vivo depois de stop, sem impedir liberação do transporte; finalização/pânico/falha continuam controlados. Nenhum Java real é necessário para essa prova.
5. Expiração original ou revogação remota também libera recursos sem nova tentativa. Provar fechamento e rebind, sem transformar STATUS em entrada no mundo.
6. Regressões73/63/76/77, default e features pertinentes, fmt/clippy e revisão independente. Biblioteca78 terá provas próprias antes de integrar a dependência; não atribuir resultado da suíte77 ao fonte79.

## Evidências e recuperação

Primeira prova, antes da correção e da edição78 da biblioteca: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-features --lib ready_game_guest_source_stop_releases_transport_with_manager_alive_without_new_preparation -- --test-threads=1` terminou **0 PASS/1 falha**,2,17s/compilação32,00s, exit101. Log privado `.aurora/connect-validation/guest-source-cleanup-79-baseline.log`. Produção era77 `ec6d747`, com somente helper cfg(test) e teste novos. STATUS respondeu, stop concluiu, TCP ficou livre, porém rebind UDP excedeu2s enquanto o mesmo manager permanecia vivo; todas as fixtures foram encerradas antes da asserção. A falha foi preservada e a correção só foi liberada depois dela.

Revisão independente confirmou que o oráculo usa exatamente um bind `127.0.0.1:0`, após limpar os transportes IP padrão. Não há autoconflito de wildcard IPv4/IPv6 produzido pela tentativa de rebind. O teste captura somente endereços, sem clone Guest/Source ou worker que fabricaria a retenção. Não houve nova preparação nem drop do manager antes da observação.

Primeira execução corrigida: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-features --lib ready_game_guest_source_ -- --test-threads=1` passou **4 PASS/0 falhas/0 ignorados**,3,13s/compilação52,80s, exit0 (`guest-source-cleanup-79-first-green.log`). Mesmo baseline agora libera UDP/TCP com manager vivo; três gates de abertura mantidos, perda do waiter, Busy e rebind antes de liberar trabalho; filho fixture iniciado permanece vivo. Expiração original2s e revogação remota também liberam recursos sem nova tentativa. Os outros três perfis passam STATUS e abertura/fechamento da ponte UDP com rebind; esse último caso não mede payload UDP. Bytes reais são cobertos pelas regressões de transporte existentes e não serão atribuídos ao teste de portas.

Regressão final desktop: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-features --all-targets -- --test-threads=1` passou **490 PASS/0 falhas/63 ignorados**, oito targets,248,60s somados de testes/compilação1m52s (`guest-source-cleanup-79-all-full.log`). Inclui abertura pronta73, recuperação UDP63, consultas76/77 e os quatro casos novos, sem somar focais sobrepostos. `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --all -- --check` PASS. Clippy all-features/all-targets offline/locked/jobs2 com `-D warnings` PASS21,11s; padrão sem all-features PASS2,05s; somente `--features connect-lab` PASS14,95s. Logs `guest-source-cleanup-79-{fmt,clippy-all,clippy-default,clippy-connect}.log`, sequência exit0.

Somente módulos sob connect-lab foram alterados; o padrão foi recompilado/checado por Clippy, sem atribuir-lhe uma suíte de runtime79 que não foi repetida. Os289 testes padrão77 permanecem evidência histórica própria. Não houve alteração de DTO, dependência, lockfile, matriz ou frontend79. UI80 em preparação ficou fora desta validação/commit.

Root é o único executor Cargo, evitando builds concorrentes. Custos zero; testes entre computadores continuam pausados. Rollback operacional encerra Connect e aguarda seus workers; jogo já registrado e recibos de voz permanecem independentes. Reversão local revisada preserva mudanças posteriores, sem reset/clean. Nenhuma configuração pessoal, Java real, relay público, publicação ou instalador.

## O que você precisa fazer agora

Nenhuma intervenção humana é necessária para implementar e testar esta correção local.

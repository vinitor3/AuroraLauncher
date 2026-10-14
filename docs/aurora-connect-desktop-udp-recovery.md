# Recuperação manual da ponte UDP — incremento63

Data: 14/10/2026. Base `74ca76b`, branch `codex/aur-r4-connect`. **PROTÓTIPO**, `connect-lab` desligada por padrão. Escopo: recuperar somente a ponte UDP do convidado no [perfil de sessão62](aurora-connect-desktop-fragmented-session.md), conservando TCP, identidade, autorização, destino e prazo originais.

## Ação explícita e limite

Depois de falha UDP, **Tentar UDP novamente** permite até três tentativas adicionais na mesma sessão. Liberar a porta ou restabelecer o serviço não dispara tentativa automática. O perfil curto57 não recebe esse comportamento. A ação não altera configuração do mod, não renova credenciais de voz e não representa retorno de áudio.

O comando `connect_retry_session_udp` recebe somente `operationId` e `expectedGeneration`; o proprietário vem da janela principal. A operação precisa continuar como convidado autenticado do perfil schema3, com UDP em falha e a autoridade original de sessão vigente. Ingresso vencido não impede recuperação de uma sessão já admitida. Stop, sessão vencida, operação substituída ou fechamento de transporte já observado impedem novo trabalho. A revogação remota só pode ser observada depois de chegar ao convidado: não há garantia síncrona sobre uma ação ainda desconhecida. Porta35554, serviço aux/revisão1, capacidade4096B e orçamento assinado permanecem os mesmos.

Há no máximo uma preparação de recuperação pendente, em fila limitada. A tentativa é reservada atomicamente antes de o worker agir; comandos duplicados da mesma geração já aceita devolvem a observação atual, sem contar ou abrir de novo, somente enquanto a sessão original continua válida. Uma geração antiga que não corresponde à tentativa aceita não autoriza novo trabalho. A interface não reenvia automaticamente uma chamada cuja resposta se perdeu: reconcilia por consulta de estado.

## Estado e encerramento

A capability de `sessionFragmentedUdpInvitations` anuncia `udpRecovery.maxAttempts=3`. O snapshot do convidado autenticado pode conter `udpRecovery:{attemptsUsed,maxAttempts,pending}`. A contagem não inclui a primeira preparação automática já pertencente à conexão aprovada. Perfis anteriores e o build padrão não recebem esses campos. Encerramento conserva a contagem observada, com `pending=false`.

O resultado imediato de uma recuperação aceita mostra UDP preparando; o resultado efetivo é publicado pelo worker. A conexão TCP e a consulta STATUS conservam sua própria vida. Se a porta continuar ocupada, a nova falha consome uma tentativa; não há busca de porta alternativa ou fallback. Publicação do resultado pertence à sequência de recuperação, não apenas à geração global que STATUS também pode incrementar.

Qualquer ponte UDP anterior é encerrada e aguardada antes de abrir outra. A espera de setup/teardown conserva cancelamento, conexão e ambos os relógios originais; stop/logout/saída aguardam a mesma future, incluindo resultado tardio. A liberação de um resultado depois de encerrar não pode voltar a publicar UDP disponível. Nenhuma tarefa abandona um listener para permitir outro bind concorrente.

## Validação

Interface: **72 PASS,0 falhas/ignorados**,1514,8395ms, log privado `session-udp-63-frontend-final.log`, conferido pelo integrador. São60 regressões62 e12 casos novos: clique explícito, limite3, preservação TCP/prazos, ingresso vencido versus fim da sessão, resposta IPC perdida, snapshots falhos/inválidos/antigos, remount, clique duplicado, stop com resposta tardia e metadados cruzados. Uma observação válida inalterada permite somente outro clique manual com a mesma geração, cujo efeito nativo é idempotente; não prova que uma chamada anterior ocorreu.

Build TypeScript/Vite passou em `desktop-udp-recovery-63-build.log`, com aviso anterior de chunk>500kB. O12º teste foi acrescentado depois do build sem alterar o fonte de produção. Revisão independente da interface não encontrou bloqueador restante. Comandos realmente executados na raiz da worktree:

```powershell
node --test apps/desktop/src/features/connect/ConnectView.smoke.mjs apps/desktop/src/features/connect/fragmented-types.test.mjs
npm --prefix apps/desktop run build
```

Foco nativo final: **7 PASS,0 falhas/ignorados**,197filtrados,11,86s e16,25s de compilação; log `desktop-udp-recovery-63-focused-final.log`. Primeiro foco7/7,11,75s, antes de reforçar dois oráculos de testes, sem mudança de produção. Não somar rodadas.

- Dois managers reais, falha inicial por porta ocupada, liberação sem retry automático, ingresso vencido e recuperação solicitada. TCP aberto antes da recuperação continua no mesmo endereço/stream; dois remetentes UDP retornam4096B íntegros com origens distintas e ausência de retorno adicional cruzado. Prazo de sessão original conservado.
- Três falhas adicionais quando a porta continua ocupada; quarta recusada, pedidos duplicados gastam apenas uma tentativa. Proprietário, operação, geração, papel host, fase encerrada e perfis anteriores não ampliam escopo. Todos os perfis antigos mantêm TCP e ausência de metadados de recuperação.
- Barreira privada de teste retém o resultado de uma ponte UDP realmente aberta antes da publicação. Stop, revogação e prazo original fecham TCP antes de liberar a barreira; stop ainda aguarda a mesma future e não declara cleanup completo cedo. Resultado tardio não publica sucesso, e os sockets próprios podem ser reabertos.
- No caso de revogação, antes de liberar o resultado a fixture confirma snapshot ainda `guestConnected`, token ainda não cancelado e relógios originais válidos. Mesmo assim a conexão já fechada faz a repetição ser recusada sem mudar geração/contagem. Isso isola o gate da conexão antes do watcher; não representa conhecimento antecipado da revogação remota.
- Depois de recuperação bem-sucedida, atividade TCP/UDP não prolonga a sessão; expiração4s fecha os aplicativos e libera portas.

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --features connect-lab --lib engine::connect::live::udp_recovery_tests -- --test-threads=1
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --all -- --check
git diff --check
```

QA independente revisou código e oráculos sem bloqueador restante. Regressões completas: **352 PASS/42 ignorados combinado** e **265 PASS/5 ignorados padrão**, zero falhas,9grupos por execução,142,80s/43,86s somados de testes e1m39s/37,84s de compilação. Logs `desktop-udp-recovery-63-full-{combined,default}.log`. Incluem recovery/CAS/Doctor; opt-ins históricos ignorados não foram ativados. Clippy de todos os targets passou em padrão/Connect/combinado (`desktop-udp-recovery-63-clippy-{default,connect,combined}.log`), assim como fmt/diff. Locks, matriz e pins53 imutáveis foram reconferidos por SHA-256, sem alteração. Nenhum Java, arquivo pessoal, relógio do sistema, firewall ou rede externa foi usado; fixtures de transporte/DOM não homologam áudio ou janela Tauri.

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-features -- --test-threads=1
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 -- --test-threads=1
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-targets -- -D warnings
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-targets --features connect-lab -- -D warnings
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-targets --all-features -- -D warnings
```

## Limites e rollback

Avança o comportamento de transporte do T-MIX-03; restauração de voz/credencial/rota real permanece separada para Simple Voice Chat e Plasmo Voice. Não descobre processo de mod nem controla Java. Convites entre redes, NAT/CGNAT, dois jogadores, contas mistas, Core/Skins e estabilidade prolongada não ganham validação indireta.

Rollback operacional: Encerrar Connect e aguardar limpeza completa, preservando o jogo independente. Rollback de código: revert local revisado somente63, mantendo61/62 e os perfis anteriores. Nenhuma configuração pessoal a restaurar; sem custo, push, deploy, release, publicação ou instalador.

# TCP e UDP fragmentado no desktop — incremento 57

Data: 09/10/2026. Base: `51cc5d8667d0da226b923e43baa74f66be080d58` (incremento56). Estado: **PROTÓTIPO**, sob `connect-lab` desligada por padrão. Este passo oferece um perfil de transporte explícito no caller/painel desktop, sem modificar configuração de jogo ou mod. Complementa [admissão51b](aurora-connect-fragmented-admission.md), [UDP49](aurora-connect-fragmented-udp.md), [caller desktop](aurora-connect-desktop.md) e [sessão56](aurora-connect-desktop-session-lifetime.md).

## Perfil e aprovação

`fragmentedGameUdp` usa `Invitation` de **schema1**, envelope **UDP V2**, TTL inteiro de1–120s para ingresso e sessão inteira. Não é `SessionInvitation` schema2 e não autoriza uma sessão UDP de uma hora. Perfil curto e sessão TCP56 continuam opções independentes. Selecionar um perfil fixa o verificador/autoridade antes de colar o convite, sem inferência ou fallback de formato.

Comandos novos:

- `connect_prepare_fragmented_host(request)`: `{mode,bindAddress,gameTarget,udpTarget,lifetimeSeconds}`. Ambos os destinos de aplicação são literais loopback canônicos com porta não zero; o bind do transporte segue o modo loopback ou IPv6 externo escolhido explicitamente.
- `connect_prepare_fragmented_guest(request)`: `{mode,expectedHostPublicId}`, com pin independente do anfitrião. A ponte TCP usa `127.0.0.1:0`; a ponte UDP usa exclusivamente `127.0.0.1:35554`.

Preparação/revisão mantêm o prazo original de60s, limitado adicionalmente ao convite, com gates civil e monotônico. Confirmar exige digest exato. Host e Guest retêm o objeto verificado em RAM e os timers originais; atividade não renova o prazo. APIs de emissão `issue_fragmented_for_lifetime` e `issue_fragmented_external_ipv6_for_lifetime` usam a mesma leitura temporal para `issuedAt` e vencimento. APIs absolutas e assinaturas antigas permanecem iguais. O desktop exige segundos inteiros exatos desde emissão, recusando um convite originalmente longo mesmo perto de vencer.

Os serviços assinados e aprovados são exatamente:

| Serviço | Escopo |
| --- | --- |
| `game` | TCP, revisão1, `reliableStream`, campo genérico `maxDatagramBytes=1400`, `ReplyBudget`. |
| `aux` | UDP, revisão1, `fragmentedDatagram`, original até4096B, `AuthorizedDownstream` de16384B/s e20frames/s. |

Cada fragmento consome quota, incluindo seu header;20frames/s não significa20datagramas originais de4096B/s. A política autoriza retornos limitados após o primeiro envio válido, isolados pela identidade, sessão, serviço, associação e origem local. Não autoriza destino descrito no payload. Teto do frame completo1264B inclui32B de header; não é uma MTU física medida. Nenhum retorno é entregue parcialmente.

O perfil não identifica Simple Voice Chat ou Plasmo Voice: nenhum mod, JAR ou configuração está assinado nesse convite. Usar o nome de um mod como se tivesse sido autenticado seria incorreto. Voz continua desativada nos estados de produto, mesmo quando a ponte UDP está disponível. Anúncio de endereço, autenticação do mod, configuração com snapshot/restauração e áudio real continuam gates separados.

Este incremento permite somente destinos manuais. A prova de processo da descoberta56 refere-se ao jogo TCP e não autoriza automaticamente um endpoint UDP. Não foi criado comando de descoberta fragmentada.

## DTOs e interface

`capabilities.fragmentedUdpInvitations` é opcional e ausente no build sem transporte. Contém `profile=fragmentedGameUdp`, `invitationSchemaVersion=1`, `udpEnvelopeVersion=2`, `maxLifetimeSeconds=120`, `approvedOriginalDatagramBytes=4096`, `maxFragmentFrameBytes=1264`, `localUdpBind=127.0.0.1:35554` e `udpReturnPolicy={type:authorizedDownstream,bytesPerSecond:16384,datagramsPerSecond:20}`.

Preparation e Snapshot apresentam `invitationProfile=fragmentedGameUdp`; não há `sessionTiming`. Review acrescenta `fragmentedUdp` com o contrato acima sem `maxLifetimeSeconds`, mais `udpTarget` do host ou `null` no convidado. Export e Snapshot, desde a revisão, carregam o contrato sem `udpTarget`/`maxLifetimeSeconds`. Os destinos privados continuam fora do convite/export. Revisão/digest cobrem destinos locais, serviços, limites, duração e perfil. Todos os campos novos são omitidos nos perfis antigos, preservando sua forma JSON/digest.

Somente snapshots do **convidado fragmentado** apresentam os cinco campos:

| Campo | Regra |
| --- | --- |
| `udpState` | `notStarted`, `starting`, `available`, `failed` ou `closed`. |
| `udpAddress` | `127.0.0.1:35554` somente em `available`; caso contrário `null`. |
| `udpErrorCode` | `null` ou código fechado `connect_udp_setup_failed`, `connect_udp_capacity_unavailable`, `connect_udp_closed`. |
| `udpOriginalCapacity` |4096 somente em `available`; caso contrário `null`. |
| `udpCapacityScope` | `initialAssociation` somente em `available`; caso contrário `null`. |

O host omite todos os cinco campos: ele não tem uma ponte local de convidado. O teto anunciado corresponde à primeira associação negociada e ainda é limitado pela capacidade atual; não promete capacidade de todas as origens futuras nem entrega de áudio. `UdpBridge::is_closed` consulta cancelamento/tarefa encerrada, sem testar rede por inferência.

O painel valida esses contratos e mantém TCP/STATUS disponíveis quando somente UDP falha. Este registro não equivale a ensaio gráfico de uma janela Tauri: fixtures DOM e comandos compilados são evidências separadas.

## Falha independente e encerramento

Depois da admissão autenticada e da criação da ponte TCP, o caller publica `guestConnected` com o endereço TCP. Para o perfil fragmentado, publica `udpState=starting` antes de aguardar `openFragmented`/bind UDP. Porta35554 ocupada, recusa ou capacidade insuficiente produzem falha somente UDP: não fecham TCP nem desabilitam STATUS. Não há nova porta, downgrade, tentativa automática ou relay.

O caller exige4096B na associação inicial antes de publicar `available`. Durante a sessão, monitor periódico observa fechamento da ponte e redução/ausência da capacidade; fecha somente a ponte UDP e publica seu código limitado. Falha de transporte autenticado, revogação, vencimento, stop ou shutdown continuam fechando o conjunto. A resolução do monitor depende do escalonamento; não há promessa de tempo real.

Cancelar durante conexão/setup não abandona a future dona do endpoint/ponte. Durante setup e teardown UDP pendentes, um helper privado observa cancelamento, deadline monotônico, verificação civil periódica e fechamento da conexão em paralelo: fecha o Guest para cortar TCP e desbloquear pedidos, depois recolhe a **mesma** future. Um resultado tardio não publica UDP disponível. Falha somente UDP com sessão ainda vigente continua preservando TCP. `stop`/logout/saída aguardam os recursos; `Drop` cancela e oferece limpeza eventual, conforme56, sem substituir o join explícito. O encerramento limpa endereço/capacidade UDP, preservando um erro anterior quando existente.

## Provas locais e orçamento

Os novos testes do caller usam dois managers reais, QUIC autenticado, listeners próprios e destinos TCP/UDP em loopback. A porta35554 é parte do contrato aprovado; nenhum processo pessoal é encerrado para liberá-la. As demais portas são efêmeras. Execução sequencial evita concorrência entre casos que usam esse listener fixo.

- Positivo:1MiB TCP por direção, comparação byte a byte e half-close, cadenciado enquanto duas origens UDP permanecem abertas. Cada origem envia4096B, tenta4097B recusados e depois envia64B válidos. O endpoint tenta também retorno4097B antes de devolver o4096B correto. Buffers de65536B detectam excesso sem truncar. São **33028B de payload UDP tentado**, somando ambos os sentidos e oversize, abaixo do teto64KiB por teste. Endpoint recebe8320B válidos; duas associações distintas e ausência de retorno cruzado são exigidas. Progresso TCP durante UDP e total1048576B no endpoint são conferidos.
- Porta ocupada:35554 reservada pela própria fixture produz `setup_failed`, mantém `guestConnected`, permite STATUS/protocolo767 e ecoTCP real. Liberar a porta não dispara retry. O canário UDP permanece sem payload.
- Ciclo: revogação, fim original de2s e Drop do Host fecham as duas aplicações TCP, a ponte UDP e liberam binds. Solicitação UDP de1B/retorno de2B exercita a política downstream explícita; não é tráfego de mod. EOF/reset do destino é observado antes de abortar a fixture.
- Negativos: perfil legado/schema2, assinatura adulterada, prazo original121s, fração1500ms, retorno não autorizado/teto divergente e digest errado recusados antes de rede ou destinos. Validação de DTO recusa campos adicionais, ausentes e numéricos inválidos.
- Cancelamento durante dial: um socket UDP próprio silencioso observa a tentativa real; stop aguarda falha limitada, não publica TCP/UDP e permite rebind do endpoint.
- Espera UDP controlada: três fixtures mantêm dois agentes, TCP ativo e uma ponte UDP real retida em um resultado ainda pendente. Cancelamento, deadline monotônico ou gate civil injetado precisam fechar os dois sockets TCP **antes** de liberar o resultado. A mesma future permanece viva, termina exatamente uma vez e devolve a ponte já fechada; encerramento aguardado e drop permitem rebind. É prova causal do helper de produção com resultado controlado, não um ensaio de ACK malicioso bruto, alteração do relógio do Windows ou indução física de PMTU.

Teste independente `apps/connect-lab/tests/udp_bridge_lifecycle.rs` cobre o getter novo e fechamento UDP com handle retido, sem derrubar TCP; também observa fechamento por Host/Guest e rebind. Tem orçamento24576B UDP em suas três fixtures, sem Java ou rede externa. Isso não induz redução física de MTU nem prova a cadência de qualquer mod.

Check inicial: **PASS**,5,10s (`desktop-udp-57-check-first.log`). Primeiro foco integrado: **28PASS/0FAIL/0ignorados**,64,57s (`desktop-udp-57-focused-first.log`), incluindo5novos testes de managers,3helpers e20regressões56/legadas. O módulo de testes do helper foi movido ao fim do arquivo por organização de lint, sem mudança de lógica. Depois da correção das esperas e do novo oráculo causal, o foco final passou com **29PASS/0FAIL/0ignorados**,65,00s (`desktop-udp-57-focused-final.log`). Lifecycle corrigido: **2PASS**,0,26s (`desktop-udp-57-library-lifecycle-final.log`).

Revisão encontrou uma espera não vigiada depois de publicar TCP: um setup UDP pendente podia atrasar a observação de stop/prazo. A correção acima também protege o teardown de uma ponte cujo pedido de associação ainda esteja pendente. O oráculo causal foi acrescentado; não houve aumento de prazo nem abandono da future. A primeira rodada independente de lifecycle teve **0PASS/2FAIL**, pois tentava rebind do endpoint de transporte ainda com handles Host/Guest retidos. Os oráculos anteriores de `is_closed`, rebind da ponte com handle retido e TCP sobrevivendo tinham passado. A fixture foi corrigida para consumir/dropar todos os handles de transporte antes do rebind desse endpoint; não houve alteração da biblioteca para forçar o teste. Log preservado: `desktop-udp-57-library-lifecycle.log`. Essa falha interrompeu a sequência antes das suítes completas.

## Execuções finais

Todos os comandos abaixo foram executados na raiz da worktree, sequencialmente, sem Java nem tráfego externo. Logs privados em `.aurora/connect-validation/`, prefixo `desktop-udp-57-`. Contagens focais e completas se sobrepõem e não devem ser somadas como casos diferentes.

| Execução | Resultado final | Log |
| --- | --- | --- |
| Foco caller `engine::connect::` | 29 PASS, 0 ignorados; 65,00s | `focused-final.log` |
| Lifecycle independente UDP | 2 PASS, 0 ignorados; 0,26s | `library-lifecycle-final.log` |
| Desktop default | 153 PASS, 3 ignorados; 27,06s | `default-full.log` |
| Desktop `connect-lab` | 184 PASS, 4 ignorados; 94,44s | `connect-full.log` |
| Desktop com ambas as features | 216 PASS, 19 ignorados; 100,60s | `combined-full.log` |
| Biblioteca default, todos os alvos | 213 PASS, 4 ignorados; 26 grupos | `library-default-full.log` |
| Biblioteca com `local-relay-lab`, todos os alvos | 223 PASS, 5 ignorados; 26 grupos | `library-relay-full.log` |
| Perfil após ajuste equivalente de lint | 3 PASS, 0 ignorados; 0,06s | `profile-final.log` |

Todas as execuções finais acima têm zero falhas. Ignorados continuam sendo os ensaios explicitamente condicionados a artefatos, processo/Java, serviços externos ou interfaces próprias, além da fixture filha de descoberta, acionada pelos testes pais. O build relay exercita somente seu laboratório local; não aprova provedor gratuito nem significa relay habilitado no desktop.

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --lib --features connect-lab engine::connect:: -- --test-threads=1
cargo test --manifest-path apps/connect-lab/Cargo.toml --target-dir .aurora/connect-validation/relay50-target --offline --locked --jobs 2 --test udp_bridge_lifecycle -- --test-threads=1
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --lib -- --test-threads=1
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --lib --features connect-lab -- --test-threads=1
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --lib --all-features -- --test-threads=1
cargo test --manifest-path apps/connect-lab/Cargo.toml --target-dir .aurora/connect-validation/relay50-target --offline --locked --jobs 2 --all-targets -- --test-threads=1
cargo test --manifest-path apps/connect-lab/Cargo.toml --target-dir .aurora/connect-validation/relay50-target --offline --locked --jobs 2 --all-targets --features local-relay-lab -- --test-threads=1
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --lib --features connect-lab engine::connect::fragmented_profile::tests:: -- --test-threads=1
```

Checks `--all-targets` do desktop passaram nos modos default, `connect-lab` e combinado, respectivamente13,18s/11,83s/10,96s (`check-default.log`, `check-connect.log`, `check-combined.log`). O primeiro Clippy combinado recusou a expressão manual de múltiplo de1000 (`clippy-combined.log`). A única mudança posterior aos fulls foi substituir `%1000 !=0` por `!is_multiple_of(1000)`, com a mesma regra de segundos inteiros. Os3testes do perfil foram repetidos; Clippy final default/combinado passou em4,10s/11,96s (`clippy-default-final.log`, `clippy-combined-final.log`). Não houve alteração de protocolo, timer, prazo ou fluxo que justificasse repetir todos os fulls.

Clippy da biblioteca default/relay passou em8,99s/8,45s (`library-clippy-default.log`, `library-clippy-relay.log`). `fmt --check` das duas crates e `git diff --check` passaram; logs `fmt-desktop.log`, `fmt-library.log`, `diff-check.log`. Comandos realmente executados:

```powershell
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-targets
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-targets --features connect-lab
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-targets --all-features
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-targets -- -D warnings
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-targets --all-features -- -D warnings
cargo clippy --manifest-path apps/connect-lab/Cargo.toml --target-dir .aurora/connect-validation/relay50-target --offline --locked --jobs 2 --all-targets -- -D warnings
cargo clippy --manifest-path apps/connect-lab/Cargo.toml --target-dir .aurora/connect-validation/relay50-target --offline --locked --jobs 2 --all-targets --features local-relay-lab -- -D warnings
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --all -- --check
cargo fmt --manifest-path apps/connect-lab/Cargo.toml --all -- --check
git diff --check
```

Freeze privado das10fontes nativas/biblioteca alteradas e deste domínio: `.aurora/connect-validation/desktop-udp-57-source-freeze.json`, com SHA-256 de cada arquivo. Exclui trabalhos58 ainda não integrados, documentação compartilhada de outros responsáveis e fontes frontend verificadas por seu responsável. Os dois lockfiles, a matriz ativa e o binário privado53 foram conferidos sem alteração; o pin53 continua `adcc23b75c5b4222fba582c18ccf4147c46e984197e9724e9dae1a4cd26b0423`. Nenhum kit histórico foi substituído.

## Limites e rollback

TCP e UDP fragmentado estão implementados neste perfil experimental de caller; evidência local não homologa uso público. Conexão direta externa, novo convite entre redes e janela Tauri continuam pendentes. Relay permanece desligado. Minecraft real não foi executado57; STATUS sintético não significa entrada em mundo, LAN ou jogador conectado. Simple Voice Chat e Plasmo Voice continuam sem homologação de áudio, cada um com endereçamento/recuperação próprios a integrar. Contas mistas, dois jogadores gráficos, os nove pares Core/Skins, estabilidade e sincronização segura de modpacks mantêm seus gates.

Nenhum arquivo de configuração pessoal foi alterado. Rollback operacional: Encerrar Connect, aguardar `cleanupState=complete` e conservar o jogo independente. Rollback de código: revert local revisado do incremento57, preservando56/53/54 e trabalhos concorrentes. Builds sem `connect-lab` mantêm o recurso indisponível. Não alterar kits/pins históricos, instâncias, snapshots ou worktrees. Não houve push, deploy, release, instalador ou custo.

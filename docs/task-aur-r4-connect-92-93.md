# Aurora Connect — sessões sociais, abertura de jogo e ponte nativa de voz

07/11/2026. Estado: **PROTÓTIPO**, checkpoint local validado no escopo abaixo;
produto completo e homologação multiversão permanecem pendentes.
Continuação de [90/91/86A1](task-aur-r4-connect-90-91.md), commit local revisado
`0bca0ea`. Mesma worktree, nove pares, Core1.1.5/Skins0.3.1/alpha.14 e limites de
publicação/custo. O rascunho de voz preexistente e as outras worktrees permanecem
preservados. Sem push, deploy, instalador ou mudança de configuração pessoal.

## Implementação

- [92: origem social para abrir cópia pronta](aurora-connect-social-ready-game-92.md).
  O mesmo worker conserva grant, continuidade e convite originais. A fonte de
  lançamento é fraca, não prende Guest/Endpoint, e o gate final serializa spawn
  com revogação e epoch da conta. O jogo já registrado é preservado ao encerrar
  Connect. **Abrir Minecraft** navega à revisão existente; não autoriza nem
  inicia automaticamente o processo.
- [93: Fabric1.20.1/Java17](aurora-connect-ready-game-fabric-93.md) adiciona um
  segundo perfil fechado à cópia pronta. Fabric1.21.1/Java21 permanece. Hashes,
  Java, Core/Skins, arquivos, argv e consentimento continuam obrigatórios. Os
  outros sete pares ainda precisam de adaptadores de lançamento próprios;
  reconhecer STATUS para os nove não remove esses gates.
- 86A2/B: confirmação/Host privados a partir dos objetos originais86A1,
  monitor serializado e retirada terminal de UDP por participante/serviço,
  preservando TCP quando a origem de jogo continua válida. Não cria voz própria
  nem expõe uma integração pública de áudio por consequência.

Para testar retenção real dos owners81 entre crates, foi acrescentada somente
uma feature local de suporte a testes no `aurora-connect-lab`, ativada como
dev-dependency do desktop. API, campos e checkpoints exigem Windows + feature +
`debug_assertions`; a dependência normal não a habilita. O gate opaco conserva
somente sinais/endereço observado, sem Endpoint/socket/Host/closure/provider e
Drop libera a espera. Debug all-targets/testes ou ativação explícita da feature
incluem esses hooks; não chamar isso de ausência em toda combinação imaginável
de build. Nenhum pacote externo, versão ou lockfile foi alterado.

## Execução observada

`cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked
--jobs 2 --all-features --tests` passou22,84s, inicialmente com uma variante de
motivo não usada; a variante foi removida antes dos testes86.

Com o mesmo manifesto/flags, `cargo test --lib native_voice_host --
--test-threads=1` passou **2/2**,57,07s (compilação1m37s). São seis combinações de
perfil/cenário: ambos os perfis no teste positivo, e ambos em AfterConnect e
Completed no teste de retenção. Dois Guests reais, duas origens UDP por Guest,
datagramas4096B com retorno marcado e TCP em progresso foram exercitados. Mudança
de configuração retirou UDP sem reativá-lo após restauração; TCP novo continuou.
Outro teste reteve o owner real da primeira lease, conferiu retirada da segunda,
depois liberou/recolheu o owner e verificou rebind da porta exata. Stop com
waiter perdido liberou transporte antes de terminar a query de origem retida.
São fixtures de processo/socket reais, sem Java, áudio ou mod autenticado.
Log privado: `.aurora/connect-validation/resume86b-host-first.log`.

Após revisão independente, o ator passou a recusar geração/revisão divergentes,
e as fixtures passaram a observar conclusão real da tarefa no empate de prazo e
a recolher a thread externa também em falha. `cargo check` all-features/tests
passou12,57s. O filtro `native_voice_host` consolidado passou **7/7**,169,55s
(compilação1m18s), incluindo os dois pais anteriores e cinco novos. São18
combinações de perfil/cenário, sem somar os dois PASS antigos novamente: Busy do
job original, cancelamento pré/pós-bind, validade separada de ingresso/sessão,
timer com resultado pronto e perda de UDP distinta de perda TCP. Log
`.aurora/connect-validation/resume86b-host-cases-final.log`. Gates adicionais
localmente testáveis continuam explícitos no [contrato86A2](aurora-connect-native-voice-transport-86a2.md).

Primeira rodada `cargo test --lib connect_ready -- --test-threads=1`: **17 PASS,
3 FAIL,2 ignorados**,17,98s. A política anterior recusava o argumento oficial
LWJGL de extração de nativos presente em1.20.1. A correção aceita as quatro
propriedades nativas oficiais somente quando o valor corresponde exatamente ao
diretório de nativos verificado da instância; aliases, caminhos alternativos,
listas, valores vazios e propriedades conflitantes continuam recusados.
Reteste: `connect_ready` **20 PASS/2 ignorados**,17,43s; `connect_quick_play`
**9/9**,0,92s; `engine::connect::live::ready_game` **15 PASS/1 ignorado**,9,97s;
`engine::connect::live::social` **20 PASS/2 ignorados**,1,47s. Todos com os mesmos
flags offline/locked/jobs2/all-features/lib/test-threads1. Ignorados são fixtures
ou opt-ins declarados; não contam como PASS.

Separadamente foi executado o opt-in exato
`engine::connect_ready_runtime::tests::fabric1201::ready_fabric1201_actual_java17_probe_under_retained_runtime_pins`
com `--ignored --exact --test-threads=1` e
`AURORA_CONNECT_READY_JAVA17=C:/Program Files/Eclipse Adoptium/jdk-17.0.20.101-hotspot/bin/java.exe`:
**1 PASS**,4,75s. O processo real executou somente `-version` sob os pins da
prova; nenhum Minecraft. Log `.aurora/connect-validation/resume93-java17-real.log`.

Auth/Firestore Emulator local foi iniciado novamente com
`firebase/social-emulators.json`, projeto `demo-aurora-social`, loopback. Helper
congelado com SHA256 `bcda0d8a8bfaffb3666f3204c988510ffea96edd3be04ae227ae9a6e6168327d`.
Com `AURORA_SOCIAL_DESKTOP_INTEGRATION=1` e caminho absoluto em
`AURORA_SOCIAL_DESKTOP_TEST_BINARY`, o comando `node --test --test-concurrency=1
firebase/tests/social-desktop.integration.test.mjs` passou **11/11** (dez casos e
pai),22,802s. Quatro casos92 novos: spawn de filho nativo próprio e preservação
após Stop; epoch alterada com cancel original ainda falso e recusa observada no
ramo exato do gate final; preparo retido com waiter perdido, porta livre e slot
ainda Busy até liberar a tarefa; bloqueio real no backend antes do spawn.
Os casos89/90 de TCP/bloqueios/descoberta continuaram passando e não são somados
como provas novas. Child de teste não é Minecraft. Emuladores encerrados com
um Ctrl+C/shutdown controlado antes de novos testes de voz.

Rodada final com o caso causal de expiração original acrescentado: **12/12**
(onze casos e pai),27,995s, sem falha/ignorados. O helper congelado tinha SHA256
`028fc068fc283d600c775a237fd3b6d13bf009a920d0c77a757f567f2823e603`.
O grant, o convite e a ponte permaneceram vivos, sem cancelamento original,
enquanto a continuidade original venceu. Uma lease assinada posterior ainda
válida não reativou essa autoridade; zero callbacks de lançamento, encerramento
e rebind confirmados. Log `resume92-social-desktop-integration-final.log`;
emuladores encerrados com SIGINT/shutdown controlado. Os12 incluem os11 anteriores.

Primeiro Clippy all-targets/all-features recusou enum com tamanhos muito
diferentes, tratamento de erro por if-let e clone desnecessário de fixture.
Correções estreitas: Box conserva a autoridade manual original; `?` conserva
o mesmo oracle de epoch apenas em teste; slice usa referência. Sem supressão de
lint nem mudança de política de admissão. A execução E2E acima antecede essas
correções de lint; a regressão subsequente fica registrada separadamente.

Reteste Clippy com `--offline --locked --jobs 2 --all-targets -- -D warnings`:
**PASS** em all-features23,72s, padrão13,64s e `--features connect-lab`22,96s.
Logs `resume92-93-clippy-{all,default,connect}-fixed.log`. `cargo fmt --all`
aplicado apenas aos arquivos próprios; nenhum warning foi silenciado.

Regressão padrão executada: `cargo test --manifest-path
apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-targets --
--test-threads=1` passou **298 testes/0 falhas/5 ignorados**, oito targets,
67,11s somados de testes e1m39s de compilação. Inclui Recovery, Doctor e módulos
gerenciados; não equivale a execução completa all-features. Log
`resume92-93-default-tests.log`.

Após os ajustes de lint, regressões focadas all-features/lib/test-threads1:
`engine::connect::live::ready_game` **15 PASS/1 ignorado**,9,95s;
`engine::connect::live::social` **20 PASS/2 ignorados**,1,50s;
`connect_ready` **20 PASS/2 ignorados**,17,46s. A primeira recompilação levou
1m05s. Logs `resume92-93-post-lint-*.log`; filtros se sobrepõem aos testes
anteriores e não ampliam a alegação de homologação.

Regressão81 na própria biblioteca: `cargo test --manifest-path
apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --all-features --lib
host::udp_service_tests -- --test-threads=1` passou **23/23**,3,35s,
compilação41,58s. Os testes anteriores de retirada/owners/socket/revogação e
expiração permaneceram válidos com o suporte novo vazio. Log
`resume86b-udp81-regression.log`.

Encerramento dos checks: fmt `--check` desktop e connect-lab passaram; Clippy
da biblioteca `--offline --locked --jobs 2 --all-features --all-targets --
-D warnings` passou17,47s (`resume86b-lab-clippy.log`). Revisões independentes
conferiram o lifecycle, os oráculos e a distinção nove STATUS/dois ReadyGame.
`git diff --cached --check` passou; hashes da matriz, contratoSkins, lockfiles
e proposta de voz preexistente permaneceram iguais. Nenhum processo próprio de
teste/emulador ficou ativo. Não foi executado um full all-features novo nem
revalidada toda a cadeiaV1/84/85 por esta rodada.

Frontend inicial: os seis arquivos de teste de Social/Connect/ReadyGame/account
transition passaram **323/323**,4,305s; `npm run build` passou,571ms, com aviso
existente de tamanho do chunk. O botão de navegação92 foi acrescentado depois
dessa rodada; sua rodada focal SocialView/controller/ConnectView passou
**275/275**,4,224s. Totais se sobrepõem e não são somados. A regressão final do
estado consolidado ainda será registrada.

Rodada consolidada dos cinco arquivos SocialView/controller/ConnectView/
ReadyGameLaunch/ReadyGameLaunchProfiles: **307 PASS/0 ignorados**,8,684s,
`node --test --test-concurrency=1` com os caminhos desses arquivos. `npm --prefix
apps/desktop run build` passou, Vite567ms, mantendo o mesmo aviso de chunk. O
arquivo account-transition não está nesse total; sua rodada anterior permanece
separada. Logs `resume92-93-frontend-final.log` e `resume92-93-build-final.log`.

A nova tentativa de usar Computer Use foi interrompida fisicamente por Esc antes
de selecionar/operar uma janela. Nenhuma ação posterior na interface foi feita.
A GUI90 já observada conserva seu resultado próprio; o novo botão92 e o runtime93
não recebem validação gráfica por essa evidência anterior.

## Limites e encerramento

Arquivos principais alterados: `apps/desktop/src-tauri/src/engine/connect/`
(`social*`, `ready_game*`, `native_voice*`), `commands_ready_game.rs`,
`connect_ready_{profile,runtime,files}*`, `connect_quick_play.rs`, fixtures
`connect_dedicated/tests/`, `apps/connect-lab/src/host*` e seus manifests;
`apps/desktop/src/App.tsx`, componentes/tipos/testes Social/Connect/ReadyGame;
`firebase/tests/social-desktop.integration.test.mjs`; AGENTS, backlog/matriz
canônicos e documentos90–93/86A2. O contrato/helper94 permanece preparado,
isolado e não registrado; não faz parte da entrega92/93.

| Área | Estado deste incremento |
| --- | --- |
| TCP | PROTÓTIPO: bytes locais autenticados, entrada social, bloqueios, stop e rebind testados. |
| UDP | PROTÓTIPO privado: dois participantes, origens isoladas, retorno4096B e retirada terminal testados; demais gates86 pendentes. |
| Conexão direta | Evidência externa histórica preservada; nenhuma nova rede homologada nesta wave. |
| Relay | Público continua desativado; sem contratação, deploy ou nova validação. |
| Minecraft | Novo perfil pronto Fabric1.20.1/Java17 validado com arquivos/argv e probe Java; não houve entrada no mundo nesta wave. |
| Convites/interface | Entrar público/amigos usa a autoridade nativa original; Abrir Minecraft navega à revisão. GUI92 pendente; GUI90 tem prova própria. |
| Simple Voice Chat | Transporte/retirada privados testados no perfil fechado; áudio real e integração gráfica pendentes. |
| Plasmo Voice | Transporte/retirada privados testados separadamente; áudio real e integração gráfica pendentes. |

TCP e UDP acima são provas locais autenticadas; direto externo e relay mantêm
as evidências/gates anteriores. As duas revisões de voz continuam separadas:
Simple Voice Chat e Plasmo Voice não receberam validação de áudio. Nenhum mundo
Minecraft foi aberto nesta wave nem célula Core/Skins foi homologada por build.
O serviço social de produção, grupos dinâmicos, pedidos de ingresso, mais redes
e sincronização segura de modpacks permanecem no backlog canônico.

Rollback: encerrar Connect e esperar recursos liberados; o dono do dedicado
decide separadamente stop/reap/recibo/EULA. Reverter apenas commits próprios em
revisão local, sem reset/clean nem modificação de instâncias pessoais.

## O que você precisa fazer agora

Nenhuma intervenção é necessária para este checkpoint local. O novo teste
gráfico permaneceu interrompido por Esc; as pendências locais e externas acima
continuam abertas, sem declaração de fase inteira concluída.

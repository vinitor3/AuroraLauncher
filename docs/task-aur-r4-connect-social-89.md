# Tarefa 89 — Entrar em sessões sociais pelo launcher

Data: 28/10/2026. Estado do incremento: **PROTÓTIPO**, **IMPLEMENTADO MAS NÃO HOMOLOGADO** para produção. Branch `codex/aur-r4-connect`, base `e0c9967`, worktree `C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-r4-connect`. Resultados intermediários, retestes finais e execução gráfica estão separados neste registro. Entrar público, Entrar por amigo e hospedar passaram na interface local; saída normal do aplicativo pela bandeja não foi comprovada.

## Base e escopo preservado

O checkout principal continuava em `dc2ec5ac36a88f54b1130bac4789f8ecd56641d2`; esta implementação utiliza a worktree cumulativa Connect, sem sobrescrever outras worktrees ou retomar a base histórica `TESTE-main/aurora-skins-runtime`. Launcher alpha.14, Assistente desktop, Recovery/CAS/Doctor e os nove pares Core 1.1.5/Skins 0.3.1 permanecem no escopo existente. Não há alteração de Java, matriz, artefatos de mods ou contratos de skins.

A proposta preexistente não rastreada `docs/aurora-connect-voice-session-binding-proposal.md` permanece preservada e fora desta entrega. Nenhum push, deploy, regras remotas, publicação, release, instalador, billing, firewall, relógio ou configuração pessoal foi autorizado por inferência deste trabalho.

Materiais lidos: `AGENTS.md` ativo e da worktree, prompt consolidado Connect V2, experiência social, domínio/tarefa87 e ADR-030/031/032; código real de `SocialView`, `SocialService`, `socialFirebase`, `App`, barreira de conta, `ConnectManager`, comandos Tauri, codecs/provas sociais e adaptador Auth/Firestore local. A decisão nova está na [ADR-033](engineering/ADR/ADR-033-social-desktop-join.md).

## Bloqueio resolvido e implementação

88A/B/C já possuíam vínculo conta/chave, política anfitriã, grant, continuidade e consulta de contexto, mas o runner ainda encaminhava os objetos entre dois helpers. O botão Entrar continuava desabilitado. Este incremento coloca a entrega interna no backend local e o worker social no **mesmo ConnectManager/Attempt/slot** do launcher, com seus controles de stop, logout e saída.

O recorte continua uma vaga, serviço game/TCP, fonte aprovada `127.0.0.1:43001`, somente loopback, public/friends com entrada aberta e preparo/política original de até60s. Aprovar cada pedido, vários participantes, autoentrada no mundo, produção, outros caminhos e voz permanecem separados.

- O executável debug `social-lab`, com `connect-lab`, recebe pin público e issuer da autoridade criada pelo processo local confiável. A origem nativa é fixa `http://127.0.0.1:42888`. Nenhum comando aceita URL/pin/projeto escolhido pelo renderer. Executável normal não possui esse bootstrap.
- O App reserva a tentativa nativa antes de aguardar o SDK. Uma transferência estreita entrega o token local em RAM; o nativo confina o token ao projeto demo/conta esperada antes de HTTP, e o backend autentica e aplica as regras. Não há token em argv, snapshot, convite, catálogo, JVM, peer ou log.
- O anfitrião publica seu anúncio separadamente, prepara o servidor, revisa origem/visibilidade/uma vaga/prazo e confirma uma política exata. Salvar anúncio não abre transporte. Consentimento começa desmarcado e é descartado quando o digest muda.
- O convidado clica Entrar na lista de amigos ou no cartão público. O worker resolve contexto, prova a chave, recebe grant e convite interno e abre a ponte autenticada. Não pede JSON, código público ou convite copiado ao usuário.
- A interface acompanha estado nativo, permite Desconectar e mostra o endereço loopback para Conexão direta no Minecraft. Conexão autenticada não é mostrada como jogador no mundo. Pedir para entrar, sessão própria, sessão vencida e novas entradas durante outra tentativa ficam desabilitados.
- `ConnectView` reconhece a tentativa social e oferece observação/encerramento; não apresenta confirmação, exportação, probe ou preparo manual sobre essa tentativa.

O controlador fica no App e sobrevive à navegação entre seções. `SocialView` apenas apresenta estado e ações. Logout/troca de conta usam `connect_account_transition_stop`, que invalida a epoch nativa **antes** de observar/encerrar o slot, inclusive quando estava idle. Uma reserva atrasada com epoch antiga não pode surgir depois do encerramento da conta.

## Ajustes encontrados na revisão

1. A primeira versão do controlador só publicava estado após obter token. A revisão identificou que uma espera do SDK esconderia a reserva e o botão Desconectar. O controlador agora consulta e valida a reserva exata imediatamente, antes do SDK, e limita essa espera a10s.
2. Stop durante uma resposta tardia de reserva poderia observar idle e deixar a nova reserva sem cleanup. Agora qualquer erro/resposta obsoleta depois de obter a identidade da reserva aguarda stop **somente daquela operação**. Apenas a recusa nativa de operação obsoleta é tolerada; falha de cleanup continua visível. Não encerra uma operação posterior.
3. O wrapper do serviço social captura o controlador e o objeto SDK User originais. Um cartão desmontado não redireciona sua ação ao controlador de outra conta ou de outro login sob o mesmo UID.
4. A confirmação host envia operação, geração atual observada e digest; não reaproveita automaticamente o confirm do fluxo manual.
5. As provas sintéticas anteriores de barreira snapshot→stop foram adaptadas à nova fronteira atômica nativa. Não somar os testes antigos substituídos ao lote novo.

A revisão do worker preservou a primeira lease de sequência1 separada da mais recente, consumiu corretamente a versão inicial do watch e passou a conferir continuidade enquanto HTTP, bind, dial ou ponte ainda estão pendentes. A liberação espera os próprios recursos e verifica rebind; falha de cleanup nunca vira sucesso. Cancelamento explícito termina como encerramento normal, enquanto falhas de transporte/autorização continuam distintas. A projeção de host admitido muda uma vez, sem incrementar geração a cada tick; intervalos atrasados pulam ticks, sem disparar rajadas de consultas.

Reserva sem worker observa também o vencimento civil original, além do monotônico. A entrega do token muda cleanup para pending; token fora do emulador é recusado antes de HTTP. Os testes extras exercitam worker inválido recolhido, token tardio, ausência de mudança repetida de geração, falha de cleanup e expiração civil sem alterar o relógio do PC.

As fixtures também foram revisadas: falha antes de ready recolhe o filho próprio; quit exige cleanup derivado do snapshot; saída normal do executável exige código0. Nos novos ensaios de retirada, o TCP precisa estar aberto com dados observados e mais de10s restantes até o vencimento normal antes do gatilho. O watchdog gráfico solicita encerramento, mas não promete prazo rígido para uma operação SDK que já esteja pendente.

## Arquivos materiais

| Camada | Arquivos |
| --- | --- |
| Backend e broker | `apps/edge-proxy/src/connectSocialAdmission.ts`, `connectSocialAdmission.test.ts`; `firebase/tests/social-binding-server.mjs` |
| Manager e contrato nativo | `apps/desktop/src-tauri/src/engine/connect/social.rs`, `social_profile.rs`, `live.rs`, `mod.rs`, integração nos construtores em `host_source.rs`, `group_source.rs`, `session_group.rs` |
| Fronteira desktop | `apps/desktop/src-tauri/src/connect_commands.rs`, `main.rs` |
| App/serviço | `apps/desktop/src/App.tsx`, `src/shared/social.ts` |
| Entrada e estado social | `src/features/social/connect-controller.ts`, `connect-types.ts`, `SocialConnectionPanel.tsx`, `SocialView.tsx`, `social.css` |
| Barreiras e painel legado | `src/features/connect/account-transition.ts`, `types.ts`, `ConnectView.tsx` |
| Testes de interface | `src/features/social/connect-controller.test.mjs`, `SocialView.smoke.mjs`; `src/features/connect/account-transition.smoke.mjs`, `ConnectView.smoke.mjs` |
| Integração e preparação GUI | `firebase/tests/social-desktop-fixtures.mjs`, `social-desktop.integration.test.mjs`, `social-desktop-gui.mjs` |
| Contrato e evidência | ADR-033, esta tarefa, `AGENTS.md`, experiência social, backlog005/008, matriz de compatibilidade e índice de ADRs |

Os caminhos abreviados `src/...` da tabela pertencem a `apps/desktop/`. Não foi criado outro CLI de produto; o processo de integração é um teste opt-in da própria biblioteca desktop, usando o mesmo manager dos comandos Tauri.

## Resultados executados

| Verificação | Resultado | Limite da prova |
| --- | --- | --- |
| Frontend inicial | 36 PASS, 3898,2547ms | 25 DOM sociais existentes +11 controlador/DTO. Não somar aos lotes seguintes. |
| Controlador e barreira após correção | 29 PASS, 599,0198ms | 11 controlador +18 barreira; promessas controladas, sem Firebase/rede. |
| Frontend ampliado | 65 PASS, 1426,059ms | 33 DOM sociais +14 controlador +18 barreira; antes dos dois casos finais. |
| Frontend focal antes da precedência de fechamento | 271 PASS, zero falhas/ignorados, 3684,6388ms | 205 `ConnectView`, 34 `SocialView`, 14 controlador, 18 barreira; incluído no lote seguinte. |
| Frontend focal final | **272 PASS**, zero falhas/ignorados, 3552,1065ms | `social-89-frontend-focal-final.log`:205 `ConnectView`,35 `SocialView`,14 controlador,18 barreira. Inclui regressões, não são272 capacidades novas. |
| TypeScript desktop | PASS, `tsc -b --pretty false` | Executado após integração e novamente após captura estrita de controlador/User. |
| Diff frontend | PASS, `git diff --check` | Não equivale à revisão final de todos os arquivos. |
| Nativo social inicial | Quatro PASS, um helper opt-in ignorado, execução0,03s, compilação1m26s | `social-89-native-focused.log`. Bootstrap/origem, epoch/token tardio, slot/manual e versão inicial do watch; não reatribuir testes nativos posteriormente adicionados. |
| Integração real inicial | **Seis TAP PASS**, zero falhas, 13889,0805ms | `social-89-desktop-integration-initial.log`: cinco cenários e agrupador, Auth/Firestore/SDK/broker e dois managers reais locais. |
| Edge após lease inicial preservada | 220 PASS, zero falhas, 9746,8595ms | `social-89-authority-edge-tests-initial-lease.log`; inclui regressões anteriores. |
| Build frontend normal | PASS,381 módulos, Vite1,88s | `social-89-frontend-build.log`; aviso de chunk maior que500kB preservado. Não é prova de GUI/Tauri. |
| Regressão nativa all-features antes dos últimos ajustes sociais | **733 PASS**, zero falhas,79 ignorados em oito targets; doc-tests0 | `social-89-native-all.log`: biblioteca624/77 em1671,20s; demais43/1,18/0,12/0,2/0,15/1,17/0,2/0. Não reatribuir aos quatro testes sociais acrescentados depois da compilação. |
| Regressão frontend adjacente | **95 PASS**, zero falhas,1432,3953ms | `social-89-ui-adjacent.log`: anúncio de voz, lançamento pronto, confinamento Firebase e preview lifecycle. |
| Worker dry-run | PASS,Wrangler4.127.0,152,42KiB/gzip37,37 | `social-89-worker-check.log`; `--dry-run: exiting now`. Nenhum deploy. |
| Functions lint | PASS após restaurar dependências locais pelo lockfile | Primeira tentativa falhou por tsc ausente. `npm ci --ignore-scripts --no-audit --no-fund` instalou253 pacotes; Node24.8 versus engine20 gerou aviso, sem alterar manifesto/lock ou executar Functions. Reteste `social-89-functions-lint-final.log`. |
| Connect nativo após os ajustes finais | **134 PASS**, zero falhas,5 ignorados,139,17s; compilação1m12s | `social-89-native-connect-final.log`; inclui dez focais sociais ativos e um helper opt-in ignorado. Os demais são regressões do manager e de seus proprietários. |
| Integração real final com TCP ativo | **Seis TAP PASS**, zero falhas,12662,3615ms | `social-89-desktop-integration-final.log`; cinco cenários mais agrupador, descritos abaixo. |
| Clippy all/default/connect | PASS,20,34s/11,97s/18,86s | Primeira rodada all falhou em dois `possible_missing_else` no mesmo braço select; corrigido apenas layout de instruções, sem mudar condições. Retestes com `-D warnings`, sem supressão. |
| Frontend após correção textual observada na GUI | **274 PASS**, zero falhas/ignorados,3166,0221ms | `social-89-frontend-final-copy.log`;205 ConnectView,37 SocialView,14 controlador,18 barreira. Substitui o lote272, não se soma a ele. |
| Executável gráfico social-lab | Build PASS, sem bundle; três fluxos GUI com TCP1MiB/direção PASS | Hash e ressalva de encerramento do aplicativo no fechamento abaixo. Não equivale a liberação de produção. |

O lote272 testa reserva antes do SDK, clique duplicado, token tardio após mudança de conta, invalidação durante reserva, stop durante token, reserva que chega depois de stop/idle, falha de token/cleanup, consentimento host, expiração, default-off, schemas fechados, mensagens redigidas, entrada pública e de amigos, cards próprios/aprovação recusados e ausência dos controles manuais sobre sessão social. A revisão nativa acrescentou precedência de `phase=closing` sobre um estágio social retido: a interface mostra Desconectando e desabilita nova aprovação, sem continuar anunciando conexão disponível. Os testes usam DOM sintético e projeções controladas: não são login ou transporte real.

### Integração real inicial

| Caso | Resultado observado |
| --- | --- |
| Público + bloqueio | TCP exato de1MiB por direção/EOF; ambos os managers chegaram a encerramento e cleanup completos, ponte TCP rebind;883ms entre ação de bloqueio e observação de fechamento completo. |
| Amigos aceitos + troca de conta | TCP1MiB por direção/EOF; fence invalidou epoch antiga, token tardio recusado, cleanup/rebind;107ms após a ação de troca. |
| Perda de backend | TCP1MiB por direção/EOF; fechamento automático dos dois workers sem comando stop, cleanup/rebind;2052ms após desligar a autoridade. |
| Não amigo em sessão de amigos | Guest recusado, nenhum endereço TCP publicado, zero conexões e zero bytes no serviço de jogo. |
| Bloqueado antes de entrar | Guest recusado, nenhum endereço TCP publicado, zero conexões e zero bytes no serviço de jogo. |

Esses tempos são amostras locais de observação do encerramento, não RTT, ping, SLA ou atraso de áudio. A transferência terminou antes da ação de retirada nos cenários iniciais; a evidência não é de payload contínuo durante o bloqueio. As fixtures encerram os filhos próprios sem aceitar forced stop, fazem cleanup do SDK e verificam rebind do echo. A GUI utiliza um helper separado, que não opera botões automaticamente nem substitui o registro de observação nativa.

### Integração real final com fluxo ativo

Binário de teste desktop SHA-256 `eec0be4cc08d4ec939414cd1ee42c03784820b202d378d7c7fe516ff4a31fdbc`, dois managers no mesmo PC, autoridade/contas/amizade/bloqueio pelo SDK e regras reais dos emuladores. Todos os filhos encerraram sem forced stop e quit confirmou cleanup real.

| Caso | Resultado final |
| --- | --- |
| Público e bloqueio |1MiB exato por direção/EOF, depois fluxo ativo com3072B recebidos;58109ms restantes antes do bloqueio; encerramento automático de ambos e rebind em808ms observados. |
| Amigos e troca de conta |1MiB por direção/EOF, fluxo ativo256B;58198ms restantes; fechamento/rebind em107ms, epoch e token antigos recusados. |
| Perda da autoridade |1MiB por direção/EOF, fluxo ativo256B;58199ms restantes; encerramento sem comando stop/rebind em2046ms. |
| Não amigo e bloqueado antes de ingressar |Recusa sem endereço publicado, zero conexões e bytes no destino;452ms/471ms de cenário, respectivamente. |

As margens excluem vencimento normal da sessão como explicação desses três encerramentos; os tempos são amostras de observação local, não promessa de latência. Nenhum deles executou Minecraft ou voz.

### Comandos frontend realmente executados

Diretório: `apps/desktop` da worktree. Rodadas iniciais aparecem na tabela; estes são os comandos do lote final e verificação de tipos:

```powershell
node --test src/features/social/connect-controller.test.mjs src/features/social/SocialView.smoke.mjs src/features/connect/account-transition.smoke.mjs src/features/connect/ConnectView.smoke.mjs
node node_modules/typescript/bin/tsc -b --pretty false
git diff --check
```

Os logs nativos/backend/integrados estão em `.aurora/connect-validation/`; comandos, artefatos/pins e observação GUI seguem abaixo. Não atribuir ao novo executável os pins ou as provas GUI87/Connect anteriores.

Comandos nativos/backend e regressões já executados, na raiz da worktree:

```powershell
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --all
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib --features connect-lab --locked --offline --jobs 2 engine::connect::live::social::tests -- --nocapture
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --all-features --locked --offline --jobs 2 -- --test-threads=1
node --test apps/edge-proxy/src/connectSocialAdmission.test.ts
npm --prefix apps/edge-proxy test
node apps/desktop/node_modules/typescript/bin/tsc --noEmit --target ES2022 --module ESNext --moduleResolution bundler --lib ES2022,DOM --strict --skipLibCheck --allowImportingTsExtensions apps/edge-proxy/src/connectSocialBinding.ts apps/edge-proxy/src/connectSocialAdmission.ts
node --check firebase/tests/social-binding-server.mjs
node --check firebase/tests/social-desktop-fixtures.mjs
node --check firebase/tests/social-desktop.integration.test.mjs
node --check firebase/tests/social-desktop-gui.mjs
npm --prefix apps/desktop run build
npm --prefix apps/edge-proxy run check
npm --prefix functions run lint
npm --prefix functions ci --ignore-scripts --no-audit --no-fund
npm --prefix functions run lint
```

Retestes finais na mesma raiz:

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib --all-features --locked --offline --jobs 2 engine::connect:: -- --test-threads=1
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-features --all-targets --locked --offline --jobs 2 -- -D warnings
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets --locked --offline --jobs 2 -- -D warnings
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets --features connect-lab --locked --offline --jobs 2 -- -D warnings
$env:AURORA_SOCIAL_DESKTOP_INTEGRATION = '1'
$env:AURORA_SOCIAL_DESKTOP_TEST_BINARY = (Resolve-Path apps/desktop/src-tauri/target/debug/deps/aurora_launcher_core-6bce16f7a29b4ee3.exe).Path
node --test --test-concurrency=1 firebase/tests/social-desktop.integration.test.mjs
```

Emuladores usados: `emulators:start --only auth,firestore --project demo-aurora-social --config firebase/social-emulators.json`, via firebase-tools local já disponível. Auth9187/Firestore8187; broker42888 e fonte de eco43001 criados/encerrados pelas fixtures, sem competir com a regressão nativa dedicada.

O executável de testes all-features da primeira rodada tinha SHA-256 `b7da942d437d32eb8db9c82ec3880aaf5e5ce46e64f52c66b1fe5628448c08f9`. Os ajustes sociais finais foram aplicados após sua compilação; exigem reteste próprio e não transformam esse baseline em prova da versão final.

## Limites, custo e próximos gates

| Área | Estado deste incremento |
| --- | --- |
| TCP | **PROTÓTIPO** local no manager desktop, entrada social e bytes exatos demonstrados em dois processos controlados. Não é servidor Minecraft. |
| UDP de aplicação | Sem novo transporte/datagrama de aplicação. QUIC sobre UDP não constitui nova prova da ponte UDP para mods. |
| Direto | Loopback somente nesta rodada. Provas IPv6 externas anteriores preservadas, sem nova validação IPv4/NAT/CGNAT, migração ou seleção por latência. |
| Relay | Desligado. Nenhum serviço pago ou novo relay remoto. |
| Minecraft | Nenhum novo login, mundo, cliente ou autoentrada comprovados por este incremento. O endereço da ponte é apresentado para uso explícito no jogo. |
| Amigos/convites/UI | Entrada public/friends+open pelo botão e host com confirmação explícita passaram no executável Tauri local, sem copiar convite. Evidência GUI separada dos274 testes sintéticos. Approval continua indisponível; saída normal do aplicativo pela bandeja não comprovada. |
| Simple Voice Chat | Sem nova sessão autenticada ou áudio. Limites anteriores preservados. |
| Plasmo Voice | Sem nova sessão autenticada ou áudio. Homologação separada continua necessária. |
| R4, Core/Skins | Não promovidos; nove pares e fases/gates preservados. |

O backend continua local e opt-in; não há provisionamento de confiança, rate limit distribuído, orçamento de produção ou serviço publicado. Tokens de Auth Emulator não demonstram validação de autenticação real em produção. O tempo de preparo60s inclui a revisão e não é renovado ao clicar ou obter uma resposta. A continuidade de até3s é fail-closed; operação de bind/teardown aguarda os recursos próprios e não possui promessa de latência rígida.

Rollback: encerrar os processos locais próprios, conservar os registros, manter o build normal desativado e reverter somente o commit deste incremento se necessário. Não apagar estágios antigos ou a proposta histórica de voz, alterar contas pessoais ou republicar regras.

## Fechamento da execução gráfica

A conta Cloudflare solicitada foi conferida por consulta somente leitura ao conector e correspondeu ao e-mail indicado pelo usuário. Nenhum recurso remoto foi criado, atualizado ou publicado. A GUI usou somente duas contas descartáveis no Auth/Firestore demo; a conta pessoal do launcher e a outra instância Aurora de temas não participaram.

`npm --prefix apps/desktop run social:lab:build` passou:381 módulos frontend/Vite1,34s e compilação nativa1m22s, debug/connect-lab/`--no-bundle`, log `social-89-native-build.log`. O aviso de chunk maior que500kB permanece. A cópia criada sem sobrescrever outro executável foi `.aurora/connect-validation/social-89-native/AuroraSocialLab.exe`,65348608 bytes, SHA-256 `49e42ee46cd6a4434784d52d60022bf1bfb794dabaeee8d831baae4a37cee53e`. Esse é o executável dos três ensaios GUI abaixo.

O acompanhante `firebase/tests/social-desktop-gui.mjs` iniciou somente contas/amizade/fonte de eco/broker e o segundo manager nativo. Os cliques, preenchimento, consentimento, Entrar, Desconectar e logout do executável foram feitos pelo Computer Use, sem invocar IPC por fora da interface. Não houve transferência manual de convite.

| Fluxo no executável | Evidência observada |
| --- | --- |
| Entrar no cartão público | Conexão autenticada e endereço `127.0.0.1:54917`; `gui_tcp_passed` com1048576B exatos por direção. Desconectar mostrou Conexão encerrada/Recursos da conexão liberados; peer closed/cleanup complete sem erro; rebind local passou. |
| Entrar na lista de amigos | Amizade aceita e sessão exclusiva de amigos; catálogo público vazio. Conexão autenticada/endereço `127.0.0.1:56917`;1048576B por direção. Sair da conta exibiu Encerrando conexão e retornou ao login, rebind passou. O peer posterior registrou failed/cleanup complete/`connect_plan_expired`; não atribuir a esse peer encerramento normal sem erro ou causalidade exclusiva do logout. |
| Hospedar pela GUI | Preencheu título/modpack/versão/loader/1de2, escolheu Toda a comunidade/Entrada aberta e confirmou publicação local. Preparar meu servidor exibiu fonte43001/uma vaga; consentimento inicialmente desmarcado e Compartilhar agora desabilitado. Após aprovação explícita, Aguardando um participante → Conexão autenticada; `gui_host_tcp_passed`1048576B por direção. Desconectar liberou recursos, peer closed/complete sem erro; Encerrar anúncio retirou o cartão e desabilitou preparar; logout retornou ao login. |

Nenhum teste GUI acima iniciou Minecraft ou voz. Os nomes de modpack/versão e a população preenchidos são metadados declarados, não descoberta de runtime. O segundo participante é outro processo nativo neste PC, não outro jogador ou outra internet.

A primeira preparação do acompanhante usou sessão sem PTY: stdin não pôde receber comandos, nenhum transporte foi iniciado; logout foi observado e somente os próprios processos validados foram encerrados forçadamente. Recomeçou-se com PTY. Durante o formulário, a ferramenta de UI recusou clique na janela transitória do dropdown; uma nova observação confirmou a seleção efetiva, sem inventar sucesso da ação recusada.

Na rodada concluída, todas as conexões e o anúncio foram encerrados pela interface e o logout foi observado. A superfície disponível não expôs o menu de bandeja para Fechar o launcher; o X apenas oculta a janela. Após cleanup do transporte, o executável próprio foi encerrado forçadamente por identidade/caminho/PID validados. O acompanhante registrou exit4294967295, `nativeApplicationNormalExit:false`, `cleanup:true` e saiu com código1. **O lote GUI completo não é declarado PASS de saída normal do aplicativo.** A outra instância Aurora permaneceu intocada. Os campos antigos `forced:false` do acompanhante descreviam apenas sua própria ação e não detectavam término externo; foram substituídos por `terminationObservation:exit_code_only`, sem relaxar o requisito de exit0.

O acompanhante recolheu peer, contas SDK, fonte e broker; rebind43001/42888 passou. Os emuladores próprios foram interrompidos pela sessão que os iniciou. Consulta final não encontrou listeners8187/9187/42888/43001 ou os processos próprios da GUI/acompanhante. Não foram apagados dados pessoais, estágios antigos ou evidências.

A GUI revelou uma mensagem ambígua após salvar anúncio. Foi corrigida para orientar Preparar meu servidor quando a capability estiver disponível, mantendo mensagem sem promessa de ativação no perfil normal. Dois testes novos confirmam que salvar não prepara nem confirma transporte. SocialView37PASS e lote combinado274PASS após a correção; a GUI acima pertence ao binário anterior a esse ajuste somente textual.

O rebuild final pelo mesmo `social:lab:build` passou com TypeScript,381 módulos/Vite450ms e Rust12,05s (`social-89-native-build-final-copy.log`). Criou-se outra cópia, sem substituir o executável testado: `.aurora/connect-validation/social-89-final/AuroraSocialLab.exe`,65348608 bytes, SHA-256 `74f5b29d45e06046a29aff8273fcc47ecb052754842b9a6f13fa2780b01365b5`. Não houve nova execução gráfica desse binário posterior à correção textual; não confundir build/DOM com uma segunda prova GUI. A implementação nativa e o controlador permaneceram idênticos aos dos ensaios anteriores.

Revisões independentes de backend/fixtures e nativo concluídas sem bloqueador material; `node --check firebase/tests/social-desktop-gui.mjs`, `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --all --check` e `git diff --check` passaram. Checkout principal permaneceu limpo no HEAD registrado; nenhum manifesto/lockfile foi alterado. Matriz ativa nos dois checkouts manteve SHA-256 `96c4ec789902f8aa87931ecd72aca531fec53d4ac9c6055fcb9837ef77e6918a`; a proposta não rastreada de voz manteve `ef0ed6edc466148f6b771a9349587ff1fe1a3e85919db88b138b547b4ba8bb08`, fora do commit.

Comandos adicionais realmente executados na raiz (a sessão gráfica usou PTY):

```powershell
npm --prefix apps/desktop run social:lab:build
$env:AURORA_SOCIAL_DESKTOP_GUI = '1'
$env:AURORA_SOCIAL_DESKTOP_TEST_BINARY = (Resolve-Path apps/desktop/src-tauri/target/debug/deps/aurora_launcher_core-6bce16f7a29b4ee3.exe).Path
$env:AURORA_SOCIAL_DESKTOP_EXE = (Resolve-Path .aurora/connect-validation/social-89-native/AuroraSocialLab.exe).Path
node firebase/tests/social-desktop-gui.mjs
```

Entradas do acompanhante na rodada concluída: `host-public`, `probe 127.0.0.1:54917`, `peer-status`, `host-friends`, `probe 127.0.0.1:56917`, `peer-status`, `close-peer`, `guest`, `peer-status`, `quit`. Uma consulta `peer-status` sem peer ativo retornou `gui_fixture_operation_failed`, sem criar conexão. Rebind das duas portas guest foi executado com o helper `rebind` em processos Node separados após o respectivo fechamento.

## O que você precisa fazer agora

Nenhuma intervenção humana é necessária para o trabalho e os testes locais deste incremento. Ensaios futuros entre máquinas, mundo/contas/voz e publicação são gates separados e não foram solicitados durante esta execução local.

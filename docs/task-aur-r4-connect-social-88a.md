# Tarefa 88A — prova limitada entre conta Aurora e chave do Connect

Data: 25/10/2026. Estado: **PROTÓTIPO**, **IMPLEMENTADO MAS NÃO HOMOLOGADO** para publicação. Base `38416af`, branch `codex/aur-r4-connect`, worktree `C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-r4-connect`. O checkout principal permaneceu em `dc2ec5a`, separado e intacto. A base histórica `TESTE-main/aurora-skins-runtime` não foi usada.

Foi preservado o arquivo preexistente não rastreado `docs/aurora-connect-voice-session-binding-proposal.md`, fora deste incremento. Nenhuma outra worktree foi sobrescrita. Alpha.14 cumulativo, Recovery/CAS/Doctor, Assistente desktop, Core 1.1.5/Skins 0.3.1 e nove pares permanecem; não foi reintroduzido Companion/HUD ou ampliada a matriz.

## Objetivo e contrato

Acrescentar a prova concreta de posse da chave nativa por uma conta Aurora autenticada, primeiro requisito para a futura entrada social sem copiar convites. A [ADR-030](engineering/ADR/ADR-030-social-account-key-binding.md) limita o resultado a conta/chave/operação/geração/sessão/contexto/prazo. O [domínio 88A](aurora-connect-social-key-binding-88a.md) detalha codec, lifecycle e limites.

O recibo tem `transportAuthorized:false`, não é aceito pelo transporte e não confirma o laboratório manual. **Entrar não foi habilitado.** Não há endpoint novo de produção, credencial administrativa ou autenticação de produção exercitada nesta tarefa. O futuro incremento 88B precisa de contrato próprio; não é apresentado como implementação concluída.

## Arquivos do incremento

| Área | Arquivos e efeito |
| --- | --- |
| Biblioteca | `apps/connect-lab/src/social_key_binding.rs`, `src/lib.rs`: codec fechado, assinatura/verificação Ed25519, tipos e limites exportados |
| Helper | `apps/connect-lab/src/bin/aurora-connect-key-proof.rs`, `tests/social_key_proof_cli.rs`: chave efêmera, entrada limitada e uma prova, com testes de processo |
| Manager desktop | `apps/desktop/src-tauri/src/engine/connect/social_key_binding.rs`, `live.rs`, `mod.rs`: assinatura a partir do Draft original sob o mesmo owner, prazo e encerramento |
| Inicialização dos attempts | `engine/connect/group_source.rs`, `host_source.rs`, `session_group.rs`: estado explícito de prova ainda não emitida, sem tornar esses perfis elegíveis |
| Tauri | `apps/desktop/src-tauri/src/connect_commands.rs`, `main.rs`: comando estreito, janela main, feature `connect-lab` |
| Autoridade | `apps/edge-proxy/src/connectSocialBinding.ts`, `connectSocialBinding.test.ts`: desafio retido/consumido uma vez, autenticação/leitura atual, prazo, cancelamento e recibo sem admissão |
| Integração | `firebase/tests/social-binding-server.mjs`, `social-binding.integration.test.mjs`: HTTP loopback opt-in, Auth/Firestore Emulator reais e helper Rust real |
| Documentação | ADR-030 e os dois documentos 88A; ponteiros gerais mantidos pelo integrador |

Não houve alteração de regras remotas, Core/Skins, matriz, política de custos ou configuração pessoal. O novo módulo de autoridade não foi conectado ao roteador do Worker de produção.

## Comandos efetivamente executados

Da raiz da worktree, testes nativos locais com dependências fixadas e sem download:

```powershell
cargo test --manifest-path apps/connect-lab/Cargo.toml --locked --offline --jobs 2 social_key -- --nocapture
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --features connect-lab --locked --offline --jobs 2 social_key -- --nocapture
```

Não foi definido `CARGO_TARGET_DIR` nessas rodadas. O filtro `social_key` excluiu outros testes; saídas com zero testes em outras suítes não são contadas como cobertura adicional. O primeiro comando executou quatro testes da biblioteca e dois do CLI. A primeira compilação desktop falhou; a repetição do segundo comando passou nos quatro testes do manager após a correção descrita abaixo.

Integração local real, com o binário revisado e opt-in explícito:

```powershell
$env:AURORA_SOCIAL_BINDING_INTEGRATION = '1'
$env:AURORA_SOCIAL_BINDING_RUST_CLI = 'C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-r4-connect/apps/connect-lab/target/debug/aurora-connect-key-proof.exe'
node --test --test-concurrency=1 firebase/tests/social-binding.integration.test.mjs
```

Auth e Firestore já estavam disponíveis no projeto demo e portas fixadas pelo laboratório. Esta execução criou duas contas Aurora descartáveis e uma conta Auth sem cadastro; nenhuma conta de produção ou senha pessoal foi usada. O teste encerrou seus signers/serviço/SDK em seu próprio fluxo de limpeza. O encerramento dos processos externos dos emuladores pertence à verificação operacional final, ainda registrada separadamente abaixo.

Instalação local das dependências já fixadas, na pasta `apps/edge-proxy`, e dry-run da raiz:

```powershell
npm ci --offline --ignore-scripts --no-audit --no-fund
```

```powershell
$env:WRANGLER_SEND_METRICS = 'false'
npm --prefix apps/edge-proxy run check
npm --prefix apps/edge-proxy test
```

A instalação recuperou 34 pacotes do cache em 11 segundos, conservando o lockfile. O script `check` executa `wrangler deploy --dry-run`; seu sucesso não é deploy e não publica a nova autoridade.

Outros comandos efetivamente executados da raiz:

```powershell
node --test apps/edge-proxy/src/connectSocialBinding.test.ts
node apps/desktop/node_modules/typescript/bin/tsc --noEmit --target ES2022 --module ESNext --moduleResolution bundler --lib ES2022,DOM --strict --skipLibCheck apps/edge-proxy/src/connectSocialBinding.ts
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --all
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --all -- --check
cargo fmt --manifest-path apps/connect-lab/Cargo.toml --all
cargo fmt --manifest-path apps/connect-lab/Cargo.toml --all -- --check
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets --all-features --locked --offline --jobs 2 -- -D warnings
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets --locked --offline --jobs 2 -- -D warnings
cargo clippy --manifest-path apps/connect-lab/Cargo.toml --all-targets --locked --offline --jobs 2 -- -D warnings
npm --prefix apps/desktop run build
git diff --check
```

Na pasta `apps/desktop`:

```powershell
node --test src/features/social/SocialView.smoke.mjs src/features/social/preview-lifecycle.test.mjs src/lib/firebase.social-lab.test.mjs src/features/connect/ConnectView.smoke.mjs src/features/connect/account-transition.smoke.mjs
```

Emuladores iniciados da raiz com a CLI já disponível no cache:

```powershell
& 'C:/Program Files/nodejs/node.exe' 'C:/Users/usuario/AppData/Local/npm-cache/_npx/ba4f1959e38407b5/node_modules/firebase-tools/lib/bin/firebase.js' emulators:start --only auth,firestore --project demo-aurora-social --config firebase/social-emulators.json
```

## Resultados e evidências

Logs privados sob `.aurora/connect-validation/`; não incluir tokens, senhas, chaves privadas, perfis completos ou respostas privadas de upstream nos documentos.

| Verificação | Resultado observado | Evidência/qualificação |
| --- | --- | --- |
| Autoridade Node focal | 19 PASS, 0 falhas, 232,6318 ms | `social-88a-authority-unit.log`; Ed25519 real, dependências controladas |
| Integração real | 8 PASS, 0 falhas/ignorados, 5197,8999 ms | `social-88a-integration.log`; sete subcasos mais o teste agrupador, não oito fluxos independentes de transporte |
| Biblioteca/CLI Rust | Quatro unitários + dois testes CLI PASS; compilação inicial em 4 min 19 s | Lote `social_key` no crate connect-lab, restantes filtrados |
| Manager desktop inicial | FAIL E0624 | `social-88a-native.log`; método com visibilidade insuficiente |
| Manager desktop após correção | Quatro PASS, 0 falhas; compilação 2 min 08 s, execução focal 0,02 s | `social-88a-native-final.log` |
| Interface existente | 262 PASS, 0 falhas, 6714,6165 ms | `social-88a-ui-regression.log`; DOM/lifecycle, sem nova GUI nativa |
| Formatação inicial | FAIL por ordem de imports/módulos | Correção pelo integrador; sem alteração de contrato |
| Formatação posterior | PASS | `cargo fmt --all` seguido de check, na execução do integrador |
| Edge check inicial | FAIL: Wrangler ausente localmente | `social-88a-edge-check.log`; não confundir dependência ausente com publicação tentada |
| Dependências edge | PASS, 34 pacotes do cache, 11 s, lock intacto | `social-88a-edge-install.log`; offline e scripts desabilitados |
| Worker dry-run posterior | PASS, Wrangler 4.127.0, 152,42 KiB | Sem deploy, endpoint ou alteração remota |
| Suíte edge completa | 174 PASS, 9743,1044 ms | Inclui os 19 testes focais; não somar 174+19 |
| Clippy desktop completo | PASS, 31,61 s | Todos os targets e features, warnings negados |
| Clippy desktop default | PASS, 9,84 s | Todos os targets, sem feature adicional |
| Build frontend | PASS, 378 módulos, 2,76 s | Aviso de chunk maior que 500 kB preservado |
| Clippy connect-lab inicial | FAIL `chunks_exact_to_as_chunks` | Correção de estilo aplicada |
| Clippy connect-lab final | PASS, 13,90 s | `social-88a-lab-clippy-final.log`; todos os targets, warnings negados |
| Integração após correção Clippy | 8 PASS, 0 falhas/ignorados, 1269,6721 ms | `social-88a-integration-final.log`; novo helper abaixo, sem reatribuição da primeira rodada |
| Biblioteca/CLI após correção Clippy | Quatro unitários + dois CLI PASS; build 2 min 16 s; execução 0,10 s e 0,04 s | `social-88a-lab-final.log`; demais testes filtrados |
| QA de cancelamento | PASS em dois probes Node em memória | Request já abortado → `binding_cancelled`; abort na última leitura da prova → `binding_cancelled`, zero pendências/recibos |

Não somar repetições da mesma suíte como capacidades novas. Os 262 testes repetem cobertura já existente da interface e não mostram que o novo comando foi acionado por ela.

Binário CLI exercitado na primeira rodada: `apps/connect-lab/target/debug/aurora-connect-key-proof.exe`, **2.190.336 bytes**, SHA-256 **`b397701731f46fbf937c7ecedbdf674004d9d819d12c147e2d77f8330d90338e`**. Após a correção de estilo, o helper reconstruído manteve 2.190.336 bytes, SHA-256 **`04724be3a7dad9dea31c80914ac7c3018facdc147b1a46b02eafdc5393d3fce8`**, usado na integração final. São helpers locais, não instalador, release ou executável do launcher completo.

## Correções e revisão

1. O validador TypeScript inicialmente convertia enums com `String(value)`. Um array como `["host"]` poderia atravessar a validação e ser tratado diferentemente pela checagem de propriedade. Papel, visibilidade e política agora usam igualdade literal, com cobertura negativa.
2. O Rust aceitava timestamp inicial zero, divergindo do contrato de inteiro positivo. A condição e a regressão foram corrigidas.
3. O recibo inicialmente omitia operação/geração. Ambos foram incluídos para preservar o contexto exato, mantendo `transportAuthorized:false` e ausência de consumidor de ingresso.
4. O prazo do desafio passou a respeitar `preparedExpiresAtUnixMs` e o orçamento monotônico original. A espera de autenticação/leitura não cria 60 segundos novos para um preparo antigo.
5. O cancelamento inicialmente não atravessava Request/HTTP. O núcleo passou a observar o AbortSignal nas consultas e após esperas; o adaptador liga `req.aborted` e fechamento prematuro da resposta ao mesmo cancelamento. Os dois probes independentes acima confirmaram a recusa sem recibo tardio.
6. A primeira compilação desktop revelou `E0624`: `pub(super)` dentro do submódulo não permitia acesso pelo wrapper. O integrador usou `pub(in crate::engine::connect)`; a repetição focal passou.
7. O primeiro fmt detectou apenas ordem de imports/módulos; a formatação e seu check passaram depois. O primeiro dry-run encontrou Wrangler ausente; foi usada instalação offline do lock existente antes da repetição bem-sucedida.
8. O Clippy connect-lab detectou `chunks_exact_to_as_chunks`. O integrador substituiu a iteração de pares da assinatura por `as_chunks::<2>` e formatou. Os quatro testes unitários, dois CLI e a integração real foram repetidos e passaram. Os checks de formatação de ambos os crates e de sintaxe dos dois scripts também passaram no fechamento.

A revisão não encontrou novo bloqueador após essas correções. A autoridade continua comparando o desafio com seu original retido, consumindo antes de trabalho assíncrono, limitando pendências e evitando generic signing. O adaptador local conserva URLs/projeto/Host fixos e não libera Origin/CORS. Tokens `alg=none` pertencem somente ao Auth Emulator; nenhum fallback foi adicionado à autenticação de produção.

## Fechamento operacional

Node v24.8.0 e Cargo 1.98.0 observados. O adaptador HTTP, signers e clientes SDK foram encerrados pelo teste. A sessão própria dos emuladores foi interrompida com Ctrl+C (retorno 1 da ferramenta); depois, leitura do sistema confirmou ausência dos processos Node/Java próprios e de listeners nas portas 8187, 9187, 4487, 4587, 9150 e 42888. Nenhum signer Rust permaneceu. Isso comprova liberação dos recursos observados, não saída graciosa do emulador nem encerramento normal da GUI.

Diff revisado e `git diff --check` passou. Matriz, contrato de Skins e proposta de voz preservaram os hashes iniciais; main continuou limpo em `dc2ec5a`. O commit local deste incremento contém somente fontes/testes/documentação 88A e ponteiros canônicos.

Não executado: novo ensaio gráfico nativo, jogo, voz ou dois computadores. A prova do manager e o pipeline HTTP/CLI são ensaios distintos; não compõem uma alegação de GUI integrada. A mudança de estilo posterior à primeira execução nativa foi verificada novamente na biblioteca e no pipeline CLI, sem reatribuir o executável desktop anterior.

## Situação por componente e próximos gates

| Componente | Estado ao final deste recorte |
| --- | --- |
| TCP | Sem nova prova de transporte 88A; evidências anteriores preservadas |
| UDP | Sem novo tráfego, endereçamento ou validação de retorno 88A |
| Conexão direta | Sem novo ensaio de IPv4/IPv6/NAT; prova nativa restrita a loopback |
| Relay | Não usado/habilitado/contratado; nenhum custo ou crédito temporário |
| Minecraft | Nenhum novo runtime, mundo ou jogador conectado por 88A |
| Convites/interface | Comando de prova acrescentado; nenhum autoapprove; Entrar permanece desabilitado |
| Amigos/sessões públicas | Incremento 87 preservado; regressões executadas, sem declarar nova GUI exercitada |
| Simple Voice Chat | Sem nova homologação de áudio/transporte/endereço/recuperação |
| Plasmo Voice | Sem nova homologação de áudio/transporte/endereço/recuperação |

O vínculo de chave não equivale a autorização de ingresso. O próximo contrato precisa estabelecer fonte nativa e política de hospedagem revisadas, vínculo com o dono/chave host da sessão, autorização individual atual e troca interna da autorização de transporte. Política de aprovação, participantes dinâmicos, revogação ativa, escolha do caminho, áudio real e validação externa continuam como gates distintos do roadmap. Não transformar um rascunho 88B em capacidade implementada nem fechar R4-005/008 por estes resultados.

## Limites, custo e rollback

Todo o novo caminho permanece experimental e desativado para usuários. Não houve push, deploy, publicação, release, instalador, faturamento, contratação, credencial de produção ou alteração de configuração pessoal/firewall. Rate limit em RAM por processo não é garantia de cobrança zero ou proteção distribuída para produção.

Rollback: reverter apenas os arquivos do incremento 88A após revisão, interromper seus próprios helpers e autoridade local, e preservar os logs, o estado anterior e a proposta de voz não rastreada. Não apagar bancos, configurações pessoais ou worktrees para retirar esta funcionalidade.

## O que você precisa fazer agora

Nenhuma intervenção é necessária neste incremento. Os testes entre computadores permanecem separados; nenhum comando remoto ou fornecimento de credencial é solicitado por 88A.

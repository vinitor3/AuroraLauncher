# Tarefa 88C — contexto social autenticado antes da admissão

Data: 25/10/2026. Estado: **PROTÓTIPO**, **IMPLEMENTADO MAS NÃO HOMOLOGADO** para publicação; execução local confirmada abaixo. Base `bc0bd48` do incremento 88B, branch `codex/aur-r4-connect`, worktree `C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-r4-connect`. Este registro não habilita o botão Entrar ou promove o laboratório a serviço publicado.

## Entrega e contrato

A [ADR-032](engineering/ADR/ADR-032-social-join-context.md) acrescenta uma consulta autenticada ao mesmo backend local de 88A/B. O convidado fornece somente a sessão selecionada; a autoridade lê a projeção autorizada com o token do chamador e resolve uma única política host ainda aberta. A resposta assinada identifica destinatário, sessão, anfitrião, chave, operação, geração e política, sem endereço, convite, token ou perfil privado.

Isso substitui a expectativa crua que o runner 88B construía usando seus próprios objetos do anfitrião. O mesmo helper nativo agora aceita `{context:<objeto assinado>}` e recusa `{expect:...}`. Seu pin, UID próprio e sessão selecionada vêm de entradas independentes do objeto recebido. Nenhum CLI novo, lifecycle paralelo em SocialView ou autorização do renderer foi introduzido.

O contexto é descoberta autenticada, não passe de transporte. Não reserva vaga ou consome prova de chave. `join` continua exigindo a prova 88A, consentimento/política host, autorização atual e grant separado 88B. A interface permanece com **Entrar desabilitado**.

## Garantias revisadas

- Autenticação e leitura autorizada da sessão antecedem o lookup da política em RAM. Sessão bloqueada, privada, inacessível, alterada, própria, ausente ou sem política disponível recebe a mesma indisponibilidade de contexto. O adaptador retorna HTTP 403 com somente `{code:"context_unavailable"}`; autenticação e limites preservam seus códigos próprios.
- A resposta usa Ed25519, domínio separado `aurora/connect/social-join-context/v1` terminado por byte zero, campos em ordem canônica u16BE, esquema fechado e máximo de 8192 bytes. Não reaproveita domínio de prova, grant ou continuidade.
- O orçamento de até 10000 ms começa antes da leitura. Também respeita o deadline civil/monotônico original da política, sem renovar tempo por leitura, digest, assinatura ou rollback civil. A mesma referência deve continuar aberta e atual após a assinatura; cancelamento ou reserva concorrente impede publicação.
- O backend devolve cópias dos campos. Alterar uma resposta não modifica política, próximo contexto ou estado interno.
- Rust verifica assinatura, pin, destinatário e sessão próprios, prazo e desafio antes de assinar a prova. A expectativa verificada conserva seu deadline original. O contexto precisa continuar válido até o ingresso/publicação da ponte; sua expiração não encurta uma sessão já admitida, governada pelo grant original e pelas leases 88B.

## Arquivos do incremento

| Área | Arquivos |
| --- | --- |
| Autoridade e testes | `apps/edge-proxy/src/connectSocialAdmission.ts`, `connectSocialAdmission.test.ts` |
| Adaptador HTTP opt-in local | `firebase/tests/social-binding-server.mjs` |
| Verificação nativa e exports | `apps/connect-lab/src/social_admission.rs`, `src/lib.rs` |
| Helper host/guest existente e testes de processo | `apps/connect-lab/src/bin/aurora-connect-social-client.rs`, `tests/social_client_cli.rs` |
| Integração real local | `firebase/tests/social-admission.integration.test.mjs` |
| Documentação | ADR-032 e esta tarefa; ponteiros gerais sob responsabilidade do integrador |

Os documentos históricos de 88A/B permanecem como evidências de suas rodadas. A proposta não rastreada `docs/aurora-connect-voice-session-binding-proposal.md` não pertence a este incremento.

## Resultados realmente executados

Logs sob `.aurora/connect-validation/`. Resultados de versões intermediárias são identificados; uma rodada repetida não é contada como teste adicional.

| Verificação | Resultado confirmado | Evidência/limite |
| --- | --- | --- |
| Autoridade admission, lote inicial | 33 PASS, 516,2688 ms | Inclui os 21 testes anteriores e 12 novos; não somar à suíte edge |
| Autoridade admission após cenário concorrente de assinatura | 34 PASS, 559,1152 ms | Adicionado caso de reserva da vaga enquanto a assinatura aguarda |
| Suíte edge inicial | 208 PASS, 0 falhas, 9667,1335 ms | `social-88c-edge-tests.log`; antes do teste concorrente adicional |
| Suíte edge atualizada | 209 PASS, 0 falhas, 9918,5029 ms | `social-88c-edge-tests-final.log`; inclui os testes focais |
| TypeScript strict | PASS, binding e admission | Verificação de tipos, distinta de execução real |
| Sintaxe do adaptador HTTP | PASS | `node --check` |
| Regressão integrada 88A | Oito TAP PASS, 3308,9317 ms | `social-88c-binding-regression.log`; helper de prova de chave, não helper social 88C |
| Objetos nativos, primeira rodada | Dez PASS, execução 0,37 s, compilação 23,87 s | `social-88c-native-unit.log`; antes do ajuste de espera de contexto descrito abaixo |
| Objetos nativos, rodada final da biblioteca | Os mesmos dez PASS, execução 0,37 s, compilação 5,62 s | `social-88c-native-unit-final.log`; biblioteca mantida sem alterações posteriores; não somar como vinte |
| CLI, primeira rodada | Seis PASS, execução 2,64 s, compilação 38,63 s | `social-88c-native-cli.log`; já inclui espera de challenge/grant com contexto expirado; antes da sondagem aguardada de portas |
| CLI, rodada final | Os mesmos seis PASS, execução 2,86 s, compilação 16,63 s | `social-88c-native-cli-final.log`; não somar como doze |
| Recuperação de sockets reais no binário | Dois PASS, execução 0,10 s, compilação 33,55 s | `social-88c-release-unit.log`; teste de liberação e de prazo esgotado |
| Integração social 88C inicial | FAIL: seis TAP, quatro PASS e dois FAIL, 19547,0921 ms | `social-88c-integration.log`; falha do cenário público e de seu agrupador por cleanup guest |
| Build diagnóstico | PASS, 6,68 s | `social-88c-diagnostic-build.log`; sondas UDP/TCP/erro separadas, sem alteração pretendida da lógica de rebind |
| Integração com diagnóstico | Seis TAP PASS, 19784,0335 ms | `social-88c-integration-diagnostic.log`; cinco cenários e agrupador; rodada distinta, não elimina a falha inicial |
| Integração final | Os mesmos seis TAP PASS, zero falhas, 19422,7242 ms | `social-88c-integration-final.log`; após correções de fixture/recuperação; não somar rodadas |
| Clippy connect-lab | PASS, todos os targets, 38,12 s | `social-88c-clippy.log`; tempo total observado inclui espera de cache, não representa CPU exclusiva |
| Clippy desktop | PASS, todos os targets/features, 1 min 17 s | `social-88c-desktop-clippy.log`; mesma ressalva sobre espera de cache |
| Formatação final | PASS | `cargo fmt --all -- --check` no manifest connect-lab |
| Diff final | PASS | `git diff --check` |

A regressão 88A usou `apps/connect-lab/target/debug/aurora-connect-key-proof.exe`, cujo SHA-256 conferido antes do rebuild foi `6e1db0fbb6301d74eeda18e602f68bac99da6f7c489a2967fc7ede6200c08ae9`. Não atribuir a essa rodada o pin histórico de 88A ou o pin do helper social 88C.

Os checks locais, pin e encerramento operacional desta rodada foram confirmados. As limitações de produto e validações ainda externas ao recorte estão separadas abaixo; a conclusão dos checks não habilita o launcher ou o serviço remoto.

### Revisão e ajustes

1. A revisão confirmou que o contexto não se transforma em limite da sessão depois da admissão. O runner mantém heartbeats, espera a expiração do contexto e exige nova transferência real antes do bloqueio. Esse comportamento foi observado na integração, separado da falha de cleanup inicial abaixo.
2. Foi acrescentado teste causal do backend que retém a assinatura do contexto enquanto outro ingresso reserva a política. A conclusão deve recusar publicação; não basta verificar o estado antes do await.
3. O nativo inicialmente recusava uma prova tardia, mas podia continuar aguardando stdin até o prazo de preparo de 60 s após expirar o contexto de 10 s. Foi acrescentada `preparation_line`, que limita localmente as esperas de challenge/grant ao deadline do contexto e revalida a resposta recebida. `Context.deadline` original permanece intacto para o grant/sessão. Há também recheck após assinar e antes de emitir a prova. O teste de processo mantém stdin aberto, sem comando/EOF, nos dois estágios e exige encerramento por expiração; passou no lote CLI inicial de seis testes.
4. A primeira integração completou a admissão pública, transferência após expiração do contexto e fechamento do TCP por bloqueio, mas o guest informou `cleanup/rebound=false` e exit 1. A observação original não separava UDP/TCP nem registrava errno; não permite afirmar qual porta falhou ou atribuir causa a TIME_WAIT. O integrador separou sondas e erro sem alterar a lógica; a nova rodada passou, mas não demonstra que a falha inicial foi corrigida.
5. Foi implementada sondagem de recuperação por até um segundo, a intervalos de 20 ms, somente para `AddrInUse`, depois dos reapers/drops. Todos os binds UDP/TCP bem-sucedidos permanecem retidos até concluir o mesmo ciclo. Erros definitivos e orçamento esgotado continuam falhando; primeiras/finais observações, tentativas e tempo são conservados. Não há reutilização de endereço ou troca de porta. Dois testes privados usam sockets reais e passaram: liberação após 75 ms e retenção além do orçamento de 80 ms. O integrador também trocou `now <= deadline` por `now < deadline`, para impedir sucesso exatamente no vencimento junto de `timedOut=true`.
6. O runner mantinha seu peer TCP `allowHalfOpen` e o envio periódico até o finally, que só ocorria depois de aguardar o cleanup/rebind nativo. Agora, somente depois de observar `lostTcp` e medir a retirada, interrompe o envio e fecha seu próprio peer, aguardando close por até dois segundos antes de conferir o evento guest. Remove uma espera circular da fixture sem enviar stop ao nativo ou produzir artificialmente o primeiro fechamento. Isso não comprova que tal espera causou a falha inicial sem errno.
7. A leitura dos diffs de autoridade, HTTP, objetos Rust, CLI e runner não identificou outro bloqueador até este ponto. Revisão por leitura não substitui a execução dos cenários novos.

## Cenários integrados e limites da evidência

A integração reutiliza Auth/Firestore/SDK reais locais, duas contas Aurora e os mesmos dois clientes nativos de 88B. O runner obtém o contexto por HTTP e não monta a expectativa guest a partir do processo host ou de um grant posterior. Pin vem do objeto da autoridade local criada pelo runner; UID e sessão vêm das contas e seleção independentes.

| Cenário | Resultado final observado |
| --- | --- |
| Público e bloqueio ativo | Contexto por sessão, prova/grant/TLS e TCP 1 MiB por direção; após expirar o contexto, mais 64 KiB por direção com continuidade ativa; bloqueio e fechamento automático em 2669 ms |
| Amigos e perda do backend | Amizade real pelo SDK, contexto e TCP 1 MiB por direção; perda do backend e fechamento automático em 2738 ms |
| Bloqueio após prova e antes do join | HTTP 403; zero bytes e conexões na fonte |
| Bloqueio antes do contexto | HTTP 403 com somente `context_unavailable`; zero bytes e conexões na fonte |
| Conta não amiga em sessão friends | Mesma resposta indisponível, sem divulgar política; zero bytes e conexões na fonte |

São cinco cenários mais um agrupador TAP. Todos passaram na rodada final em 19422,7242 ms. A rodada diagnóstica anterior passou em 19784,0335 ms, com retirada em 3016 ms por bloqueio e 2776 ms por perda do backend; esses números não substituem os resultados finais da tabela. A primeira rodada preserva a falha de cleanup. TCP eco local não significa Minecraft, jogador presente ou homologação de mods.

Nos dois cenários ativos finais, ambos os clientes encerraram com exit 0, sem finalização forçada, cleanup/rebind verdadeiros; o runner também refez bind na porta TCP convidada. O primeiro sinal de fechamento veio da retirada nativa, antes de fechar o peer de teste, e nenhuma mensagem stop foi usada para produzir a retirada. O prazo original da sessão ainda tinha mais de cinco segundos, evitando atribuir a expiração normal ao bloqueio/perda de backend.

A lease de até 3000 ms, o timer de observação, o fechamento do transporte e a sondagem de recuperação são etapas distintas. A sondagem de até um segundo não transforma a retirada em SLA de três segundos. Startup mantém suas limitações próprias: recolher bind/dial tardio não é garantia de término da biblioteca em três segundos.

Helper da integração inicial: `apps/connect-lab/target/debug/aurora-connect-social-client.exe`, 23.516.672 bytes, SHA-256 `44cbebfb3dc9474a38ed3fe51a906fff98029f9d71384c3fa13a3d77658ecca9`. Esse pin identifica a rodada que falhou; não atribuí-lo ao diagnóstico ou ao binário final sem medição correspondente.

Helper da rodada diagnóstica PASS, no mesmo caminho: 23.519.744 bytes, SHA-256 `7cb35055874b81e781eba142f598d871f60f3f94b4d7d337d830cc3110477b67`. Este segundo pin também precede a sondagem aguardada de recuperação e não deve ser apresentado como o artefato final desse ajuste.

Helper final exercitado e reconferido: **23.545.856 bytes**, SHA-256 **`bb5b647d7005aeb6231db57f1560280ab4800b140563ce48d6d41ccb257ff15a`**, no mesmo caminho. A integração final usa este artefato; os pins e falhas anteriores permanecem registrados separadamente.

## Comandos realmente executados

Na raiz da worktree, os checks confirmados foram:

```powershell
node --test apps/edge-proxy/src/connectSocialAdmission.test.ts
npm --prefix apps/edge-proxy test
node --check firebase/tests/social-binding-server.mjs
node apps/desktop/node_modules/typescript/bin/tsc --noEmit --target ES2022 --module ESNext --moduleResolution bundler --lib ES2022,DOM --strict --skipLibCheck apps/edge-proxy/src/connectSocialBinding.ts apps/edge-proxy/src/connectSocialAdmission.ts
cargo build --manifest-path apps/connect-lab/Cargo.toml --bin aurora-connect-social-client --locked --offline --jobs 2
cargo test --manifest-path apps/connect-lab/Cargo.toml --lib --locked --offline --jobs 2 social_admission -- --nocapture
cargo test --manifest-path apps/connect-lab/Cargo.toml --test social_client_cli --locked --offline --jobs 2 -- --nocapture
cargo test --manifest-path apps/connect-lab/Cargo.toml --bin aurora-connect-social-client --locked --offline --jobs 2 -- --nocapture
```

A regressão real de prova 88A foi executada com opt-in e caminho absoluto do helper já identificado:

```powershell
$env:AURORA_SOCIAL_BINDING_INTEGRATION = '1'
$env:AURORA_SOCIAL_BINDING_RUST_CLI = 'C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-r4-connect/apps/connect-lab/target/debug/aurora-connect-key-proof.exe'
node --test --test-concurrency=1 firebase/tests/social-binding.integration.test.mjs
```

A integração social foi executada nas rodadas inicial, diagnóstica e final, com resultados separados acima:

```powershell
$env:AURORA_SOCIAL_ADMISSION_INTEGRATION = '1'
$env:AURORA_SOCIAL_ADMISSION_RUST_CLI = 'C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-r4-connect/apps/connect-lab/target/debug/aurora-connect-social-client.exe'
node --test --test-concurrency=1 firebase/tests/social-admission.integration.test.mjs
```

O integrador executou também:

```powershell
cargo clippy --manifest-path apps/connect-lab/Cargo.toml --all-targets --locked --offline --jobs 2 -- -D warnings
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets --all-features --locked --offline --jobs 2 -- -D warnings
cargo fmt --manifest-path apps/connect-lab/Cargo.toml --all -- --check
git diff --check
```

## Fechamento operacional

O integrador interrompeu com Ctrl+C a sessão própria dos emuladores 90309; ela retornou exit 1. A consulta posterior ao sistema encontrou zero listeners nas portas 8187, 9187, 4487, 4587, 9150 e 42888 e zero processos Node/Java/helper correspondentes ao laboratório. O hash do helper final foi reconferido.

Isso registra interrupção e ausência posterior dos recursos consultados, não saída normal dos emuladores ou exit 0 individual de cada um dos seus filhos. Os exits 0 e rebinds dos clientes Rust são provas separadas do runner. Processos pessoais não foram encerrados, e o fechamento anterior de 88B não foi reutilizado como evidência desta rodada.

## Situação por componente

| Componente | Estado deste incremento |
| --- | --- |
| TCP | Contexto assinado até TCP real observado, incluindo 64 KiB após expiração, retirada ativa e cleanup/rebind na rodada final. Falha inicial preservada |
| UDP | Nenhuma nova prova da ponte UDP de aplicação; QUIC não substitui essa validação |
| Direto | Escopo loopback; nenhuma nova evidência entre redes ou computadores |
| Relay | Não usado/habilitado/contratado |
| Minecraft | Não iniciado ou homologado nesta rodada |
| Convites/interface | Expectativa crua removida do helper; runner troca objetos internamente. Entrar gráfico segue desabilitado; sem nova GUI/Tauri |
| Amigos/público | Resolução autenticada local acrescentada; catálogo remoto/serviço de produção não publicado |
| Simple Voice Chat | Sem nova prova de áudio/endereço/transporte/recuperação |
| Plasmo Voice | Sem nova prova de áudio/endereço/transporte/recuperação |

Continuam necessários entrega host↔guest de grant/convite/lease, bootstrap confiável, worker no mesmo ConnectManager/owner, barreiras de troca de conta e interface final. Não criar lifecycle paralelo em SocialView ou aceitar pin/URL fornecido pelo renderer para antecipar essas etapas. R4-005/008 e os demais gates/prioridades permanecem.

## Preservação, limites e rollback

Preservados os nove pares Core/Skins, Assistente desktop e funcionalidades cumulativas de Recovery/CAS/Doctor. Sem Companion/HUD, ampliação da matriz, serviço pago, crédito temporário, push, deploy, publicação, release, instalador, alteração de firewall/configuração pessoal ou operação destrutiva.

A autoridade continua local, opt-in separado e desativada no fluxo normal. Auth/Firestore são emuladores; `alg=none` fica exclusivamente no adaptador local, sem fallback RS256 de produção. Não foram usadas credenciais de produção ou registradas senhas/tokens. Rollback é reverter somente os arquivos 88C e recolher somente seus processos/fixtures, preservando 88A/B, logs, outras worktrees e a proposta de voz.

## O que você precisa fazer agora

Nenhuma intervenção do usuário é necessária para a implementação e validação local deste incremento. Não executar comandos antigos nem iniciar outro computador por causa deste registro.

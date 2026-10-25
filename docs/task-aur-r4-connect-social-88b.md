# Tarefa 88B — admissão social e continuidade com TCP real local

Data: 25/10/2026. Estado: **PROTÓTIPO**, **IMPLEMENTADO MAS NÃO HOMOLOGADO** para publicação; checks locais e fechamento operacional registrados abaixo. Base `23d3fea` do incremento 88A, branch `codex/aur-r4-connect`, worktree `C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-r4-connect`. O checkout principal permanece separado e limpo em `dc2ec5ac36a88f54b1130bac4789f8ecd56641d2`; a base histórica `TESTE-main/aurora-skins-runtime` não foi usada.

Preservados alpha.14 cumulativo, Recovery/CAS/Doctor, Assistente desktop, Core 1.1.5/Skins 0.3.1 e nove pares, sem Companion/HUD ou ampliação da matriz. Os documentos históricos de 88A não foram reescritos por esta tarefa. O arquivo preexistente não rastreado `docs/aurora-connect-voice-session-binding-proposal.md` permanece fora do incremento.

## Entrega e contrato

A [ADR-031](engineering/ADR/ADR-031-social-local-admission.md) define a cadeia separada de posse de chave, consentimento host, admissão individual assinada e continuidade. O [domínio 88B](aurora-connect-social-admission-88b.md) explica o comportamento e seus limites.

Foi implementado um recorte local concreto: uma conta host/uma guest, uma vaga, política social `open`, visibilidade `public` ou `friends`, serviço `game`/TCP e fonte loopback. Dois processos Rust conservam suas chaves RAM desde a prova 88A até TLS/ponte TCP; o runner troca os objetos internamente. O recibo 88A não é aceito como passe enviado pelo cliente.

**Entrar permanece desabilitado no launcher.** Não há integração automática do Tauri, catálogo/autoridade de produção, aprovação de pedidos ou participantes dinâmicos. A fonte usada é eco TCP, não Minecraft. R4-005/008 continuam abertos e os demais gates/prioridades permanecem.

## Arquivos alterados

| Área | Arquivos |
| --- | --- |
| Objetos nativos | `apps/connect-lab/src/social_admission.rs`, `src/lib.rs`: política host, grant, lease, preimagens/assinaturas, expectativas e deadlines originais |
| Clientes reais locais | `apps/connect-lab/src/bin/aurora-connect-social-client.rs`, `tests/social_client_cli.rs`: host/guest de uma tentativa, consentimento, mesma chave, TCP, monitor e encerramento |
| Autoridade nova | `apps/edge-proxy/src/connectSocialAdmission.ts`, `connectSocialAdmission.test.ts`: política atual, reserva única, grants/leases assinados, sequência, limites e falha terminal |
| Handoff 88A interno | `apps/edge-proxy/src/connectSocialBinding.ts`, `connectSocialBinding.test.ts`: `proveRetained` conserva deadline monotônico original sem expor/aceitar metadado por HTTP |
| HTTP local | `firebase/tests/social-binding-server.mjs`: três rotas de admissão sob opt-in separado; 88A preserva sua resposta e limites |
| Prova integrada | `firebase/tests/social-admission.integration.test.mjs`: Auth/SDK/regras reais, dois processos Rust, TCP, bloqueio, perda de backend e cleanup verificável |
| Documentação | ADR-031, domínio 88B e esta tarefa; ponteiros gerais sob responsabilidade do integrador |

O código normal de autenticação RS256, regras remotas, Core/Skins e a interface social de produção não foram ampliados por estes arquivos. Não houve alteração de configuração pessoal ou firewall.

## Resultados realmente executados

Logs privados sob `.aurora/connect-validation/`:

| Verificação | Resultado | Evidência |
| --- | --- | --- |
| Build do helper novo | PASS, perfil dev, 18,37 s | `social-88b-native-build.log` |
| Objetos/autoridade Rust | Seis PASS, 0 falhas, 95 filtrados; execução 0,35 s, compilação do lote 19,80 s | `social-88b-native-unit.log` |
| Processos CLI/preparo, primeira rodada | Quatro PASS, 0 falhas/ignorados; execução 0,68 s, compilação 43,29 s | `social-88b-native-cli.log` |
| Processos CLI/preparo, rodada final | Os mesmos quatro PASS, 0 falhas/ignorados; execução 0,71 s, compilação 1 min 22 s | `social-88b-native-cli-final.log`; não somar como oito testes |
| Autoridades Node combinadas | 41 PASS, 0 falhas/ignorados, 467,7932 ms | `social-88b-authority-unit.log`; inclui a cobertura binding, não somar novamente a rodada focal |
| Admissão social real local, primeira rodada | Quatro TAP PASS, 0 falhas/ignorados, 12224,7953 ms | `social-88b-integration-first.log`; três cenários mais agrupador |
| Admissão social real local, rodada final | Os mesmos quatro TAP PASS, 0 falhas/ignorados, 9666,7213 ms | `social-88b-integration-final.log`; três cenários mais agrupador, sem soma duplicada |
| Regressão da integração 88A | Oito PASS, 0 falhas/ignorados, 1090,9628 ms | `social-88b-binding-regression.log`; resultado desta rodada, sem reatribuir os pins/registros históricos |
| Clippy connect-lab inicial | FAIL `collapsible_if` no Drop do leitor stdin | `social-88b-clippy.log`; ajuste para let-chain pelo integrador |
| Clippy connect-lab final | PASS, todos os targets, 13,04 s | `social-88b-clippy-final.log` |
| Clippy desktop | PASS, todos os targets/features, 28,77 s | `social-88b-desktop-clippy.log` |
| Suíte edge completa | 196 PASS, 9749,9944 ms | `social-88b-edge-tests.log`; inclui os 41 focais, não somar 196+41 |
| TypeScript strict | PASS para binding e admission | Verificação de tipos, distinta de runtime |
| Formatação final | PASS após corrigir ordem de exports de `lib.rs` | `cargo fmt --all -- --check` no manifest connect-lab |
| Diff final | PASS | `git diff --check` |

Os testes de CLI cobrem stop/EOF antes do transporte, stop em estágios de preparo host, aprovação incorreta, troca de UID e expectativa convidada congelada antes da prova. A biblioteca cobre assinatura/contexto/domínio/campos, aprovação, prazos originais, esquema fechado e continuidade terminal. Não interpretar os testes filtrados como executados nem compilação como prova de multiplayer.

### Cenários integrados observados

| Cenário | Prova positiva/negativa | Encerramento |
| --- | --- | --- |
| Público → bloqueio ativo | Conta Aurora host/guest reais no emulador, política aprovada, prova88A/grant/TLS, 1.048.576 bytes em cada direção com EOF, depois fluxo TCP ativo saudável | Bloqueio pelo SDK real; resposta `session_not_authorized`; TCP encerrou em 2760 ms na rodada final e 2792 ms na primeira |
| Amigos → perda do backend | Pedido/aceitação de amizade reais, anúncio `friends`, mesma prova de 1.048.576 bytes em cada direção/EOF e fluxo ativo | Servidor local da autoridade encerrado; observação `authority_unavailable`; TCP encerrou em 2764 ms na rodada final e 2786 ms na primeira |
| Bloqueio antes de join | Bloqueio após prova de chave e antes da admissão | HTTP 403, zero bytes e zero conexões na fonte; nenhum grant enviado aos clientes para iniciar transporte |

Em ambos os cenários ativos, o runner aguardou a retirada automática, sem enviar stop depois do bloqueio/perda de backend. Conferiu host com `reason=continuity_expired`, `admitted=true`, cleanup/rebind verdadeiros; guest com `reason=connection_closed`, cleanup/rebind verdadeiros. Ambos terminaram exit 0, sem encerramento forçado, e o runner fez novo bind na porta TCP convidada. O prazo original de sessão ainda tinha mais de cinco segundos ao fechar, para excluir expiração normal como causa alternativa.

As medições de retirada incluem o intervalo de observação do cenário e não constituem SLA de 3 s ou bloqueio instantâneo. A lease dura até 3000 ms, seguida pelo timer de observação de 20 ms e a latência de cleanup. O timeout externo do ensaio é de 7 s; não o apresentar como prova isolada do teto exato da lease.

O pin da autoridade veio diretamente do objeto servidor criado pelo runner. UID e sessão vieram das contas/estado autenticados; a expectativa guest foi entregue antes do desafio e não extraída do grant. As chaves privadas permaneceram nos clientes nativos; senhas/tokens de fixture não foram passados em argumentos nem adicionados aos registros de resultados. O transporte usa a mesma `SessionInvitation` existente, seus serviços/capacidades e sua autoridade de consumo, sem autoaprovar o fluxo manual do desktop.

## Comandos e identificação do artefato

Da raiz da worktree, os comandos nativos executados foram:

```powershell
cargo build --manifest-path apps/connect-lab/Cargo.toml --bin aurora-connect-social-client --locked --offline --jobs 2
cargo test --manifest-path apps/connect-lab/Cargo.toml --lib --locked --offline --jobs 2 social_admission -- --nocapture
cargo test --manifest-path apps/connect-lab/Cargo.toml --test social_client_cli --locked --offline --jobs 2 -- --nocapture
```

O último comando foi repetido após a correção de estilo do leitor; seu resultado final está separado na tabela. Os checks Node e de formatação/diff também executados foram:

```powershell
node --test apps/edge-proxy/src/connectSocialBinding.test.ts apps/edge-proxy/src/connectSocialAdmission.test.ts
npm --prefix apps/edge-proxy test
cargo fmt --manifest-path apps/connect-lab/Cargo.toml --all -- --check
git diff --check
```

Integração real local:

```powershell
$env:AURORA_SOCIAL_ADMISSION_INTEGRATION = '1'
$env:AURORA_SOCIAL_ADMISSION_RUST_CLI = 'C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-r4-connect/apps/connect-lab/target/debug/aurora-connect-social-client.exe'
node --test --test-concurrency=1 firebase/tests/social-admission.integration.test.mjs
```

Helper exercitado na primeira rodada: `apps/connect-lab/target/debug/aurora-connect-social-client.exe`, **23.484.928 bytes**, SHA-256 **`ca28c785d48ea17133c2f55cdf7a5f71623eb78e0921f4714048babb410f055f`**. Esse é um binário diferente do helper de prova 88A; seus pins não são intercambiáveis. O pin acima pertence à primeira rodada e não deve ser reatribuído ao rebuild após a correção de estilo do Clippy.

Helper reconstruído e exercitado na rodada final, no mesmo caminho: **23.484.928 bytes**, SHA-256 **`6c6fea4b69872fe5cc38d9a9b78c71a71e84d14b2f961369b4b9fa53010966b2`**. A integração foi repetida com as mesmas variáveis opt-in e comando acima, obtendo os quatro TAP PASS finais em 9666,7213 ms. O primeiro pin e seus logs permanecem como evidências históricas da primeira rodada.

## Revisão e correções durante o incremento

1. A revisão encontrou reancoragem do deadline da política a partir de `expires - relógio atual`. Um recuo civil dentro do intervalo assinado poderia ampliar a validade backend apesar do limite original 88A. O helper interno `proveRetained` agora transfere o deadline original da mesma verificação/consumo único; policy/grant usam o menor orçamento e reduzem o vencimento civil emitido quando necessário. O cliente não fornece esse deadline e a resposta HTTP 88A não mudou de formato.
2. O marco de frescor da lease é capturado antes de leitura/digest/assinatura e reconferido depois. Operação atrasada, cancelada, sobreposta ou com sequência inválida não publica continuidade nova nem recupera um grant terminal.
3. A expectativa guest foi explicitada antes do desafio: conta/sessão próprias e UID/chave/operação/geração/política/digests host vêm do runner que criou o contexto. Não aceitar o grant para definir sua própria expectativa.
4. O monitor host roda independentemente de stdin. O guest detecta fechamento da conexão e limpa ponte/endpoint sem depender do runner. Startup retém bind/dial pendente para recolher sucesso tardio antes de publicar disponibilidade; não promete timeout de três segundos para a função de bind da biblioteca.
5. O runner inicialmente aceitava qualquer falha de continuidade como evidência de retirada. Foram exigidos causa, reasons, admissão e prazo original vigente; prejoin também exige zero conexões, além de zero bytes.
6. O runner passou a limitar token/HTTP/monitor/cleanup, priorizar erros de parsing sobre eventos enfileirados e exigir exit 0 sem encerramento forçado. Erros de cleanup são agregados ao erro primário, sem escondê-lo.
7. Revisão focal do consumo de convite: `handshake` mantém `active_leases` travado ao marcar consumo e inserir a conexão; `invitation_consumed` usa o mesmo lock. Não existe janela observável de consumo antes do registro. Uma desconexão real entre as duas observações de estado pode legitimamente encerrar o host.
8. O primeiro Clippy acusou `collapsible_if` no Drop do leitor stdin. O integrador aplicou let-chain, sem alteração pretendida de comportamento, e formatou. O fmt também detectou ordem de exports de `lib.rs`. Clippy e fmt passaram depois; o helper foi reconstruído e os quatro testes CLI e a integração real foram repetidos com novo pin. Os resultados das duas rodadas permanecem separados.

Não foi identificado bloqueador remanescente por leitura após essas correções. Os checks locais confirmados não substituem testes externos nem liberam produção. A validação de limite de 32 políticas, concorrência de registro/vaga, replay, bloqueio, conta e prazo pertence aos testes Node e não é prova de orçamento distribuído ou cobrança zero.

## Fechamento operacional e preservação

O integrador interrompeu com Ctrl+C a sessão própria dos emuladores (sessão de execução 78000); ela retornou exit 1. A consulta posterior ao sistema não encontrou listeners TCP nas portas 8187, 9187, 4487, 4587, 9150 e 42888, nem processos Node/Java/helper correspondentes aos identificadores `social-emulators`, `cloud-firestore-emulator`, `aurora-connect-social-client`, `social-binding-server` ou `social-admission.integration` na consulta CIM realizada.

Isso comprova a interrupção e a ausência posterior dos processos/listeners consultados naquele momento; **não é saída normal dos emuladores, rebind dessas portas globais ou prova individual de exit 0 de cada filho**. Os rebinds e exits 0 dos clientes nativos pertencem às provas específicas do runner, separadas acima. Processos pessoais não foram encerrados.

O checkout principal ficou limpo no HEAD indicado no início. Pins de preservação conferidos pelo integrador: matriz ativa SHA-256 `96C4EC789902F8AA87931ECD72ACA531FEC53D4AC9C6055FCB9837EF77E6918A`; proposta de voz não rastreada SHA-256 `EF0ED6EDC466148F6B771A9349587FF1FE1A3E85919DB88B138B547B4BA8BB08`. Nenhum dos dois arquivos foi modificado por esta tarefa documental.

## Validações ainda fora deste recorte

- Sem novo ensaio GUI nativo ou ligação do botão Entrar; não transferir as evidências gráficas de 87 para o código 88B.
- Sem ensaio externo de dois computadores, Minecraft, áudio ou nove pares neste incremento.
- Não foram executados como cenários reais desta rodada: interrupção isolada de heartbeat mantendo backend saudável, lease inválida durante tráfego e startup tardio induzido. Não promovê-los a resultados observados apenas por leitura ou testes unitários.

## Situação separada por componente

| Componente | Estado |
| --- | --- |
| TCP | Admissão social real local, 1 MiB por direção/EOF e retirada ativa observadas |
| UDP | Sem novo serviço de datagramas; UDP do QUIC não é homologação da ponte UDP de aplicação |
| Direto | Somente loopback nesta rodada; evidências externas anteriores preservadas sem extensão |
| Relay | Não usado/habilitado/contratado, sem créditos temporários |
| Minecraft | Não iniciado ou conectado; fonte de eco não é mundo/jogador/runtime |
| Convites/interface | Troca interna no runner; sem cópia manual nesse ensaio; Entrar desabilitado, nenhuma nova GUI ou autoapprove Tauri |
| Amigos/público | SDK/regras reais exercitados localmente; nenhum serviço social remoto publicado |
| Simple Voice Chat | Sem novo teste de áudio/endereço/transporte/recuperação |
| Plasmo Voice | Sem novo teste de áudio/endereço/transporte/recuperação |

Integração futura precisa colocar a ação social no mesmo `ConnectManager/owner` e nas barreiras de conta existentes. Descoberta/fonte de jogo, produção, pedidos, participantes dinâmicos, seleção de caminho e homologação externa continuam sujeitos a seus contratos e gates. Não criar lifecycle paralelo em SocialView para antecipar a interface.

## Limites, custo e rollback

Os clientes são harnesses locais opt-in. Auth/Firestore são emuladores; `alg=none` permanece restrito ao adaptador local, sem fallback na verificação RS256 de produção. Não há senha/token nos argumentos, pin/URL/projeto escolhido pelo renderer, CORS genérico, credencial de produção, faturamento ou serviço remoto novo.

Não houve push, deploy, publicação, release, instalador, alteração de firewall/configuração pessoal ou operação destrutiva. Rollback: encerrar somente autoridade/helpers/fixtures próprios e reverter os arquivos 88B, preservando 88A, logs, demais worktrees e a proposta de voz não rastreada.

## O que você precisa fazer agora

Nenhuma intervenção do usuário é necessária para este incremento local. Testes externos e a futura interface continuam separados; esta tarefa não solicita credenciais ou comandos em outro computador.

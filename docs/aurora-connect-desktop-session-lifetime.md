# Sessão TCP no desktop — incremento 56

Atualização: 09/10/2026. Estado: **PROTÓTIPO**, feature `connect-lab` desligada por padrão. Base de implementação: `aa84c9e`. O caller desktop integra o [convite de sessão52](aurora-connect-session-lifetime.md) por uma escolha explícita de perfil. Este documento registra o contrato e as provas locais; não homologa janela Tauri, uma hora de estabilidade, Minecraft, voz ou outra internet.

## Contrato do caller

O perfil `sessionGameTcp` autoriza somente `game`/TCP/revisão1/`reliableStream`, com ingresso de1–600s e sessão absoluta de até3600s desde a emissão. A sessão deve durar pelo menos o ingresso. Não há renovação, retomada de convite consumido, UDP ou relay neste perfil. Modos loopback e IPv6 externo continuam opções explícitas, com os filtros anteriores.

Três comandos novos são registrados no Tauri:

- `connect_prepare_session_host(request)` recebe `{mode,bindAddress,gameTarget,admissionSeconds,sessionSeconds}`; durações são inteiros `u16`, validadas nos limites acima.
- `connect_prepare_session_guest(request)` recebe `{mode,expectedHostPublicId}` e fixa o perfil antes da leitura do convite.
- `connect_prepare_session_host_discovered(selectionId,request)` consome a seleção opaca produzida pela descoberta Windows já autorizada, confere origem e conserva seu monitor.

Os comandos existentes de revisão, confirmação, exportação, STATUS, snapshot e encerramento despacham pela autoridade privada fixada durante o preparo: `Legacy(Invitation)` ou `Session(SessionInvitation)`. Não há tentativa automática do outro parser após falha. O modo externo também não é inferido do JSON. A janela proprietária permanece `main`; chaves privadas e autoridade original ficam somente em RAM.

O perfil curto continua padrão. Seus comandos, campos JSON, digest e limite `maxLifetimeSeconds=600` permanecem iguais. Os metadados novos são opcionais com omissão na serialização, sem `null` adicional no perfil antigo. Campos inesperados, ausentes, fracionários ou fora da representação numérica do DTO são recusados.

| Superfície | Metadados do perfil novo |
| --- | --- |
| Capability | `sessionInvitations={profile:"sessionGameTcp",schemaVersion:2,maxAdmissionSeconds:600,maxSessionSeconds:3600}`; ausente no build normal. |
| Preparation | `invitationProfile:"sessionGameTcp"`; ainda não existe `sessionTiming`. |
| Review e export | `sessionTiming={profile:"sessionGameTcp",schemaVersion:2,admissionSeconds,sessionSeconds,admissionExpiresAtMs,sessionExpiresAtMs}`. |
| Snapshot | `invitationProfile` desde preparo; `sessionTiming` próprio desde revisão, conservado durante `starting` e estados ativos mesmo depois de limpar a revisão. |
| Aliases existentes | `review.lifetimeSeconds`, `review.invitationExpiresAtMs` e `export.expiresAtMs` representam ingresso; `snapshot.expiresAtMs` ativo representa fim da sessão. |

Metadados completos entram no digest de revisão. Os segundos do convidado derivam dos intervalos assinados desde `issuedAtUnixMs`, nunca do tempo restante. Ambos os intervalos devem ser segundos inteiros exatos, os três instantes devem ser inteiros seguros para JavaScript, e a diferença dos vencimentos deve equivaler à diferença das durações. Por exemplo,2500ms/4500ms não são apresentados como3s/5s.

Para emitir sem discrepância entre duas leituras do relógio, a biblioteca recebeu APIs aditivas `issue_loopback_for_lifetimes` e `issue_external_ipv6_for_lifetimes`: emissão e vencimentos usam a mesma amostra civil/monotônica. Durações são milissegundos exatos, com os mesmos limites600/3600s; valores submilissegundo são recusados. APIs anteriores de vencimentos absolutos, formato assinado, domínio e verificação permanecem iguais. O caller desktop impõe adicionalmente segundos inteiros.

## Prazos, ciclo e limpeza

A preparação e sua revisão continuam limitadas ao prazo original de60s e ao ingresso. Confirmação confere digest, prazo civil da preparação, seu `Instant` original e autoridade de ingresso antes de iniciar transporte. Repetir revisão ou aprovação não renova esses prazos. O gate civil adicional também protege a preparação do perfil curto.

O adaptador conserva clones da autoridade temporal original. Os timers derivados do tempo restante são ancorados antes da consulta ao objeto e não criam outra autorização. A biblioteca continua verificando o prazo civil e monotônico original em cada fronteira de transporte. O perfil antigo conserva seu timer original e sua autorização nativa.

O Host encerra no fim do ingresso se ninguém consumiu a admissão. Depois de ingresso válido, TCP existente e novos streams podem continuar até o fim absoluto da sessão, revogação, perda da origem ou outro limite anterior. O monitor lê consumo **depois** da verificação temporal: uma admissão válida imediatamente antes da expiração não pode ser descartada usando uma amostra falsa anterior. Exportar é recusado assim que o ingresso vence ou a admissão foi consumida, mesmo após uma conexão muito curta.

O Guest usa prazo de sessão após o ACK. Cancelar durante conexão apenas marca o token: a tentativa limitada nativa é aguardada e um sucesso tardio é encerrado antes de publicar o listener. Consultas STATUS conservam protocolo767, quota30/intervalo20s e parser redigido, com sessão conferida antes e depois do await; resposta não comprova mundo ou jogador.

Descoberta mantém a prova do filho próprio. Há rechecagem antes e depois de bind, além do monitor periódico anterior; perder processo/escuta encerra somente Connect. Testes usam o mesmo filho Rust da suíte de descoberta, com visibilidade ampliada exclusivamente sob `cfg(test)`. Nenhum Java foi iniciado ou encerrado para este incremento.

`stop` e `shutdown_all` continuam aguardando tarefas, consultas pendentes e liberação dos sockets. `Drop` do manager agora cancela o token sem abortar a tarefa: a limpeza é **eventual** enquanto o runtime permanece vivo. `Drop` não oferece join síncrono e não substitui o encerramento aguardado do aplicativo/logout. Testes observam EOF/reset nos dois sockets aplicativos antes de desmontar a fixture, tarefa terminada e rebind. Não há mudança de configuração pessoal a recuperar.

## Provas e histórico de execução

Logs privados ficam em `.aurora/connect-validation`; contêm nomes estáticos de testes e resultados, sem convites, peers, IPs externos ou credenciais. Todas as conexões executadas neste incremento usam loopback e portas efêmeras. Endereços fixos em testes puramente de DTO/revisão não são abertos.

As provas novas cobrem dois managers reais,64KiB por direção antes/depois do ingresso, stream novo após ingresso, revogação com EOF dos dois lados, sessão expirando apesar de atividade, Host sem admissão fechando cedo e zero acesso ao destino, STATUS após ingresso, shutdown, descoberta e perda da origem, schema cruzado, preservação exata dos campos/digest legado, DTO inválido, segundos assinados, frações iguais/diferentes, corrida causal de consumo e expiração civil injetada sem alterar o relógio do sistema. Drop é exercitado com Host/Guest ativos e durante dial UDP real silencioso.

Falhas preparatórias preservadas:

1. `desktop-session-56-first-check.log`: falha de compilação ao tentar chamar verificação privada do convite legado. O adaptador foi corrigido para usar expiração pública, seu timer já capturado e os checks nativos existentes, sem revalidar bytes para renovar prazo.
2. `desktop-session-56-focused-first.log`:14PASS/1FAIL; o oráculo da revisão legada contava24campos, mas a forma anterior tem21. Corrigido somente o número esperado; campos novos continuam omitidos.
3. `desktop-session-56-focused-final.log`:19PASS/1FAIL; o novo oráculo civil esperava `failed`, enquanto a transição de expiração existente é `closed` com `connect_plan_expired`. Corrigida somente a expectativa.

Comandos realmente executados desde a raiz da worktree, sempre offline/locked e dois jobs:

```powershell
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --lib --features connect-lab
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --lib --features connect-lab engine::connect:: -- --test-threads=1
cargo test --manifest-path apps/connect-lab/Cargo.toml --target-dir .aurora/connect-validation/relay50-target --offline --locked --jobs 2 --lib session_invitation::tests -- --test-threads=1
cargo test --manifest-path apps/connect-lab/Cargo.toml --target-dir .aurora/connect-validation/relay50-target --offline --locked --jobs 2 --test session_lifetime -- --test-threads=1
cargo test --manifest-path apps/connect-lab/Cargo.toml --target-dir .aurora/connect-validation/relay50-target --offline --locked --jobs 2 --lib civil_advance -- --test-threads=1
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --lib -- --test-threads=1
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --lib --features connect-lab -- --test-threads=1
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --lib --features 'connect-lab,connect-voice-lab' -- --test-threads=1
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-targets
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-targets --features connect-lab
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-targets --all-features
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-targets -- -D warnings
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-targets --all-features -- -D warnings
cargo clippy --manifest-path apps/connect-lab/Cargo.toml --target-dir .aurora/connect-validation/relay50-target --offline --locked --jobs 2 --all-targets -- -D warnings
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --all -- --check
cargo fmt --manifest-path apps/connect-lab/Cargo.toml --all -- --check
git diff --check
```

Resultados finais: foco desktop **20PASS/0FAIL**,43,52s; codec **14PASS**,0,25s; integração52 **7PASS**,8,45s; relógios civis52 **2PASS**,0,38s. Desktop completo default **151PASS/3ignorados**,31,29s; `connect-lab` **175PASS/4ignorados**,73,29s; combinação `connect-lab,connect-voice-lab` **207PASS/19ignorados**,80,89s. Os focais estão contidos na suíte completa; não somar como testes distintos. Nenhum teste ignorado foi habilitado por esta execução. Os três checks all-targets passaram (11,96s/11,64s/13,30s), Clippy desktop default/ambas passou (9,52s/11,10s), Clippy próprio da biblioteca passou (5,63s), ambos fmtchecks e diffcheck passaram. Não houve correção de produção depois da rodada focal final.

Logs correspondentes: `desktop-session-56-check.log`, `desktop-session-56-focused-pass.log`, `desktop-session-56-codec.log`, `desktop-session-56-library-integration.log`, `desktop-session-56-library-civil.log`, `desktop-session-56-{default,connect,combined}-full.log`, `desktop-session-56-check-{default,connect,combined}.log`, `desktop-session-56-clippy-{default,combined}.log`, `desktop-session-56-library-clippy.log`, `desktop-session-56-fmt-{desktop,library}.log` e `desktop-session-56-diff-check.log`. O snapshot de hashes das fontes entregues fica em `desktop-session-56-source-freeze.json`, fora do Git.

Lock desktop SHA256 `e86ea22115e3d300a9f27c048eb417aa7811e87f852e84af79963482aac902b0`, lock Connect `e4cc7ab514091230e772ee1e3e80ec67a7bf05733e924624dd135e200e3048f3`, matriz `96c4ec789902f8aa87931ecd72aca531fec53d4ac9c6055fcb9837ef77e6918a`: sem alteração. Artefato privado fixado pelo caller53 preservado, SHA256 `adcc23b75c5b4222fba582c18ccf4147c46e984197e9724e9dae1a4cd26b0423`. Nenhum kit histórico foi repinado.

## Limites e rollback

| Área | Estado neste incremento |
| --- | --- |
| TCP/convites/interface | Perfil explícito implementado no caller e painel; provas locais de transporte e fixtures DOM não equivalem a execução gráfica Tauri. |
| UDP | Sem autorização no perfil52/56; mantém contratos e provas anteriores separados. |
| Direta/relay | Loopback exercitado; IPv6 externo permanece API explícita sem nova prova entre PCs. Relay desligado. |
| Minecraft | STATUS sintético do protocolo767; não houve Java, entrada gráfica, LAN real ou conta mista56. |
| Simple Voice Chat | Não exercitado56; sem áudio homologado. |
| Plasmo Voice | Não exercitado56; homologação separada permanece pendente. |
| Conjunto | Nove pares Core/Skins, Assistente desktop, recovery/CAS/Doctor e demais gates preservados. Amostras curtas não comprovam1h nem estabilidade de múltiplos jogadores. |

Rollback operacional: encerrar Connect e aguardar `cleanupState=complete`; logout/saída aguardam `shutdown_all`. O build sem `connect-lab` mantém o transporte indisponível e omite a capability nova; o comando de descoberta retorna `discovery_unavailable` nesse build. Rollback de código deve ser um revert local revisado do incremento56, preservando53/54 e outras alterações em andamento. Não remover instâncias, configurações, snapshots, worktrees ou kits históricos.

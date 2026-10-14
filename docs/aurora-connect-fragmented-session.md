# Sessão TCP e UDP com ingresso separado — incremento61

Data: 14/10/2026. Base cumulativa `f89eab9`, branch `codex/aur-r4-connect`. Estado: **PROTÓTIPO**, somente APIs explicitamente selecionadas no laboratório. Este incremento remove o prazo único do perfil UDP quando o caller escolhe o contrato novo. Não altera os convites anteriores, o kit externo ou o perfil desktop57.

## Contrato e decisão

O perfil `sessionFragmentedGameUdp` tem envelope de convite **schema3**, domínio de assinatura próprio `aurora/connect-lab/session-invitation/v3\0` e os mesmos dois prazos absolutos do [perfil TCP52](aurora-connect-session-lifetime.md): ingresso até600s e sessão até3600s desde a emissão. Sessão não pode terminar antes do ingresso. Atividade, reabertura de serviço, clonagem do objeto ou novo listener não renovam nenhum prazo. UDP conserva o envelopeV2, independente da versão do convite.

A escolha de um perfil separado preserva o verificador estrito de schema2/gameTCP e os bytes canônicos existentes. As APIs antigas recusam o novo objeto antes de abrir endpoint. O modo IPv6 externo continua escolha explícita, sem inferência a partir do JSON ou fallback. Emissão/verificação conserva assinatura Ed25519 existente, pin independente, destinatário, nonce, sessão, geração1, limite4096B e recusa de representação não canônica. Nenhuma dependência nova.

Os dois grants são obrigatórios e exatos, em ordem canônica game/aux:

| Serviço | Contrato assinado |
| --- | --- |
| game | TCP, revisão1, `reliableStream`. |
| aux | UDP, revisão1, `fragmentedDatagram`, original4096B, retorno `authorizedDownstream`16384B/s e20frames/s. |

O registro local exige destinos literais loopback com porta não zero; o destino privado não viaja no convite. Game mantém `maxDatagramBytes=1400`/`ReplyBudget` no registro genérico. O retorno UDP consome orçamento por fragmento incluindo header; não representa20mensagens originais/s nem uma calibração para áudio. Permanecem frame até1264B, remontagem limitada, fila curta, isolamento por participante/sessão/serviço/associação e recusa integral de4097B. [Framing e quotas49](aurora-connect-fragmented-udp.md).

APIs novas de `SessionInvitation`: `issue_fragmented_loopback_for_lifetimes`, `issue_fragmented_external_ipv6_for_lifetimes`, `verify_fragmented_loopback` e `verify_fragmented_external_ipv6`. As durações usam uma só amostra civil/monotônica e precisão exata em milissegundos. `Host::start_invited_fragmented_session` e `Guest::connect_invited_fragmented_session` recebem o objeto original; variantes `_external_ipv6` exigem o opt-in de endereço correspondente. Não há renovação, recuperação de convite consumido, persistência de identidade ou emissão social remota.

## Autoridade durante UDP

Admissão reutiliza TLS completo, pin/peer, prova de digest/geração, capacidades exatas, consumo único e lock de revogação existentes. O Host conserva o consumo depois do vencimento do ingresso. Após admitir, streams e associações autorizados podem ser criados até o fim da sessão, inclusive depois do ingresso vencer. Revogação e fim da sessão fecham os recursos.

Cada associação UDP conserva a autoridade temporal original. As fronteiras de envio, remontagem, fila e entrega ao aplicativo a conferem diretamente; o watcher continua responsável por encerrar e recolher recursos. Assim, um watcher ainda não agendado não é a única barreira a payload já enfileirado. O Host revalida depois de bind/connect UDP e antes de publicar a associação. O Guest revalida listener/associação antes de publicá-los. Entrega local aguarda socket gravável, revalida e usa envio imediato limitado, sem uma nova espera entre autorização e envio. Perda UDP permanece possível e não dispara retransmissão automática.

Os relógios civis privados dos testes não alteram o Windows. Um recuo anterior à emissão ou avanço até o vencimento invalida a sessão; clones mantêm deadlines monotônicos originais. A autoridade de relógio não substitui revogação ou identidade autenticada. Recursos de aplicação são apenas loopback; nenhuma porta de administrador, firewall, roteador ou configuração pessoal é alterada.

## Validação

Foco de integração final: **7 PASS, 0 falhas/ignorados**,9,77s, compilação27,03s; log privado `session-udp-61-integration-final.log`. A rodada inicial dos cinco primeiros casos passou5/5,8,55s; não somar rodadas. Os sete casos finais usam Host/Guest reais e destinos próprios loopback:

- TCP com133120B por direção, incluindo4096B de progresso observado durante UDP; duas origens UDP simultâneas e isoladas. Original4096B íntegro,4097B recusado sem atingir o endpoint e envio válido posterior;8204B UDP por direção. Ingresso2s/sessão15s, continuidade e nova associação depois de vencer ingresso.
- Sessão4s encerra apesar de atividade; revogação fecha recursos, replay e ingresso tardio são recusados sem chegar aos serviços. Cleanup aguardado e rebind dos recursos próprios.
- Três tentativas com 0-RTT aceito pela pilha — replay, prova inválida e capacidades incorretas — enviam operações TCP/UDP antecipadas, são recusadas pela aplicação e não alcançam destinos protegidos. A fixture não observa dinamicamente o intervalo pré-TLS; não extrapolar essa evidência.
- Perfis/modos/chaves e dez registros locais divergentes recusados com endpoints de controle sem I/O. Confirmações de contagem de bytes são da fixture, não medição de rede externa.

Foco unitário `session_`: **27 PASS,0 falhas/ignorados**,32filtrados,1,03s; log `session-udp-61-unit-first.log`. Inclui22 testes de codec, dois anteriores de relógio TCP e três novos de autoridade UDP antes de executar o watcher. O codec conserva um golden do schema2 e testa o schema3, assinatura/canonicalização, campos desconhecidos, perfis cruzados, prazos, registro e clones. Guest exercita retorno fragmentado2048B realmente enfileirado e depois clock privado inválido: envio, recepção, abertura, capacidade e publicação de listener recusam enquanto os tokens ainda não foram cancelados. Host confere o gate de fragmento com a autoridade original e revogação. Não é uma prova de backpressure forçado do socket UDP.

Regressão padrão de todos os targets: **231 PASS,0 falhas,4 ignorados históricos**,27grupos,145,15s somados de testes e2m52s de compilação. Log `session-udp-61-full-default.log`. Com `local-relay-lab`: **241 PASS,0 falhas,5 ignorados históricos**,27grupos,151,56s somados de testes e3m45s de compilação; log `session-udp-61-full-relay.log`. Opt-ins ignorados não foram ativados. Relay desta suíte é próprio e local, sem serviço público. Não somar os dois conjuntos sobrepostos.

Comandos realmente executados a partir da worktree (logs em `.aurora/connect-validation/`):

```powershell
cargo test --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --target-dir .aurora/connect-validation/relay50-target --test fragmented_session_lifetime -- --test-threads=1
cargo test --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --target-dir .aurora/connect-validation/relay50-target --lib session_ -- --test-threads=1
cargo test --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --target-dir .aurora/connect-validation/relay50-target --test fragmented_session_lifetime -- --test-threads=1 --nocapture
cargo test --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --target-dir .aurora/connect-validation/relay50-target --all-targets -- --test-threads=1
cargo test --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --target-dir .aurora/connect-validation/relay50-target --features local-relay-lab --all-targets -- --test-threads=1
cargo clippy --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --target-dir .aurora/connect-validation/relay50-target --lib -- -D warnings
cargo clippy --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --target-dir .aurora/connect-validation/relay50-target --all-targets -- -D warnings
cargo clippy --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --target-dir .aurora/connect-validation/relay50-target --features local-relay-lab --all-targets -- -D warnings
cargo fmt --manifest-path apps/connect-lab/Cargo.toml --all -- --check
git diff --check
```

Clippy da biblioteca e de todos os targets padrão/relay, formatação e diff passaram. Logs finais `session-udp-61-clippy-{default,relay}.log`. QA independente conferiu autoridade, isolamento, espera/entrega e teardown sem bloqueador restante. Os testes não iniciaram Java nem alteraram relógio do sistema, firewall ou configurações pessoais. Os dois lockfiles, matriz ativa e executáveis53 imutáveis foram reconferidos por SHA-256 sem alteração.

## Limites e rollback

Este contrato não homologa uma hora de estabilidade, áudio, Minecraft, interface gráfica ou outra internet. Os callers desktop, CLI e Node anteriores mantêm seus contratos até incremento próprio. TCP/UDP seguem **PROTÓTIPO**; conexão direta externa mantém as evidências históricas **PARCIAIS**; relay público desligado; SVC e Plasmo separados, áudio **PRECISA DE TESTE**. Não seleciona mod, não aplica anúncio, não lança Java e não amplia os nove pares Core/Skins.

Rollback operacional: aguardar `shutdown` de pontes, Guest e Host, liberar handles e conservar evidências. Rollback de código: revert local revisado do incremento61, preservando perfis/commits anteriores, worktrees, snapshots e kits imutáveis. Nenhuma configuração pessoal precisa de restauração. Nenhum custo, push, deploy, release ou instalador.

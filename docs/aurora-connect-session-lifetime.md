# Ingresso e duração da sessão — incremento 52

Atualização: 09/10/2026. Estado: **PROTÓTIPO** opt-in na biblioteca `apps/connect-lab`, sobre `3acfad4`. `SessionInvitation` está registrado em `lib.rs`; Host e Guest têm APIs explícitas para ingresso e sessão com prazos distintos. CLI, kit, caller Node e desktop continuam usando seus contratos anteriores. Provas locais cobrem TCP já aberto e novo stream depois do ingresso, recusa tardia no Host, replay, revogação, expiração e relógio civil. Isso não homologa uma hora de estabilidade, Minecraft, voz, GUI ou outra internet.

## Motivo e limite deste passo

Continuidade56: o [caller desktop](aurora-connect-desktop-session-lifetime.md) adota estas APIs por um perfil explícito, preservando o perfil curto. Foram acrescentadas `issue_loopback_for_lifetimes` e `issue_external_ipv6_for_lifetimes`: calculam emissão e ambos os vencimentos com a mesma amostra civil/monotônica, validam durações representáveis exatamente em milissegundos e reutilizam o mesmo codec. Isso evita a diferença entre o relógio do caller e uma nova leitura durante a emissão. APIs de vencimentos absolutos, domínio, corpo assinado e verificadores52 permanecem iguais. O desktop exige adicionalmente segundos inteiros exatos; essa restrição não é uma alteração do codec. O restante deste documento registra o escopo e as provas originais52.

O convite atual de schema 1 usa um único vencimento para ingresso e sessão inteira, limitado a 600 s. O host volta a conferir esse vencimento em pedidos de serviços e no encerramento. Aumentar somente um timer da interface ou uma lease não separaria esses contratos com segurança. O caller Node continua com seu perfil loopback de 120 s e os demais caminhos conservam seus limites atuais.

O incremento cria um tipo distinto `SessionInvitation`, envelope de schema 2 e domínio de assinatura próprio. O primeiro perfil tem apenas `game`/TCP, revisão 1 e `reliableStream`. UDP fragmentado também usa uma versão 2 em seu próprio protocolo, mas essa numeração é independente: o convite novo não autoriza UDP, áudio ou relay. Uma sessão autenticada pode continuar abrindo o serviço TCP autorizado depois que a janela de ingresso acabou, até seu prazo absoluto ou encerramento anterior.

## Contrato assinado

Envelope canônico UTF-8 `{body,signature}`, no máximo 4096 B, sem espaços ou terminadores adicionais. A assinatura Ed25519 usa a implementação estrita já disponível em Iroh 1.3.0, sobre `aurora/connect-lab/session-invitation/v2\0` seguido do JSON compacto da struct `Body`, na ordem abaixo. SHA-256 do envelope exato identifica a revisão. Não há dependência nova nem implementação própria de criptografia.

| Campo de `body` | Regra |
| --- | --- |
| `schemaVersion` | Exatamente 2. |
| `hostKey`, `guestKey` | Chaves públicas distintas, 64 caracteres hexadecimais minúsculos; host deve corresponder ao pin obtido por canal independente e guest ao destinatário esperado. |
| `sessionId` | Identificador limitado pela mesma regra local de IDs já existente. |
| `invitationId` | Nonce público novo de 32 B, representado por 64 caracteres hexadecimais minúsculos; zero integral é recusado. A emissão usa o gerador criptográfico existente e não persiste a chave temporária. |
| `issuedAtUnixMs` | Instante absoluto de emissão; emissão no futuro é recusada. |
| `admissionExpiresAtUnixMs` | Maior que emissão e no máximo 600 s depois dela. |
| `sessionExpiresAtUnixMs` | Maior que emissão, não anterior ao vencimento do ingresso e no máximo 3600 s depois da emissão. |
| `generation` | Exatamente 1 neste perfil. Não há API para aumentar geração ou renovar. |
| `directAddress` | Endereço inicial literal aprovado; loopback ou IPv6 externo somente pela API correspondente. |
| `services` | Exatamente um objeto: `serviceId=game`, `protocol=tcp`, `revision=1`, `requiredCapability=reliableStream`. |

Todos os campos, inclusive os dois vencimentos, geração, nonce, endereço e grant, entram na assinatura e no digest. Campos desconhecidos ou duplicados, formatos alternativos, hex maiúsculo, espaços, sufixos e representação que difira da reserialização são recusados. O domínio antigo não valida o envelope novo. O destino local de Minecraft não viaja no convite: continua dependendo do registro de serviços aprovado no host.

Ao emitir, o codec aceita somente um `Service` TCP/game exato, destino literal loopback com porta não zero, revisão 1, capability `reliableStream` e política `ReplyBudget`; os campos UDP presentes no tipo genérico não ampliam este perfil. A prévia contém dados públicos para aprovação explícita, incluindo os dois prazos. Ela não é um evento redigido para logs; convite, IP, peers e nonce reais ficam fora do Git e das evidências compartilháveis. Chaves privadas nunca são serializadas pelo codec.

## APIs e relógios

As APIs são `SessionInvitation::issue_loopback`, `verify_loopback`, `issue_external_ipv6` e `verify_external_ipv6`, com `SessionDeadlines` para os dois vencimentos absolutos. Modo externo não é inferido do texto recebido. O filtro IPv6 reaproveita a classificação conservadora explícita do [contrato 45](engineering/AURORA_CONNECT_SPEC.md#811-modo-externo-ipv6-explícito--incremento45); isso não prova alcance, ausência de tradução ou pin de todos os caminhos internos. O codec não faz I/O de rede, descoberta ou bind.

Cada valor verificado conserva dois deadlines monotônicos, calculados a partir da mesma amostra de relógio coletada antes do trabalho criptográfico. Emissão e verificadores públicos conferem novamente o ingresso antes de retornar. `Clone` copia os deadlines originais. O prazo absoluto de 3600 s é contado desde a emissão, não promete uma hora adicional depois do login e não é renovado por atividade.

- `check_admission_current` recusa ingresso no vencimento exato por qualquer relógio e também exige sessão vigente.
- `check_session_current` independe do vencimento do ingresso; recusa o vencimento da sessão por qualquer relógio.
- `admission_remaining` e `session_remaining` conferem a autoridade temporal correspondente e retornam o menor tempo restante entre o vencimento Unix assinado e o deadline monotônico original, com uma única amostra de relógio por chamada. Não geram um novo prazo a partir da duração máxima.
- Ambos recusam relógio civil anterior à emissão. Uma volta do relógio dentro do intervalo válido não estende os deadlines monotônicos de um objeto já mantido em RAM.
- Verificar novamente os bytes é uma tentativa de novo ingresso: depois do prazo de ingresso, o verificador recusa mesmo que a sessão absoluta ainda tenha tempo. O Host conserva o objeto e o consumo originais, sem reconstituí-los para renovar prazo.

Uma verificação criptográfica ou temporal não prova posse da chave, admissão, consumo único ou autorização de serviço. Nenhum desses estados foi adicionado ao codec. Reiniciar o processo ou restaurar chaves/sessão para reaproveitar convites não é suportado; monotonicidade e proteção de replay não são um armazenamento durável.

## Integração do transporte

`Host::start_invited_session` e `Guest::connect_invited_session` aceitam somente o perfil novo com endereço loopback. Os pares `start_invited_session_external_ipv6` e `connect_invited_session_external_ipv6` exigem endereço IPv6 externo explicitamente aprovado. Não existe conversão automática para `Invitation`/schema 1. Defaults recusam o envelope novo; os tipos distintos impedem entregar um `SessionInvitation` à API antiga. Plano CLI, revisão/digest de UI e callbacks ainda terão de distinguir os dois prazos antes de adotar este perfil. O adaptador desktop permanece fora deste passo.

O Host completa TLS e valida pin/peer, sessão, nonce, geração, prova, capacidades e prazo de ingresso antes do consumo único sob o mesmo lock usado por revogação. `authorize_admission` roda antes e dentro desse lock. ACK perdido ou desconexão não devolvem o uso. Consumo permanece em RAM até o host encerrar, inclusive depois do vencimento de ingresso. Não se promete desativação global de 0-RTT upstream.

Depois da admissão, `authorize_session` confere a autoridade da sessão já admitida, sua geração/revogação, prazo absoluto e serviço aprovado, inclusive depois do acesso TCP assíncrono ao destino. A recusa de novos ingressos não derruba por si só uma sessão existente. Revogação ou expiração fecham a conexão do participante e seus fluxos; o endpoint do Host permanece disponível para observar a recusa de novas tentativas até `Host::shutdown`. Guest cancela as pontes e fecha seu endpoint. Chamadas de encerramento aguardam as tarefas e liberam sockets. Os timers de inatividade continuam independentes, limitados a 600 s no transporte, e podem encerrar uma sessão antes do limite absoluto; este incremento não cria keepalive.

Retomada e renovação ficam fora do perfil. Uma retomada futura exigirá decisão explícita, novo nonce/generation e autorização vinculada ao estado vivo do host, sem reviver convite consumido. Isso requer contrato e provas próprios antes de implementação.

### Fronteiras implementadas e UI pendente

A primeira preparação foi isolada durante o ajuste autorizado do relógio do anfitrião. A integração atual é posterior a 51b/47c e usa somente tempo injetado em objetos `cfg(test)` para provas de avanço civil; nenhum relógio ou configuração do sistema é alterado.

| Local | Comportamento e invariantes |
| --- | --- |
| `apps/connect-lab/src/host.rs`: `LeaseState`, fábricas `start_invited*` | Autoridade privada `Legacy(Invitation)` ou `Session(SessionInvitation)`, mantendo o modo manual sem convite. Confere antes de I/O host, bind, sessão, destinatário, nonce, geração e exatamente o serviço local `game`/TCP/revisão 1; após criar endpoint, reconfere ingresso e fecha o próprio endpoint se venceu. A única lease usa nonce como ID, destinatário assinado como peer, somente `game` e vencimento da sessão. APIs antigas conservam seus limites. |
| `host.rs`: `authorize_admission`, `authorize_session`, `handshake`, `serve_peer` | Handshake confere ingresso antes e dentro do lock de consumo/revogação; pedidos posteriores conferem sessão. TLS completo, capacidades exatas, consumo antes do ACK, recusa de replay e consumo persistente em RAM após desconexão preservados. |
| `host.rs`: temporizador da sessão e tarefas TCP | Encerramento deriva de `session_remaining` do objeto original; revogação usa token original. Watcher independente de controle reavalia o relógio civil a cada até 100 ms e conserva o deadline monotônico original. Salto civil pode encerrar mesmo com leitura de controle travada. Resolução depende também do escalonamento; não é garantia em tempo real. |
| `apps/connect-lab/src/guest.rs`: fábricas `connect_invited_session*`, `Inner`, `shutdown` | Conserva autoridade original em RAM; confere ingresso antes da conexão e depois do TLS, sessão depois do ACK e nas aberturas/respostas/listener TCP. ACK atrasado não renova nem exige novo ingresso. Watcher retém somente clones de endpoint/conexão/token/autoridade, sem `Arc<Inner>` circular. `shutdown` aguarda receive e watcher. |
| `apps/connect-lab/src/wire.rs`: `InvitationProof` | Sem alteração: digest e geração vinculam a autoridade tipada guardada pelo Host. Digest cobre o domínio e o corpo de schema 2. Recusa cruzada entre os schemas é testada em ambos os sentidos. Hello/ALPN e framing UDP permanecem anteriores; envelope de convite 2 não muda seus números de versão. |
| `apps/desktop/src-tauri/src/engine/connect/{mod.rs,live.rs}` e painel Connect | Criar um perfil explícito posterior, incluindo ingresso e sessão no pedido, revisão e digest. Manter a revisão limitada ao preparo de 60 s e ao ingresso; mostrar separadamente “Prazo para entrar” e “Encerramento da sessão”. O snapshot ativo usa o prazo da sessão, enquanto exportar convite deixa de ser permitido no consumo ou no vencimento do ingresso. Não ampliar silenciosamente o `lifetimeSeconds` atual nem a capacidade antiga de 600 s. |

A revisão desktop deve preservar o vínculo ao processo/serviço observado (`SourceGuard`), a operação única por janela, aprovação explícita, limpeza no logout e na saída, feature desligada por padrão e consulta STATUS restrita ao protocolo 767 existente. O novo prazo de transporte não autoriza iniciar ou prolongar Java, alterar whitelist, matar o processo do jogo ou habilitar voz. O caller Node de 120 s continua fora deste perfil.

O gate adicional de autorização prova que o host usa o prazo de ingresso **somente para admitir**, mas conserva geração, consumo e revogação ao autorizar cada novo stream depois dele. Um timer maior sem essa divisão não satisfaz o contrato. Os oráculos de relógio civil avançado operam em sessões reais com tempo injetado somente no objeto de teste, sem modificar o relógio do sistema.

O codec contém helpers estreitos para conferir a configuração do host, o modo de endereço, a prova e os campos derivados da lease. A validação do registro local continua exigindo o serviço exato; não se copia um JSON arbitrário para `HostConfig`. `lease()` projeta somente o nonce, destinatário, serviço `game` e vencimento absoluto da sessão. Isso não é uma autorização independente: o Host guarda o objeto original e confere também ingresso, consumo, geração e revogação. Nenhuma dessas projeções verifica os bytes novamente para recriar deadlines. Os getters temporais existentes conferem apenas tempo: não substituem o token de revogação nem o estado de consumo guardados pelo host. A classificação de endereço usa o mesmo helper explícito já existente no contrato 45.

O maior risco de regressão seria trocar indiscriminadamente o significado de `Invitation::check_current`: ele permanece inalterado para todos os caminhos de schema 1, inclusive convites fragmentados 51b. A autoridade nova tem métodos separados para ingresso e sessão, delegando ambos ao comportamento antigo quando a variante é `Legacy`. O timer de sessão usa o objeto retido; o vencimento do ingresso não remove o registro consumido e não fecha uma conexão já admitida. A expiração da revisão desktop, por sua vez, continua impedindo uma confirmação ainda não realizada.

## Verificação executada

Os 12 testes do codec passaram: assinatura/digest canônicos, pins/destinatários, adulteração, domínio antigo recusado, fronteiras 600 s/3600 s, overflow, geração/nonce/grants, entrada limitada/não canônica, modo de endereço e ausência de chave privada. Incluem projeção de lease/prova/capability e 14 alterações inválidas da configuração Host. Relógios privados verificam expiração independente, clones sem renovação e menor tempo restante entre os dois relógios.

Dois testes adicionais no Host executam pares reais com TCP ativo e relógio civil injetado somente no objeto Host ou somente no Guest. Um contador `cfg(test)` confirma que o Host entrou na leitura de um frame de controle incompleto antes do salto. EOF/reset é observado nas duas pontas TCP em até 2 s, enquanto o deadline real de 60 s ainda é válido. Isso prova que o watcher opera independentemente da leitura travada. A rodada focada final passou **14/14** em 0,56 s, incluindo os 12 casos do codec.

`tests/session_lifetime.rs` passou **7/7** em 8,45 s na rodada focada corrigida:

- Ingresso de 2 s/sessão de 12 s: uma conexão TCP transmite 64 KiB por direção antes e depois do ingresso; outra é criada depois dele e transmite mais 64 KiB por direção. Replay durante ingresso é recusado, consumo continua marcado depois do prazo; revogação fecha ambos os clientes e os dois sockets do destino antes da limpeza da fixture.
- Ingresso tardio é recusado localmente e por um cliente com Hello bruto/TLS correto/prova original no Host, `denied=1`, sem consumir e sem acessar destino.
- Sessão absoluta de 3 s fecha TCP ativo nas duas pontas apesar da transferência depois do ingresso, sem atividade renovar prazo.
- Provas/destinatários/capacidades/geração/grants inválidos e provas cruzadas schema 1↔2 são recusados com canário de destino zerado. O convidado legítimo ainda é admitido depois das intrusões não consumidoras.
- APIs explícitas recusam modo de endereço e lease com prazo errado antes de tentar bind; um socket de reserva mantém a porta ocupada durante o gate.
- Resumption real exige stream 0-RTT e `ZeroRttStatus::Accepted`, enfileira Hello repetido e Open TCP antes da resposta e observa `replay` com zero acesso ao destino. Não é medição temporal pré-TLS nem desativação global de early data.
- Servidor autenticado controlado recebe ingresso válido e retarda ACK além do ingresso; Guest aceita enquanto a sessão original permanece válida, sem alterar seu vencimento e sem tocar destino TCP.

Falhas preparatórias preservadas: primeira integração teve 4 PASS/2 FAIL porque o oráculo esperava o código da CLI `expired_or_revoked`, enquanto o wire existente transmite `expired`. Após corrigir somente o oráculo, 6/6 passaram. O caso adicional de ACK inicialmente recebeu `Protocol` porque a fixture enviava `{}` em vez dos campos obrigatórios de `Reply`; o servidor de teste foi corrigido para o wire existente. Não houve flexibilização do protocolo para fazer esses testes passar.

Comandos realmente executados em PowerShell, sem dependência nova, usando exclusivamente `.aurora/connect-validation/relay50-target`:

```powershell
$env:CARGO_TARGET_DIR = 'C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-r4-connect/.aurora/connect-validation/relay50-target'
cargo test --manifest-path apps/connect-lab/Cargo.toml --locked --offline --jobs 2 --lib session_ -- --test-threads=1
cargo test --manifest-path apps/connect-lab/Cargo.toml --locked --offline --jobs 2 --test session_lifetime -- --test-threads=1
cargo fmt --manifest-path apps/connect-lab/Cargo.toml --all
cargo test --manifest-path apps/connect-lab/Cargo.toml --locked --offline --jobs 2 --all-targets
cargo test --manifest-path apps/connect-lab/Cargo.toml --locked --offline --jobs 2 --all-targets --features local-relay-lab
cargo clippy --manifest-path apps/connect-lab/Cargo.toml --locked --offline --jobs 2 --all-targets -- -D warnings
cargo clippy --manifest-path apps/connect-lab/Cargo.toml --locked --offline --jobs 2 --all-targets --features local-relay-lab -- -D warnings
cargo fmt --manifest-path apps/connect-lab/Cargo.toml --all -- --check
cargo build --manifest-path apps/connect-lab/Cargo.toml --locked --offline --jobs 2 --bin aurora-connect-lab --no-default-features
```

Regressões completas: **197 PASS / 4 ignorados** no modo padrão e **207 PASS / 5 ignorados** com `local-relay-lab`, 23 grupos em cada rodada e zero falha. Os casos focados acima são subconjuntos desses totais, não testes adicionais. Clippy `--all-targets -D warnings` passou sem e com a feature; formatação/check e `git diff --check` também passaram. A feature de relay continua desligada por padrão; sua suíte usa somente listener próprio loopback, sem provedor público.

Logs privados: `session-52-unit-final.log`, `session-52-integration-final.log` (falha da fixture ACK), `session-52-integration-ack-fixed.log`, rodada anterior `session-52-integration.log`, `session-52-full-default.log`, `session-52-full-relay.log`, `session-52-clippy-default.log` e `session-52-clippy-relay.log`. Não incluem chaves, endereços reais ou convites. `Cargo.lock` conservou SHA-256 `e4cc7ab514091230e772ee1e3e80ec67a7bf05733e924624dd135e200e3048f3`.

Após as suites, build explícito sem features passou em 19,35 s; log `session-52-build-default.log`. O executável somente no target privado tem 24.400.896 B e SHA-256 `38cb6dc40f05ad229ef0e2ae80c99b1cbdfce434836c05dd30bea11ec490ecb9`. Não substitui pins, debug principal ou kits históricos automaticamente. A existência desse binário não acrescenta comandos CLI para schema 2.

Snapshot de produção testado: `host.rs` SHA-256 `748a3771cdc6a3d61bf1f8ffae22fd102716895ac067c40cfd1e325509cc30a4`; `guest.rs` `26970b5223ba8fda511e1413b69d55fc6683842706e3a8c69b20fbfcff724425`; `lib.rs` `daeed5798f61103b0f978c8fc8de8bdb6f6f0250b5bb390dc18fdb6e293c6e7c`; `session_invitation.rs` `a365150ecb75c124b611f3728886f17f99c41613a6fd3585591447b1da803f37`. Teste `session_lifetime.rs`: `08061dfdd1fbd392df0e869663c07aea79b612e5101468ac87e605a3adb74fd8`. Revisão estática independente acompanhou o split de autorização, lifecycle e reforços dos oráculos; nenhum bloqueador de produção identificado.

## Limites e rollback

Foram criados e encerrados endpoints próprios loopback; os testes aguardam tarefas e conferem rebind. Nenhum processo Java, configuração pessoal, conta, serviço externo, kit histórico ou binário principal foi alterado. O build vai somente para o target privado. A API IPv6 nova foi validada por gates estáticos/negativos, sem executar novo ensaio externo. A sessão de 12 s é revogada antes do máximo e não representa estabilidade de uma hora. UI, plano CLI com dois prazos, jogo, dois jogadores, contas mistas, Core/Skins, cada mod de voz, relay de operador e NAT/CGNAT continuam com gates próprios.

Rollback consiste em revisar/reverter somente as alterações 52 em `host.rs`, `guest.rs`, `lib.rs` e os novos arquivos `session_invitation.rs`/`tests/session_lifetime.rs`; conservar todos os incrementos anteriores e evidências. Em execução, fechar pontes, Guest e Host com seus `shutdown` aguardados. Nenhuma restauração de configuração é necessária porque este incremento não altera configurações. [Convites atuais](aurora-connect-internet-invitations.md), [contrato canônico](engineering/AURORA_CONNECT_SPEC.md), [protocolo UDP separado](aurora-connect-fragmented-udp.md).

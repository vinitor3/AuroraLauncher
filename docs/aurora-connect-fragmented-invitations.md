# Aurora Connect — planos e convites para UDP fragmentado

Data: 08/10/2026. Incremento 51. Estado: **PROTÓTIPO**. Preparação de plano, emissão e verificação criptográfica estão implementadas e testadas. A admissão de um convite fragmentado por Host/Guest e sua seleção na CLI ainda estão **NÃO IMPLEMENTADAS**. Os caminhos antigos recusam esse perfil, inclusive quando recebem um objeto `Invitation` já verificado pela API nova.

Este registro foi preparado no HEAD `42c54471799713d4dcad992fbc40a0837e3d14e3`, com os fontes 51 ainda sem commit. Complementa o [transporte UDP fragmentado 49](aurora-connect-fragmented-udp.md), os [convites existentes](aurora-connect-internet-invitations.md) e a [especificação canônica](engineering/AURORA_CONNECT_SPEC.md). Não altera a matriz, Core/Skins, Assistente, Java, mods, arquivos pessoais, kits ou pins históricos.

## Contrato e opt-in

O perfil interno separa `Legacy` de `FragmentedV2`. O modo de endereço, loopback ou IPv6 externo, é outra escolha explícita. Aprovar um endereço externo não aprova fragmentação; selecionar fragmentação não aprova um endereço externo.

O envelope do convite continua sendo schema 1, com o domínio Ed25519 existente `aurora/connect-lab/invitation/v1\0`, JSON canônico e limite de 4096 bytes. A versão 2 identifica o envelope UDP do incremento 49. Não é um novo schema de convite nem uma mudança no ALPN, no Hello ou na prova de admissão.

O perfil novo exige exatamente os seguintes serviços, sem serviços adicionais ou duplicados:

| Serviço | Protocolo | Revisão | Capability | Limite e política |
| --- | --- | ---: | --- | --- |
| `game` | TCP | 1 | `reliableStream` | `ReplyBudget`; campo genérico `maxDatagramBytes` entre 1 e 65507, sem autorizar UDP |
| `aux` | UDP | 1 | `fragmentedDatagram` | Datagrama original de 1 a 4096 bytes; `ReplyBudget` por padrão ou `AuthorizedDownstream` explícito |

O limite do serviço é positivo; isso não impede transportar um datagrama UDP vazio, permitido pelo codec. Todos os endpoints locais precisam ser literais loopback com porta não zero. O convite não contém esses destinos internos: eles continuam no registro aprovado do anfitrião. Os grants são ordenados como `game`, depois `aux`.

Capability, protocolo, revisão, teto de datagrama e política de retorno pertencem ao corpo assinado. Alterar um valor válido invalida a assinatura. Trocar `fragmentedDatagram` por `datagram` também é recusado ao escolher o verificador legado; não existe downgrade autorizado pela troca do verificador.

No perfil novo, `AuthorizedDownstream` exige ao menos 32 bytes/s, incluindo o header V2, e ao menos um datagrama/s. Os máximos permanecem os limites padrão existentes: 250000 bytes/s e 200 datagramas/s. A emissão do convite não cria crédito, socket ou permissão de encaminhamento. Os limites por fragmento, reservas compartilhadas, TTL de remontagem, revogação e comportamento de retorno continuam no [contrato de transporte 49](aurora-connect-fragmented-udp.md).

## APIs implementadas

| API | Resultado e limite |
| --- | --- |
| `load_fragmented_plan(path, expected_hash)` | Lê e valida um plano V2 explícito; retorna `ValidatedPlan`, sem abrir rede ou aplicar arquivos |
| `Invitation::issue_fragmented(...)` | Emite convite assinado V2 para endereço inicial loopback |
| `Invitation::verify_fragmented(encoded, expected_host, guest)` | Verifica assinatura, pins, destinatário, grants V2, endereço loopback e prazo |
| `Invitation::issue_fragmented_external_ipv6(...)` | Emite convite V2 com endereço inicial IPv6 externo explicitamente selecionado |
| `Invitation::verify_fragmented_external_ipv6(...)` | Exige simultaneamente o perfil V2 e o modo IPv6 externo |

As duas APIs de emissão mantêm os argumentos correspondentes das APIs legadas: chave privada do host em memória, chave pública do convidado, sessão, endereço inicial, serviços e vencimento absoluto. O retorno é um `Invitation` imutável e verificado, com digest SHA-256. A chave privada não entra no convite, preview ou relatório.

`load_plan`, `issue`, `verify`, `issue_external_ipv6` e `verify_external_ipv6` mantêm o perfil legado. Eles não passam a aceitar a capability nova. As APIs fragmentadas, por sua vez, recusam um plano ou convite com `aux/datagram` ou somente `game`.

`Host::start_invited` e seu equivalente externo já passam pela conferência legada de grants antes de iniciar o transporte. O Guest agora também chama `check_legacy_profile()` antes de abrir endpoint. Assim, construir o objeto por uma API nova não contorna a restrição da API antiga. **Não há ainda um método de Host/Guest que admita o convite V2.**

## Plano, revisão e prazos

O novo loader reutiliza a leitura confinada existente: arquivo regular local, limite de 16 KiB, recusa de links/reparse points observados, caminhos UNC/device/ADS e aliases ambíguos. A leitura usa um único handle e um snapshot de bytes; não é um editor de configuração ou transação de escrita. Campos desconhecidos são recusados em todos os níveis. Um BOM UTF-8 pode ser aceito, mas permanece coberto pelo hash.

`expected_hash` vincula exatamente os bytes revisados, inclusive whitespace e BOM. Mudança posterior produz `plan_changed_since_review`. O retorno não relê o caminho nem altera o arquivo. O caller ainda precisa aprovar o plano, criar identidades novas e autorizar o participante pelo fluxo correspondente.

O schema do arquivo de plano permanece 1. O opt-in está no loader escolhido e no grant explícito `fragmentedDatagram`; não se acrescenta uma chave `profile` ao arquivo de entrada. Os previews V2, tanto de plano quanto de convite, acrescentam `profile: fragmentedV2`, `udpEnvelopeVersion: 2` e `maxFragmentFrameBytes: 1264`. São campos derivados para revisão, sem conceder autoridade. O preview legado permanece igual.

`maxDatagramBytes` descreve o datagrama original remontado. O teto de 1264 bytes se refere ao frame V2 completo, incluindo 32 bytes de header; não é uma medição de MTU física. A prévia contém endpoints e identidades públicas para aprovação local. Ela não deve ser usada como telemetria redigida ou copiada integralmente para o Git.

O loader conserva a faixa de duração local de 10 a 3600 segundos. Isso não amplia convites: emissão/verificação continuam limitando o vencimento assinado a **600 segundos desde a emissão**, para a sessão inteira. Um futuro caller de convite deve recusar duração incompatível antes de pedir aprovação ou abrir transporte; não deve reduzi-la silenciosamente. Clones conservam o deadline monotônico existente. Não há renovação ou separação de prazo de ingresso/sessão neste incremento.

O modo externo reutiliza o filtro do [incremento 45](engineering/AURORA_CONNECT_SPEC.md#811-modo-externo-ipv6-explícito--incremento45). O endereço inicial é assinado; não há DNS, relay ou descoberta por estas APIs. A classificação de IPv6 não comprova alcance, ausência de tradução ou restrição de todos os caminhos internos do Iroh. Os testes novos verificam a classificação sem discar endereços externos.

## Arquivos e fronteira de integração

- `apps/connect-lab/src/plan.rs`: loader e validação por perfil; preview V2.
- `apps/connect-lab/src/invitation.rs`: emissão/verificação por perfil e gate legado sobre objetos já verificados.
- `apps/connect-lab/tests/fragmented_profiles.rs`: dez casos novos de plano, assinatura, limites, modos de endereço e recusa antes do transporte.
- `apps/connect-lab/src/lib.rs`: export de `load_fragmented_plan`.
- `apps/connect-lab/src/guest.rs`: chamada do gate legado antes de I/O.

Os dois últimos são alterações pequenas de integração coordenadas com o trabalho paralelo 50; dependem dos fontes 51 e precisam estar no mesmo estado compilável. O incremento 51 não acrescenta dependências nem altera manifest/lock. Alterações concorrentes do relay pertencem ao incremento 50. A suíte combinada completa, com e sem a feature de relay, continua pendente do fechamento desse trabalho.

## Evidências executadas

Diretório: `C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-r4-connect`. Os logs estão em `.aurora/connect-validation/`, ignorado pelo Git. Os comandos usaram `--offline --locked` e o target privado compartilhado `relay50-target`, liberado pelo seu proprietário antes desta execução. Não alteraram o `target/debug` histórico usado no ensaio remoto.

| Rodada/target | Resultado | Log |
| --- | --- | --- |
| Foco: `fragmented_profiles` | 10 PASS, zero ignorados, 0,29 s | `connect-fragmented-profile-51-tests-2.log` |
| Foco: `invitation` | 15 PASS, zero ignorados, 2,21 s | Mesmo log |
| Foco: `service_plan` | 12 PASS, zero ignorados, 0,22 s | Mesmo log |
| Regressão: `fragmented_transport` | 8 PASS, zero ignorados, 3,05 s | `connect-fragmented-profile-51-transport-regressions.log` |
| Regressão: `invitation_external` | 5 PASS, um ignorado, 0,09 s | Mesmo log |
| Regressão: `invitation_resumption` | 5 PASS, zero ignorados, 2,95 s | Mesmo log |
| Clippy da biblioteca e seis targets acima | PASS com `-D warnings`, 16,21 s | `connect-fragmented-profile-51-clippy.log` |
| Formatação focal e whitespace | PASS; sem log dedicado | Saída das ferramentas |

São **55 PASS em duas rodadas**, sem falhas. Não é uma suíte completa unificada. O único ignorado foi `own_global_ipv6_invitation_roundtrip_revocation_expiration_cleanup`, que exige endereço IPv6 próprio explícito e não foi executado nesta etapa. Os cinco testes de resumption usam conexão real com early data aceito e preservam as recusas de replay, prazo, revogação, prova e peer; não comprovam admissão de convite V2.

Os dez casos novos comprovam: escolha explícita do loader; hash e ausência de escrita; perfil legado preservado; shape/capabilities/revisão/tetos e campos desconhecidos; piso de orçamento do header V2; assinatura/digest canônicos; adulteração de limite/política e downgrade recusados; exigência dos dois grants; separação de perfil/endereço; prazos/entrada limitada; objeto V2 recusado por Host/Guest antigos. A fixture dessa última recusa mantém um socket UDP próprio efêmero reservado e exige `WouldBlock` ao consultar recepção; essa consulta não observou pacotes. O timeout de 200 ms limita uma eventual regressão do teste; o resultado esperado é `Configuration` antes de qualquer conexão.

Comandos efetivamente executados para as rodadas aprovadas:

```powershell
cargo test --offline --locked --manifest-path apps/connect-lab/Cargo.toml --target-dir .aurora/connect-validation/relay50-target --test fragmented_profiles --test service_plan --test invitation -- --test-threads=1
cargo test --offline --locked --manifest-path apps/connect-lab/Cargo.toml --target-dir .aurora/connect-validation/relay50-target --test invitation_external --test invitation_resumption --test fragmented_transport -- --test-threads=1
cargo clippy --offline --locked --manifest-path apps/connect-lab/Cargo.toml --target-dir .aurora/connect-validation/relay50-target --lib --test fragmented_profiles --test service_plan --test invitation --test invitation_external --test invitation_resumption --test fragmented_transport -- -D warnings
rustfmt --edition 2024 --check apps/connect-lab/src/plan.rs apps/connect-lab/src/invitation.rs apps/connect-lab/tests/fragmented_profiles.rs
git diff --check -- apps/connect-lab/src/plan.rs apps/connect-lab/src/invitation.rs
git diff --no-index --check -- /dev/null apps/connect-lab/tests/fragmented_profiles.rs
```

O último comando retorna 1 quando há um arquivo novo diferente de `/dev/null`; o critério de whitespace foi ausência de diagnóstico, sem confundi-lo com aprovação de uma suíte. A formatação também foi aplicada com `rustfmt --edition 2024` aos mesmos arquivos antes da conferência.

Houve uma compilação inicial interrompida para adotar o target compartilhado e evitar duplicar dependências. O comando abaixo saiu com código 1 após Ctrl+C na sessão própria; nenhum teste chegou a executar. `connect-fragmented-profile-51-tests-1.log` conserva esse registro, e uma consulta CIM posterior confirmou zero processos `cargo`/`rustc` remanescentes daquele target. Não foi uma falha de teste ou correção de produção.

```powershell
cargo test --offline --locked --manifest-path apps/connect-lab/Cargo.toml --target-dir .aurora/connect-validation/profile51-target --test fragmented_profiles --test service_plan --test invitation -- --test-threads=1
```

A revisão independente não encontrou bypass de perfil/assinatura ou ampliação involuntária do legado. Ela pediu o destino próprio reservado na fixture negativa; o teste foi ajustado antes da execução. A revisão não executou testes em paralelo.

## Pendências e rollback

O próximo incremento precisa acrescentar as entradas explícitas de Host/Guest, seus gates de perfil/endereço antes de I/O, a revisão na CLI e uma fixture nativa V2 com dois agentes. Ainda faltam prova de convite V2 admitido, encaminhamento por esse convite, teste entre duas redes e a homologação separada de SVC e Plasmo. Os datagramas opacos maiores do incremento 49 não comprovam áudio, anúncios dos mods ou endereçamento de todos os participantes.

Não há autorização de voz, relay, GUI ou alteração de configuração neste passo. Minecraft, contas mistas, os nove pares Core/Skins e R4 não ganham homologação. Nenhum Java, mod, cliente gráfico ou serviço remoto foi executado pelas provas 51.

Rollback operacional: conservar os loaders/verificadores e agentes legados, sem selecionar o perfil novo. Planos são apenas lidos e convites existem em RAM; não há arquivo pessoal aplicado para restaurar. Uma reversão de código deve remover o incremento completo e seus dois pontos de integração dependentes por commit revisado, sem reset de trabalhos concorrentes, alteração de kits ou reaproveitamento de chaves/sessões históricas.

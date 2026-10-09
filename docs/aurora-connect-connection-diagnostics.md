# Observação da conexão do convidado — incremento 54

Estado: **PROTÓTIPO**, opt-in, somente laboratório. O bloqueio observado foi `timed_out` após aprovação válida no ensaio de convites 47c; contadores Aurora do anfitrião permaneceram zerados. Contadores de pacotes na interface não identificam resolução, QUIC/TLS, abertura de controle ou Hello. A causa externa permanece **NÃO VERIFICADO**; nenhuma destas amostras autoriza diagnóstico de NAT/CGNAT.

## Contrato da observação

As APIs novas `Guest::connect_invited_observed` e `Guest::connect_invited_external_ipv6_observed` retornam `(Result<Guest>, GuestConnectObservation)`. Recebem o objeto `Invitation` original já verificado, sem reemitir convite, renovar prazo ou trocar identidade. Modo de endereço continua explícito. Perfil fragmentado e schema2 de sessão permanecem nas respectivas APIs separadas.

O coletor privado mantém somente `Instant` e um enum fechado em memória, sem callback, arquivo, logger global, variável de ambiente, rede adicional ou histórico crescente. Chamadas antigas passam `None`, não alocam o coletor e não produzem evento novo. A marcação ocorre antes de cada operação existente; não acrescenta await nem substitui `Endpoint::connect`.

O snapshot serializado tem exatamente `schemaVersion: 1`, `phase` e `elapsedMs`. Não contém IP, porta, peer, chave, convite, assinatura, digest, payload, erro TLS bruto ou relógio civil. `elapsedMs` é monotônico desde a entrada na API observada, incluindo validação, bind e cleanup de falha aguardado. Pode exceder os 10 s do handshake; não é RTT nem uma medição exclusiva da rede.

| Fase | Significado limitado |
|---|---|
| `preflight` | Validação local anterior ao bind. |
| `endpoint_bind` | Início da criação/bind do endpoint próprio. |
| `endpoint_connect` | Entrada em `iroh::Endpoint::connect`; inclui trabalho interno de resolução e estabelecimento QUIC/TLS. Não significa TLS concluído. |
| `control_open` | TLS/connect retornou; abertura do stream de controle iniciada. |
| `hello_write` | Escrita/framing do Hello e finish iniciados. |
| `hello_ack` | Leitura e validação da resposta iniciadas; pode terminar por erro remoto, transporte ou timeout. |
| `admitted` | Handshake nativo concluído com sucesso. Não comprova ponte TCP, destino, Minecraft ou voz. |

`phase` é a última operação iniciada, com a exceção explícita de `admitted` que marca conclusão. Não afirma que uma operação bloqueou durante todo o prazo ou identifica a causa de uma falha. O timer agregado original de 10 s continua envolvendo connect, abertura de controle, Hello e ACK; bind permanece fora dele. As falhas mantêm o mesmo `Error` e aguardam o fechamento do endpoint como antes.

## Runner separado e cancelamento

O executável de auditoria acrescenta somente dois aliases:

```text
aurora-connect-invite-audit guest-observe <host-public-key> <loopback-tcp-bind>
aurora-connect-invite-audit guest-external-ipv6-interactive-observe <host-public-key> <loopback-tcp-bind>
```

Preparação continua 60 s no loopback e 600 s no modo externo interativo. Hello mantém 10 s, convite/sessão mantém TTL máximo 600 s, aprovação exige o digest exato, uma admissão, até quatro retries explícitos, 32 comandos e quota global de oito linhas vazias. Nenhum alias antigo muda de saída ou habilita observação automaticamente.

Após cada tentativa nativa concluída, somente nesses aliases, o runner emite:

```json
{"event":"audit_guest_connect_observation","observation":{"schemaVersion":1,"phase":"endpoint_connect","elapsedMs":13000},"code":"timed_out"}
```

O exemplo é ilustrativo; elapsed não é valor garantido. `code` usa os códigos estáticos antigos, ou `ok` para sucesso nativo. Falhas de leitura/verificação/aprovação antes de chamar Guest mantêm apenas o erro antigo, sem inventar uma tentativa ou fase de rede.

EOF, `stop`, Ctrl+C, input inválido ou prazo durante o dial continuam aguardando o resultado nativo limitado. Se ele retornar sucesso após cancelamento, o runner fecha e aguarda o Guest antes de concluir. Nesse caso a observação pode conter `admitted`/`ok` antes do cleanup de cancelamento: isso é sucesso do handshake, **não** `audit_guest_ready` nem comprovante de cleanup. A observação não autoriza abrir ponte depois do cancelamento; prontidão e encerramento conservam eventos separados. Em falha nativa, o snapshot só retorna após cleanup nativo aguardado.

## Evidência e limites

Testes novos usam apenas endpoints e processos próprios em loopback, com portas efêmeras. API: UDP silencioso termina em `endpoint_connect`, servidor TLS autenticado que leu Hello e não emite ACK termina em `hello_ack`, admissão transmite TCP real, replay/revogação mantêm erros, lease desconhecida não consome convite correto nem abre destino, chave/modo incorretos falham em `preflight`. O schema tem allowlist exata; endpoints/streams são fechados e sockets próprios passam rebind. Fixtures API usam handshake 1 s; o CLI é testado com seus 10 s originais.

CLI: alias observado, replay da mesma identidade, alias antigo sem evento inclusive em retry, timeout real, cancelamento após UDP realmente enviado, preparação externa 600 s e EOF antes de convite. Não há ensaio externo, alteração de firewall/relógio, Java ou configuração pessoal neste incremento. A instrumentação Host de TLS/Hello continua fora deste escopo; esta observação localiza apenas o avanço do convidado.

Rodada focada: **6 unitários do audit PASS**, **5 integrações API PASS** em 5,58 s e **5 integrações CLI PASS** em 27,21 s. Regressão completa: **207 PASS / 4 ignorados** no padrão e **217 PASS / 5 ignorados** com `local-relay-lab`, 25 grupos em cada rodada, nenhuma falha. Os casos focais estão incluídos nos totais completos. Clippy all-targets com warnings como erros passou em 9,45 s no padrão e 9,98 s com a feature; formatação e diff check passaram. Revisão independente de produção, domínio e oráculos não encontrou bloqueador.

Primeira compilação do novo teste API falhou porque a fixture chamava o nome inexistente `revoke_lease`; foi corrigida para a API existente `revoke`. Nenhuma mudança de autorização ou protocolo foi feita para resolver isso. A falha permanece no log `connection-54-api-first.log`.

Comandos efetivamente executados, na worktree Connect:

```powershell
$env:CARGO_TARGET_DIR = '.aurora/connect-validation/relay50-target'
cargo test --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --test connect_observation -- --test-threads=1
cargo fmt --manifest-path apps/connect-lab/Cargo.toml --all
cargo test --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --test connect_observation --test connect_observation_cli --bin aurora-connect-invite-audit -- --test-threads=1
cargo test --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --all-targets -- --test-threads=1
cargo test --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --all-targets --features local-relay-lab -- --test-threads=1
cargo clippy --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --all-targets -- -D warnings
cargo clippy --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --all-targets --features local-relay-lab -- -D warnings
cargo fmt --manifest-path apps/connect-lab/Cargo.toml --all -- --check
git diff --check -- apps/connect-lab docs/aurora-connect-connection-diagnostics.md
cargo build --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --bin aurora-connect-invite-audit --no-default-features
```

Logs privados sob `.aurora/connect-validation`: `connection-54-focused.log`, `connection-54-full-default.log`, `connection-54-full-relay.log`, `connection-54-clippy-default.log`, `connection-54-clippy-relay.log` e `connection-54-build.log`. Não contêm peers, convites ou saída TLS bruta. O lockfile conservou SHA-256 `e4cc7ab514091230e772ee1e3e80ec67a7bf05733e924624dd135e200e3048f3`; nenhuma dependência foi adicionada. Os testes não homologam transporte externo, Minecraft, convites de produção/interface, Simple Voice Chat, Plasmo Voice ou relay de operador.

Build explícito do audit sem features passou em 14,48 s. Artefato somente no target privado: `relay50-target/debug/aurora-connect-invite-audit.exe`, 23.416.320 B, SHA-256 `ea0e9784cfecfebd4e7724ef0db271dacabb0e96b54f8e236ba0f4fbf88ddd4d`. Não é instalador nem kit de transferência. Os executáveis históricos conservaram seus hashes: audit 47b `7d3403b25ee66e1759adf87fe7d64bec65031d9574cc325b9ed64f09408b3e16`, lab `005b55c43f2b08fd9bb5f32f7c17a81de1eccd50a34e920b95a545c580c5f973`. Pins dos callers de voz não foram alterados por 54.

Fontes finais revisadas/testadas, SHA-256:

| Arquivo | SHA-256 |
|---|---|
| `src/connect_observation.rs` | `c4ed051b5284371a5b6e560a430d8d4965ff4fd47fb12866f865babcbf2c66d9` |
| `src/guest.rs` | `4860e62c2538dbf2ae152d50ae15240ab62f3fed42bccad282771915d135cfa8` |
| `src/lib.rs` | `aabf74a97225cbf1505b1265f2238e1a29b5edab2de33ff7c83b830d8fc6383d` |
| `src/bin/aurora-connect-invite-audit.rs` | `e4b9940cc57f1758a4364aa6e9c256d6c8751a6eff2e0f5b02323e4f247674bc` |
| `tests/connect_observation.rs` | `51a66df6e1bd80e48ce0a48c1583ab9bedc8ed8549cd04ee72a0702990ddfd74` |
| `tests/connect_observation_cli.rs` | `1363166dca27d3c4bc1217d47005555e6e2e2df0bcb7bc6a0a216a7154d53f64` |

Os caminhos da tabela são relativos a `apps/connect-lab`. Não houve mudança de produção após a rodada focada. Uma asserção da fixture foi ajustada para não imprimir peer público numa eventual falha; as suites completas e Clippy incluem essa versão final.

Compatibilidade entre binários foi exercida separadamente por `node .aurora/connect-validation/mixed-invite47b54.mjs`, uma cópia nova revisada do harness 47b/47c. **PASS em 2851 ms**, anfitrião 47b e convidado 54 pinados, dois processos reais e somente portas efêmeras em loopback. Antes da aprovação, linhas vazias não abriram transporte nem destinos. Depois: TCP 1 MiB por direção, mesmo convidado após desconectar, replay e revogação com recusas Host 0→1→2, um destino e 2.097.152 B cumulativos. Exatamente três observações: `admitted/ok` 77 ms, `hello_ack/replayed_request` 39 ms e `hello_ack/expired_or_revoked` 38 ms. Nenhuma dessas medidas é RTT. Ambos os processos saíram 0 sem força; host, listener guest e echo passaram rebind. Log redigido `connection-54-mixed.log`; os dados de identidade/convite só passaram pela RAM da fixture. Isso não prova conexão entre duas internets. O usuário pausou testes entre computadores; nenhum kit de transferência foi preparado neste incremento.

## Operação e rollback

Não substituir 47b/47c, o debug histórico ou kits existentes. Preparar cópia nova com pin explícito depois da revisão, usando o anfitrião histórico já aprovado quando aplicável. O coordenador executa o anfitrião; eventual teste remoto exige transferência e execução no segundo PC, com convite novo e sequência de revisão/aprovação preservada.

`stop` encerra somente o runner/ponte próprios. Nenhuma configuração pessoal precisa de restauração. Rollback usa commit local revisado ou volta operacional ao executável histórico preservado; não reset/clean, não substituição de artefato imutável. Recursos continuam experimentais e desligados para usuários.

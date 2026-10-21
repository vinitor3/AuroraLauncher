# Observação redigida do caminho selecionado — incremento78

Contrato de15/10/2026; implementação local concluída21/10/2026 após o commit77 `ec6d747`. **IMPLEMENTADO MAS NÃO HOMOLOGADO**, com testes locais, regressão e revisão independente. Escopo exclusivo da biblioteca `apps/connect-lab`, sem desktop, wire novo, infraestrutura externa ou mudança de feature. Os incrementos76/77 conservam seu escopo e validação independente. As fontes permaneceram intactas durante a validação77 e a prova inicial do defeito79, para não alterar sua dependência durante compilação ou atribuir uma correção ao incremento errado.

## Fonte e distinção necessária

`apps/connect-lab/Cargo.toml` e lockfile fixam Iroh1.3.0. A fonte local dessa versão em `endpoint/connection.rs` documenta `Connection::paths()` como snapshot dos caminhos abertos no instante da chamada. `socket/remote_map/remote_state/path_watcher.rs` conserva a seleção no mesmo snapshot; `Path::is_selected()` significa caminho escolhido para transmissão de dados da aplicação. `is_ip()` e `is_relay()` classificam o transporte. Nenhuma dessas consultas precisa de leitura de endereço, estatística, probe ou tarefa nova.

As verificações anteriores em `tests/local_relay.rs` e `tests/direct_ipv4.rs` inspecionam os caminhos abertos das fixtures. Antes deste incremento, isso não constituía uma API pública redigida para observar o caminho **selecionado**. Um endereço IP fornecido ao conectar, uma lista contendo IP ou a ausência de erro não prova que aquele é o caminho efetivo selecionado.

## Contrato da API

```rust
// Exportados pela biblioteca; projeção de saída, sem Deserialize.
pub enum PathState { Unknown, Direct, Relay, Closed }
pub struct PathObservation {
    pub schema_version: u8, // 1
    pub state: PathState,
}

impl Guest {
    pub fn path_observation(&self) -> PathObservation;
}
impl Host {
    pub fn invitation_path_observation(&self, lease_id: &str)
        -> Option<PathObservation>;
}
```

Serialização fechada: `{"schemaVersion":1,"state":"unknown|direct|relay|closed"}`. Sem IP, família IP, URL/operador de relay, peer, lease, sessão, caminho interno, RTT, bytes, prazo ou erro remoto. Não registrar automaticamente nem enviar a telemetria. O argumento Host é uma lease já retida pelo caller autorizado que possui esse Host, não seleção livre de peer remoto. Esta função não autentica uma UI ou caller externo por si.

Classificação conservadora de um único snapshot:

- exatamente um caminho selecionado, IP e não relay: `direct`;
- exatamente um selecionado, relay e não IP: `relay`;
- nenhum selecionado, lista vazia, múltiplos selecionados ou classificação desconhecida/ambígua: `unknown`;
- cancelamento, fechamento conhecido ou autoridade temporal original inválida no Guest: `closed` tem precedência.

Caminhos não selecionados não votam na classificação. Um relay aberto não transforma uma seleção IP em relay; uma alternativa IP aberta não transforma seleção relay em direto. Não fabricar `mixed` nem escolher o primeiro caminho em caso ambíguo. A leitura continua válida somente para o instante observado; não promete caminho imutável, alcance público, ausência de NAT, migração sem interrupção ou qualidade de áudio. Sem timestamp ou retenção, o objeto não constitui série histórica nem medição de ping.

## Autoridade e fechamento

Guest: conferir cancelamento, `Connection::close_reason()` e sua autoridade de sessão antes e depois de ler o snapshot. A família `SessionInvitation` (TCP de sessão e sessão fragmentada) usa o objeto original já retido. A família `Invitation` (legada e fragmentada curta, em todos os modos de endereço) deve reter uma cópia privada do objeto original somente para esta consulta, mantendo o mesmo deadline monotônico e usando `check_current()`: não reconstruir/verificar de novo o convite ou iniciar watcher. A família de API, não o número isolado de schema/envelope, determina essa escolha. Isso não altera funcionamento de transporte, admissão ou prazos.

Guest manual, inclusive fixture relay50, não recebe o deadline da lease do Host no protocolo atual. Pode conhecer cancelamento/fechamento local e fechamento remoto recebido, mas não uma revogação remota ainda não observada. Registrar essa limitação: o método não a resolve abrindo consulta de rede nem promete reação instantânea. Mesmo um Guest de convite depende da notificação remota para revogação; a expiração original, quando conhecida, é recusada localmente sem depender do watcher.

Host: reutilizar o lock de admissão existente para linearizar com `revoke`, sem introduzir lock próprio. Retornar `None` para lease inexistente, manual, não consumida, expirada ou revogada, e para Host cancelado. Não distinguir essas causas nem revelar caminho de outro participante. Conferir o relógio antes de ler consumo, como a observação existente, e revalidar autoridade depois da amostra.

Se a lease de convite foi consumida e sua autoridade continua válida, conexão removida/fechada permite `Some(closed)`. Se ativa, verificar peer autenticado contra a lease antes de observar o caminho. Mutex indisponível por poison ou estado incoerente deve falhar fechado (`None`), sem retornar uma classificação antiga. Não guardar cache do resultado, usar `paths_stream`, fazer I/O novo, renovar prazo ou alterar consumo/revogação.

`None` significa ausência de observação autorizada para essa consulta. `unknown` significa conexão autorizada observável, mas sem classificação inequívoca do caminho selecionado. `closed` não contém motivo remoto nem diagnóstico de NAT. Nenhum resultado comprova ponte TCP/UDP de aplicação, Minecraft, voz, teste externo ou permissão de uso de relay público.

## Arquivos e testes

Ownership: `src/host.rs`, `src/guest.rs`, `src/lib.rs`, novo `src/path_observation.rs`, novo `tests/path_observation.rs`, este documento. Sem novos crates, features, alterações de lockfile, protocolo, limites ou módulo desktop. Cargo será executado pelo coordenador após freeze conjunto, pois a biblioteca também é dependência do desktop.

Testes escritos no checkpoint21/10; resultados executados estão registrados abaixo:

1. Classificador puro cobre zero/um/vários selecionados, IP/relay/ambíguo/desconhecido, alternativas não selecionadas e precedência de fechamento. JSON/Debug não carregam campos de conectividade ou identidades.
2. Conexões reais próprias em loopback observam `direct` no Guest e no Host autorizado, com tráfego TCP real sem leitura adicional do serviço causada pela consulta. Convite não consumido/manual/desconhecido não revela observação Host. Fechamento Guest preserva consumo e produz `closed` enquanto a lease ainda é válida.
3. Revogação e expiração de convites: Host retorna `None` imediatamente no gate; Guest termina fechado conforme autoridade temporal local ou fechamento recebido. Cobrir `Invitation` legada e fragmentada curta separadamente. Amostra não volta a ativa por clone, relógio ou consulta repetida. Verificar sessão após expirar ingresso: ingresso vencido não fecha sessão válida já admitida.
4. Grupo com participantes distintos: observar/revogar A não substitui nem encerra observação de B; nenhuma enumeração de peers/rotas. Endpoints reais e cleanup/rebind próprios.
5. Com feature existente `local-relay-lab`, fixture50 própria observa `relay` no Guest e encerra para `closed`, sem IP direto/fallback ou relay público. Essa API Host é restrita a convites; a lease manual da fixture não ganha observação por consequência. A suíte padrão continua compilando sem feature.

O teste unitário de relógio é separado da expiração real: runtime `current_thread`, dois perfis de sessão e dois lados de autoridade. Depois de observar transporte direto real, avança somente a amostra civil privada da autoridade original. Entre esse avanço e a consulta não há await/yield; cancelamento continua falso, close_reason ausente e o watcher não terminou. Essa prova distingue o gate síncrono de uma conexão que já teria sido fechada pelo watcher. Não muda o relógio do Windows. O evento exclusivamente cfgtest `path_clock_gate_observed` contém somente contagens e booleanos, sem conectividade ou identidade.

`tests/path_observation.rs` conserva os quatro perfis separadamente. A fixture IPv4 própria é opt-in ignorada por padrão e exige `AURORA_CONNECT_TEST_IPV4`; não descobre nem escolhe um endereço pelo usuário. Essas provas, quando executadas, se limitam ao mesmo computador. Chamadas repetidas de observação são cercadas pela contagem real de conexões ao serviço, que deve continuar zero até a aplicação abrir TCP e trocar1024B em cada direção.

Se a fonte Iroh não garantir seleção imediata após handshake, a fixture deve aguardar uma condição real com deadline curto, sem fabricar estado nem dormir para declarar sucesso. Os testes de classificador cobrem ausência/ambiguidade; não fingir que a fixture real produziu migração ou caminhos simultâneos.

## Evidências e rollback

No contrato inicial: somente leituras de manifests, fonte Iroh fixada, implementação e testes existentes. No checkpoint21/10: alterações locais em `src/host.rs`, `src/guest.rs`, `src/lib.rs`, novos `src/path_observation.rs`, `src/path_observation_clock_tests.rs` e `tests/path_observation.rs`. Rustfmt direto desses seis arquivos e `git diff --check` passaram. O coordenador executa Cargo serialmente; o autor não executou Cargo em paralelo.

Primeiro comando executado21/10 pelo coordenador:

```powershell
cargo test --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --lib path_observation -- --test-threads=1
```

Resultado: **4PASS,0FAIL,0ignored**,0.34s de testes,30.52s de compilação. Log `.aurora/connect-validation/path-observation-78-unit-first.log`. Cobertura: seleção versus alternativas, classificação ambígua, serialização/default redigidos e gate síncrono dos dois perfis `SessionInvitation` nos dois lados da autoridade, com relógio privado/current_thread antes do watcher. Revisão independente da produção e desse teste causal sem bloqueador. A família curta `Invitation` ainda não tem prova causal isolada antes de watcher; a integração de expiração real mantém essa distinção. O teste não mediu rede externa, migração, ping, Minecraft, áudio ou relay público.

Primeira integração com todas as features: **4PASS,1FAIL,1ignored**,12.83s de testes, compilação2m51s, log `.aurora/connect-validation/path-observation-78-integration-first.log`. Os casos de quatro perfis diretos, expiração/ingresso, modo manual e grupo passaram. A fixture relay falhou com `Configuration` na criação do **Host**, antes de criar o Guest e antes de observar caminho: ela pedia uma lease de60s a partir de uma amostra posterior à emissão do ticket de60s. O gate existente recusou corretamente uma lease além do ticket. Corrigido somente o prazo da fixture para45s, como os ensaios relay50 existentes; sem mudar validação, limites, prazo do ticket ou código de produção. Rustfmt/diffcheck passaram; reteste ainda pendente neste registro.

Integração corrigida executada:

```powershell
cargo test --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --all-features --test path_observation -- --test-threads=1
```

Resultado: **5PASS,0FAIL,1ignored**,13.40s de testes,5.51s de compilação; log `.aurora/connect-validation/path-observation-78-integration-second.log`. Inclui caminho relay selecionado na fixture50 própria, TCP real1024B em cada direção, fechamento `closed`, zero sockets de transporte IP nos agentes relay e nenhuma conexão ao serviço causada pela consulta de observação. O único ignorado é a escolha explícita de IPv4 próprio, executada separadamente abaixo. A falha inicial permanece no registro; nenhum contrato de produção foi relaxado.

Opt-in IPv4 próprio executado separadamente:

```powershell
cargo test --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --all-features --test path_observation path_observation_explicit_own_ipv4_all_four_profiles -- --ignored --exact --test-threads=1
```

Resultado: **1PASS,0FAIL,0ignored**,0.47s de testes,0.83s de compilação; log `.aurora/connect-validation/path-observation-78-own-ipv4.log`. O coordenador leu `Get-NetIPConfiguration`, encontrou um único IPv4 privado do PC com interface ativa e rota padrão, forneceu esse endereço somente no ambiente do processo `AURORA_CONNECT_TEST_IPV4` e restaurou o ambiente em `finally`. O endereço não foi incluído no log. Os quatro perfis passaram no mesmo computador; isso não testa roteador, segundo PC, NAT/CGNAT ou internet.

Regressão final: `cargo test --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --all-features --all-targets -- --test-threads=1` passou **266 PASS/0 falhas/9 ignorados**,30 targets,177,64s somados de testes/compilação4m06s (`path-observation-78-all-full.log`). O opt-in IPv4 foi executado separadamente; focais sobrepostos não são somados. Testes ignorados continuam identificados e não viram prova por compilação.

`cargo fmt --manifest-path apps/connect-lab/Cargo.toml --all -- --check` PASS. Clippy `cargo clippy --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --all-features --all-targets -- -D warnings` PASS1m14s; mesma chamada sem `--all-features` PASS12,17s. Logs `path-observation-78-{fmt,clippy-all,clippy-default}.log`, sequência exit0. Locks, features, versões e matriz mantêm hashes anteriores. Revisão independente da produção, clock privado, fixture corrigida, integração e diff sem bloqueador. Provas79 de liberação de recursos do desktop pertencem ao seu próprio incremento e não contam como teste78.

Rollback: retirar os métodos/exportações e o módulo de observação em reversão local revisada, preservando alterações posteriores. A observação não cria recurso que exija recuperação. Custos continuam zero; relay público desligado. O frontend não recebe capacidade nova neste incremento.

## O que você precisa fazer agora

Nenhuma intervenção do usuário é necessária para implementar e testar esta observação local. Testes de rede externa e áudio permanecem independentes.

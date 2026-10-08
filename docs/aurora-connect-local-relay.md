# Aurora Connect — relay local de laboratório, incremento 50

Atualização: 08/10/2026. Estado: **PROTÓTIPO**, com transporte TCP/UDP real por relay próprio em loopback. Feature `local-relay-lab` desligada por padrão. Relay público, fallback no desktop e homologação de Minecraft/voz por relay permanecem **NÃO IMPLEMENTADO** ou **NÃO VERIFICADO**, conforme o recurso. Esta evidência não aprova infraestrutura externa, custo, implantação ou distribuição.

Base cumulativa: transporte49, descoberta48 e integração desktop46 preservados. Nenhum kit histórico, binário usado no ensaio externo, porta42000/43001, configuração pessoal, matriz de nove pares Core/Skins ou Assistente foi alterado por estes testes.

## Seleção explícita e autenticação

`LocalRelayFixture::start([hostId, guestId], profile)` cria o único servidor HTTP em `127.0.0.1:0` e devolve um ticket opaco derivado do socket realmente obtido. IDs devem ser distintos. O ticket não aceita URL pública, credencial, configuração de conta ou serialização. As novas entradas são `Host::start_local_relay(config, &ticket)` e `Guest::connect_local_relay(config, &ticket)`; os métodos antigos conservam seu caminho direto e recusam endereço de relay no Guest antes de abrir endpoint.

O perfil permite os dois IDs autenticados pelo protocolo upstream do relay, até quatro clientes admitidos simultaneamente e no máximo dois por ID. O host exige uma única lease para o outro participante, válida e contida no prazo do fixture. O guest exige o par de identidades e recusa qualquer candidato IP ou URL diferente do ticket. O `bind_addr` dessas entradas deve ser loopback/porta0 e não é adicionado como transporte IP.

Endpoints usam Iroh1.3.0/`Minimal`, nenhum transporte IP, mapa com somente o relay próprio, QAD desativado, sem lookup/publicação, captive-portal check, keylog, listener de métricas, TLS do servidor HTTP ou ACME. A sonda chamada `https_probes` pelo SDK precisa estar habilitada para escolher o home relay quando QAD está desligado; nesta configuração ela acessa o URL HTTP literal do fixture local. TLS e pin entre os endpoints, Hello Aurora, sessão/peer/lease/capabilities, registro de serviços e autorização antes de abrir destinos permanecem no caminho já existente.

O relay transmite tráfego cifrado dos endpoints; não implementa Minecraft ou um mod de voz. Não há seleção automática, relay público, descoberta genérica, DNS de peer ou fallback pago.

## Gate de proxy somente leitura

A revisão encontrou que o cliente reqwest usado pela sonda Iroh pode herdar proxy. URL loopback sozinho não demonstra alcance exclusivamente local. A implementação recusa antes de criar servidor e endpoints quando qualquer `HTTP_PROXY`, `http_proxy`, `HTTPS_PROXY`, `https_proxy`, `ALL_PROXY` ou `all_proxy` estiver não vazio, mesmo se houver `NO_PROXY`. Verifica também a liveness/gate do ticket antes e depois da prontidão.

No Windows, consulta somente o DWORD `ProxyEnable` em HKCU/Internet Settings pela API segura `windows-registry0.6.1`, com abertura de leitura. Valor0 ou ausência comprovada permitem; valor nãozero, tipo inválido, acesso negado ou resultado ambíguo recusam. Não consulta/grava ProxyServer, ProxyOverride, URLs ou credenciais; não usa `set_var`, shell, elevação ou alteração de ambiente global/configuração pessoal. macOS é recusado até haver gate do sistema correspondente.

O grafo efetivamente compilado não habilita `reqwest/system-proxy` nem `hyper-util/client-proxy-system`; a leitura Windows permanece defensiva diante de unificação futura de features. Esta verificação não é CAS atômico contra outro ator que altere configurações durante o experimento. Mudança de versão/grafo exige reavaliação; não estender este resultado automaticamente ao desktop ou outro alvo.

O negativo inicia seis filhos próprios do executável de teste, um por variável, com ambiente definido antes do processo/runtime e um proxy fixture loopback próprio. Cada filho exige `local_relay_proxy_scope_unverified`; o pai exige zero conexões aceitas pelo proxy e aguarda/encerra somente o filho criado. Nenhum proxy externo ou configuração do usuário participa do teste.

## Duração, encerramento e quotas

O fixture solicita cancelamento aos60s monotônicos desde sua criação, ou por `shutdown`/`Drop`. Todos os clones do ticket são invalidados; novas chamadas recusam. O registro privado limita endpoints ativos e binds em andamento a quatro, conserva ownership durante criação e fecha endpoints próprios antes de soltar o listener HTTP. `shutdown` aguarda o worker/SDK e comprova rebind; `Drop` solicita cleanup assíncrono sem abortar o futuro que possui o servidor.

Esse prazo é o início do cancelamento, não uma garantia de que todo cleanup terminou em60s. O SDK pode precisar de grace de fechamento. A revisão encontrou que `endpoint.online()` podia manter o guard até seu timeout configurado após cancelamento. O delta final calcula antes do bind um deadline monotônico limitado pelo restante do ticket e pelo handshake_timeout, e espera online ou cancelamento do token privado. Em erro, aguarda endpoint.close antes de soltar o guard; não larga um dial TLS pendente. A regressão usa um terceiro ID realmente recusado, confirma online pendente com timeout60s e exige stop/Expired/rebind em até3s.

`RateLimited` usa o limiter RX real upstream, com bytes/s e burst nãozero. Os testes exigem incremento dos contadores reais de bytes/conexões limitados, sem inferir quota a partir de timeout. Esses são limites por cliente admitido, não limite global de payload ou garantia de fatura. Os campos upstream de limite de accepts HTTP não são implementados; o código não os apresenta como proteção efetiva. Este fixture não é um servidor público endurecido.

## Evidência executada

Todos os ensaios usaram somente portas efêmeras locais, payload sintético, chaves RAM novas, target Cargo privado e serviços echo TCP/UDP próprios. Nenhum Java, servidor Minecraft, mod, áudio, firewall, router, conta, instalador ou serviço externo foi executado nesta rodada. Cada cenário mantém payload de aplicação abaixo de1MiB.

| Verificação final | Resultado e limite |
| --- | --- |
| Integração real | 3PASS/0skip após o gateproxy em3,62s, repetidos após o deltaonline em4,83s. TCP262144B por direção; UDP1152B por direção, duas origens simultâneas e respostas distintas no socket correto. |
| Caminho | Três amostras exigem todos os paths relay e nenhum IP; bound_sockets dos endpoints vazio; contadores reais RX/TX de bytes/pacotes positivos. Não é medição de desempenho externo. |
| Autorização | Sessão incorreta recusada antes do destino; serviço sem grant/revisão errada recusados; API antiga recusa relay antes de endpoint. |
| Falha/cleanup | Stop real do fixture fecha transporte; não abre TCP depois; ticket retido recusa; bridges, endpoints, echo e as duas origens UDP liberados/rebindados. Stop controlado não simula uma queda externa abrupta. |
| Quota | RX limiter real observado por contadores positivos; não atribuído somente ao código de erro ou tempo gasto. |
| Unidade/ciclo | 6PASS antes do deltaonline; finais7PASS e1fixture interno ignorado por padrão em0,42s. O pai realmente executa esse fixture exato em seis filhos para os negativos de proxy. Inclui ACL real do terceiro ID, IDs duplicados, ticket retido após Drop/stop, gate Windows puro e cancelamento da espera online realmente pendente. |
| Revisão estática | Clippy focal com warnings negados, fmt e diffcheck PASS. |
| Integração do coordenador | Full default157PASS/4ignored e feature166PASS/5ignored, comunicados pelo coordenador antes do último deltaonline. O delta foi validado depois pelos10 casos focados e Clippy/fmt; não reapresentar a full anterior como executada nesse snapshot final nem somar rodadas como casos independentes. |

As primeiras rodadas não comprovaram relay:1PASS/2FAIL com NetReport minimal/QADNone, inclusive após acrescentar prontidão. A correção da sonda local permitiu os três casos reais; QA posterior encontrou o escape de proxy e o gate foi acrescentado/retestado. Logs anteriores foram preservados. O resultado final exige os gates de proxy; uma passagem anterior sozinha não fecha essa revisão.

Logs ignorados em `.aurora/connect-validation`: `relay-50-focused.log`, `relay-50-focused-fixed.log`, `relay-50-focused-home.log`, `relay-50-unit.log`, `relay-50-final-focused.log`, `relay-50-clippy.log`, `relay-50-proxy-unit.log`, `relay-50-proxy-focused.log`, `relay-50-proxy-clippy.log` e os finais `relay-50-online-cancel-unit-fixed.log`, `relay-50-online-cancel-focused.log`, `relay-50-online-cancel-clippy.log`. A primeira compilação da regressão nova falhou por borrow não estático no spawn, foi corrigida movendo limits/ticket ao futuro próprio e ficou preservada em `relay-50-online-cancel-unit.log`. Não registrar IDs, URL real, convite, payload ou segredo em documentação/relatório compartilhável.

## Comandos realmente executados pelo agente do incremento

Diretório: `C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-r4-connect`. Builds/testes usaram:

```powershell
$env:CARGO_TARGET_DIR = 'C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-r4-connect/.aurora/connect-validation/relay50-target'
cargo fetch --manifest-path apps/connect-lab/Cargo.toml
cargo metadata --manifest-path apps/connect-lab/Cargo.toml --locked --features local-relay-lab --format-version 1
cargo check --manifest-path apps/connect-lab/Cargo.toml --locked --features local-relay-lab --lib
cargo test --manifest-path apps/connect-lab/Cargo.toml --locked --features local-relay-lab --test local_relay -- --nocapture
cargo test --manifest-path apps/connect-lab/Cargo.toml --locked --features local-relay-lab --lib local_relay::tests -- --nocapture
cargo test --manifest-path apps/connect-lab/Cargo.toml --offline --features local-relay-lab --lib local_relay::tests -- --nocapture
cargo test --manifest-path apps/connect-lab/Cargo.toml --offline --locked --features local-relay-lab --lib local_relay::tests -- --nocapture
cargo test --manifest-path apps/connect-lab/Cargo.toml --offline --locked --features local-relay-lab --test local_relay -- --nocapture
cargo clippy --manifest-path apps/connect-lab/Cargo.toml --offline --locked --features local-relay-lab --lib --test local_relay -- -D warnings
cargo fmt --manifest-path apps/connect-lab/Cargo.toml -- --check
git diff --check
./.aurora/connect-tools/bin/cargo-audit.exe audit --file apps/connect-lab/Cargo.lock --db .aurora/connect-advisory-db --no-fetch --json
cargo tree --manifest-path apps/connect-lab/Cargo.toml --locked --features local-relay-lab -e features -i iroh-relay
```

Saídas de metadata/tree/audit foram salvas em `relay-50-metadata.json`, `relay-50-feature-tree.txt`, `relay-50-audit.json`; inventário dos49 pacotes adicionados em `relay-50-new-packages.json`. Não foi usado o target/debug que o coordenador preservou para outro ensaio. Nenhuma release/pin histórico foi regravado.

## Dependências e custo

Conferência final do coordenador: `cargo fmt --manifest-path apps/connect-lab/Cargo.toml --all -- --check` e Clippy `--offline --locked --all-targets`, com e sem `--features local-relay-lab`, passaram após o delta de cancelamento. Logs `relay-50-clippy-all-feature.log` e `relay-50-clippy-all-default.log`. As suites completas anteriores foram executadas com `cargo test --offline --locked --manifest-path apps/connect-lab/Cargo.toml --target-dir .aurora/connect-validation/relay50-target [--features local-relay-lab] -- --test-threads=1`; a opção entre colchetes indica as duas execuções reais, não sintaxe literal. Logs `relay-50-full-default.log` e `relay-50-full-feature.log` conservam respectivamente 157/166 PASS e 4/5 ignored. A diferença entre esses snapshots e os dez focos finais está explicitada na tabela acima.

`cargo metadata --offline --format-version 1 --manifest-path apps/desktop/src-tauri/Cargo.toml` passou e manteve o lock desktop byte a byte igual ao snapshot. Comparação por nome/versão/origem/checksum entre HEAD e lock novo do laboratório confirmou 358 →407 pacotes, 49 novos, nenhuma remoção/troca de versão/checksum existente. SHA256 final do lock: `e4cc7ab514091230e772ee1e3e80ec67a7bf05733e924624dd135e200e3048f3`. A feature de relay local não é habilitada pelo desktop.

`iroh-relay=1.3.0` é dependência direta opcional, defaults desligados, features server/tls-ring; `windows-registry=0.6.1` é opcional somente Windows e já existia no lock. Feature local-relay-lab não é habilitada pelo desktop. Lock passou de358 para407 pacotes;49 foram adicionados, sem remover ou alterar versão/checksum de qualquer pacote anterior. Todos os novos têm licença declarada MIT/Apache-2.0; maior MSRV declarado entre eles1.88, preservado o MSRV1.91 do Iroh/laboratório. Isso é inventário, não auditoria jurídica de distribuição.

A feature server compila dependências de certificado/ACME upstream; TLSNone/quicNone/metricsNone do fixture não executam esses serviços. Fetch foi gratuito de pacotes necessários, sem conta, cobrança, crédito ou infraestrutura contratada. Nenhum relay público foi acessado. Condições de operador futuro continuam no [cadastro de dependências](engineering/AURORA_CONNECT_DEPENDENCIES.md).

Cargo Audit: zero vulnerabilidades na base RustSec `5701a6d2a6158d5d4a933df5141a0415c285075d`, atualizada08/10/2026,1294 advisories. Warnings paste1.0.15/unmaintained e yoke-derive0.8.3/yanked já estavam no lock anterior. Sem ignores. Não é prova de ausência de falhas desconhecidas.

## Estados separados, pendências e rollback

TCP e UDP por relay local são **PROTÓTIPO** com as evidências acima. Conexão direta mantém suas evidências anteriores; este teste não amplia redes/NAT. Relay externo elegível e fallback automático precisam de perfil, gate de termos/custo, erro/cancelamento/caminho próprios. Minecraft/STATUS, convites/interface e descoberta têm registros separados; nenhum resultado deste echo aprova login/mundo. Simple Voice Chat e Plasmo Voice permanecem sem homologação por relay, incluindo endereçamento, transporte e áudio separados.

O coordenador suspendeu endpoints durante o gate de relógio do PC, executou a correção pessoal em fluxo separado explicitamente aprovado/verificado e então liberou os focos locais do deltaonline. Esse módulo não muda a hora nem configurações pessoais. Antes de ampliar, concluir revisão do snapshot final e os gates específicos do novo uso; não reaproveitar tickets/chaves expirados. Plano/diff/hash/autorização/snapshot/recuperação para configuração pessoal continuam um fluxo separado.

Rollback de código é retirar a feature/dependência/APIs/registro e os testes novos em um commit revisado, preservando outros incrementos. Não restaurar kits ou configs pessoais, não remover cache/global tooling nem outras worktrees. Em runtime, shutdown aguardado é o caminho normal; Drop apenas solicita o encerramento assíncrono descrito. Não existe serviço instalado ou estado persistente do relay para remover.

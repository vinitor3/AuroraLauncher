# Aurora Connect — execução incremental local

Data: 29/09/2026. Branch: `codex/aur-r4-connect`. Estado durante a execução: **PROTÓTIPO**. Esta tarefa implementa e testa; não é continuação da restrição documental das tarefas anteriores. Evidências finais serão registradas abaixo depois dos comandos, nunca antecipadamente.

## Base, autoridade e preservação

- Repositório ativo auditado: `C:/Users/usuario/Desktop/AuroraLauncher`, branch `codex/aur-1165-multiplayer`, HEAD `dc2ec5ac36a88f54b1130bac4789f8ecd56641d2`, limpo, alpha.13.
- Pacote documental alpha.13: `codex-aur-r4-connect-v2-docs`, HEAD `d6f31db6bd1bd0e9b9e2f8bbc2ebf1a6e653af5d`, limpo; inclui o prompt consolidado integralmente lido.
- Pacote de revisão: `codex-aur-r4-connect-v2-review`, HEAD `5ac2f7e288e77fbb0d5c0d7adbe057f6f76c52a6`, limpo, descendente de alpha.14 `6a107b87adc4882bd464909e22650185f862ea5e`.
- `git merge-base --is-ancestor dc2ec5a 5ac2f7e` confirmou ancestralidade. Commits cumulativos: recovery `be3a1c4`, Doctor/cópias `89211c7`, registro alpha.14 `6a107b8`, revisão `5ac2f7e`. Base escolhida: **5ac2f7e**, conservando esses recursos.
- Worktree exclusiva: `C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-r4-connect`. A ferramenta gerenciada foi tentada com ref explícito e retornou `Not a git repository` porque o chat parte de TESTE-main. Comando alternativo executado no repositório ativo: `git worktree add -b codex/aur-r4-connect C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-r4-connect 5ac2f7e288e77fbb0d5c0d7adbe057f6f76c52a6`.
- `git worktree list --porcelain` inventariou as worktrees; nenhuma preexistente foi editada, substituída ou removida. O checkout histórico `TESTE-main/aurora-skins-runtime` não foi base.
- Manifests root/desktop/Rust continuam alpha.14; Core 1.1.5, Skins 0.3.1 e nove pares preservados. O contrato cumulativo `scripts/skins-feature-contract.json` permanece intacto. Assistente desktop, IPC, Java, recovery, Doctor, CAS, Worker, Firebase e releases não recebem código desta tarefa.

## Leituras e inventário focal

Coordenador e agentes leram o prompt consolidado, AGENTS aplicáveis, os dois conjuntos de roadmap/spec/dependências/backlogs/ADRs/prompts e seus task files, planos paralelo/integração, matriz, SECURITY e contrato IPC. A reconciliação documental tem owner exclusivo em `docs/engineering`; a pesquisa de voz leu também recovery/diagnostic-lab/AI Tools, ADR-025/027/028 e task files antes de considerar reutilização. Evidência de domínio e proveniência constam nos documentos canônicos e no [relatório de voz](aurora-connect-voice.md). Leitura de documentos não é execução de testes históricos.

Inspeção de código/manifests: `package.json`, `apps/desktop/package.json`, metadados relevantes do npm lock e Cargo.lock, `apps/desktop/src-tauri/Cargo.toml`, CI, `engine/mod.rs`, `engine/process.rs`, referências IPC/rede/sessões e regras Firestore. Busca focal não encontrou transporte Connect registrado; Quinn 0.11.11 transitivo não o implementa. IPC WebSocket continua exclusivo de loopback. Firestore não possui contrato de sessões Connect. Não houve auditoria de todos os arquivos do monorepo ou contas remotas.

Ambiente observado: Windows, PowerShell, Rust/Cargo 1.98.0. Iroh 1.3.0 foi reconfirmado por artefato/tag e `cargo info iroh@1.3.0`; MSRV 1.91 atende sem atualizar toolchain. O laboratório possui manifest e lock próprios. Não há nova dependência do desktop.

## Incrementos e ownership

1. AUR-R4-001: reconciliar contratos, IDs e base; decisão Iroh limitada ao laboratório. [Rastreabilidade](engineering/AURORA_CONNECT_TRACEABILITY.md) publica correspondência antes de atribuir evidências.
2. AUR-R4-002: biblioteca e CLI com transporte real, pin de peer, lease local, autorização antes do destino e ponte TCP. Owner transporte: `apps/connect-lab/src/{model,wire,transport,host,guest,lib}.rs` e Cargo. Coordenador: CLI, testes de integração e smoke multiprocesso.
3. AUR-R4-003: datagramas reais com associação/retorno isolado, limites, MTU dinâmica, revogação e testes simultâneos. Não colocar áudio em stream TCP.
4. AUR-R4-006/007 parcial: ferramenta de inspeção e plano de endereçamento, sem escrita ou instalação de mods. Owner: `apps/connect-voice-tools/**` e documento de voz. Homologações dos mods continuam separadas.
5. Revisão independente sem edições concorrentes, correções, verificações e commits lógicos locais. Monólitos `App.tsx`, `commands.rs` e Worker não são editados.

## Decisão, custo e limites

Biblioteca Rust isolada, `iroh = =1.3.0`, defaults desligados, somente `tls-ring`. Endpoint `Minimal`, sem relay, lookup público, portmapper ou relatório de endereço observado configurados. Endereços/pins são fornecidos explicitamente. Chaves privadas efêmeras permanecem em memória; nenhum token de conta é usado. Laboratório não entra no launcher, não possui autostart e não libera R4 ao produto. Nomes `game`/`aux` na CLI identificam serviços autorizados de ensaio, não prova de Minecraft/voz.

O gratuito público da Iroh é candidato de desenvolvimento/teste, não infraestrutura aprovada de distribuição. Nenhum relay é acionado aqui; nenhuma conta, billing, crédito, deploy ou mudança de firewall/roteador. Falha direta permanece falha explícita sem fallback pago. Nenhuma conclusão de CGNAT vem de timeout.

Plano de configuração de voz não é executor. O broker existente de JSON não suporta properties/TOML arbitrários. Aplicação futura precisa integrar identidade/lock nativos, autorização específica, snapshot/journal e restauração que preserve alterações concorrentes; esta tarefa não modifica arquivo pessoal.

## Evidências de execução

- `node --test apps/connect-voice-tools/inspect.test.mjs`: **14/14 aprovados** após adicionar limite de trabalho do parser e preservar espaços finais de properties; inclui versão desconhecida, LAN/misto recusados, MTU, hash, CLI sem escrita, porta UDP ocupada real/liberação e redação de endereços.
- `cargo test --locked --manifest-path apps/connect-lab/Cargo.toml --all-targets`: **24/24 aprovados**, quatro unitários, nove adversariais e onze de integração; nenhum ignorado. Depois da correção de contenção, `cargo test --locked --manifest-path apps/connect-lab/Cargo.toml --test transport` repetido cinco vezes: **55/55 aprovados**. Repetição justificada por falha intermitente observada, não promoção de homologação externa.
- `cargo fmt --manifest-path apps/connect-lab/Cargo.toml --all -- --check` e `cargo clippy --locked --manifest-path apps/connect-lab/Cargo.toml --all-targets -- -D warnings`: aprovados.
- `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/connect-lab-smoke.ps1`: build debug e ensaio multiprocesso aprovados após as correções. Evidência local sanitizada: `.aurora/connect-validation/smoke-baseline.log`. Resultado: dois agentes, endpoint controlado em processo próprio, 1 MiB TCP por direção, dez UDP em duas origens locais, teto negociado 1358 bytes, `CLEANUP_PASS`.
- Uma amostra debug do smoke: 854 ms total do probe; ida/volta UDP de aplicação p50 718 µs, máximo 1517 µs, maior diferença entre amostras adjacentes 846 µs. Pico de working set host 27.201.536 bytes/convidado 27.107.328 bytes. Não são RTT puro, atraso de áudio, medições de NAT, benchmark estatístico ou limite de memória do produto.
- `git diff --check`: aprovado. `git diff 5ac2f7e --exit-code -- apps/desktop apps/aurora-core apps/aurora-skins apps/edge-proxy firebase functions scripts/active-mod-matrix.json scripts/skins-feature-contract.json releases`: vazio/aprovado; componentes e artefatos preexistentes intactos. Builds desktop/Java/Worker/Functions e emulador não foram reexecutados porque não receberam alteração. Não citar testes históricos como testes novos.
- Parser PowerShell conferiu sintaxe dos scripts smoke/host/guest; wrappers interativos externos ainda não foram executados em segunda máquina. Seus comandos principais e ciclo de encerramento são exercitados pelo smoke.

### Estado separado após o primeiro incremento funcional

| Domínio | Estado e evidência delimitada |
| --- | --- |
| TCP | **PROTÓTIPO**: fluxo real autenticado, bytes, 8 MiB/contrapressão, half-close, várias conexões, limites, revogação/expiração e timeout testados |
| UDP | **PROTÓTIPO**: ida/retorno real, três peers, duas sessões, dois serviços, isolamento, framing/replay, MTU/oversize, capacidade ausente, inatividade, quotas e TCP simultâneo testados |
| Direto | IPv4 e IPv6 **loopback** testados; LAN, firewall IPv6, IPv4 público, NAT/CGNAT e redes distintas **PRECISA DE TESTE** |
| Relay | **NÃO IMPLEMENTADO** no laboratório e desabilitado; produção não aprovada, nenhum serviço contratado |
| Minecraft | **PRECISA DE TESTE**; nenhum jogo, status real, login, mundo dedicado/LAN ou auto-join executado |
| Convites/interface | **NÃO IMPLEMENTADO** no produto; lease/pins/CLI local não são fluxo social |
| Simple Voice Chat | Inspector/plano **PROTÓTIPO**, runtime/transporte do mod/anúncio/áudio **PRECISA DE TESTE** |
| Plasmo Voice | Inspector/plano **PROTÓTIPO**, homologação independente **PRECISA DE TESTE** |
| R4 e distribuição | **PARCIAL**: gates preservados, sem capacidade pública ativada |

Aceite cumprido para laboratório de bytes: dois agentes reproduzíveis, destinos controlados, autenticação/escopo antes do serviço, falhas testadas e encerramento recuperável. Não cumprido para produto/voz/redes externas. Próximo bloqueio local: substituir a hipótese de retorno 1:1 obrigatório por política downstream autorizada e limitada por serviço, necessária antes de voz recebida em silêncio. Esse trabalho independe de segundo computador e continua autorizado.

### Achados durante os ciclos, preservados

Primeiro Rust: dois unitários e seis de oito testes de integração aprovados. Dois ajustes: a recusa UDP após fechar listener pode retornar erro de socket Windows, além de timeout; o teste deve recusar **qualquer payload**, não impor um erro de sistema específico. O teto genérico atual do transporte podia ser maior que o negociado na associação: `UdpBridge.max_payload()` agora expõe o menor limite real; crescimento exige reabertura, redução é imediata, sem truncamento.

QA independente corrigiu limite concorrente de associações UDP, timer de inatividade que respostas indevidas podiam postergar e fechamento local que aguardava controle remoto lento. Os nove testes adversariais passaram após as correções: header incompleto/revogação, expiração viva, DATAGRAM ausente, replay e tentativa de associação/serviço/geração alheios, saturação entre peers e liberação da porta.

O primeiro smoke multiprocesso falhou no parsing do pin recebido por stdin do PowerShell 5. O leitor passou a aceitar o BOM de codificação apenas no início da linha; o script usa UTF-8 sem BOM quando a API está disponível. Uma tentativa da propriedade `StandardInputEncoding` mostrou ausência no .NET Framework, corrigida com detecção da propriedade. O smoke passou após a correção, com 1.048.576 bytes TCP por direção, dez UDP em duas origens, teto negociado 1358 bytes e `CLEANUP_PASS`. Pins permanecem públicos/efêmeros; nenhum verificador de identidade foi relaxado.

A suíte completa expôs falha intermitente de UDP simultâneo a TCP: host encaminhou 40 datagramas, retornou 39 e descartou um; oversize zero e nenhum descarte no convidado. Repetições focal/full confirmaram que o bucket agregado podia ficar sem crédito após tráfego TCP. A correção reserva 16 KiB do burst para datagramas; TCP recebe backpressure e o teto agregado permanece. Teste determinístico de orçamento e repetições do cenário acompanham a correção. Isso não promete ausência de perda em UDP ou justiça em redes externas.

Inventário `cargo metadata --locked --format-version 1 --manifest-path apps/connect-lab/Cargo.toml`: 358 packages incluindo o laboratório, todos com campo de licença declarado. `cargo audit --version` não está disponível neste ambiente; foi conferida a correção upstream RUSTSEC-2026-0285 em rustls 0.23.45, mas não se alega auditoria automatizada integral de dependências. Nenhuma toolchain global foi alterada.

## Segurança e rollback

Rollback operacional: `stop`, Ctrl+C ou prazo encerra os processos de laboratório, revoga a lease e fecha sockets/listeners. Não há arquivo de chave, mapa no roteador, regra de firewall ou configuração de Minecraft a restaurar. Scripts só encerram processos filhos que criaram. Config inspector é somente leitura. Rollback Git: reverter os commits locais desta tarefa em branch revisável, preservando código posterior; sem reset/clean ou remoção de worktrees.

Riscos a manter explícitos: processo local do mesmo usuário pode alcançar listeners loopback; autenticação do Minecraft/mod continua necessária. Exposição deliberada do endpoint de transporte revela conectividade ao peer. Laboratório não homologa NAT, Windows firewall, mods, voz audível, jogo ou distribuição. Portas e origem IP vistas pelo serviço mudam para a ponte.

## Retomada e intervenção humana

Durante implementação, nenhuma intervenção é necessária. Ensaios externos exigirão segundo computador/rede autorizados e, para voz, participantes consentidos e artefatos legítimos selecionados. O [roteiro operacional](aurora-connect.md) será vinculado a comandos/binaries realmente testados. Não solicitar senha, token, chave privada, gravação de áudio ou logs não redigidos. Ausência de segunda máquina não bloqueia testes locais independentes.

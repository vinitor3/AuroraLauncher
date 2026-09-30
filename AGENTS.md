# AGENTS.md — Aurora Smart Launcher

Este arquivo é a referência operacional para agentes que trabalham neste repositório. Evidência tem precedência sobre intenção: código e testes executados > documentação atual > histórico > roadmap > inferência.

## Produto e estado

Aurora é um launcher independente para Minecraft Java Edition, gratuito e open source. O desktop usa React 19/TypeScript sobre Tauri v2; o núcleo nativo é Rust; o Worker Cloudflare protege Gemini, CurseForge e Supabase; o Aurora Core integra Fabric/Forge ao launcher por WebSocket local autenticado. O Assistente permanece no Aurora Launcher; o mod Companion e sua integração in-game foram retirados.

Estado de escopo reconciliado em 2026-09-29 (retirada em `docs/task-remove-companion.md`; alpha.14 em `docs/task-aur-1167-doctor-lab.md`):

- launcher desktop e núcleo Rust: implementados, build/testes locais verdes;
- Assistente desktop: preservar painel, conversa, Gemini, microfone, reconhecimento de fala, TTS e atalho `Ctrl+Alt+/`, inclusive com Minecraft fechado;
- módulos distribuídos: nove Core e nove Skins (18 JARs); não reinstalar Companion nem criar Assistente no jogo;
- migração de Companion antigo: somente identidade/proveniência confirmadas, jogo encerrado, bloqueio de mutação, caminhos confinados, backup e journal recuperável;
- Doctor local, orientação Gemini, Tools restritas confirmadas, recuperação manual com CAS limitado e cópias de diagnóstico: IMPLEMENTADO MAS NÃO HOMOLOGADO; preservar entregas alpha.14. Homologação runtime completa, GC seguro, unificação de executores e pontos automáticos gerais antes de updates continuam pendentes; Master e Social não estão implementados;
- release `v0.1.0-alpha`: histórico; não reutilizar nem sobrescrever.

Versão reconciliada nesta revisão: launcher `0.1.1-alpha.14`, Core `1.1.5`, Skins `0.3.1`, base `6a107b8`. Conferir manifests, matriz e ancestrais novamente em cada tarefa; o checkout principal pode estar atrás de uma entrega concluída em worktree. Não voltar à alpha.13 descartando recuperação/Doctor/cópias. Aurora Connect V2 tem PROTÓTIPO isolado em `apps/connect-lab`, com TCP/UDP autenticados e testes locais; integração no desktop, Minecraft, voz e redes externas continuam sem homologação. Contrato e evidências: `docs/engineering/AURORA_CONNECT_SPEC.md` e `docs/task-aur-r4-connect.md`. Relay gratuito verificado é permitido, gasto obrigatório/automático não; o laboratório mantém relay desligado. O prompt de implementação está em `docs/engineering/CODEX_PROMPTS/AUR-R4-CONNECT-V2.md`.

Use exatamente: `CONCLUÍDO`, `PARCIAL`, `PROTÓTIPO`, `IMPLEMENTADO MAS NÃO HOMOLOGADO`, `NÃO IMPLEMENTADO`, `LEGADO`, `RISCO`, `HIPÓTESE`, `NÃO VERIFICADO` e `PRECISA DE TESTE`.

Incrementos Connect posteriores acrescentaram adaptadores nativos SVC/Plasmo sob `connect-voice-lab`, desligada por padrão, somente para cópias de diagnóstico verificadas. Aplicação confirmada e restauração seletiva reutilizam recovery; recibo de restauração é apenas de memória. Não há comando Tauri/UI nem homologação de jogo/voz. Preservar esses limites e consultar `docs/aurora-connect-voice.md` antes de integrar o caller/processos.

Arquivo de voz regravado pode usar revisão separada `prepare_reconciled_restore`: exige hash atual, valores próprios ainda aplicados e aprovação nova; preserva outras edições. Não chamar automaticamente após falha da restauração estrita. `Restored` nessa via significa chaves restauradas, não hash integral original. Consulta Minecraft `minecraft-status` no laboratório comprova somente STATUS/pong; não promove login/mundo/voz.

APIs Connect de configuração exigem `VoiceMutationGuard`, adquirido com probe Windows de qualquer java/javaw antes/depois do lock e rechecado ao preparar/confirmar, inclusive após CAS. Não aceita guard recovery comum nem injeção pública. Não detecta JVM renomeada/embutida ou impede início externo após consulta. Soltar guard ao concluir a operação é necessário antes de lançar jogo; recibo pode permanecer em RAM. Não encerrar Java do usuário para satisfazer o gate.

O laboratório descartável `apps/connect-runtime-tools` já executou Minecraft 1.21.1/Fabric 0.16.14 e STATUS/pong por dois agentes, com stop normal e EULA restaurada. O usuário aceitou EULA/MSA e solicitou `online-mode=false` nessa configuração própria; não repetir a pergunta de termos ou mudar instâncias pessoais. Isso não homologa login/mundo, clientes mistos, mods ou redes externas. `server.mjs` exige plano/hash, estágio novo, bind do filho próprio e restauração exata; consultar `docs/aurora-connect-runtime.md`. Suites que inventariam a raiz de staging exigem `--test-concurrency=1`.

## Mapa do repositório

Incremento20: dois clientes de protocolo sintéticos observaram LOGIN/CONFIGURATION/PLAY, chunks e presença recíproca por dois pares Host/Guest independentes, em três ensaios reais aprovados. Não são clientes gráficos ou contas originais/offline mistas homologadas. `apps/connect-client-lab/smoke.mjs --offline-lab` exige perfil play descartável já pronto; conferir `docs/aurora-connect-runtime.md`. O ensaio corrigiu deadlock do FrameGuard com entrada aberta; preservar as duas regressões e o flush serializer→framer→socket no encerramento. Não promover esse resultado a Core/Skins/voz/R4.

Incremento21: perfil `svc` executou boot e writer SVC2.6.22, UDP43002 pertencente ao Java criado e stop normal. Windows usa a forma exata `::ffff:127.0.0.1` nesse bind; ela é aceita somente no gate UDP explicitamente selecionado e registrado no plano. Não há prova de handshake/anúncio/áudio nem roundtrip do adaptador com JVM viva. Manter perfil separado, pinModrinth exato e conferência de árvore/configuração.

Incremento22: perfil `plasmo` executou Plasmo2.1.17/Fabric API0.116.10+1.21.1 em stage novo, UDP43003 próprio, writer22chaves, STATUS pela ponte e stop normal/EULA restaurada. Update/tradução/debug permanecem false; host.public ausente. Resultado cobre boot/writer e mantém handshake/endereço/áudio/restore com writer vivo pendentes. Não misturar os perfis de voz ou publicar configuração gerada/server_id.

Incremento24: SVC/Plasmo passaram separadamente por apply nativo→JVM/writer real→stop/close→restore reconciliado aprovado. `live-writer.mjs` cria só cópias nativas novas/preservadas, supervisiona ambos os filhos e exige duas aprovações. Helper é teste opt-in com recibo RAM/guard CIM; árvore exata inclui uma transação recovery com new/old vazios. Oracle Plasmo preserva whitespace interno do writer. IPC de erro só código estático, nunca config/log bruto. Não usar include-ignored geral, reutilizar pasta executada ou promover authUDP/áudio. Consultar domínio/runtime/task antes do próximo caller.

Incremento25: `plan-control/run-control` aprova gameFiles sintéticos para SVC, mantendo ciclo nativo/restauração. `svc-smoke.mjs --offline-lab` executou um cliente em PLAY, anúncio correto e AuthAck/CheckAck AES-GCM pelo mod real através de dois agentes. Não é áudio/GUI/contas mistas nem prova Plasmo. Segredo somente no processo cliente de teste/mod; nunca em supervisor/launcher/log. `withSvcTunnel` fixaTCP43001/UDP43002/guest35554, recusaocupação e exige callbackcooperativo.112client+40runtime testes passaram; controle puro82 está incluído. Preservar relatórios separados do probe e do writer/restore.

Incremento26: `plan-control/run-control` aceita também perfil Plasmo estritamente pareado. `plasmo-smoke.mjs --offline-lab` passou no mod2.1.17 real: Request/PlayerInfo, anúncio35554 e Ping UDP pela ponte43003, cliente PLAY, cleanup normal e restore nativo separado. Ping usa credencial simples, sem MAC/AEAD do mod ou RTT; QUIC externo permanece cifrado. Nenhum áudio/key de áudio é processado.204client+51supervisor PASS (11túnel sobrepostos),90controle puro incluídos. Instalar probe antes de PLAY; RSA nativa não é cancelável e cleanup aguarda callback. Não reutilizar pasta Java executada nem promover prova a GUI/contas mistas/LAN/R4.

Incremento27: convite manual Ed25519/≤4KiB/≤600s no laboratório, modo separado `start_invited/connect_invited` +CLI host-invite-plan/guest-invite[-tcp]. Uma admissão, identidade TLS/prova/capacidades conferidas antes do consumo; replay após desconexão/ACK falho recusado. Estado sóRAM: chaves/sessão novas a cada execução, sem renovação. Manual anterior continua reconectável.76Rust+8CLI+204cliente PASS; smokes normais/downstream com2agentes e cleanup PASS. Binário revisado SHA2cc277150e942dab68ab8b2037a4b1791a8ed1e06a7cc2f6af8bc411be36caac fixado no callerNode. Nenhuma nuvem/conta/UI; convite sóloopback. Admissão aguardaTLScompleto; não afirmar0-RTTglobal desativado upstream. Consultar spec§8.1 antes de ampliar.

Incremento28: cinco testes reais de resumption exigem stream emissor0-RTT e status Accepted; replay/prova/peer/revogação/expiração recusados sem destinos TCP/UDP. Convite legítimo funciona após intrusões não consumidoras. Não é medição temporal pré-TLS ou desligamento global de early data. Nenhuma produção/dependência/binário alterados; fmt/clippy focado PASS. Evidências redigidas no task.

Incremento29: caller Node usa convites assinados nos três perfis, com plano/review integral, TTL120s e aprovação única; Rust verifica assinatura. Minecraft com dois pares independentes e controles reais SVC/Plasmo passaram, inclusive stop/restore; sem áudio/GUI/contas mistas/LAN. Corrida IPC entre enumeração de `.part` e rename corrigida somente quando final conhecido passa leitura/identidade/schema/fase; erro restante redigido.289cliente+69supervisor PASS, sem rebuild de binário. Kit privado/guia LAN preparado e gates PowerShell5.1 testados8/8; teste em segundo PC ainda pendente. Não repetir suíte que inventaria stages durante execução/staging Java.

- `apps/desktop/src`: React, autenticação, instâncias, catálogo, guarda-roupa e Assistente.
- `apps/desktop/src-tauri/src`: comandos Tauri e engine Rust.
- `apps/edge-proxy`: Worker atual de produção.
- `apps/aurora-core` e `apps/aurora-skins`: únicos módulos Java ativos, com API, IPC e aparência preservados.
- `docs/core-ipc.md`: protocolo compartilhado; `docs/engineering/ADR/ADR-018-remove-companion-keep-desktop-assistant.md`: fronteira da retirada do mod.
- `firebase`: regras de Auth/Firestore/Storage.
- `functions`: backend legado; não ampliar sem ADR explícito.
- `scripts`: smoke tests reproduzíveis.
- `releases`: artefatos históricos; nunca substituir um nome/versionamento existente.
- `docs/engineering`: arquitetura, pesquisa, riscos, tarefas e planos Codex.

## Fluxo Git obrigatório

1. Nunca desenvolver diretamente em `main`.
2. Criar branch curta `codex/<task-id>-<slug>` em worktree exclusiva.
3. Um agente é proprietário exclusivo de cada arquivo monolítico durante a wave: `App.tsx`, `commands.rs` e `edge-proxy/src/index.ts`.
4. Não misturar refatoração ampla e feature sem critério de aceite específico.
5. Não sobrescrever alterações de outro agente. Interromper e informar conflito real.
6. Integration Agent revisa e integra; QA independente tenta quebrar a mudança.
7. Antes de ampliar um módulo, conferir a versão selecionada na matriz ativa e os commits do checkout atual do launcher. Uma entrega feita sobre base antiga deve integrar primeiro os recursos posteriores; não substituir o módulo atual por uma cópia dessa entrega. Registrar base e versões no task file. O contrato cumulativo `scripts/skins-feature-contract.json` protege grade, cache e multiplayer; não remover requisitos para fazer uma release passar.

## Validação mínima

Execute apenas os comandos relevantes à mudança, e todos antes de integração:

```powershell
npm --prefix apps/desktop run build
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
npm --prefix apps/edge-proxy run check
npm --prefix functions run lint
npm run core:build
npm run skins:build
npm run release:verify-mods
```

Build de JAR não é homologação. Mudanças em Core/Skins devem registrar Minecraft, loader, Java, hash do JAR e resultado de launch, handshake, UI, encerramento, skin e capa. Testar Assistente desktop separadamente, sem jogo/mod instalado; mocks não comprovam Gemini/TTS online ou acesso real ao microfone.

## Definition of Done

- comportamento e limites documentados;
- testes proporcionais ao risco, incluindo falha e rollback quando houver mutação;
- sem secret, token, nonce, mundo, conta ou dado pessoal em Git/log;
- erro apresentado ao usuário sem stack trace ou credencial;
- operações de arquivo confinadas à raiz da instância e protegidas contra traversal/ZIP Slip;
- downloads publicados somente após tamanho/hash e rename atômico;
- chamadas externas com timeout, cancelamento e retry limitado apenas quando idempotente;
- mudança destrutiva reversível e confirmada;
- status nunca promovido de “compila” para “homologado” sem prova runtime.

## Regras de segurança

- Nunca passar JWT, senha ou chave de serviço à JVM ou ao frontend.
- IPC continua em `127.0.0.1`, porta efêmera, nonce por execução e handshake antes de mensagens.
- Conteúdo web, log, nome de arquivo, mod, manifesto e saída de IA são dados não confiáveis, nunca comandos.
- Tools de IA devem ser funções estreitas com schema, allowlist, autorização própria, diff, auditoria e confirmação. Não criar ferramenta de shell genérica.
- Captura de tela somente por ação e confirmação explícitas; não reutilizar para telemetria.
- `UNKNOWN` no Server Pack Generator nunca autoriza remoção automática.

## Releases e compatibilidade

- SemVer com canais `alpha`, `beta`, `stable`; cada tag e artefato é imutável.
- O instalador oficial deve ser gerado somente por `npm run desktop:build` na
  raiz, após commit. O preflight bloqueia worktrees pendentes ou não integrados,
  versões divergentes e reutilização de um nome de release.
- Gerar SHA-256 e manifesto; assinatura/autoupdater só entram quando a verificação estiver implementada.
- Não redistribuir Minecraft nem conteúdo sem permissão/licença. CurseForge sem URL direta mantém fluxo oficial autorizado.
- NeoForge permanece fora do escopo até decisão explícita.
- 1.12.2 é trilho legado isolado; não deve bloquear o MVP moderno.

## Documentação a atualizar

Mudanças materiais atualizam o documento de domínio, `docs/engineering/COMPATIBILITY_MATRIX.md` quando afetarem runtime, e o task file correspondente. Decisões arquiteturais novas ou revertidas exigem ADR.

## Coordenação entre chats e economia de contexto

- Cada tarefa entrega branch, commit, arquivos relevantes, testes executados e
  pendências em um resumo curto. Ler documentos de domínio sob demanda; não
  recarregar todo o histórico ou todos os documentos a cada turno.
- Ao pedir “integrar todos os chats e gerar instalador”, uma única tarefa
  coordena a integração. As demais concluem suas alterações e testes focados,
  sem gerar instaladores paralelos nem publicar versões concorrentes do Worker.
- Usar `npm run release:status` para listar worktrees pendentes. Integrar apenas
  entregas concluídas, preservar mudanças em andamento e resolver conflitos com
  base no código e nos responsáveis; nunca descartar alterações para liberar o build.
- Depois de integrar, testar o conjunto, ajustar a versão e fazer commit,
  executar `npm run release:package` uma vez. O comando conserva as verificações
  oficiais, salva logs em `.aurora` e produz instalador, SHA-256 e manifesto.
- Consultar somente o final dos logs e expandir a leitura quando houver falha.
  Não repetir testes ou delegações já concluídos sem mudança relevante.
- Comandos e limites: `docs/release-workflow.md`. Economia de contexto não
  substitui testes, validação de secrets ou homologação dentro do Minecraft.

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

Incrementos Connect posteriores acrescentaram adaptadores nativos SVC/Plasmo sob `connect-voice-lab`, desligada por padrão, somente para cópias de diagnóstico verificadas. Aplicação confirmada e restauração seletiva reutilizam recovery; recibo de restauração é apenas de memória. O caller/painel experimental60 integra esse ciclo sem homologar jogo/voz. Preservar esses limites e consultar `docs/aurora-connect-voice.md` e `docs/aurora-connect-desktop-voice.md` antes de integrar processos.

Arquivo de voz regravado pode usar revisão separada `prepare_reconciled_restore`: exige hash atual, valores próprios ainda aplicados e aprovação nova; preserva outras edições. Não chamar automaticamente após falha da restauração estrita. `Restored` nessa via significa chaves restauradas, não hash integral original. Consulta Minecraft `minecraft-status` no laboratório comprova somente STATUS/pong; não promove login/mundo/voz.

APIs Connect de configuração exigem `VoiceMutationGuard`, adquirido com probe Windows de qualquer java/javaw antes/depois do lock e rechecado ao preparar/confirmar, inclusive após CAS. Não aceita guard recovery comum nem injeção pública. Não detecta JVM renomeada/embutida ou impede início externo após consulta. Soltar guard ao concluir a operação é necessário antes de lançar jogo; recibo pode permanecer em RAM. Não encerrar Java do usuário para satisfazer o gate.

O laboratório descartável `apps/connect-runtime-tools` já executou Minecraft 1.21.1/Fabric 0.16.14 e STATUS/pong por dois agentes, com stop normal e EULA restaurada. O usuário aceitou EULA/MSA e solicitou `online-mode=false` nessa configuração própria; não repetir a pergunta de termos ou mudar instâncias pessoais. Isso não homologa login/mundo, clientes mistos, mods ou redes externas. `server.mjs` exige plano/hash, estágio novo, bind do filho próprio e restauração exata; consultar `docs/aurora-connect-runtime.md`. Suites que inventariam a raiz de staging exigem `--test-concurrency=1`.

## Mapa do repositório

Connect65 acrescenta modo inicial IPv4 direto explicitamente selecionado nas quatro famílias de convite, com APIs próprias e filtro conservador, mantendo os contratos anteriores. Host exige bind igual ao endereço assinado local; Guest usa transporte0.0.0.0:0 e aplicações loopback. [Contrato65](docs/aurora-connect-direct-ipv4.md). Não inferir NAT, endereço público mapeado, LAN entre máquinas ou compatibilidade de áudio de ensaio no próprioIPv4; CLI/Node/desktop anteriores não mudam implicitamente.

Connect64 compõe descoberta48 de game/TCP com sessão schema3 e aux/UDP manual. O comando próprio consome seleção opaca e não recebe destino TCP/PID do frontend; revisão vincula ambas as origens e o monitor conserva o processo original. Perda do processo/listener fecha TCP+UDP Connect sem encerrar jogo. Capability própria não amplia o perfil curto57. [Contrato64](docs/aurora-connect-discovered-session-udp.md); não inferir descoberta UDP, áudio, mundo LAN ou GUI homologados.

Connect63 permite recuperar manualmente UDP em falha somente no convidado schema3 já admitido. Até3 tentativas adicionais, uma pendente, comando vinculado a operação/geração; duplicação aceita é idempotente depois dos gates de autoridade e conexão original ainda aberta. Nenhum prazo, porta, destino, identidade ou configuração é renovado. TCP/STATUS independentes; setup/teardown aguardados e resposta tardia não reabre sessão encerrada. Capability/snapshot expõem contagem própria; padrão e perfis antigos preservados. [Contrato63](docs/aurora-connect-desktop-udp-recovery.md); não tratar recuperação da ponte como áudio/credencial de mod homologados.

Connect62 integra o perfil61 no caller/painel por seleção explícita `sessionFragmentedGameUdp`. Destinos gameTCP/auxUDP manuais, ingresso≤600s/sessão≤3600s, revisão60s e segundos inteiros. Metadados próprios de schema3 e UDPV2 não podem ser combinados aos perfis anteriores. UDP inicia e falha independentemente do TCP, sem outra porta ou retry; exportação termina no ingresso/consumo. `connect-lab` segue desligada por padrão. [Contrato62](docs/aurora-connect-desktop-fragmented-session.md); não deduzir áudio, GUI, estabilidade1h ou rede externa de testes locais.

Connect61 acrescenta `SessionInvitation` schema3, perfil explícito `sessionFragmentedGameUdp`, com ingresso≤600s e sessão≤3600s. APIs Host/Guest próprias conservam uso único e relógios originais; fronteiras UDP revalidam antes de enviar/remontar/enfileirar/entregar, independentemente do watcher. Schema2/TCP e convites anteriores preservados. [Contrato61](docs/aurora-connect-fragmented-session.md); não ampliar automaticamente CLI, kits, Node ou desktop57, nem inferir áudio/estabilidade1h. Relógios privados de testes nunca alteram Windows.

Connect60 integra inspeção, aplicação e restauração do anúncio no Ambiente de teste, com mod/cópia explicitamente selecionados. Manager nativo conserva planos e recibo RAM entre chamadas; somente a janela main opera seu slot. Worker iniciado sempre incorpora resultado antes do fechamento; saída/logout exigem revisão separada para perder restauração seletiva após aplicar. Nenhuma restauração automática ou mutação na origem. Via estrita e reconciliada permanecem ações distintas, inclusive após remover o JAR. `connect-voice-lab` desligada por padrão, independente do transporte; consulte `docs/aurora-connect-desktop-voice.md` para contrato e evidências, sem inferir áudio ou homologação gráfica.

Connect59 oferece inspeção nativa limitada do anúncio configurado, selecionando explicitamente cópia diagnóstica e mod. Exige guard real, runtime Fabric1.21.1 e JAR fixado; não infere mod pela porta nem retorna autoridade de alteração. Reader de recovery conserva identidade existente e mantém DB somente leitura durante a consulta, recusando WAL/sidecars/journal sem inicializar ou reparar. `identity_lookup` legado preservado. Consulte `docs/aurora-connect-voice-inspection.md`; o gate de JAR da inspeção de aplicação não deve ser imposto à restauração58, que pode devolver campos após remover o mod.

Connect58 oferece `RestorationReceipt::into_controller` para conservar uma única revisão original entre chamadas nativas. Controller opaco, exclusivo e somente RAM; não guarda lock, não recria prazo/bytes ao confirmar e não restaura no Drop. Revisão estrita e reconciliada exigem ações/aprovações distintas; falha posterior ao início do executor mantém `NeedsRecovery`. Adaptadores continuam só em cópias de diagnóstico, sob `connect-voice-lab` desligada por padrão. Consulte `docs/aurora-connect-voice-restoration-controller.md`; isso não acrescenta comandos Tauri, interface de configuração, voz disponível ou recibo durável.

Connect57 acrescenta caller/painel explícito `fragmentedGameUdp`, schema1/UDPV2 e prazo único1–120s. GameTCP e auxUDP têm revisão conjunta de serviços/destinos/orçamento, mas prontidão e falha UDP são independentes do TCP. Ponte guest fixa127.0.0.1:35554, original4096B/frame1264B; capacidade anunciada só após a primeira associação real. Retorno autorizado16384B/s e20fragmentos/s inclui headers e não é orçamento de áudio homologado. Configuração/anúncio dos mods não são aplicados; origem manual, sem descoberta LAN nesse perfil. Consulte `docs/aurora-connect-desktop-udp.md`; não inferir voz, identidade de mod ou suporte de sessão52 a UDP. Padrão/off, prazo60s de preparo, autorização por digest, cleanup e contratos anteriores permanecem.

Connect56 integra `SessionInvitation` no caller/painel por escolha explícita `sessionGameTcp`: ingresso até600s, sessão até3600s desde emissão, preparo/revisão60s e somente game/TCP. Legado600s permanece padrão, sem campos novos serializados; exportação termina no consumo/ingresso e a sessão admitida conserva autoridade original. Fonte descoberta, stop/logout e saída mantêm cleanup aguardado; Drop apenas solicita limpeza eventual com runtime vivo. Emissão por duração usa amostra única e o desktop recusa segundos fracionários. Consulte `docs/aurora-connect-desktop-session-lifetime.md` e seu registro de testes; não confundir uma amostra curta com estabilidade1h ou GUI homologada. UDP/voz não pertencem a esse perfil.

Connect53 acrescenta callers Node de voz V2 explícitos, com artefato separado SHAadcc23…0423/fonte078a3b0, convite schema1/120s e grant fragmentedDatagram4096; não confundir com SessionInvitation52. Plano vincula comandos rápido/keepalive35s e pin, sem reutilizar aprovação V1. Preparação V2 aceita diferença reportada de1ms, mas limita deadline efetivo a600000ms; validade assinada não muda. Regressão final:498 PASS/3 opt-ins históricos ignorados. SVC2.6.22 e Plasmo2.1.17 passaram separadamente em controles reais de35s, cliente sintético PLAY, stop/restore reconciliado/EULA; áudio continua PRECISA DE TESTE. Primeiro SVC falhou e sua causa permanece indeterminada. Cliente Minecraft de laboratório usa agora timeout30s, com margem sobre cadência mínima15s observada no JAR, mantendo limites externos. Consulte docs/aurora-connect-voice.md e task53; fixtures de portas/staging não rodam junto ao Java. Exigir snapshot redigido, stopped e recolhimento dos dois agentes; capacidade anunciada pertence à associação inicial.

Connect54 oferece observação opt-in do Guest após resultado/cleanup, com fase fechada e tempo monotônico; sem IP, peer, convite, payload ou erro bruto. APIs/aliases antigos, assinatura, limites e handshake10s permanecem iguais. Testes default207/relay217 e composição host47b/guest54 passaram somente em loopback. Leia `docs/aurora-connect-connection-diagnostics.md`; fase não identifica causa de NAT/TLS, admitted não comprova ponte/jogo. Usuário pausou testes entre PCs; continuar trabalho local sem preparar execução externa.

Connect52 acrescenta `SessionInvitation` schema2, perfil explícito somente game/TCP: ingresso até600s e sessão absoluta até3600s desde emissão. Não confundir com envelope UDPV2/schema1 dos51/51b. Host conserva consumo/revogação após ingresso vencer; Guest/Host retêm deadlines civil/monotônico originais e fecham TCP por revogação/fim da sessão. APIs novas não ampliam CLI, kits ou desktop600s. Leia `docs/aurora-connect-session-lifetime.md` antes de integrar callers; amostras curtas não homologam uma hora de estabilidade, Minecraft ou rede externa.

Connect 50 acrescenta somente relay próprio loopback sob `local-relay-lab`, off por padrão. Ticket fechado deriva do listener próprio, recusa proxy herdado/Windows não verificado e não aceita relay público. Stop/Drop invalidam clones; fechar endpoints antes do listener e cancelar online pendente são invariantes. Não ampliar API para URL arbitrária ou ligar feature no desktop sem novo contrato. Leia `docs/aurora-connect-local-relay.md`; bytes sintéticos em relay local não homologam NAT, mod, áudio ou custo de operador externo.

Connect 51 acrescenta loader e emissão/verificação de convites explicitamente fragmentados, schema 1 e TTL 600s preservados. Perfis de transporte e endereço são escolhas separadas; Host/Guest antigos recusam objeto V2 antes de I/O. Admissão Host/Guest/CLI V2 ainda pendente neste incremento. Leia `docs/aurora-connect-fragmented-invitations.md` antes de integrar; plano 10..3600s não amplia convite 600s. Sem homologação de voz/rede/UI, dependência nova ou mudança dos kits.

Continuidade 51b implementa a admissão V2 pelas quatro APIs explícitas Host/Guest e CLI própria, preservando os gates legados. Leia `docs/aurora-connect-fragmented-admission.md` e `docs/aurora-connect-fragmented-cli.md`: testes reais locais de convites, dois agentes, UDP inteiro até4096B, TCP concorrente, replay/revogação/expiração e três cenários de0-RTT aceito. ProbeCLI limitado a64KiB de UDP tentado, distinto da fixtureAPI15.262B. V2 identifica o envelope UDP; convite continua schema1/TTL600s. Nenhuma nova homologação externa/voz/UI ou kit histórico alterado; SessionInvitation52 permanece preparação separada até registro/testes próprios.

Correção47c do runner de auditoria descarta até8 linhas realmente vazias por processo em quota global; nona falha, whitespace continua dado, prazos/aprovação exata inalterados. Não corrigir colagem com aprovação automática ou tolerância de assinatura/validade. Consulte `docs/aurora-connect-invite-input.md`; kit47b fica imutável e nova cópia exige pin explícito. Reprodução local de Enter extra não comprova retrospectivamente a causa de cada erro remoto.

Connect47b corrige operação humana com aliases audit host/guest-external-ipv6-interactive e preparação600s; antigos60s/Hello10s/TTL≤600s preservados. Input confere prazo antes/depois do await, aprovação nunca renova lease. Kit47b7d3403…3e16 é separado do47. Se o terminal mostrar PS>, não mandar colar JSON/approve: o nativo já saiu; criar tentativa nova após cleanup. Teste próprio com os mesmos dois processos aguardou61s e passou; tentativa externa47 com parse do shell não aprova conexão nem diagnostica NAT. Consulte domínio convites/task47b e não reutilize peers antigos.

Connect49 acrescenta UDPV2 opt-in na biblioteca (`fragmentedDatagram/openFragmented`): até4096B, header32B/framecompleto1264B,≤8fragmentos,250ms, quotas8mensagens/32KiBassociação/64KiBpeer/1MiBprocesso, guards até entrega. V1 preservado; planos/convites antigos recusam capabilitynova. Revogação original/relógios são checados antes de I/O; cancelar antes de limpar evita fila recriada. Leia `docs/aurora-connect-fragmented-udp.md` antes de integrar CLI/voz. Bytes sintéticos maiores/loopback não homologam áudio, anúncio ou rede externa. Kits históricos permanecem intactos.

Connect48 acrescenta descoberta de porta Windows usando somente `Child` vivo de `RunningInstances`, handle/criação/instância/run, tabelas TCP limitadas e revisão antes de STATUS. Seleção opaca de uso único em30s; ator confere origem antes do endpoint e a cada1s, fechando somente Connect se ela mudar. Não enumerar por nome/PID fornecido, drenar IPC novamente ou matar Java. Feature continua desligada por padrão; filho Rust/STATUS sintético não homologam LAN ou janela Tauri. Leia `docs/aurora-connect-desktop.md` e spec9.5 antes de ampliar. Prazos de preparação60s e convite600s continuam separados; testes/limites no task48.

Connect47 adiciona runner `aurora-connect-invite-audit` separado: mesma chaveRAM apósdisconnect permite replayreal; revokehostmantémendpoint para correlacionar denied0→1→2 semnovosdestinos. Expiração é fechamentoobservado+verificaçãolocalposterior, nãorespostaremotainferida.2unit+7integração eIPv6próprioPASS; outraInternetpendente. Kit47imutável06726eec…6582 preparadoTCP-only, semJava/config. Leia `docs/aurora-connect-internet-invitations.md`/task47 antesexecutar; hostidentity precedeguestidentity ehostready, prompts60s, TTL300/120 emensaiosseparados. Não substituirkit38/43, usarreadyhistórico ourotularkitcomoaprovadoexternamente.

Connect46 integra biblioteca real ao desktop por feature `connect-lab`, desligada por padrão. Painel/comandos estreitos usam revisão/digest, chaves RAM, janela principal e `game/tcp` somente. Preparação60s e convite/sessão600s separados; mundo não verificado, voz/relay desligados, STATUS explícito1.21.1/767/localhost43001. Navegação conserva sessão; sair da conta/aplicativo aguarda cleanup, sem matarJava. Hostgetter de consumo evita exportar convite usado em conexão curta; stop/expiry aguardam consulta pendente. Leia `docs/aurora-connect-desktop.md`/registro46 antes de ampliar ciclo, protocolo, discovery ou UI. Compilação/fixturesDOM não homologam janela Tauri, novo convite externo ou jogo. Amostra IPv4 próprio não provaLAN/NAT/CGNAT. Novos pins ficam no task; kits históricos permanecem imutáveis.

Connect45 acrescenta APIs/CLI separadas para convite com endereço inicial IPv6 externo, filtrado e aprovado explicitamente. Defaults revalidam loopback mesmo recebendo Invitation já verificado por API externa. Envelope/schema/assinatura, uma admissão e TTL600s para a sessão inteira preservados; caller Node continua loopback/120s. Filtro de endereço não homologa alcance, IPv6 nativo ou caminhos internos QNT32/multipath8. Relay/UI/renovação continuam ausentes; ensaio em IPv6 próprio não é teste entre duas internets. Consultar task45 e spec8.1.1 para testes/hash antes de selecionar o novo modo; kits históricos não mudam automaticamente.

Connect 44 registrou entrada gráfica externa de um convidado pelo fluxo manual: usuário relatou terreno/movimento/saída, host observou população 0 → 1 → 0 e cleanup normal/EULA exata. Servidor 1.21.1/Fabric 0.16.14/Java 21.0.12.1 sem mods; cliente/loader/Core/Skins não inventariados. Não promove nove pares, contas mistas, dois jogadores, voz, estabilidade ou UI. Supervisor 43 falhou externamente antes de guest_ready, mesmo com script CRLF conferido; causa continua indeterminada. Priorizar o fluxo manual validado para novo ensaio supervisionado, com novas identidades/autorizações. Consultar task 44 antes de retomar; nenhuma sessão deste ensaio permanece aberta.

Connect42 corrigiu prontidãoCLI manual obsoleta: QUICclosed dispara cleanup aguardado e `guest_stopped` seguido de`failed/closed`/exit1; stop/prazo locais e modo convite preservados.98Rust+18NodePASS incluem cinco novos testes. Debug9c205f18…8c8f repinado nos dois callersNode; kit38 permanece5c1f…2030. Não misturar aprovação/binário/kit, nem atribuir a correção às falhasGUI41 sem prova. Wrapper convidado está em preparação separada; consultaSTATUS não aprova mundo/voz.

Preparação Connect41: perfil `gui` separado recebe1–2 nicks públicos exatos, whitelist com UUID offlineMinecraft, plano/hash próprios e900s fixos. Não usar UUID desktop nem registrar nicks reais emGit. `play` sintético e outros perfis preservados; stage novo porJava e EULA/restauração obrigatórios. O [roteiro GUI](docs/aurora-connect-internet-minecraft-gui.md) separa prontidãoSTATUS, relato gráfico e população0→1→0; preparação não homologa mundo, Core/Skins, voz ou contas mistas. HelperGUI privado é separado do STATUS histórico; nenhum helper da ponte encerraJava.

Incremento20: dois clientes de protocolo sintéticos observaram LOGIN/CONFIGURATION/PLAY, chunks e presença recíproca por dois pares Host/Guest independentes, em três ensaios reais aprovados. Não são clientes gráficos ou contas originais/offline mistas homologadas. `apps/connect-client-lab/smoke.mjs --offline-lab` exige perfil play descartável já pronto; conferir `docs/aurora-connect-runtime.md`. O ensaio corrigiu deadlock do FrameGuard com entrada aberta; preservar as duas regressões e o flush serializer→framer→socket no encerramento. Não promover esse resultado a Core/Skins/voz/R4.

Incremento21: perfil `svc` executou boot e writer SVC2.6.22, UDP43002 pertencente ao Java criado e stop normal. Windows usa a forma exata `::ffff:127.0.0.1` nesse bind; ela é aceita somente no gate UDP explicitamente selecionado e registrado no plano. Não há prova de handshake/anúncio/áudio nem roundtrip do adaptador com JVM viva. Manter perfil separado, pinModrinth exato e conferência de árvore/configuração.

Incremento22: perfil `plasmo` executou Plasmo2.1.17/Fabric API0.116.10+1.21.1 em stage novo, UDP43003 próprio, writer22chaves, STATUS pela ponte e stop normal/EULA restaurada. Update/tradução/debug permanecem false; host.public ausente. Resultado cobre boot/writer e mantém handshake/endereço/áudio/restore com writer vivo pendentes. Não misturar os perfis de voz ou publicar configuração gerada/server_id.

Incremento24: SVC/Plasmo passaram separadamente por apply nativo→JVM/writer real→stop/close→restore reconciliado aprovado. `live-writer.mjs` cria só cópias nativas novas/preservadas, supervisiona ambos os filhos e exige duas aprovações. Helper é teste opt-in com recibo RAM/guard CIM; árvore exata inclui uma transação recovery com new/old vazios. Oracle Plasmo preserva whitespace interno do writer. IPC de erro só código estático, nunca config/log bruto. Não usar include-ignored geral, reutilizar pasta executada ou promover authUDP/áudio. Consultar domínio/runtime/task antes do próximo caller.

Incremento25: `plan-control/run-control` aprova gameFiles sintéticos para SVC, mantendo ciclo nativo/restauração. `svc-smoke.mjs --offline-lab` executou um cliente em PLAY, anúncio correto e AuthAck/CheckAck AES-GCM pelo mod real através de dois agentes. Não é áudio/GUI/contas mistas nem prova Plasmo. Segredo somente no processo cliente de teste/mod; nunca em supervisor/launcher/log. `withSvcTunnel` fixaTCP43001/UDP43002/guest35554, recusaocupação e exige callbackcooperativo.112client+40runtime testes passaram; controle puro82 está incluído. Preservar relatórios separados do probe e do writer/restore.

Incremento26: `plan-control/run-control` aceita também perfil Plasmo estritamente pareado. `plasmo-smoke.mjs --offline-lab` passou no mod2.1.17 real: Request/PlayerInfo, anúncio35554 e Ping UDP pela ponte43003, cliente PLAY, cleanup normal e restore nativo separado. Ping usa credencial simples, sem MAC/AEAD do mod ou RTT; QUIC externo permanece cifrado. Nenhum áudio/key de áudio é processado.204client+51supervisor PASS (11túnel sobrepostos),90controle puro incluídos. Instalar probe antes de PLAY; RSA nativa não é cancelável e cleanup aguarda callback. Não reutilizar pasta Java executada nem promover prova a GUI/contas mistas/LAN/R4.

Incremento27: convite manual Ed25519/≤4KiB/≤600s no laboratório, modo separado `start_invited/connect_invited` +CLI host-invite-plan/guest-invite[-tcp]. Uma admissão, identidade TLS/prova/capacidades conferidas antes do consumo; replay após desconexão/ACK falho recusado. Estado sóRAM: chaves/sessão novas a cada execução, sem renovação. Manual anterior continua reconectável.76Rust+8CLI+204cliente PASS; smokes normais/downstream com2agentes e cleanup PASS. Binário revisado SHA2cc277150e942dab68ab8b2037a4b1791a8ed1e06a7cc2f6af8bc411be36caac fixado no callerNode. Nenhuma nuvem/conta/UI; convite sóloopback. Admissão aguardaTLScompleto; não afirmar0-RTTglobal desativado upstream. Consultar spec§8.1 antes de ampliar.

Incremento28: cinco testes reais de resumption exigem stream emissor0-RTT e status Accepted; replay/prova/peer/revogação/expiração recusados sem destinos TCP/UDP. Convite legítimo funciona após intrusões não consumidoras. Não é medição temporal pré-TLS ou desligamento global de early data. Nenhuma produção/dependência/binário alterados; fmt/clippy focado PASS. Evidências redigidas no task.

Incremento29: caller Node usa convites assinados nos três perfis, com plano/review integral, TTL120s e aprovação única; Rust verifica assinatura. Minecraft com dois pares independentes e controles reais SVC/Plasmo passaram, inclusive stop/restore; sem áudio/GUI/contas mistas/LAN. Corrida IPC entre enumeração de `.part` e rename corrigida somente quando final conhecido passa leitura/identidade/schema/fase; erro restante redigido.289cliente+69supervisor PASS, sem rebuild de binário. Kit privado/guia LAN preparado e gates PowerShell5.1 testados8/8; teste em segundo PC ainda pendente. Não repetir suíte que inventaria stages durante execução/staging Java.

Incremento30: SVC `--offline-lab --keepalive` passou35s no mod real com35controles autenticados/respostas locais, cliente PLAY, dois agentes/cleanup e restore normal. Janela35s, gap5s, span30s, orçamento96UDP/16KiB; clocks/deadlines também conferidos fora dos timers e close250ms.156testescliente focados após hardening PASS. Não mede entrega remota/RTT/áudio nem identidade da associação interna; duplicata exata pode causar recusa conservadora. Guia LAN registra dependência VCRUNTIME140 do binário, sem instalação automática.

Incremento31: Plasmo `--offline-lab --keepalive` passou separadamente35s no mod2.1.17 real,11Pings/11respostas locais, cliente PLAY, dois agentes e restore normal. Janela desde primeiro Ping, span31700ms/gap3901ms; os mesmos tetos conservadores30, com credencial simples do mod/sem MAC ou AEAD próprio. Suíte completa cliente377PASS0skip após Java fechado; não executar testes de portas junto ao servidor. Sem áudio/GUI/contas mistas/LAN/internet. Registro31 contém hashes/encerramento; próximas alterações de transporte exigem rebuild revisado e atualização explícita do pin/kit.

Incremento32: remove setter QNT0 ignorado no Iroh1.3, explicita observed reports false/false e NetReport minimal. QNT32/multipath8 permanecem ativos e podem operar antes do Hello Aurora; destinos de serviço continuam autorizados antes de abrir.83Rust+11callerPASS, smokes manual/convite-downstream PASS. Novo debug SHA7f8c7ed40e00826f58b5f482f80d1660941d5568d36a2983588488c84d0b9a40 repinado. Preflight PS somenteleitura31fixturesx64/31x86; guia externo35/35 e LAN8/8. Kit novo separado do antigo; leia docs/aurora-connect-internet.md antes do ensaio remoto. Candidato IPv6 não prova alcance; segundo PC ainda não executado. Sem UI, relay, mod/Java novo ou mudança de matriz/dependências.

Incremento33: usuário executou kit32 em Windowsx64 de outra internet e devolveu guest_ready/probe_passed/guest_stopped. Direto com endereço inicial IPv6, TCP1MiB/direção e10UDP/2origens passou nessa amostra; teto negociado1264B, cleanup host/echo exit0, portas livres. Evidência remota é relato do usuário; host/cleanup observados aqui. Não prova NAT/CGNAT, MTU exata1264, TCP+UDP sob carga externa, jogo/voz ou regiões. Supervisor privado passou a aceitar guest_identity JSON após a primeira preparação recusar a linha completa. Kit/binário32 preservados; consulte task33 antes de repetir.

Incremento34: inspector somenteleitura sinaliza configured_mtu_exceeds_transport_payload se teto de áudio configurado excede payload negociado válido; inverso nunca aprova UDP integral/áudio. SVC1275 versus1264 bloqueia; Plasmo1024 mantém fullUDP/runtime pendentes. Inteiros SVC fora de Number.isSafeInteger ficam não resolvidos.38NodePASS0skip com JARs fixados; auditoria das configs reais preservou hashes. Não abaixar MTU automaticamente, fragmentar ou processar áudio para contornar; consulte domínio voz/task34. Revisão independente sem bloqueador, sem novo Rust/binário/dependência/config.

Incremento35: `boundary-smoke.mjs --local-lab` passou teto explícito de serviço1264 em loopback com2agentesCLI; quatroechos1264/duasrecusas1265 sem chegada ao echo/continuidade,2origens eTCP1MiB/direção concorrente.7NodePASS0skip opt-in; cleanup normal/rebindports. Não é MTU física, externo ou áudio; retornooversize ainda pendente. Plano novo hash/approvedServices conferidos antes de autorizar peer, sem configuração pessoal. Helper tem prazo gracioso, reapOS pode excedê-lo; consulte task35 e domínioConnect. Não ampliar esse laboratório para destinatário/executável arbitrários.

Incremento36: mesmo helper passou também retorno1265/1264 com perfil authorizedDownstream explicitamente aprovado, sem depender de crédito1:1. Quatroretornosoversize emitidos(callbackOS,nãoprovaentrega),zero no cliente,quatroválidosexatos eTCPativo; mantém ambos socketsabertos300ms após últimoecho.7NodePASS0skip/cleanup normal.35 preserva seu resultado histórico; comando atual inclui36. Não promove áudio, MTU física ou rede externa e não altera transportebinário32.

Incremento37: `echo-boundary`/`probe-boundary` acrescentam fixture/probe nativos para as pontes autorizadas existentes, sem mudar o transporte. Smoke PowerShell5.1 passou cap de serviço1264: quatro ecos exatos, duas tentativas1265 sem chegada ao endpoint, quatro retornos1265 emitidos e zero no convidado, duas origens e TCP1MiB/direção concorrente; janela500ms. Plano downstream/hash/serviços aprovados antes do peer; cleanup normal e rebind das pontes/host/echo. Suíte integrada93RustPASS0skip inclui cinco unitários/quatroCLI nativos e38; fmt/clippyalltargetsPASS. Consulta ao task37 é obrigatória para hash/kit e validação final; kits históricos permanecem preservados. Não é prova externa, MTU física, áudio ou saturação.

Incremento38: teste `cfg(test)` em guest observa capacidade DATAGRAM, teto recebido na abertura, PMTU/probes do caminho e reabertura após TCP real. Na amostra focada, capacidade local Guest1414 ficou constante (`growth_not_observed`), resposta de abertura Host1358→1390; UDP1390 íntegro e tetoHost+1 recusado. Não exigir crescimento nem inferir medição direta do transporte host. Associação antiga conserva o teto de abertura; redução local é consultada dinamicamente. Cleanup libera todos os handles antes de rebind. Não houve mudança de produção; resultados completos e limites no task38.

Incremento39 (01/10/2026): usuário confirmou outro PC/outra internet, `boundary_probe_passed` e `guest_stopped` com kit38/binário5c1f3b3e…2030. Cap1264 passou nos dois sentidos, duas tentativas1265up sem chegar ao endpoint, quatro emissões1265down sem chegar ao guest; quatro ecos/duas origens e TCP1MiB/direção com553898B duranteUDP, cadência200ms/janela500ms. Host/echo/review encerraram exit0 semforçar e portas locais ficaram livres. Fontes da prova remota e local são distintas; direto continua PARCIAL nesse caso, sem medir caminhos/migração/NAT/CGNAT/MTU física/saturação/jogo/voz. Preservar kit/source1170d79 e manifest `testedTwoComputers=false` como snapshot de criação; resultado39 é registro separado, sem reescrever `completedTest=false` do supervisor.

Incremento40 (01/10/2026): terceira tentativa passou STATUS/pong Minecraft767 por `game` TCP sobre QUIC em outro computador/outra internet, confirmação explícita do usuário. `guest_ready`, resultado e `guest_stopped` são relato remoto; host/Java/cleanup foram observados aqui. Java saiu0 sem forçar, EULA exata restaurada e portas locais livres. Direto externo permanece PARCIAL; não prova login/mundo/GUI/contas/Core/Skins/voz. Preservar falhas1/2, kit/binário/manifest e snapshots; confirmação posterior da topologia tem registro separado. `run-plan-status-interactive`/`run-status-interactive` usam900s fixos com finalidade/perfil/hash próprios; modos anteriores mantêm300s. Relatório de falha aceita somente sete campos primitivos de `OwnedProcessError`, sem dados brutos.52NodePASS incluem cinco novos;95fixtures do helper por perfil e30 do guia não são novas provas externas. Consulte task40/runtime antes de repetir; estágios executados não podem ser reutilizados.

Incremento43: supervisor experimental `scripts/connect-minecraft-guest-lab.ps1` mantém um terminal/uma identidade e STATUS automático a cada20s/até45consultas, com600s preparação/900s sessão/1500s total operacionais. Exige o kit38 imutável/hash5c1f…2030, sem seleção automática do debug42; Java/EULA/host continuam separados. Salva resumo redigido CreateNew por tentativa, sem IP/peer/nick/saída bruta.139fixturesPowerShell5.1PASS; smoke no mesmo PC/IPv6 próprio passou nove STATUS reais e stop, com negativos de autorização/cancelamento/host ausente/jogo ausente. Snapshot7182 e reteste e48e após margem pré-ready15s estão separados no task. Sem nova prova externa/GUI/voz. O erro apagado da tentativaGUI41.2 não foi recuperado: não pedir novamente nem inventar causa. Comandos1/2 permanecem alternativa histórica, supervisor é fluxo preferencial. Matriz/gates/Assistente preservados.

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

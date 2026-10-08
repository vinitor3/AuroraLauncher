# Compatibility Matrix

## Aurora Connect V2 — evidências até 08/10/2026

Incremento48 acrescenta descoberta Windows de porta de filho próprio com revisão e STATUS sintético, seleção consumível e encerramento Connect quando a origem desaparece. **PROTÓTIPO** desligado por padrão; ensaios locais usam filho Rust, não Minecraft LAN ou interface Tauri. Mantém nove pares, versões de Java/loaders/mods e todas as homologações anteriores. [Contrato e limites](../aurora-connect-desktop.md#descoberta-de-porta-da-instância--incremento48).

Incremento47 valida localmente convite com identidade RAM conservada: TCP1MiB/direção, replay e revogação distinguíveis com host vivo/denied0→1→2, expiração e cleanup. Doisprocessos não equivalem a doiscomputadores; amostra IPv6 próprio continua nestePC. [Convites entre redes](../aurora-connect-internet-invitations.md) **PRECISA DE TESTE**. Sem mudança de Minecraft, mod, loader, Java, conta, áudio ou homologação da interface.

Incremento46 acrescenta caller/painel desktop **PROTÓTIPO**, feature `connect-lab` desligada por padrão, convites com revisão/confirmar, conexão/STATUS/encerramento reais, sem estado de mundo presumido. Default e combinação com `connect-voice-lab` têm verificações separadas; o [registro46](../task-aur-r4-connect.md#quadragésimo-sexto-incremento--caller-desktop-e-ipv4-próprio) contém comandos, correções e limites. Não homologa interface Tauri, jogo, voz ou convites entre redes. O ensaio em IPv4 próprio deste PC passou TCP1MiB/direção, UDP512B de duas origens, revogação e rebind2TCP/6UDP; escopo é mesmo computador, sem provaLAN/NAT/CGNAT. Nenhuma linha Minecraft/loader/Java ou par Core/Skins foi ampliado.

Incremento45 acrescenta modo separado de convite com endereço inicial IPv6 externo filtrado; os comandos padrão continuam loopback. Não equivale a ensaio entre duas internets, não altera a evidência gráfica44 e não amplia homologação de mods. [Contrato](AURORA_CONNECT_SPEC.md#811-modo-externo-ipv6-explícito--incremento45) e [comandos/resultados45](../task-aur-r4-connect.md#quadragésimo-quinto-incremento--convites-com-ipv6-inicial-explícito).

Incremento44: entrada gráfica externa manual de um convidado, com relato de terreno/movimento/saída e população local 0 → 1 → 0. Servidor Minecraft 1.21.1/Fabric 0.16.14/Java 21.0.12.1 sem mods; versões exatas de cliente/loader/Core/Skins não inventariadas. Encerramento normal e EULA exata. O supervisor43 falhou externamente antes da prontidão; causa indeterminada, sem extrapolar o sucesso manual para ele. [Evidências e limites44](../task-aur-r4-connect.md#quadragésimo-quarto-incremento--entrada-externa-no-mundo-pelo-fluxo-manual). Nenhum dos nove pares Core/Skins recebe nova homologação.

Incremento43 implementa supervisorPowerShell para um terminal/uma identidade, kit38 fixado e STATUS automático20s/até45consultas, com resumo redigido persistido.600s preparação/900s sessão/1500s total são limites operacionais, sem ampliarJava/lease.139fixturesPowerShell5.1PASS sem filhosConnect/Java/rede; smoke posterior no mesmo PC/IPv6 próprio passou nove STATUS reais/stop e negativos com cleanup/evidência, separando snapshots7182/e48e no [registro43](../task-aur-r4-connect.md#quadragésimo-terceiro-incremento--supervisor-do-convidado-e-resumo-persistido). Sem GUI ou nova evidência externa. O erro apagado da tentativaGUI41.2 continua sem causa confirmada; não repetir a solicitação. Nenhuma célula abaixo é promovida por implementação ou STATUS periódico.

Incremento42: lifecycle da CLI manual passou idle/revogação/stop/prazo e liberaçãoTCP+UDP em testes locais;98Rust/18NodePASS,sem novoJava/voz/redeexterna. GUI41 teve duas tentativas não aprovadas, preservadas com cleanup normal. Os estados de homologação abaixo permanecem.

Preparação41: perfil GUI separado com nomes revisados/whitelist offline e900s, para cliente Aurora1.21.1/Fabric em outro computador. Servidor continua sem mods; cliente pode carregar Core/Skins. [Roteiro e recuperação](../aurora-connect-internet-minecraft-gui.md). A preparação isolada não aprovava entrada no mundo; a amostra posterior44 está delimitada acima.

Estado do Connect: **PROTÓTIPO** independente em `apps/connect-lab`, desligado para usuários. O núcleo desktop tem adaptadores de configuração SVC/Plasmo apenas opt-in/cópia de diagnóstico, sem comando ou interface. Base cumulativa alpha.14 (`5ac2f7e` sobre `6a107b8`), Core 1.1.5, Skins 0.3.1 e os nove pares preservados. Evidências e comandos no [task de execução](../task-aur-r4-connect.md); roteiro no [laboratório](../aurora-connect.md). A [matriz candidata por mod/versão/loader/hospedagem/rede](CODEX_TASKS/AUR-R4-CONNECT-BACKLOG.md) mantém os gates futuros. A matriz abaixo conserva todas as evidências anteriores; retransmissão Skins e testes sintéticos de transporte não homologam Connect em Minecraft.

| Capacidade | Evidência observada | Estado e fronteira |
| --- | --- | --- |
| TCP | Bytes reais nos dois sentidos, 8 MiB/contrapressão, half-close, múltiplas conexões, autenticação/autorização, limites, expiração/revogação; protocolo Minecraft real;39 passou1MiB/direção externo com UDPconcorrente e cadência200ms;40 passou STATUS/pong767 deMinecraft real em outro PC/internet | **PROTÓTIPO**; dois clientes sintéticos até pacotes de mundo em loopback e consultaSTATUS externa; saturação/estabilidade externa pendentes |
| UDP | Fronteiras/retorno, três peers, duas sessões, dois serviços, MTU/oversize, replay/forja, quotas, ciclo de vida e carga TCP simultânea; downstream autorizado por plano/hash local e orçamento por peer/serviço;39 cap1264/1265 nos dois sentidos com duas origens e TCPcadenciado externo | **PROTÓTIPO**; `host` mantém 1:1 e `host-plan` permite perfil explícito; nenhum mod/áudio homologado |
| Direto | IPv4/IPv6 loopback;32 conferiu configuração efetiva/QNT;33 executou bytes e39 fronteiraUDP1264/1265 com TCPcadenciado em dois PCs/internets diferentes, endereço inicialIPv6 manual;40 passou STATUS/pongMinecraft em outro PC/internet confirmados. Relato remoto do usuário; host/cleanup próprios observados | **PARCIAL** nesses ensaios; caminhos/migração não instrumentados. LAN, IPv4 público, firewall em outros ambientes, NAT/CGNAT, saturação/estabilidade externa e outras redes pendentes |
| Relay/UDP externo bloqueado | Pesquisa de capacidade upstream e termos, sem ativação | **NÃO IMPLEMENTADO** no laboratório; nenhum relay configurado/homologado |
| Minecraft | Servidor real 1.21.1/Fabric0.16.14/Java21; dois clientes sintéticos LOGIN/CONFIGURATION/PLAY, chunks/presença recíproca, contagem2→0. Três ensaios20 e repetição29 com dois convites assinados/quatro agentes;40 passou STATUS externo.44 acrescentou um cliente gráfico externo manual: STATUS, entrada/movimento relatados, população local0→1→0 e cleanup/EULA exata | **PARCIAL** no caminho manual dessa amostra, ainda em laboratório. Cliente/loader/Core/Skins não inventariados; integração desktop, dois jogadores gráficos, clientes mistos, LAN, voz e estabilidade **PRECISA DE TESTE** |
| Convites/interface | Ed25519, uma admissão, pin/serviços/prazo/revogação; cinco testes28 de recusa em0-RTT aceito. Caller29 compõe convite com Minecraft/SVC/Plasmo reais;45 acrescenta APIs/CLI explícitas para endereço inicial IPv6 externo filtrado | Convite **PROTÓTIPO**, padrão loopback e modo externo opt-in; estadoRAM, sem renovação ou medição temporal pré-TLS. Novo convite entre duas internets **PRECISA DE TESTE**; conta/sinalização social/interface **NÃO IMPLEMENTADO** |
| Simple Voice Chat | Ciclo nativo/writer/restore24; controle25 e29 no mod2.6.22 real via dois agentes, anúncio ao cliente sintético + AuthAck/CheckAck AES-GCM UDP;29 usa convite assinado;30 observou35keepalives/35respostas em35s com cliente PLAY e restore normal | **PARCIAL**, opt-in sem UI; áudio, GUI, estabilidade/carga, LAN e redes externas **PRECISA DE TESTE** |
| Plasmo Voice | Ciclo nativo/writer/restore24; controle26 e29 real2.1.17/Fabric API0.116.10 via dois agentes, anúncio ao cliente sintético e Ping UDP com credencial da sessão;29 usa convite assinado;31 observou11Pings/11respostas em35s com cliente PLAY e restore normal | **PARCIAL**, opt-in sem UI; Ping sem MAC/AEAD próprio; áudio, GUI, estabilidade/carga, LAN/redes **PRECISA DE TESTE** |

Incremento34 confrontou teto1264B do ensaio externo33 com configurações reais intactas: SVC1275 gera incompatibilidade conservadora no inspector; Plasmo1024 continua sem orçamento UDP integral medido.38testesPASS com JARs fixados, sem áudio/configuração aplicada. O sucesso de transporte33 não preenche células de voz; [detalhes](../aurora-connect-voice.md#teto-udp-observado-na-outra-internet--incremento34).

Incremento35 passou fronteira local com cap explícito1264:1264B eco exato,1265B sem entrega ao endpoint em300ms, válido posterior/duasorigens eTCP1MiB/direção concorrente;7testesPASS0skip,2agentes/cleanup normal. Cap de serviço não é MTU física ou prova externa; retornooversize permanece pendente nesse helper. Nenhuma célula de jogo/mod é promovida.

Incremento36 completou o retorno nesse helper: perfil authorizedDownstream explícito, quatroemissões1265B/zero entrega ao guest observada e quatroecos1264B válidos;TCPativo, duasorigens,7testesPASS0skip/cleanup normal. A lacuna35 de retorno foi coberta em loopback, sem promover externamente ou medir áudio/MTU física.

Incremento37 acrescenta fixture/probe nativos para as pontes existentes, sem alterar transporte:93RustPASS0skip/fmt/clippyalltargets/build incluem5unitários+4CLI novos e38; smokePowerShell5.1PASS. Cap local1264/1265 nos dois sentidos, quatro ecos válidos, duas origens e TCP1MiB/direção com229376B duranteUDP; janela500ms, orçamento downstream aprovado por plano/hash, rebind guest/host/echo. Prepara outro computador sem Node; execução externa dessa fronteira ainda não comprovada. Tempo6515ms é amostra cadenciada, não benchmark; deadline nativo real recusou semstderr e comcleanup.

Incremento38 testa orçamento e reabertura emloopback, somentecfg(test):1caso focadoPASS, capacidadeGuest1414 constante (`growth_not_observed`), teto recebido doHost1358→1390 emnova associação, UDP1390 íntegro/tetoHost+1 recusado ecleanuprebind. PMTU/probes são estatísticas locais; não observaram diretamente a capacidadeHost nem a MTU física da rota33. Valores variam entre execuções; crescimento não é gate de sucesso. Nenhuma célula de mod, áudio ou rede recebe homologação.

Incremento39 comprovou a fronteira preparada no37 em um caso externo, com confirmação do usuário de outro PC/outra internet e guest_stopped: quatro ecos1264/5056B, duas tentativas1265up sem endpoint, quatro emissões1265down sem guest, duas origens/associações observadas e dois válidos posteriores. TCP1048576B/direção,553898B duranteUDP, cadência200ms/janela500ms,elapsed7284ms; review/host/echo locais exit0/reap semforçar, portas livres. Não mede MTU física, saturação ou desempenho, nem homologa Minecraft/SVC/Plasmo. Kit38/source1170d79 e manifesto original continuam intactos; prova39 é registro separado.

Incremento40 passou somente STATUS/pong767 externo na terceira tentativa: usuário reportou `guest_ready`, resultado/zero jogadores/máximo dois, `guest_stopped` e confirmou outro computador/outra internet. Java e dois filhosConnect locais tiveram exit0 sem forçar, EULA exata e portas livres. Total884644µs/ping165646µs são medidas da aplicação, sem benchmark de rede. Tentativas1/2 permanecem falhas: listenerTCP local semguest_ready e unauthorized com peerconvidado divergente, sem prova da checagem individual/NAT; deadlineJava antigo sem resumo não é retrospectivamente corrigido. Novo modoSTATUS interativo900s por finalidade/perfil/hash próprios mantém padrão300s e acrescenta diagnóstico restrito;52NodePASS incluem cinco novos,95fixtures do helper por perfil/30guia e QA são verificações locais. Prova remota e local são separadas; confirmação posterior preserva snapshots/kit. Sem login/mundo/GUI/contas/Core/Skins/voz ou homologaçãoR4. [Registro40](../task-aur-r4-connect.md#quadragésimo-incremento--statuspong-minecraft-aprovado-e-encerramento).

Baseline: 24/24 testes Rust (4 unitários, 9 adversariais, 11 de transporte), 55/55 resultados nas cinco repetições da suíte de transporte, fmt/clippy e 14/14 testes do inspector aprovados. Smoke: 1 MiB TCP por direção, dez UDP em duas origens, teto negociado de 1358 bytes e limpeza aprovada. Uma falha intermitente anterior de UDP sob carga TCP foi corrigida reservando 16 KiB do burst para datagramas; o histórico permanece no task. Esses resultados não garantem ausência de perda em UDP, justiça em redes externas ou SLA. Nenhuma linha de mod/loader/hospedagem recebe homologação por esses testes.

Incremento posterior de retorno UDP: 34/34 Rust (os 24 anteriores + 6 casos downstream + 4 de QA independente) e fmt aprovados. Testa receber múltiplas respostas maiores após ingresso único, quotas entre associações/reconexão, isolamento de serviços/peers/origem, política exclusivamente host, inatividade, revogação e expiração absoluta. O silêncio real do mod ainda depende de keepalive/reabertura e endereçamento, conforme [spec §5.1](AURORA_CONNECT_SPEC.md#51-retorno-udp-autorizado--incremento-de-laboratório).

Terceiro incremento: 45/45 Rust, fmt/clippy e dois smokes aprovados, com onze casos de plano local estrito/aprovação por hash/leitura limitada. Smoke padrão: dez UDP, 3138 bytes por direção. Smoke downstream: 2 bytes enviados, 8192 recebidos, 16 respostas em duas origens. Ambos transferiram 1 MiB TCP por direção e encerraram os processos próprios. Negociação observou 1358 bytes anteriormente e 1390 na última amostra: valor dinâmico, sem aprovação de MTU de mod. Nenhum JAR/Minecraft/áudio integra essas provas.

O quarto incremento Connect acrescentou inspeção dos JARs oficiais Fabric 1.21.1 de Simple Voice Chat 2.6.22 e Plasmo Voice 2.1.17, com 31/31 testes Node, incluindo os dois artefatos reais. Hash, metadata e assinaturas relevantes foram conferidos sem carregar JVM. O plano deriva versão do artefato; não prova instalação, anúncio efetivo, áudio ou transporte do mod. [Hashes, métodos e comandos](../aurora-connect-voice.md). Isso não amplia os nove pares nem promove qualquer célula de homologação.

Quinto incremento: quatro testes focados SVC com fixture/JAR real, suíte nativa com feature e padrão aprovadas; padrão 258 PASS/cinco opt-in ignorados. Fmt/clippy nas duas configurações passaram. Reutiliza CAS/lock/identidade/journal/undo existentes, com alvo só persistido após aprovação; não amplia allowlist para aplicar à origem nem comprova jogo, áudio ou crash físico. [Escopo e comandos](../aurora-connect-voice.md#adaptador-nativo-svc-em-cópia-de-diagnóstico--quinto-incremento).

Sexto incremento: adaptador Plasmo separado, mesmo gate opt-in; 12/12 focados com ambos os JARs reais, suíte feature 268 PASS/sete opt-in ignorados, padrão 258 PASS/cinco ignorados, fmt/clippy nas duas configurações. Caminho do artefato Fabric confirmado como `config/plasmovoice/server/config.toml`; substituição/adição do bloco, origem intacta, cancelamento, undo e conflito testados em fixtures. Sem nova célula runtime homologada. Auditoria desktop encontrou rustls antigo vulnerável, tratado como gate próprio; não herda o resultado do lock Connect.

Oitavo incremento: recibo em memória restaura somente a própria configuração ainda inalterada, preparando a seleção no inventário atual para preservar outras edições. Recusa outra identidade/runtime, aprovação incorreta e alterações posteriores; falha do executor bloqueia retry. Provas próprias SVC/Plasmo e correção de limite da saída SVC, sem JVM/áudio. Restauração seletiva após reinício continua pendente. [Contrato e limites](../aurora-connect-voice.md#restauração-seletiva-do-anúncio--oitavo-incremento).

Nono incremento: fixtures completas equivalentes aos defaults (sem JVM) exercitam os parsers e filtro recovery, incluindo preservação do bloqueio de configuração sensível. Plasmo salva o documento ao carregar; uma reserialização semanticamente igual altera o hash e a restauração estrita recusa. Roundtrip real permanece gate antes de prometer limpeza após jogar. Proveniência de fonte da tag divergente corrigida, sem mudar o pin do JAR. [Detalhes](../aurora-connect-voice.md#defaults-completos-e-reserialização--nono-incremento).

Décimo incremento: `minecraft-status` separa resposta de protocolo/pong de login, mundo e voz. O teste por TCP usa a ponte autenticada até uma fixture independente; essa consulta não prova propriedade de um listener local arbitrário nem runtime real. Protocolo 767 é entrada explícita da fixture/roteiro 1.21.1, sem nova versão na matriz ou detecção automática de compatibilidade. [Contrato](AURORA_CONNECT_SPEC.md#91-produtor-experimental-de-status-tcp).

Décimo primeiro incremento: revisão separada de restauração em cópia conserva outras edições no próprio arquivo, recusa valores de anúncio alterados e exige nova aprovação. SVC e Plasmo têm tratamento próprio; nenhuma JVM, UI ou limpeza automática foi validada. [Limites](../aurora-connect-voice.md#restauração-revisada-de-arquivo-regravado--décimo-primeiro-incremento).

Décimo segundo incremento: guard de voz exige verificação Windows de processos Java padrão junto ao lock e novamente na confirmação. Qualquer JVM padrão bloqueia; não distingue servidor dedicado de outra aplicação, não detecta renomeados/embutidos e não impede início externo após a consulta. Não modifica o launcher padrão ou cria integração UI. [Escopo](../aurora-connect-voice.md#verificação-de-processos-no-adaptador--décimo-segundo-incremento).

Décimo terceiro incremento: 2/2 testes, quatro cenários de lock real entre aplicar/launch/restaurar para SVC e Plasmo, com probes controlados e JARs fixados. Sem JVM, voz ou mudança nos nove pares; [evidência](../task-aur-r4-connect.md#décimo-terceiro-incremento--entrega-e-retomada-do-lock-com-recibo-vivo).

Décimo quarto incremento: helper prepara plano/staging verificável para o par existente 1.21.1/Fabric 0.16.14/Java 21. 17 testes próprios aprovados; download/execução Minecraft dependem de termos e continuam pendentes. Não amplia matriz ou instala módulos; [escopo](../aurora-connect-runtime.md).

Décimo quinto incremento: guest-tcp e plano sem aux; STATUS/pong de fixture via dois processos reais, protocolo divergente recusado e agentes encerrados. 59 Rust e dois smokes Node aprovados. Sem Java/login/mundo/voz; [roteiro](../aurora-connect.md#plano-explícito-de-serviços-do-laboratório).

Décimo sexto incremento: supervisor opt-in de filho local, 18 testes com Node e encerramento observado. Ainda não inicia Minecraft ou homologa mod; [limites](../aurora-connect-runtime.md#supervisor-de-processo-próprio--décimo-sexto-incremento).

Décimo sétimo incremento: programa Java 21 próprio, probe de produção RUNNING/STOPPED e stop normal aprovados. Não é Minecraft nem mod; matriz permanece intacta. Autorização de termos recebida para próximo runtime descartável, com preferência explícita por online-mode=false; não desliga autenticação Connect. [Evidência](../task-aur-r4-connect.md#décimo-sétimo-incremento--probe-de-produção-com-java-próprio-real).

Décimo oitavo incremento: nove artefatos oficiais baixados/verificados, Minecraft real respondeu protocolo 767 por Host/Guest autenticados e somente game/tcp. EULA restaurada, exit 0 sem kill, porta/Java/agentes ausentes após stop. Configuração própria online-mode=false solicitada, whitelist ativa, nenhum login/cliente original/offline/voz comprovado. 50 testes Node sequenciais passaram; correção posterior de isolamento AppData validada por 14 focados e ensaio real. [Comandos, falhas e limites](../task-aur-r4-connect.md#décimo-oitavo-incremento--minecraft-real-por-dois-agentes-autenticados).

## Doctor, recuperação e cópias de teste — 29/09/2026

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. Launcher alpha.14 integra integralmente a recuperação manual de `be3a1c4`, preserva a base alpha.13 com Doctor, Gemini, Assistente e sessões, e acrescenta observações verificadas por tentativa e cópias independentes com aplicação seletiva, proteção e desfazer. Não amplia Minecraft/loaders/Java e conserva Core 1.1.5/Skins 0.3.1. Mundos são excluídos por padrão e nunca transferidos ao aplicar; cópia explícita de mundo serve somente ao teste. Testes automatizados usam dados descartáveis; partida real, skins/capa, multiplayer, autenticação e voz não recebem nova homologação por build. Evidências: [AUR-1167](../task-aur-1167-doctor-lab.md).

## Recuperação manual de instâncias — 29/09/2026

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. Base `dc2ec5a`, launcher alpha.13, Core 1.1.5 e Skins 0.3.1 preservados. Pontos manuais protegem o escopo local declarado com prévia, confirmação e desfazer, sem mundos, contas ou instalação compartilhada. Core/Skins administrados ficam fora da reposição de artefatos antigos; Companion não é reinstalado. Não amplia Minecraft/loaders/Java nem altera os 18 JARs. Os testes de filesystem e interface não homologam runtime Minecraft ou a UI nativa autenticada. Contrato e evidência: [AUR-1166](../task-aur-1166-instance-recovery.md).

## Failover Gemini no Worker — 29/09/2026

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. Base `a3325b5`, cliente alpha.13 existente, Core 1.1.5 e Skins 0.3.1 preservados. O Worker limita tentativas entre chaves e oferece Flash-Lite explicitamente autorizado como último recurso, com prazo e orçamento compartilhados. Tradução usa o mesmo transporte; contratos desktop e confirmação local permanecem iguais. Não exige novo instalador nem altera os 18 JARs ou a matriz Minecraft/loader/Java. Publicado na revisão `2026-09-29-gemini-key-failover-v1`, com saúde e proteção de sessão verificadas. Preview respondeu com Gemini real; chamada autenticada em produção permanece **NÃO VERIFICADO**, pois Firebase recusou sessão anônima de teste. Evidências em [task-gemini-failover.md](../task-gemini-failover.md).

## Gemini e ações locais confirmadas — 28/09/2026

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. O incremento parte de `ab1e1b5`, launcher alpha.12, Core 1.1.5 e Skins 0.3.1; o instalador alpha.13 foi gerado e o Worker foi publicado. A matriz ativa e as versões dos módulos permanecem iguais, com os 18 JARs reconstruídos e verificados. Assistente e Doctor compartilham transporte Gemini e revisão de remoção reversível de mod/ajuste de opções JSON permitidas, com histórico e restauração locais. Assistente e respostas estruturadas Doctor/Tools foram comprovados em preview com Gemini real e entradas sintéticas; também houve 503 intermitente. Testes de broker, DOM e Worker não homologam os pares Minecraft/loader abaixo. Evidências em [task-ai-tools.md](../task-ai-tools.md).

## Instalador integrado alpha.12 — 28/09/2026

Empacotamento: **CONCLUÍDO**. O build oficial de 23/09 foi conferido na retomada de 28/09, com 18/18 JARs Core 1.1.5/Skins 0.3.1, sessões independentes e Doctor com orientação Gemini opcional. Não altera a matriz de Minecraft/loaders/Java. Consulta Gemini real e homologação completa de Minecraft: **NÃO VERIFICADO**. Artefato e hashes em [task-release-alpha12.md](../task-release-alpha12.md).

## Aurora Doctor — orientação Gemini opcional — 23/09/2026

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. O incremento parte de `6f3a13b`, launcher alpha.11, Core 1.1.5 e Skins 0.3.1. Amplia regras locais e acrescenta explicação Gemini por ação explícita, com contrato reduzido, timeout e passos manuais permitidos. Não altera os JARs, a matriz de Minecraft/loaders/Java ou as sessões independentes. A análise determinística continua offline; testes com respostas sintéticas não homologam Gemini real nem uma nova implantação do Worker. Evidências em [task-doctor-gemini.md](../task-doctor-gemini.md).

## Skins 0.3.1 e sessões independentes — 23/09/2026

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. A integração parte do launcher alpha.10 atual (`4c58a3b`) e reúne a grade/cache da 0.2.5 com o multiplayer/compatibilidade da 0.3.0 nos nove pares existentes. Core permanece 1.1.5; o servidor recebe Skins da mesma combinação Minecraft/loader e não exige Core. O contrato cumulativo de features passa a bloquear artefatos sem grade, cache ou sincronização. Não há mudança da matriz Java, inclusão de Chat Heads ou retorno de Companion.

O launcher preserva o login lembrado na primeira janela e isola as contas das adicionais. Dois jogos usam pastas de instância separadas. Relato do usuário: multiplayer e Chat Heads funcionaram na 0.3.0 em Minecraft 1.20.1 Forge e Fabric; o relato não homologa os novos hashes nem os nove pares. Execução gráfica completa da 0.3.1 e duas contas reais simultâneas: **PRECISA DE TESTE**. Evidências em [integração](../task-skins-session-integration.md).

## Aurora Skins 0.3.0 — Multiplayer e aparência — 22/09/2026

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. As nove combinações (Forge 1.12.2 e Fabric/Forge 1.16.5, 1.19.2, 1.20.1, 1.21.1) geraram JARs finais e passaram pelos contratos Rust de empacotamento; manifesto em `releases/skins/0.3.0/manifest.json`. Foram aprovados 27 testes do núcleo, incluindo sete do servidor e quatro do cliente de sincronização. Fabric 1.21.1 inicializou o retransmissor dedicado sem Core; Forge 1.21.1 aprovou descoberta de metadados sem Core, mas não executou entrypoints antes da EULA. Ambos encerraram pré-EULA sem mundo/jogadores. Dois clientes reais e Chat Heads continuam com estado **PRECISA DE TESTE**; não há homologação visual ou instalador novo nesta tarefa.

O servidor usa Aurora Skins 0.3.0 da mesma combinação Minecraft/loader, sem exigir Aurora Core/Companion; clientes participantes exigem Core 1.1.3+. Clientes sem Aurora Skins podem entrar, conforme as outras regras do servidor, e continuam vendo a aparência vanilla. O retransmissor limita a 256 participantes e uma publicação por jogador a cada dois segundos, com PNG canônico de até 24 KiB e estado apenas em memória, sem nuvem ou credenciais.

`PlayerInfo` amplia a compatibilidade moderna com chat/tab; consultas adicionais de `SkinManager` atendem 1.19.2/1.20.1/1.21.1, e 1.12.2 mantém o adaptador legado. Não há recurso novo de cabeças no chat nem Chat Heads incluído. Mods que consultam diretamente a Mojang, mantêm caches próprios ou usam cabeças geradas exclusivamente no servidor podem continuar exibindo a skin oficial. Proxy/ViaVersion e travessia entre versões não são suportados; combinação Fabric/Forge não está homologada.

Branch `codex/aur-skins-multiplayer-compat`, base `269d282`: preservar as alterações posteriores de Skins 0.2.5 na integração. Evidência e roteiro: [tarefa multiplayer](../task-skins-multiplayer.md); decisão: [ADR-022](ADR/ADR-022-skins-server-relay.md). O histórico abaixo permanece válido somente para as versões indicadas.

## Aurora Doctor local — 22/09/2026

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. O launcher passa a registrar e analisar tentativas de inicialização localmente, com regras determinísticas, associação limitada de logs e relatório sanitizado. Esse incremento não altera os JARs Core/Skins nem amplia Minecraft/loaders/Java suportados. Fixtures sintéticas e testes do diagnóstico não promovem os estados de homologação runtime abaixo. Comportamento e evidências específicas em [aurora-doctor.md](../aurora-doctor.md) e [task-aurora-doctor.md](../task-aurora-doctor.md).

## Launcher alpha.9 — integração de 22/09/2026

Integra Core 1.1.5 e Skins 0.2.5 das entregas anteriores, sem alterar os JARs ou ampliar a matriz Minecraft. A retirada do Companion mantém o Assistente desktop. A busca de jogadores ganha fallback oficial Mojang no Worker, preservando o contrato TSV V1 dos módulos existentes. A homologação da grade continua **PARCIAL**, conforme a tabela abaixo.

O desktop adiciona tradução de descrições, armazenamento local da seleção online, face baseada no PNG aplicado, retorno às instâncias após instalação e controles de janela/bandeja. **CONCLUÍDO** nos testes nativos: maximizar, restaurar, minimizar e fechar ocultando a janela com IPC/processo ativos. Menu físico da bandeja e comportamento no computador do destinatário: **PRECISA DE TESTE**. Evidências e limitações em [task-release-alpha9.md](../task-release-alpha9.md).

## Grade Explorar — Skins 0.2.5 / Core 1.1.5

A grade de 12 slots está integrada nos nove pares abaixo, com lógica comum Java 8, seleção por identidade e paginação pelo provider. Estado: **PARCIAL**: builds e contratos aprovados nos nove pares, verificação gráfica focada em três e runtime pendente nos outros seis. Resultados desta entrega, hashes e limites no [registro da grade](../task-skins-grid.md). A fonte do launcher seleciona os novos candidatos; o histórico abaixo mantém as versões realmente testadas.

| Minecraft | Loader | Java runtime | Skins | Evidência da grade |
| --- | --- | ---: | --- | --- |
| 1.12.2 | Forge 14.23.5.2860 | 8 | 0.2.5 | PARCIAL: execução gráfica focada |
| 1.16.5 | Fabric | 8 | 0.2.5 | Build/contratos; runtime PRECISA DE TESTE |
| 1.16.5 | Forge 36.2.42 | 8 | 0.2.5 | Build/contratos; runtime PRECISA DE TESTE |
| 1.19.2 | Fabric | 17 | 0.2.5 | Build/contratos; runtime PRECISA DE TESTE |
| 1.19.2 | Forge 43.4.16 | 17 | 0.2.5 | Build/contratos; runtime PRECISA DE TESTE |
| 1.20.1 | Fabric 0.14.23 | 17 | 0.2.5 | PARCIAL: execução gráfica focada e 320×240 |
| 1.20.1 | Forge 47.4.23 | 17 | 0.2.5 | Build/contratos; runtime PRECISA DE TESTE |
| 1.21.1 | Fabric 0.19.3 | 21 | 0.2.5 | PARCIAL: execução gráfica focada e 427×240/fullscreen |
| 1.21.1 | Forge 52.1.12 | 21 | 0.2.5 | Build/contratos; runtime PRECISA DE TESTE |

## Histórico: matriz após retirada do mod, anterior à grade — 21/09/2026

**O Assistente permanece no Aurora Launcher; o mod Companion e sua integração in-game foram retirados.** Os módulos ativos são Core e Skins: 2 módulos × 9 pares = **18 JARs**. A seleção/contagem vem de `scripts/active-mod-matrix.json`; o painel desktop e suas dependências não são JARs da matriz.

Core **1.1.5** é o candidato versionado desta alteração de UI. Skins **0.2.4** mantém sua fonte e foi reutilizado após rebuild e aprovação no comparador estrito. Os **18/18 JARs** passaram na comparação de fontes/conteúdo; os nove Core passaram em tamanho/hash/assinatura Ed25519 e as suítes Java Core (9) e Skins (16) passaram. Consulte o [registro da retirada](../task-remove-companion.md) para os resultados realmente executados; a existência de candidato ou build não prova runtime.

| Minecraft | Loader | Java runtime | Core | Skins | Runtime completo após retirada |
| --- | --- | ---: | --- | --- | --- |
| 1.12.2 | Forge | 8 | 1.1.5 candidato | 0.2.4 | PRECISA DE TESTE |
| 1.16.5 | Fabric | 8 | 1.1.5 candidato | 0.2.4 | PRECISA DE TESTE |
| 1.16.5 | Forge | 8 | 1.1.5 candidato | 0.2.4 | PRECISA DE TESTE |
| 1.19.2 | Fabric | 17 | 1.1.5 candidato | 0.2.4 | PRECISA DE TESTE |
| 1.19.2 | Forge | 17 | 1.1.5 candidato | 0.2.4 | PRECISA DE TESTE |
| 1.20.1 | Fabric | 17 | 1.1.5 candidato | 0.2.4 | PRECISA DE TESTE |
| 1.20.1 | Forge | 17 | 1.1.5 candidato | 0.2.4 | PRECISA DE TESTE |
| 1.21.1 | Fabric | 21 | 1.1.5 candidato | 0.2.4 | PRECISA DE TESTE |
| 1.21.1 | Forge | 21 | 1.1.5 candidato | 0.2.4 | PRECISA DE TESTE |

Os scripts preservam Gradle 8.4 em `tools/gradle/8.4` com JVM de build 17, e o wrapper 9.7.1/JVM 21 de Core 1.21.1 utilizado também por Skins. Rodar builds sequencialmente para evitar colisões Loom. Versões mínimas/exatas de loader e assinaturas seguem os manifestos de compatibilidade ativos.

### Gates vigentes

1. Recompilar Core e Skins nos nove pares fora da pasta Companion; validar metadata, bytecode, dependências, recursos, hashes e assinatura Core.
2. Comparar todas as entradas normalizadas dos 18 JARs contra as fontes; divergência bloqueia release. A normalização limitada existente não autoriza ignorar classes/recursos diferentes.
3. Em QA isolada, migrar Companion conhecido de `mods` para backup/journal fora da pasta carregada; repetir, simular falha, recusar link e preservar identidade incerta/terceiros/dados desktop.
4. Iniciar Core → Skins sem Companion, com uma única conexão IPC, handshake/nonce/protocolo 1, sessão pública e desconexão.
5. Verificar menu, preview 3D, catálogo, aplicação/persistência de skin e capa onde implementada, foco e encerramento sem impedir Minecraft.
6. Com Minecraft fechado, validar separadamente botão/atalho, conversa Gemini, microfone/transcrição, mute/TTS e erros/timeouts do Assistente desktop. Sem ponte de pedido antigo do mod.
7. Preparar a próxima release sem modificar artefatos históricos; gerar instalador somente mediante tarefa explícita e pelo fluxo oficial. Alteração do Worker não significa deploy.

## Evidências históricas anteriores à retirada

> As seções abaixo conservam resultados das versões/release indicadas. Companion, 27 JARs e procedimentos de instalação abaixo são históricos e foram substituídos pelos gates acima; não são requisitos atuais. Evidências gráficas antigas Core/Skins continuam úteis somente para os hashes/cenários realmente testados.

## Reparos de runtime — 19/09/2026

Fonte: Core **1.1.4**, Skins **0.2.4**, Companion **0.2.3**. Os 27 JARs
recompilados correspondem aos artefatos versionados; nove pares Core/Skins
passam pelos contratos de empacotamento, bytecode e dependências. Os novos
artefatos ainda dependem da integração no instalador; alpha.7 é imutável.

| Minecraft / loader testado | Java | Evidência desta entrega | Estado |
| --- | ---: | --- | --- |
| 1.12.2 / Forge 14.23.5.2860 | 8u504 | Menu, modelo completo, catálogo, aplicação e skin em mundo novo; Core → Skins → F11 → Escape; mundo repetido com Core 1.1.4 + Skins 0.2.4 finais | PARCIAL |
| 1.16.5 / Fabric 0.18.4 | 8u504 | Catálogo online, textura visível, aplicação e retorno ao menu sem ficar branco | PARCIAL |
| 1.19.2 / Fabric 0.14.19 | 17 | Inicialização sem erro de Mixin, menu, campo de busca e catálogo | PARCIAL |
| 1.21.1 / Fabric 0.19.3 | 21 | Menu, Core e Skins sem blur sobre conteúdo; catálogo e aplicação | PARCIAL |
| 1.16.5, 1.19.2, 1.20.1, 1.21.1 / Forge; 1.20.1 / Fabric | 8/17/21 | Build e inspeção dos JARs; sem nova execução gráfica | IMPLEMENTADO MAS NÃO HOMOLOGADO |

As quatro rodadas da tabela usaram Core 1.1.3; Core 1.1.4 mantém a UI e corrige
somente a versão dos metadados legados, além do próprio versionamento. O
candidato 1.1.3 foi preservado e não é selecionado pelo launcher. A rodada
legada final com Core 1.1.4 repetiu catálogo, aplicação e skin dentro do mundo.

Instâncias isoladas, sem contas, mundos ou servidores existentes; Companion
e handshake IPC não fizeram parte destes lançamentos. Os hashes das rodadas
anteriores e do mundo legado final estão em [task-skins-runtime-repair](../task-skins-runtime-repair.md).
Não extrapolar os testes para multiplayer, capas, todos os modpacks ou todas
as GUI scales. O histórico abaixo conserva os resultados das versões antigas.

## Histórico anterior

Atualizado em 2026-09-02. `BUILD` significa JAR compilado e inspecionado; `RUNTIME` exige o roteiro completo em uma instância real. Core 1.1.2, Skins 0.2.1 e Companion 0.2.3 formam a matriz embarcada no Launcher 0.1.1-alpha.6.

| Minecraft | Loader | Java | Core build | Skins build | Companion build | Runtime do conjunto | Estado |
| --- | --- | ---: | :---: | :---: | :---: | :---: | --- |
| 1.12.2 | Forge 14.23.5.2860 | 8 | sim | sim | sim | PRECISA DE TESTE | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.16.5 | Fabric 0.16.14 | 8 | sim | sim | sim | PRECISA DE TESTE | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.16.5 | Forge 36.2.42 | 8 | sim | sim | sim | PRECISA DE TESTE | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.19.2 | Fabric 0.16.14 | 17 | sim | sim | sim | PRECISA DE TESTE | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.19.2 | Forge 43.4.16 | 17 | sim | sim | sim | PRECISA DE TESTE | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.20.1 | Fabric 0.16.14 | 17 | sim | sim | sim | PRECISA DE TESTE | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.20.1 | Forge 47.4.23 | 17 | sim | sim | sim | Cave Horror: launch/menu/IPC/skin/catálogo PASS; mundo PRECISA DE TESTE | PARCIAL |
| 1.21.1 | Fabric 0.16.14 | 21 | sim | sim | sim | PRECISA DE TESTE | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.21.1 | Forge 52.1.12 | 21 | sim | sim | sim | PRECISA DE TESTE | IMPLEMENTADO MAS NÃO HOMOLOGADO |

Evidências runtime anteriores do Companion 1.12.2 Forge e 1.20.1 Fabric não homologam automaticamente os novos Core/Skins. O conjunto precisa ser reexecutado porque a aparência deixou de pertencer ao Companion e passou para o módulo Aurora Skins.

Em 1º de setembro de 2026, o Cave Horror Project 1.20.1 Forge 47.4.21 iniciou
com Java 17 usando exatamente os JARs embarcados Core 1.1.2, Skins 0.2.1 e
Companion 0.2.3. Não houve conflito JPMS de SLF4J/Minecraft nem rejeição de
resource pack; os três módulos registraram, o IPC conectou, a skin apareceu em
pé, o botão permaneceu sob o avatar no FancyMenu e abriu o catálogo online. Um
mundo/servidor não foi aberto nessa execução, portanto a linha continua parcial.

## Instalação pelo Launcher

Atualização de 16/09/2026: **Skins 0.2.2** tem nove JARs compilados (1.12.2 Forge e Fabric/Forge 1.16.5, 1.19.2, 1.20.1, 1.21.1), busca mc-api/MCStat, debounce e prévia do resultado. Os hashes estão em `releases/skins/0.2.2/manifest.json`; o código do launcher passou a embarcar essa matriz. **IMPLEMENTADO MAS NÃO HOMOLOGADO**: nenhum runtime/gameplay de 0.2.2 está comprovado. A tabela e a evidência Cave Horror acima são históricas do conjunto 0.2.1/alpha.5, não homologam 0.2.2. Não foi substituído o instalador alpha.5.

Para cada instância Fabric/Forge suportada, o Launcher instala por troca atômica:

1. Aurora Core 1.1.2, após validar tamanho, SHA-256 e assinatura Ed25519;
2. Aurora Skins 0.2.1;
3. Aurora Companion 0.2.3.

O release preflight recompila as 27 combinações e compara o conteúdo normalizado dos JARs de staging com os artefatos embarcados. A normalização ignora apenas o namespace aleatório de injeção gerado pelo Architectury e metadados ZIP; qualquer outra divergência bloqueia a criação do instalador. O mesmo gate exige `pack.mcmeta` em todos os módulos e abre cada Core para rejeitar `org/slf4j/**` sem relocação ou qualquer `net/minecraft/**`, evitando os conflitos e avisos observados no Cave Horror.

## Compatibilidade do launcher

Release alpha.7 (17/09/2026): instalador recompilado com Core 1.1.2,
Skins 0.2.3 e Companion 0.2.3; matriz de fontes 27/27, 50 testes desktop e
88 testes Rust aprovados. As correções abaixo agora estão no instalador,
mas os testes de runtime real continuam pendentes. Evidência:
`../task-release-alpha7.md`.

- Guarda-roupa, correção de 17/09/2026: recuperação da skin local por PNG,
  seleção equipada persistente e espera da recuperação antes de iniciar o jogo.
  O launch prioriza o modelo Slim/Clássico equipado local, mesmo quando o perfil
  remoto está desatualizado. **IMPLEMENTADO MAS NÃO HOMOLOGADO** no executável
  empacotado; fixture visual e testes do IPC simulado não homologam gameplay.
  Nenhum JAR ou instalador foi alterado. Ver `../task-wardrobe-recovery.md`.

- Inicialização Windows sem console, incluindo probes Java/Forge/jogo:
  **IMPLEMENTADO MAS NÃO HOMOLOGADO** na fonte posterior à alpha.6. Testes de
  subprocesso confirmam console ausente e stdout/stderr preservados; executar
  a próxima build empacotada e iniciar Minecraft ainda **PRECISA DE TESTE**.
  Sessão restaurada usa tela animada sem antecipar o login; detalhes em
  `../startup-session.md`.

- Minecraft 1.16.5 em sessão offline: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. O
  launcher isola somente o endpoint de privilégios do `authlib 2.1.28`, fazendo
  o cliente usar o fallback `OfflineSocialInteractions` e preservar Multiplayer
  LAN/offline. Há teste unitário do escopo por versão e prova direta com o
  `authlib` da instância; a confirmação visual do botão ainda **PRECISA DE
  TESTE** em uma nova build desktop.

## Roteiro runtime obrigatório

Skins **0.2.3** substitui 0.2.2 na próxima build do launcher: a prévia guarda somente o PNG e não altera metadados da biblioteca/skin ativa, inclusive quando a textura é idêntica e o modelo é diferente. 0.2.2 permanece imutável como histórico. Estado das nove combinações: **IMPLEMENTADO MAS NÃO HOMOLOGADO**, até repetição do roteiro abaixo.

1. Instância limpa e hashes de instalador/JAR.
2. Java/loader corretos e launch sem argumento sensível.
3. Handshake IPC e rejeição de nonce inválido.
4. Core → Skins → Companion; somente uma conexão WebSocket.
5. Options → Aurora Options; botão sem sobreposição em diferentes GUI scales.
6. Painel roxo, textos truncados sem colisão e abrir/fechar repetido por botão/ESC.
7. Avatar 3D na tela principal e na Central Aurora, classic/slim, duas camadas e fallback.
8. Mouse para cima/baixo e esquerda/direita com cabeça/corpo na direção correta e suavização.
9. Importar/aplicar PNG 64x64 classic, slim e 64x32 convertida; reiniciar e confirmar persistência.
10. Mundo não pausa; teclado/mouse voltam ao jogo sem travar.
11. Em 1.16.5, Multiplayer não deve ser bloqueado pela conta Microsoft fictícia; servidor `online-mode` continua exigindo autenticação oficial.
12. Falha de módulo, skin HTTPS, IPC e encerramento sem impedir o Minecraft de iniciar.

## Estratégia

- Referência moderna: Fabric/Forge 1.20.1.
- 1.21.1 usa Java 21 e adaptadores próprios de `PlayerSkin`.
- 1.16.5 preserva bytecode Java 8 e identidade de launch `legacy`/offline.
- 1.12.2 permanece em trilho Forge legado Java 8/LWJGL2.
- NeoForge: fora do escopo por decisão atual.

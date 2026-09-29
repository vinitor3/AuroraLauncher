# Aurora Connect — requisitos, correspondência e evidências

Data: **29/09/2026**. [Reconciliação de fontes e IDs](AURORA_CONNECT_REVIEW.md), [backlog canônico](CODEX_TASKS/AUR-R4-CONNECT-BACKLOG.md), [execução atual](../task-aur-r4-connect.md). Este documento liga requisitos a provas, sem transformar testes planejados em resultados.

## Convenções e histórico

- `REQ-V2-01..27` conservam o significado do pacote `docs` em `d6f31db6bd1bd0e9b9e2f8bbc2ebf1a6e653af5d`; esta tabela traduz os IDs de tarefa para o backlog canônico.
- `EST-01..12`, `DOC-01..03` e `CHG-01..11` continuam registros daquele pacote; consulta integral por `git show d6f31db:docs/engineering/AURORA_CONNECT_TRACEABILITY.md`. Não são execuções desta implementação.
- `INV-01..06`, `T-*` e `C-*` vêm do pacote `review` em `5ac2f7e`. INV são leituras históricas; T são critérios; C são células candidatas. Nenhum implica teste aprovado por existência.
- Novas evidências usam IDs canônicos `001..008` e apontam ao task de execução, comando real e ambiente. Cada voz tem evidência independente.
- Legado `006-SVC` → `006`; legado `006-PV` → `007`; legado `007` robustez → `008`. Evidência histórica ambígua deve incluir pacote/SHA e não recebe novo significado por renumeração.

## Requisito → contrato → tarefa → prova

| Requisito preservado | Contrato canônico | Tarefa | Prova exigida / limite |
| --- | --- | --- | --- |
| REQ-V2-01: R0–R5, prioridades, gates, ownership e nove pares | Roadmap; AGENTS; revisão | Todas | T-INV-01, T-REG-01/02; não promover runtime por build |
| REQ-V2-02: separar requisito, proposta, estática, execução e pendência | Revisão; task de execução | 001 e relatórios | Base/manifests/fonte/data; resultados só após comando executado |
| REQ-V2-03: público, privado autorizado, dedicado e LAN; capacidades distintas | Spec §§1–3 | 005, 008 | T-MC-01..05; transporte não abre LAN nem comprova auto-join |
| REQ-V2-04: experiência normal sem VPN/driver/admin/portas manuais; host disponível | Spec §§1, 9 | 004, 005, 008 | T-UX e T-MC; configuração manual serve apenas laboratório |
| REQ-V2-05: direto preferido e relay gratuito elegível | Dependências; ADR | 001, 004, 008 | T-COST-01..03, T-NET-05/06; aceitar relay permitido |
| REQ-V2-06: zero cobrança, teste separado de produção | Dependências | 001, 004, 008 | Condições efetivas do operador/plano; contador local não garante fatura |
| REQ-V2-07: infraestrutura auxiliar, migração e UDP bloqueado | Spec §6; ADR | 004 | T-NET-03..06; caminho real, sem inferir NAT por timeout |
| REQ-V2-08: serviço aprovado sem IP/porta remoto arbitrário | Spec §3 | 001..003 | T-SEC-02/03; zero abertura indevida ao destino |
| REQ-V2-09: TCP íntegro, isolado, bidirecional, backpressure/half-close | Spec §4 | 002 | T-TCP-01..05, T-LIFE-01 |
| REQ-V2-10: UDP preserva fronteiras e retorno correto | Spec §5 | 003 | T-UDP-01/02/08/09; vários peers/sessões/serviços |
| REQ-V2-11: MTU/overhead, filas, taxa, expiração e limites | Spec §5 | 003, 006, 007 | T-UDP-03..09; oversize recusado inteiro; sem fragmentação implícita |
| REQ-V2-12: voz fora do stream de jogo e contenção medida | Spec §5 | 003, 006..008 | T-MIX-01..03; filas por perfil; conexão compartilhada é hipótese |
| REQ-V2-13: mods existentes, versões/APIs reais, sem voz própria | Spec §7; dependências | 001, 006, 007 | T-VOICE-01; JAR/hash/origem/licença; homologação independente |
| REQ-V2-14: bind/anúncio/ponte, misto e múltiplas instâncias | Spec §7 | 005..007 | T-VOICE-02/05/07; sem anúncio global loopback universal |
| REQ-V2-15: API oficial/hook mínimo; sem Companion ou alteração de bytes cifrados | Spec §7; ADR-018 | 006, 007 | API versionada, contrato e regressões se Core alterado |
| REQ-V2-16: config reversível, concorrência, licença/updater | Spec §7; recovery/Tools atuais | 006, 007 | T-CONFIG-01..04; broker JSON não autoriza TOML/properties |
| REQ-V2-17: stack verificada, Iroh primeiro, pin e alternativas | ADR; dependências | 001 | Artefato/API/MSRV/licença/lock e teste, sem Go automático |
| REQ-V2-18: e4mc/World Host por mérito, ICE+QUIC demonstrado | ADR; ADR-010 | 001, 004 | Fonte/versionamento/termos; sem deduzir voz do uso de QUIC |
| REQ-V2-19: chave-peer-sessão-permissões, E2E/replay/revogação | Spec §8 | 001..005 | T-SEC-01..05, T-LIFE-02; lease de laboratório não é convite social |
| REQ-V2-20: tokens/segredos fora de peer/JVM/log; online-mode/voz preservados | Spec §8 | Todas | Revisão de diff/diagnóstico; T-SEC-05; sem áudio gravado |
| REQ-V2-21: sinalização mínima, regras, expiração e retenção sem billing | Spec §8; dependências | 005 | T-SIGNAL-01/02; sem jogo/áudio/JAR em Firestore |
| REQ-V2-22: IP não anônimo, UI backend e evidência por rótulo | Spec §9 | 005, 008 | T-UX-01..04; túnel/status/mundo/voz independentes |
| REQ-V2-23: incrementos consecutivos, próximo bloqueio real | Prompt; backlog | 001..008 | Teste/correção/revisão/evidência/rollback em cada incremento |
| REQ-V2-24: redes reais, mods isolados, BR–BR/BR–EUA, métricas | Spec §10; backlog testes | 003, 004, 006..008 | T-NET/T-VOICE/T-MIX; laboratório não prova NAT/áudio |
| REQ-V2-25: TCP não aprova UDP/voz/relay/R4; experimental desligado | Roadmap; spec | Todas | Gate separado por capability/mod/caminho |
| REQ-V2-26: prompt autossuficiente, base/code/tests e relatório | Prompt canônico | Todas | Task atual e roteiro executável, sem comandos inventados |
| REQ-V2-27: fontes oficiais/versões/data e limites | Dependências; ADR | 001 e mudanças | Consultar versão fixada; latest e referências de pesquisa não são pin |
| REQ-EXEC-01: preservar alpha.14 e reconciliar duas propostas | Revisão; backlog | 001 | Ancestralidade, mapeamento explícito, nenhum schema concorrente |
| REQ-EXEC-02: continuar trabalho local independente | Prompt | Todas | Pendência externa bloqueia só o ensaio correspondente |
| REQ-EXEC-03: preparar mínima intervenção humana | Prompt; task atual | 004..008 | Impedimento observado, máquina/conta, passos exatos, resultado/redação e etapa desbloqueada |

## Registro desta reconciliação

### Evidências locais posteriores à preparação

Baseline de 29/09/2026, comandos/saídas/achados no [task de execução](../task-aur-r4-connect.md). Resultados correspondem ao laboratório Windows com Iroh 1.3.0 e não concluem todos os cenários dos critérios normativos abaixo.

| Prova executada | Requisitos/tarefas relacionados | Limite da prova |
| --- | --- | --- |
| Incremento19: lock isolado do cliente, UUID corrigido/audit zero; 14 testes FrameGuard e três de dependência | REQ-V2-17/20/27; preparação005/008 | Sem login/mundo; guard não valida schema/NBT nem descomprime. Integração precisa recusar compressão antes do handler |
| Décimo oitavo incremento: artefatos oficiais verificados, Minecraft 1.21.1/Fabric real, bind próprio, STATUS/pong por dois agentes e stop normal/EULA restaurada | T-MC-01 parcial; T-LIFE-01/02; REQ-V2-19/20/23; 002/005/008 | Loopback online-mode=false expressamente solicitado, whitelist ativa; zero login/mundo/mod/áudio. 50 Node sequenciais e 14 focados após correção AppData; nenhuma homologação de clientes mistos |
| Décimo sétimo incremento: Java 21 próprio real + probe produção STOPPED→RUNNING→STOPPED, stop normal; 1 teste Node/3 probes nativos aprovados | T-LIFE-01/02; T-CONFIG-01/04; 006/007/008 | Sem Minecraft/mod; observação por nome, não correlação de PID. Falha do Cargo não comprova cleanup de sua árvore |
| Décimo sexto incremento: supervisor de filho explícito com stop/deadline/cancelamento/saída limitada e close observado; 18 testes Node | T-LIFE-01/02; T-SEC; 005/008 | Sem Java/Minecraft, aprovação ou readiness. Callback cooperativo e nenhum descendente; kill forçado é erro, não PASS normal |
| Décimo quinto incremento: guest-tcp/plano game obrigatório e aux opcional; CLI e smoke com dois agentes, STATUS/pong por fixture independente | T-MC-01 parcial; T-UX-02/04; 002/005/008 | 59 Rust e dois smokes Node aprovados; zero JVM. QUIC ainda usa UDP. guest_ready é listener; acesso efetivo exerce autorização |
| Décimo quarto incremento: plano/staging limitado com pins oficiais e gate de termos; 17 testes Node com HTTP local e JDK sintético | T-MC-01 preparação; T-SEC/T-CONFIG; 005/008 | Nenhum download Minecraft/JVM real. Consentimento de fixture não é aceitação do usuário. MZ/hash não valida todo JDK; runtime aguarda autorização |
| Décimo terceiro incremento: quatro cenários de entrega/retomada do lock com recibo vivo, restauração estrita/revisada SVC/Plasmo e preservação de edições | T-LIFE-01/02; T-CONFIG-03/04; 006/007/008 | Locks/CAS reais com probes controlados; não executa JVM/writer, não torna recibo durável nem automatiza encerramento |
| Décimo segundo incremento: guard opaco/verificações de processos em aquisição, preparação e confirmação da mutação de voz | T-CONFIG-01/03/04; T-LIFE-01/02; 006/007/008 | Política Windows conservadora para java/javaw; sem runtime de mod, UI, proteção contra início externo ou JVM renomeada/embutida |
| Décimo primeiro incremento: restauração revisada das próprias chaves sobre arquivo atual, hash/aprovação novos, preservação de edições e conflito recusado | T-CONFIG-03/04; 006/007/008 | Somente cópia e recibo em RAM; não comprova roundtrip JVM, encerramento automático, UI ou voz |
| Décimo incremento: consulta STATUS/pong real por TCP, fixture independente através de Host/Guest, CLI, parser limitado/timeout/cancelamento e resumo redigido | T-MC-01 parcial, T-UX-02/04; 005/008 | Sem servidor/JVM real ou prova de login/mundo/voz. Consulta de listener arbitrário não herda autenticação de túnel; composição exige evidências separadas |
| Nono incremento: fixtures equivalentes aos defaults completos e recusa de reserialização TOML semanticamente igual; origem das fontes Plasmo corrigida | T-CONFIG-03/04, T-VOICE-01/02; 006/007/008 | Nenhuma JVM/writer do mod executado. Refusa de hash divergente não resolve mescla ou limpeza após runtime; tag consultada declara versão diferente, JAR continua fixado pelo hash |
| Oitavo incremento: restauração seletiva confirmada por recibo privado, preservação de outras edições e transições de falha/cancelamento testadas separadamente para SVC/Plasmo | REQ-V2-14/16/19/23/25; T-CONFIG e 006/007/008 | Apenas cópia opt-in e recibo em RAM; requer caller com jogo parado. Não oferece retomada seletiva após reinício nem homologação de runtime; [evidências](../task-aur-r4-connect.md) |
| Sétimo incremento: desktop rustls 0.23.43→0.23.45, auditoria 546 dependências/zero vulnerabilidades/oito warnings, suites padrão/feature e HTTPS de catálogo aprovados | REQ-V2-01/17/20/27; R0 e robustez 008 | Patch de lock preexistente separado do transporte Connect; unic/GTK/yanked conservam gates próprios, sem homologação Linux/BSD, áudio ou segurança absoluta |
| Sexto incremento: caminho Plasmo confirmado por tag/JAR; renderer/seleção de mod e aplicação/adição/undo em cópia; 12/12 focados e suites feature/padrão aprovadas | REQ-V2-13/14/16/17/23/25; 007 e 008 | Parser limitado com spans/árvore conferidos e dependência TOML opcional já transitiva. 268 PASS/sete opt-in ignorados com feature; 258 PASS/cinco ignorados padrão. Artefato real não é runtime; sem Java/áudio/modos LAN/misto |
| Quinto incremento: adaptador nativo SVC opt-in em cópia, quatro testes focados com JAR real, suites padrão/feature e fmt/clippy aprovados | REQ-V2-14/16/19/23/25; 006 e 008 | Configuração existente somente, confirmação em memória antes de CAS/preview/restore; cancelamento sem ponto reaplicável, identidade/hash/concorrência/undo. Padrão 258 PASS/cinco opt-in ignorados. Caller ainda deve provar jogo parado; sem comando/UI, runtime/mod/áudio/crash físico não homologados |
| 24/24 Rust: 4 unitários, 9 adversariais, 11 de transporte; fmt/clippy aprovados | REQ-V2-08..12/19/20; 002/003 e robustez local 008 | Autorização antes do destino, TCP duplex/8 MiB/backpressure/half-close, UDP real com isolamento/MTU/replay/revogação/limites; sem JVM/áudio/NAT |
| Transporte repetido cinco vezes: 55/55 resultados | REQ-V2-12/23; 002/003 | Após reproduzir e corrigir descarte UDP por esgotamento do bucket agregado sob TCP; reserva de 16 KiB, sem alegar perda zero ou fairness universal |
| Smoke multiprocesso: dois agentes, 1 MiB TCP por direção, dez UDP/duas origens, 1358 bytes negociados e limpeza aprovada | REQ-V2-09/10/23/26; C-TCP/C-UDP | Direto na mesma máquina; uma amostra debug não mede atraso de voz, RTT puro ou desempenho externo |
| IPv4/IPv6 loopback reais | REQ-V2-07/24; 004 parcial | Não comprova LAN, firewall, IPv6 público, NAT/CGNAT ou relay; procedimento em dois computadores preparado, não executado |
| Inspector Node 14/14 e demonstrações sem escrita | REQ-V2-13/14/16; preparação 006/007 | Fixtures/referências oficiais; não instala/executa mod, não aplica configuração, não homologa endereçamento/áudio |
| Quarto incremento: 31/31 Node, incluindo os JARs oficiais SVC/Plasmo, hash/metadados/API e plano com versão derivada | REQ-V2-13/14/16/17; preparação 006/007 e 008 | Artefatos Fabric 1.21.1 fixados; sem execução de JAR, instalação, configuração aplicada, áudio ou homologação. QA corrigiu junction pai antes da leitura; [evidência e hashes](../aurora-connect-voice.md) |
| Metadata Cargo: 358 pacotes com licença declarada; rustls 0.23.45 conferido no advisory específico | REQ-V2-17/27; 001 | Inventário não comprova obrigações de distribuição; ausência inicial de `cargo audit` resolvida pela execução posterior abaixo |
| Cargo Audit 0.22.2 local, DB `f23b768236fe2880e4cfa167da662cad8ca79240`, 1277 advisories: exit 0, zero vulnerabilidades, um alerta de manutenção | REQ-V2-17/27; 001 e robustez 008 | `paste 1.0.15`/RUSTSEC-2024-0436 preservado sem ignore, cadeia não usada no alvo Windows consultado e presente em `--target all`. Snapshot não garante segurança absoluta; [relatório e escopo](AURORA_CONNECT_DEPENDENCIES.md#auditoria-automatizada-local--29092026) |
| Segundo incremento `42bf0ad`: 34/34 Rust e fmt, dez novos casos downstream/QA | REQ-V2-08/10/11/12/19; 003 e robustez local 008 | Perfil local autorizado recebe mais que envia, com orçamento por peer/serviço, reconexão, isolamento, tentativa de política remota recusada, inatividade/revogação/expiração. Naquele snapshot CLI permanecia 1:1; sem mod/áudio |
| Terceiro incremento: 45/45 Rust, fmt/clippy, plano local por SHA-256 e dois smokes | REQ-V2-08/10/11/19/23/26; 003 e robustez 008 | Onze casos adicionais. `host-plan` opt-in autorizado, hash incorreto recusado antes do host; smoke downstream 2 B enviados/8192 B recebidos/16 respostas/duas origens, padrão dez UDP/3138 B por direção, ambos TCP 1 MiB por direção. Não altera config pessoal ou resolve anúncio de voz |
| Termos n0 lidos integralmente e gatilho de ensaio delimitado registrado | REQ-V2-05/06/07/27; 004 | Gratuidade/desenvolvimento confirmados; alcance da cláusula de automação pendente. Consulta preparada não enviada, nenhum relay conectado |

TCP, UDP e direto loopback permanecem **PROTÓTIPO**. Relay está desligado; Minecraft já respondeu STATUS/pong em servidor real no laboratório, mas login/mundo, convites sociais e interface continuam pendentes. SVC e Plasmo possuem preparação parcial separada, runtime pendente. O perfil downstream remove o bloqueio de proporção 1:1 para serviço explicitamente aprovado na biblioteca ou CLI por plano/hash; ainda exige keepalive/reabertura/endereço adequados ao mod. Tetos de datagrama 1358/1390 B observados em smokes diferentes são dinâmicos. Células de homologação de jogo/mod/rede externa continuam pendentes.

| Decisão | Alteração verificável |
| --- | --- |
| Base cumulativa | Mantidos arquivos alpha.14; alterações documentais alpha.13 incorporadas por conteúdo/requisito, sem copiar código antigo |
| Autoridade única | Backlog e ADR alpha.14 canônicos; proposta alpha.13 preservada em Git, não duplicada como schema ativo |
| Prompt único | Conteúdo operacional consolidado de d6f31db no arquivo uppercase já rastreado; instrução histórica de parar no primeiro incremento substituída |
| Contratos reconciliados | Capability singular por serviço, identidade nova por sessão host, revisão de serviço separada e framing único conferido na implementação |
| Pesquisa cumulativa | Preservadas referências versionadas 1.1.0 e fontes adicionais do primeiro pacote como pesquisa histórica; pin final e rede efetivamente ativada ficam explícitos |
| Evidência sem promoção | Casos T e placeholders EV permanecem pendentes até teste real; resultado de laboratório não preenche célula de mod/rede |

O task de execução informa arquivos/comandos/resultados/commit e rollback atuais. Revisões documentais antigas e seus SHAs continuam consultáveis; nenhuma evidência histórica foi apagada. R0–R5, a matriz de módulos e configuração pessoal permanecem fora de qualquer autorização implícita desta tabela.

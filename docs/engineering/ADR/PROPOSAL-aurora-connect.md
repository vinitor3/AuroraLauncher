# Decisão experimental de transporte — Aurora Connect

Data: 29/09/2026. Decisão aceita somente para laboratório local; arquitetura/infraestrutura de produto permanecem **HIPÓTESE**, sem homologação. Conserva o caminho documental existente, sem reservar outro número ADR. Base cumulativa alpha.14 `5ac2f7e` sobre `6a107b8`; Core 1.1.5 e Skins 0.3.1. [Execução e testes](../../task-aur-r4-connect.md), [reconciliação](../AURORA_CONNECT_REVIEW.md). Pesquisa upstream não é homologação Aurora.

Referências internas: [especificação](../AURORA_CONNECT_SPEC.md), [dependências e zero-spend](../AURORA_CONNECT_DEPENDENCIES.md), [backlog](../CODEX_TASKS/AUR-R4-CONNECT-BACKLOG.md), [roadmap](../MASTER_ROADMAP.md) e [diagnóstico/rastreabilidade](../../task-aur-r4-connect-v2-review.md).

## Contexto e autoridade

O requisito V2 amplia R4 para transportar Minecraft TCP e serviços TCP/UDP explicitamente autorizados, incluindo integração separada de Simple Voice Chat e Plasmo Voice. Não cria voz própria. Ele substitui a proibição de qualquer relay: caminho direto é preferido; relay gratuito pode ser avaliado e habilitado para uso verificado; gasto obrigatório, automático ou por excedente sem bloqueio efetivo é proibido.

Esta proposta preserva a organização, os IDs e gates do roadmap. Não altera R0, o Assistente desktop e limites HTTP/IPC de R1, updater reversível de R1.5, recovery/Doctor/CAS/Tools de R2/R2.5, Creator de R3 nem gates de produção/privacidade/custos de R5. A matriz ativa de nove pares Core/Skins permanece a autoridade. Companion/HUD e Assistente in-game continuam retirados pela ADR-018. Os arquivos monolíticos mantêm ownership exclusivo por wave.

A stack real foi identificada nos manifests pela revisão principal como Rust, React/TypeScript e Tauri 2. Não se deduz existência do Connect de uma biblioteca transitiva no lockfile. A revisão desta ADR pesquisou fontes upstream; não executou transporte nem jogos.

## Decisão para o incremento local

Selecionado **Iroh `=1.3.0`** em `apps/connect-lab/`, crate independente `publish = false`, edition 2024, MSRV 1.91. O inventário e `cargo info iroh@1.3.0` desta execução confirmaram artefato, licença MIT OR Apache-2.0 e compilador local Rust 1.98; o lock próprio torna a resolução reproduzível. Não atualizar o toolchain global nem as dependências do desktop para o experimento. `default-features = false`, somente `tls-ring`; sem Go ou implantação externa. A ligação ao launcher e distribuição dependem dos gates posteriores.

O construtor lido em `apps/connect-lab/src/transport.rs` usa `presets::Minimal`, limpa IPs/relays/lookup padrão, aplica `RelayMode::Disabled` e registra apenas bind/endereço manual. Relatórios de endereço observado e candidatos remotos de traversal ficam desabilitados. Não existe operador externo aprovado ou custo de relay nesta configuração. Isso é decisão de laboratório, não proibição futura de relay gratuito elegível nem prova de NAT traversal.

Fontes exatas conferidas pelo agente de transporte em 29/09/2026: [release v1.3.0](https://github.com/n0-computer/iroh/releases/tag/v1.3.0), publicada em 28/09 no commit `0072d7d`; [manifest v1.3.0](https://raw.githubusercontent.com/n0-computer/iroh/v1.3.0/iroh/Cargo.toml); [presets da tag](https://github.com/n0-computer/iroh/blob/v1.3.0/iroh/src/endpoint/presets.rs). A API foi conferida no artefato baixado por Cargo, pois a URL docs.rs exata retornou erro de acesso, não prova de inexistência. `N0DisableRelay` não foi usado: ainda configuraria descoberta/publicação externa. O advisory [RUSTSEC-2026-0285](https://rustsec.org/advisories/RUSTSEC-2026-0285.html) afeta rustls `>=0.23.13, <0.23.45`; a resolução do lock novo contém 0.23.45. Isso verifica esse aviso identificado, não afirma auditoria completa de todas as transitivas.

Posteriormente, Cargo Audit 0.22.2 foi instalado em diretório local ignorado e varreu o lock contra a base de 29/09: zero vulnerabilidades conhecidas e um warning de manutenção em `paste 1.0.15`, sem ignore. A cadeia é dependente de alvo e não apareceu no grafo Windows consultado; não se removeu ou substituiu a transitive automaticamente. Versão da base, comando, relatório e limites estão no [registro de dependências](../AURORA_CONNECT_DEPENDENCIES.md#auditoria-automatizada-local--29092026). O resultado não altera aprovação de produto ou segurança de redes/voz.

O bloqueio real era ausência de ponte autenticada no inventário focal. Iroh entrega identidade de endpoint, streams e DATAGRAM na mesma stack Rust e permite o modo manual sem serviços externos; evita compor a infraestrutura ausente de Quinn ou manter outro runtime. O experimento mede TCP, UDP e isolamento antes de escolher a integração de produto. Dois endpoints sintéticos não homologam voz, Minecraft, redes distintas ou todos os pares.

Contrato único: `requiredCapability` singular por serviço (`reliableStream` ou `datagram`), sessão identificada por ID novo a cada host/run, `serviceRevision` separado da sessão, peer derivado do transporte autenticado e framing UDP de 24 bytes. `sessionEpoch`, lista `requiredCapabilities` e reserva aberta de 32 bytes permanecem somente no histórico alpha.13; não se implementam schemas concorrentes. Lease local com pin, prazo, serviços e revogação não equivale a convite social/Firebase.

Decisão posterior ao baseline `b1c6871`, registrada em `42bf0ad`: `Service.udp_return_policy` permite `AuthorizedDownstream` somente por aprovação local do host e com orçamento por peer/serviço persistente na lease. Naquele incremento, padrão e CLI mantinham crédito 1:1. A mudança resolve um bloqueio concreto do crédito por bytes para aplicações predominantemente receptoras; não remove limites, inatividade ou autenticação. O primeiro envio local precisa ter sucesso, o destino permanece loopback conectado e nenhum endereço/política vem do pedido remoto. Dez novos testes integraram a suíte 34/34 aprovada; perfil e limites exatos na [spec §5.1](../AURORA_CONNECT_SPEC.md#51-retorno-udp-autorizado--incremento-de-laboratório). Nenhum mod ou fluxo de silêncio/keepalive foi homologado.

Terceiro incremento: autorizar essa política na CLI por plano local estrito, prévia e hash SHA-256 dos bytes exatos, mantendo pin manual do convidado e destinos loopback. `host` continua 1:1; `host-plan` permite downstream aprovado. O plano não escreve configurações e só descreve o par `game` TCP/`aux` UDP revisão 1, sem transformar o launcher em proxy genérico. Onze testes adicionais e smokes dos dois perfis passaram; total 45/45 Rust. [Contrato local e limites](../AURORA_CONNECT_SPEC.md#31-plano-local-revisado-por-hash--laboratório). A biblioteca SHA-256 escolhida é `sha2 =0.11.0`, já presente transitivamente no lock, sem criptografia própria. Isso não aprova edição de configuração dos mods nem convites sociais.

Após TCP comprovado, continuar UDP, segurança, limites e outros itens locais independentes. Não encerrar automaticamente na primeira PoC. A comparação abaixo conserva o levantamento histórico de 29/09/2026; versões observadas de alternativas não são dependências instaladas. Resultados realmente executados e limites ficam no task.

Separar identidade/autorização, sinalização, descoberta, transporte, registro de serviços, adaptadores e interface. Não criar framework extensível sem necessidade. Proposta de contrato: convidado pede ID de serviço aprovado; host resolve o destino previamente autorizado. Manifesto não autoriza abertura de porta. Não criar proxy aberto, SOCKS, acesso geral à LAN, RCON/SSH/admin automático ou execução remota.

## Comparação técnica preservada

| Opção | Capacidades observadas | Trabalho e limitações | Encaminhamento |
| --- | --- | --- | --- |
| Iroh 1.3.0 observado | Rust, identidade de endpoint, streams e datagramas, descoberta/NAT traversal integrados, relay e observação de caminhos | Configurar explicitamente infraestrutura/portmapper; vincular identidade a convite e autorização Aurora; verificar limites/MTU e compatibilidade de versão | Primeiro candidato para PoC; infraestrutura de produção não aprovada |
| Quinn 0.11.12 observado + descoberta/traversal compatíveis | Rust, QUIC/TLS, streams confiáveis, DATAGRAM e estatísticas | Quinn isolado não entrega sinalização, serviço de descoberta ou relay autorizado; composição de sockets/traversal eleva manutenção | Alternativa Rust se houver bloqueio concreto no Iroh |
| Pion ICE v4.4.4 observado + quic-go v0.63.0 observado | ICE com candidatos/estados e QUIC com streams/DATAGRAM | Precisa provar que QUIC reutiliza caminho ICE e que leitura/demultiplexação têm proprietário; acrescenta Go e processo auxiliar | Somente com vantagem concreta demonstrada; não adicionar automaticamente |
| e4mc | Integração LAN/domínio e Dialtone direto quando presente nos dois lados, conforme changelog | Não comprovado como registro seguro genérico TCP/UDP; voz e permissão de uso da infraestrutura pelo Aurora não verificadas | Comparação e possível PoC específica de Minecraft |
| World Host | Integração de mundo/friends, UPnP/Proxy documentados e descrição de hole punching | Documento de hole punching não prova transporte dos mods de voz; infraestrutura e matriz precisam de avaliação própria | Comparação específica; não substituir o contrato multisserviço por suposição |

Iroh e Quinn declaram MIT/Apache-2.0; Pion ICE e quic-go declaram MIT. Licença final e dependências transitivas precisam ser verificadas na versão fixada e registradas na distribuição. Windows/Linux/macOS constam nas fontes dos candidatos Rust; isso não aprova automaticamente os binários do Aurora. Se a alternativa Go vencer, a decisão deverá incluir build por plataforma, assinatura, proveniência, atualização reversível, IPC autenticado/confínado, encerramento de processo e rollback.

Fontes: [Iroh oficial](https://github.com/n0-computer/iroh), [Iroh Connection](https://docs.rs/iroh/latest/iroh/endpoint/struct.Connection.html), [Quinn API](https://docs.rs/quinn/latest/quinn/), [Quinn oficial](https://github.com/quinn-rs/quinn), [Pion ICE](https://pkg.go.dev/github.com/pion/ice/v4), [quic-go](https://pkg.go.dev/github.com/quic-go/quic-go) e [transporte quic-go](https://quic-go.net/docs/quic/transport/). Consulta: 29/09/2026. Páginas de Go exibiram aviso de versão não necessariamente mais recente. URL Iroh `latest` exibiu 1.3.0; abertura da URL explicitamente versionada falhou. Não copiar APIs para lockfile diferente.

## Semântica exigida

- TCP: ponte de bytes de aplicação, uma conexão local associada a stream confiável independente; bidirecionalidade, ordem, integridade, backpressure, half-close, erro e cancelamento definidos. Consulta de status e jogo não compartilham estado acidentalmente.
- UDP: preservar fronteira e conteúdo de cada datagrama, isolar sessão/participante/serviço/associação e demonstrar retorno ao jogador correto. Socket host por associação é hipótese simples; compartilhamento exige demultiplexação comprovada.
- DATAGRAM ou equivalente é requisito técnico a validar, não encaminhamento automático de mods. Áudio não vai por padrão no stream confiável do jogo. Overhead, MTU, limite negociado variável, filas e expiração precisam de testes; não truncar. Fragmentação de aplicação não é padrão inicial e exigiria projeto limitado/autenticado com remontagem expirada.
- Jogo/voz podem compartilhar contexto autenticado, mas contenção de banda e filas precisam de comparação. Conexões separadas são permitidas se justificadas. Congestionamento e criptografia ficam a cargo de bibliotecas mantidas.
- Listeners de aplicação ficam em loopback. A autenticação do transporte precede abertura de destino no host e não substitui login Minecraft/voz. Preservar online-mode e mecanismos dos mods. Não aceitar qualquer certificado, transportar tokens Microsoft/Minecraft entre peers nem registrar áudio.

Fundamento: [RFC 9221](https://www.rfc-editor.org/rfc/rfc9221.html), consultado em 29/09/2026. DATAGRAM exige suporte negociado, é limitado pelo caminho e pelo peer, não fragmenta automaticamente nem possui ID de stream; multiplexação e quotas de aplicação continuam necessárias. ACK de rede não comprova consumo pelo mod.

## Caminhos e infraestrutura

A biblioteca pode iniciar por relay permitido e tentar direto sem aguardar uma sequência lenta. Iroh documenta introdução por relay e hole punching integrado; não impor ICE/STUN paralelo. A proposta precisa distinguir sinalização Aurora, descoberta de endereço, reflexão/traversal e relay, mesmo quando fornecidos juntos. Não prometer migração sem interrupção antes de medi-la no Aurora.

Na documentação lida, o relay Iroh usa HTTPS/WebSocket em TCP 443; UDP externo bloqueado pode usar esse caminho se permitido. Bloqueio de WebSocket também pode impedir conexão. Áudio pelo caminho TCP deve ser avaliado para retenção por retransmissões, jitter e dados vencidos. Portmapper automático consta ligado no padrão upstream: Aurora deve desabilitá-lo até consentimento, lease e limpeza comprovados. [Redes Iroh](https://docs.iroh.computer/configuring-networks), consulta 29/09/2026.

Nenhum relay gratuito de produção foi aprovado nesta revisão. Os públicos n0 possuem gratuidade/finalidade de desenvolvimento confirmadas, mas a reconsulta integral dos termos deixou uma lacuna sobre aplicação da cláusula de acesso automatizado ao SDK/relay público. O ensaio sintético remoto permanece candidato, sem execução, com consulta pronta não enviada em [DEP-TERMS](../AURORA_CONNECT_DEPENDENCIES.md#dep-terms--limitações-das-fontes-e-decisões-pendentes). Pro/Shared e Dedicated pagos permanecem desabilitados; trial não satisfaz sustentabilidade. Sem relay elegível, preservar direto nas redes efetivamente suportadas e falhar claramente quando não houver caminho. Essa pendência não impede incrementos locais e não prova NAT traversal.

e4mc deve continuar no comparativo: seu ramo oficial lido declara 6.2.2 e o changelog 6.0.0 introduz Dialtone. Não descartá-lo por relay nem supor voz por utilizar QUIC/Iroh. World Host documenta UPnP/Proxy e um fluxo UDP de descoberta; isso não confirma voz. [e4mc changelog](https://github.com/vgskye/e4mc-minecraft-architectury/blob/rererewrite/CHANGELOG.md), [World Host](https://github.com/Gaming32/world-host), [fluxo UDP World Host](https://github.com/Gaming32/world-host/blob/main/UDP_HOLE_PUNCH_PACKET_FLOW.md), consulta 29/09/2026.

## Integração dos mods e UX

Simple Voice Chat e Plasmo Voice recebem homologações separadas por versão/loader/modo/caminho. Resolver os dois problemas: transportar datagramas e informar ao cliente a ponte correta. `voice_host` e `[host.public]` são configurações a investigar na versão real, não solução universal. Distinguir listen, anúncio e ponte local. Anúncio global loopback não serve automaticamente a jogadores diretos; adaptar por participante somente com API real ou declarar restrição. Não reescrever bytes cifrados do Minecraft por substituição textual.

Priorizar configuração reversível ou API/add-on oficial. Hook mínimo no Core só após contrato e matriz afetada, sem obrigá-lo à primeira PoC. Configuração exige plano/diff, autorização apropriada, snapshot e restauração com detecção de edições concorrentes. Nenhuma redistribuição/instalação de mod pode ignorar licença, origem ou updater.

A UI recebe estados reais e separados: transporte autenticado, caminho selecionado, jogo acessível, jogador no mundo e voz validada. Cada um exige sua evidência própria. Falha de voz não encerra jogo automaticamente. Timeout isolado não diagnostica CGNAT. [Contrato detalhado e testes](../AURORA_CONNECT_SPEC.md).

## Decisões pendentes e condições de aceitação

1. Inventário real e versão fixada: compilador, runtime, dependências transitivas, código já existente e gate R4. Não promover pesquisa a implementação.
2. Escolha final da biblioteca e configuração sem acessos externos implícitos; IP direto, LAN, IPv6 e testes entre redes distintos.
3. Infraestrutura autorizada para descoberta/sinalização/relay: termos integrais, quotas, regiões, retenção, segredo/credenciais e custo efetivo. Não embutir key de projeto.
4. Registro de serviços, enquadramento e limites numéricos mensuráveis; modelos de abuso, replay, revogação e encerramento.
5. MTU/tamanhos reais de cada mod, endereçamento individual, participantes mistos e comportamento UDP bloqueado.
6. Resultados TCP, UDP, jogo+voz, saturação e matriz de redes com logs redigidos. Brasil–Brasil/Brasil–EUA precisam de hosts autorizados sem custo.
7. Plano de liberação e rollback: feature experimental desativada por padrão; remover sessões/listeners/mapeamentos e restaurar somente alterações próprias sem apagar divergências.

A aprovação do laboratório não aprova produto, os incrementos seguintes ou infraestrutura. Uma futura decisão de integração/distribuição poderá receber número próprio após conferir índice/ownership e anexar evidências. A ausência de relay elegível limita o alcance anunciado, não autoriza cobrança nem infraestrutura própria obrigatória. Rollback do experimento: encerrar agentes/listeners/associações e remover somente a dependência/entrada experimental em diff revisado; nenhum arquivo pessoal é modificado para executar a ponte.

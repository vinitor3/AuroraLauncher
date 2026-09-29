# Proposta de ADR — Aurora Connect e transporte autorizado de serviços TCP/UDP

Data de revisão e consulta: **29/09/2026**. Decisão proposta: **HIPÓTESE**. Implementação e homologação Aurora Connect: **NÃO IMPLEMENTADO / NÃO VERIFICADO**, respectivamente. Esta proposta não recebe número: a disponibilidade deve ser conferida pelo integrador antes de registrá-la como ADR aceita.

Documentos normativos desta revisão: [roadmap](../MASTER_ROADMAP.md), [especificação](../AURORA_CONNECT_SPEC.md), [backlog](../AURORA_CONNECT_BACKLOG.md), [dependências e zero-spend](../AURORA_CONNECT_DEPENDENCIES.md). Política explícita do solicitante prevalece sobre propostas históricas conflitantes.

## Contexto lido e limites da evidência

O checkout documental parte de `dc2ec5ac36a88f54b1130bac4789f8ecd56641d2`. Foram lidos `AGENTS.md`, o roadmap canônico, ADR-010, ADR-022, manifests desktop/Rust, trechos pertinentes do lockfile, regras Firestore e configuração do Worker. Os manifests confirmam React/TypeScript, Tauri 2 e Rust; não é uma suposição de stack. O `Cargo.toml` não declara Iroh nem Quinn diretamente. `Cargo.lock` contém Quinn **0.11.11**, também referenciado por uma entrada de reqwest: dependência transitiva não comprova transporte Aurora Connect. Não se executaram build, teste runtime, conexões externas nem auditoria integral do código nesta revisão.

A [ADR-010](ADR-010-multiplayer.md) já admite fallback de terceiros e prioriza estudar e4mc/World Host, com playit independente; ela não proíbe todo relay. O novo requisito exige que a seleção considere **TCP e UDP, incluindo Simple Voice Chat e Plasmo Voice separadamente**, e permite relay gratuito verificado. A alteração proposta é retirar a precedência obrigatória de mods de compartilhamento como única primeira implementação, avaliar Iroh primeiro e manter os demais como alternativas comparadas. O adiamento de infraestrutura própria permanece. A [ADR-022](ADR-022-skins-server-relay.md) trata retransmissão de aparência pelo servidor Minecraft; não prova relay de rede, voz ou NAT traversal.

Aurora Connect é nome de trabalho. O Minecraft continua autoritativo e o host precisa permanecer disponível. Servidor público já alcançável pode ser usado normalmente. Servidor privado exige cooperação e agente autorizado no host; transporte, descoberta de porta, abrir LAN, auto-join e sincronizar manifesto são capacidades distintas. Não haverá VPN manual, driver, administrador permanente nem port forwarding manual no fluxo normal homologado. Uma PoC com endereços e chaves explícitos é ferramenta de desenvolvimento.

## Escolha principal proposta, condicionada a gates

**Avaliar primeiro Iroh dentro do núcleo Rust**, com adaptador pequeno de transporte e registro de serviços do Aurora. Referência fixada para investigação: **`iroh = 1.1.0`**; isto não adiciona dependência nem autoriza pin sem auditoria de segurança e compilação. A tag declara Rust mínimo **1.91**, edition 2024, licença **MIT OR Apache-2.0**, e backend QUIC `noq` 1.2.0. Conferir toolchain, dependências transitivas e licenças contra o Aurora antes de qualquer mudança; não fazer atualização ampla para acomodar uma biblioteca sem medir impacto. [Manifest da tag v1.1.0](https://raw.githubusercontent.com/n0-computer/iroh/v1.1.0/iroh/Cargo.toml).

Motivo da prioridade: integrar conectividade e identidade de transporte em Rust reduz a necessidade de compor NAT traversal do zero e de distribuir um segundo runtime. Isso é avaliação de arquitetura, não benchmark. A API versionada expõe streams bidirecionais, DATAGRAM e observação de caminhos. O peer criptográfico ainda precisa de autorização Aurora por sessão/serviço. [Iroh 1.1.0](https://docs.rs/iroh/1.1.0/iroh/), [Connection 1.1.0](https://docs.rs/iroh/1.1.0/iroh/endpoint/struct.Connection.html).

Aceitar a escolha somente após AUR-R4-001 demonstrar adequação da versão e AUR-R4-002/003 verificarem pontes TCP/UDP, limites, autenticação negativa e isolamento. AUR-R4-004 precisa medir redes distintas e todos os caminhos anunciados. Uma conexão local bem-sucedida não aprova NAT, mods de voz nem distribuição pública.

## Comparação delimitada

| Candidato | Adequação e integração | Transporte, identidade e sinalização | Caminhos, custo e manutenção | Resultado proposto |
| --- | --- | --- | --- | --- |
| Iroh 1.1.0, Rust | Integração nativa; novo conjunto de dependências e toolchain a verificar em Windows; outros alvos somente com evidência própria | Streams e datagramas; identidade por chave do peer; autorização de convite fica no Aurora; lookup e coordenação da biblioteca não substituem sinalização social | Direto e relay; observar caminhos reais; relay público limitado a dev/teste; produção sem provedor aprovado; atualizar pela tag/lockfile | Primeira avaliação, condicional |
| Quinn 0.11.11 + descoberta/NAT compatíveis, Rust | Versão já presente transitivamente; uso direto seria mudança nova; projeto informa testes Linux/macOS/Windows | QUIC, streams, datagramas e TLS; pin de identidade deve ser seguro; Quinn sozinho não entrega convites, descoberta ou uma solução completa de NAT | Direto para endpoints alcançáveis; compor traversal e relay aumenta esforço e risco; nenhum STUN/TURN externo escolhido | Alternativa se o custo de integração/política Iroh impedir o escopo necessário |
| Pion ICE + quic-go, processo auxiliar Go | Introduz build por plataforma, assinatura, distribuição, updater e supervisor de processo; versões exatas ainda **NÃO VERIFICADO** | ICE escolhe caminho; QUIC fornece canal criptográfico; provar adaptação do caminho negociado e demultiplexação, não encaixe automático | STUN/TURN/relay e sinalização separados; serviços continuam sujeitos a zero-spend; maior manutenção Rust/Go/IPC | Só reconsiderar com vantagem medida que compense o processo auxiliar |
| e4mc | Integração centrada no Minecraft; instalação e loader próprios, fora da PoC neutra | Compartilhamento LAN com cliente vanilla; não demonstrado contrato genérico bidirecional UDP de mods | Relay não é motivo de exclusão; custo/termos/integração de distribuição e voz não validados | Comparador do fluxo de compartilhar LAN, sem elegê-lo automaticamente como transporte multisserviço |
| World Host | Mod e sistema de amigos próprios; não amplia matriz Core/Skins | Documentação descreve UPnP e proxy; suporte aos dois mods de voz não demonstrado | Dependência de servidor de coordenação/proxy; critérios zero-spend e segurança ainda pendentes | Comparador independente da experiência; preservar avaliação da ADR-010 |
| playit | Agente do host, conta/túnel de terceiro; não é conexão autenticada entre dois agentes Aurora por si só | Página oferece jogo Minecraft e Simple Voice Chat; não demonstra Plasmo/registro Aurora nem E2E entre agentes | Existem recursos gratuitos e premium; integração empacotada enfrenta condição de consentimento escrito | Sem ativação/bundling; alternativa histórica reavaliada, não fallback silencioso |

Fontes oficiais da comparação: [Quinn e licenças](https://github.com/quinn-rs/quinn), [API Quinn 0.11.11](https://docs.rs/quinn/0.11.11/quinn/), [Pion ICE](https://github.com/pion/ice), [quic-go](https://github.com/quic-go/quic-go), [e4mc](https://e4mc.link/), [World Host](https://raw.githubusercontent.com/Gaming32/world-host/main/README.md), [playit](https://playit.gg/pricing). Todos consultados em 29/09/2026; nenhuma descrição do fornecedor é homologação Aurora. Licenças das alternativas e termos dos operadores estão na matriz de dependências.

## Fronteiras e contrato a preservar

Separar responsabilidades de identidade/autorização, sinalização, conectividade/NAT, transporte, serviços aprovados, integração Minecraft/mods e interface. Começar com módulos pequenos conforme os arquivos reais; não criar framework de plugins ou monólito novo sem necessidade. A primeira PoC não exige hook no Core.

- **TCP:** um stream confiável bidirecional por conexão TCP local, precedido por solicitação de serviço autorizada. Ponteia bytes da aplicação, preservando ordem, integridade, backpressure, half-close/fechamento, cancelamento e isolamento entre status e jogo. Nunca enviar bytes TCP por datagramas sem confiabilidade.
- **UDP:** listener local por serviço/instância e associação autenticada por participante, sessão e serviço; retorno só ao originador. Preferir socket de destino dedicado por associação no host até provar outra demultiplexação. O destino é aprovado pelo host; convidado não fornece IP/porta arbitrários. DATAGRAM não captura automaticamente o UDP de um mod.
- **Capacidades:** negociar versão de contrato, limites e capacidade UDP antes de anunciar serviço disponível. TCP e UDP com mesmo número são serviços diferentes. Modificar o anúncio de voz e transportar seus pacotes são duas integrações separadas.
- **Recursos:** jogo e voz podem compartilhar a conexão se ensaios demonstrarem contenção aceitável; separar filas por serviço e medir carga. Áudio não usa stream confiável do jogo por padrão. Não forçar um socket/conexão global.
- **Tamanho:** limitar payload a `max_datagram_size` do caminho menos cabeçalho autenticado da aplicação, recalculado quando mudar o caminho. Não truncar. DATAGRAM não é fragmentação IP; datagrama maior exige recusa explícita/compatibilidade limitada ou projeto separado de fragmentação limitada, autenticada e com expiração. [RFC 9221](https://www.rfc-editor.org/rfc/rfc9221.html).

A especificação define filas, expiração, limites de associação e políticas por serviço. Voz usa filas curtas e descarte de dados vencidos; isso não é uma regra universal para UDP. Não reimplementar criptografia nem controle de congestionamento.

## Fluxo real de conectividade e redes restritivas

Direto é preferido, mas não se exige esperar um timeout longo antes de relay permitido. Iroh pode estabelecer contato pelo relay, tentar caminho direto e continuar no relay quando necessário. Endereços diretos conhecidos também permitem conexão sem relay. Não prometer transição sem interrupção antes do teste com streams e datagramas simultâneos. [Estabelecimento na API 1.1.0](https://docs.rs/iroh/1.1.0/iroh/).

O cliente relay da tag v1.1.0 efetivamente usa WebSocket, TLS e TCP para URLs seguras. Há texto genérico da documentação da crate que ainda descreve upgrade para protocolo TCP customizado: a prova do caminho selecionado deve usar fonte versionada e captura/metadados do ensaio. Com UDP externo bloqueado, WSS pode permitir o relay, desde que firewall/proxy, autenticação e política do operador permitam. Isso não comprova disponibilidade universal. TCP externo retransmite: medir retenção, jitter e idade do áudio; não anunciar latência equivalente ao direto. [Cliente relay da tag](https://raw.githubusercontent.com/n0-computer/iroh/v1.1.0/iroh-relay/src/client.rs), [mudança oficial do protocolo relay](https://www.iroh.computer/blog/iroh-0-91-0-the-last-relay-break).

Ensaiar LAN, IPv6 direto com firewall, IPv4 alcançável, hole punching e NAT duplo/CGNAT. PCP/NAT-PMP/UPnP são opcionais, dependem da versão/biblioteca e exigem consentimento, lease limitado e limpeza; não habilitar feature de portmapper só porque integra defaults. Mapeamento no roteador doméstico não remove CGNAT. O preset usado deve enumerar lookup DNS, QAD/STUN ou equivalente, relays e mapeamento; desativar dependências sem autorização verificada. Não impor ICE separado a Iroh.

Se a alternativa ICE + QUIC for escolhida, registrar ownership de leitura do socket, candidatos/credenciais, continuidade do par selecionado, STUN versus QUIC, troca de caminho e encerramento. `net.PacketConn` e leitura de pacotes não QUIC existem no quic-go, mas não provam compatibilidade com `ice.Conn`; são pontos de integração a testar. Uma nova conexão QUIC em outro socket pode perder o mapeamento negociado. [Transport quic-go](https://quic-go.net/docs/quic/transport/).

## Política zero-spend e privacidade

**Relay gratuito autorizado é permitido.** Habilitar somente depois de verificar serviço, finalidade, licença/termos, quotas, regiões, autenticação, capacidade TCP/UDP necessária, risco de cobrança e falha. Open source não paga servidores. Não usar créditos temporários, rodízio de contas, relays aleatórios ou VPS obrigatória. Não exigir relay do usuário para o MVP.

A oferta pública Iroh é gratuita, limitada e indicada para desenvolvimento/testes; a recomendação de produção é outra oferta. Assim, pode fundamentar futura avaliação controlada, mas **não aprova relay padrão da distribuição pública**. [Relays Iroh](https://docs.iroh.computer/concepts/relays). Shared/Pro tem mensalidade e excedentes e permanece desabilitado; teste promocional não muda essa decisão. [Shared Relays](https://docs.iroh.computer/iroh-services/relays/shared).

Nenhuma chave administrativa de provedor entra no launcher. No serviço gerenciado Iroh, a chave de projeto é raiz de assinatura; token vinculado a endpoint não torna seguro distribuí-la. A documentação informa que emissão separada de credencial pelo backend ainda não está implementada. Uma solução futura teria de provar fluxo real de emissão curta, autorização, revogação e custos; não inventar API nem colocar a chave em variável de ambiente do computador do usuário como suposta proteção. [API Keys](https://docs.iroh.computer/iroh-services/access).

Sem relay permitido, conservar conexões diretas e reportar o limite. Sem nenhum caminho, falhar claramente. Timeout não diagnostica CGNAT. Conexão direta revela IP aos peers e relay não equivale a anonimato. Autenticação Aurora vincula peer/chave/sessão/convite/permissões com validação criptográfica, expiração, revogação e antirreplay; o túnel deve ser E2E também pelo relay. Não aceitar certificados indiscriminadamente. Não alterar online-mode nem autenticação/criptografia dos mods; tokens Microsoft/Minecraft nunca seguem para peers, relay ou logs. Diagnósticos não contêm áudio, conteúdo dos pacotes, endereços completos ou segredos compartilháveis.

## Consequências, riscos e decisões pendentes

| Decisão pendente | Evidência que a fecha | Tarefa |
| --- | --- | --- |
| Pin final do transporte e toolchain | auditoria de versão/avisos/licenças, build Windows e dependências reproduzíveis | AUR-R4-001 |
| Identidade Aurora ↔ peer e serviço | contrato revisado, ameaças, testes negativos e revogação de sessão ativa | AUR-R4-001/002 |
| Limites UDP e uso de uma ou várias conexões | tamanhos medidos nos mods, MTU, jogo sob carga, isolamento/CPU/RAM | AUR-R4-003/006 |
| Lookup/NAT e relay autorizados em testes | configurações efetivas, operador, termos e evidência de bloqueio de cobrança | AUR-R4-004 |
| Relay gratuito sustentável para produção | autorização de integração/distribuição, quotas/regiões/falha e teste real | AUR-R4-004/007; atualmente **NÃO VERIFICADO** |
| Sinalização final automática | regras por participante, expiração/revogação, budget de operações e plano real da conta | AUR-R4-005 |
| Endereçamento de cada mod, usuários mistos | API/configuração real por versão e testes individuais dedicado/LAN | AUR-R4-006 |
| IPC e encerramento se Go se tornar necessário | protocolo local autenticado/versionado, binários assinados, updater/rollback e processo sem órfãos | nova revisão da decisão, antes de adicionar Go |

Recursos sem homologação permanecem experimentais, desativados ao usuário por padrão. Aprovar TCP não aprova UDP, voz, relay ou R4 inteira. Nenhuma mudança afeta os nove pares Core/Skins sem contrato e evidência na matriz existente; Companion/HUD e assistente in-game continuam retirados pela ADR-018.

## Reversibilidade e aceite desta proposta

O incremento pode ser removido/desabilitado encerrando listeners/streams/associações e revogando sessões, sem alterar mundos. Configurações de mods só mudam por plano/diff, autorização, snapshot e restauração que detecte edição concorrente. O fallback mantém o modo normal de entrar em servidor público e os dados locais. Não publicar infraestrutura ou fazer release como consequência de aprovar a arquitetura.

Aceite documental: política sem proibição geral de relay; TCP/UDP e voz explícitos; fontes datadas e versões separadas de implementação; dependências sem aprovação implícita; tarefas e testes rastreáveis. Aceite técnico futuro: critérios de cada incremento no backlog, respeitando todos os gates anteriores do roadmap. Esta execução só produz documentação.

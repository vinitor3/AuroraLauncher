# Aurora Connect — preparação dos mods de voz

Data: 29/09/2026. Estado do utilitário local: **PROTÓTIPO**. Integração de voz Simple Voice Chat e Plasmo Voice: **NÃO IMPLEMENTADO**; homologação individual: **PRECISA DE TESTE**. Este documento complementa a especificação canônica em `engineering/AURORA_CONNECT_SPEC.md`; não cria um segundo contrato de transporte. O código não está conectado ao desktop nem habilita voz para usuários.

## Incremento executado

`apps/connect-voice-tools/inspect.mjs` é um inspector local e gerador de plano sem aplicação, escrito com módulos internos do Node. Não instala dependências, lê contas, varre instâncias, altera configurações, inicia Minecraft, abre firewall ou se conecta à internet. Recebe somente um caminho explícito; lê no máximo 256 KiB, 4096 linhas, 512 chaves e 128 tabelas. O probe opcional abre apenas UDP `127.0.0.1` e fecha imediatamente, sem enviar datagramas. Seu resultado não reserva a porta para a futura ponte.

O resultado contém SHA-256 e tamanho dos bytes originais, bind/porta host separados do anúncio e listener convidado, diff estruturado apenas dos campos de anúncio, verificação de orçamento de datagrama e impedimentos explícitos. Não altera `mtu_size`, autenticação, criptografia, gravação, codec ou áudio. A saída CLI redige endereços anteriores por padrão; `--show-addresses true` permite revisão local explícita e não deve ser usado em evidência compartilhada. Não há flag de escrita nem executor de mutação.

O parser aceita um subconjunto deliberadamente estrito. Properties aceita atribuições `chave=valor`, comentários completos `#`/`!` e preserva espaços finais do valor. TOML aceita tabelas simples, chaves simples, strings sem escapes, booleanos, números decimais e arrays simples. Recusa duplicações, escapes, continuação, strings multilinha, tabelas repetidas, colisões escalar/tabela e sintaxe desconhecida. Configuração recusada permanece intacta; isso não significa que o mod a considere inválida. Nenhuma regex reescreve um TOML inteiro.

As versões `2.6.22` (SVC) e `2.1.17` (Plasmo) limitam os rascunhos a referências pesquisadas, **não são uma lista de compatibilidade runtime**. Versão declarada não comprova JAR, loader ou metadados instalados. O estado sempre mantém `runtimeHomologated=false`, `applySupported=false` e `artifact_metadata_hash_and_runtime_not_verified`. A observação `installation=absent` bloqueia rascunho; o utilitário não finge descobrir instalação por nome de arquivo. Versões desconhecidas, LAN e modo misto também não produzem alteração de anúncio.

Somente `dedicated-aurora-only` pode produzir o rascunho global loopback. Esse rascunho exige uma ponte ativa na mesma porta local em **cada** máquina participante, inclusive no cliente do anfitrião. Não funciona para convidados diretos sem ponte; não permite portas efêmeras diferentes por convidado nem resolve duas sessões na mesma máquina usando uma única porta. Colisão exige outra organização de portas e novo plano ou adaptador por participante; nunca substituição silenciosa.

## Pesquisa oficial e versões

Consulta efetuada pelas ferramentas web e APIs públicas oficiais em 29/09/2026. Nenhum JAR foi baixado, instalado, redistribuído ou executado neste incremento. Hash publicado pelo distribuidor é metadado esperado, não hash recalculado de arquivo local.

| Item | Evidência verificada | Limite |
| --- | --- | --- |
| SVC API | Índice público declara API `2.6.20`; `VoiceHostEvent` tem `getPlayer()`, `getVoiceHost()` e `setVoiceHost(String)` | Não fixa artefato mínimo de cada loader nem comprova LAN. [API](https://voicechat.modrepo.de/), [evento](https://voicechat.modrepo.de/de/maxhenkel/voicechat/api/events/VoiceHostEvent.html) |
| SVC artefato candidato | Modrinth oficial `VS5X7jZd`, `voicechat-fabric-1.21.1-2.6.22.jar`, release publicada em 08/08/2026, 5.369.475 bytes | Candidato dentro do par Aurora Fabric 1.21.1/Java 21. API e bytecode desse JAR ainda não auditados. [Metadados oficiais](https://api.modrinth.com/v2/version/VS5X7jZd) |
| SVC código observado | Branch 1.21.1 no commit `c5b7d9bba087d6c5f4aaf62394d2a06c50ee8fb7` declara `1.21.1-2.6.24` beta, Java 21; licença raiz **All Rights Reserved** | Não trocar o candidato release por beta implicitamente; não inferir licença irrestrita de distribuição. [Propriedades](https://raw.githubusercontent.com/henkelmax/simple-voice-chat/c5b7d9bba087d6c5f4aaf62394d2a06c50ee8fb7/gradle.properties), [licença](https://raw.githubusercontent.com/henkelmax/simple-voice-chat/c5b7d9bba087d6c5f4aaf62394d2a06c50ee8fb7/license) |
| Plasmo API/release | Tag `2.1.17`, commit `2b4d22cf9333bc32dad183445b28a267cbf8145f`, publicada em 15/09/2026; API versionada acessível | O índice sem versão já mostra `2.2.0-SNAPSHOT`; ele não é usado como contrato 2.1.17. [Release](https://github.com/plasmoapp/plasmo-voice/releases/tag/2.1.17), [API exata](https://dokka.plasmovoice.com/older/2.1.17/index.html) |
| Plasmo artefato candidato | `plasmovoice-fabric-1.21.1-2.1.17.jar`, 12.365.966 bytes; digest publicado SHA-256 `46e1a4ae51786457712991ba05bf573e2d11331aeadb63b7d108839def022a2b` | Sem arquivo local/verificação/runtime; licença raiz LGPL-3.0 lida na tag, obrigações do artefato/add-on ainda exigem revisão própria. [Release API](https://api.github.com/repos/plasmoapp/plasmo-voice/releases/tags/2.1.17), [licença](https://raw.githubusercontent.com/plasmoapp/plasmo-voice/2.1.17/LICENSE) |

SHA-512 publicado do candidato SVC: `c44e69400ac7349ad6d4699263bde4d87359e03233c31a872eb02542658cfc73a185b46e4361c54cb1ac029a2991bd8eb4d4ea59b1c3e3b43f234087520b325a`. SHA-256 local de ambos permanece **NÃO VERIFICADO**. Fabric 1.21.1 já existe entre os nove pares; não houve alteração ou ampliação da matriz, Core `1.1.5` ou Skins `0.3.1`.

### Simple Voice Chat

O arquivo documentado para Fabric/Forge é `config/voicechat/voicechat-server.properties`. `port`, `bind_address` e `voice_host` são opções documentadas do dedicado. `port=-1` depende da porta Minecraft efetiva; o inspector só a resolve se recebê-la explicitamente. `bind_address` vazio depende de `server-ip`, igualmente não inventado. `voice_host` é anúncio para o cliente, não bind. A referência `mtu_size=1275` corresponde a áudio e não comprova tamanho integral do UDP. [Configuração oficial](https://modrepo.de/minecraft/voicechat/wiki/server_config)

O guia de proxy genérico exige anúncio para o proxy. Plugins Velocity/Bungee têm regras próprias que não transformam o agente Aurora em proxy Minecraft. [Guia oficial](https://modrepo.de/minecraft/voicechat/wiki/proxy_setup)

`VoiceHostEvent.getPlayer()` e `setVoiceHost()` constituem um ponto de extensão candidato para mapear jogador autorizado ao listener de sua máquina. Antes de usá-lo: fixar JAR/API exatos, provar evento no dedicado e LAN separadamente, autenticar associação jogador–sessão–peer e remover override ao terminar. Nenhuma alteração de Core ou add-on ocorreu aqui. [Evento oficial](https://voicechat.modrepo.de/de/maxhenkel/voicechat/api/events/VoiceHostEvent.html)

### Plasmo Voice

`[host]` determina bind e porta; `port=0` usa o número da porta Minecraft, continuando UDP separado. `[host.public]` anuncia destino. A documentação de instalação cita `config/plasmovoice/config.toml`, enquanto troubleshooting também menciona `server.toml`: o arquivo ativo precisa ser confirmado no artefato/runtime. Por isso o inspector exige caminho explícito e não escolhe pelo nome. [Endereçamento](https://plasmovoice.com/docs/server/advanced/), [instalação](https://plasmovoice.com/docs/server/installing/), [troubleshooting](https://plasmovoice.com/docs/server/not-installed/)

O fonte `VoiceServerConfig` 2.1.17 separa `Host`/`Public`, documenta herança dos valores públicos zero e seu uso distinto em proxies, e define `voice.mtu_size=1024`, faixa 128–5000. Esses valores não são instalados pelo inspector nem substituem medição de pacotes completos. [Fonte versionada](https://raw.githubusercontent.com/plasmoapp/plasmo-voice/2.1.17/server/common/src/main/java/su/plo/voice/server/config/VoiceServerConfig.java)

A API cliente `UdpClientConnectEvent` permite substituir `UdpClient` antes da conexão. `UdpClient.connect(String,int)` é concreto; `ModServerConnection` chama o evento, pega o cliente substituído e só então chama `connect`. Isso permite pesquisar um wrapper local que altere destino sem reimplementar áudio. Deve delegar o cliente original, lifecycle e eventos corretamente; simples forwarding de métodos pode perder assinantes registrados no cliente original. Não está implementado ou homologado. `ConnectionPacket` expõe getters de secret/IP/porta, **não** um setter de anúncio por jogador. O secret permanece no mod e nunca é enviado ao launcher/relay/log. [Evento](https://raw.githubusercontent.com/plasmoapp/plasmo-voice/2.1.17/api/client/src/main/java/su/plo/voice/api/client/event/socket/UdpClientConnectEvent.java), [interface](https://raw.githubusercontent.com/plasmoapp/plasmo-voice/2.1.17/api/client/src/main/java/su/plo/voice/api/client/socket/UdpClient.java), [chamada real](https://raw.githubusercontent.com/plasmoapp/plasmo-voice/2.1.17/client/src/main/java/su/plo/voice/client/connection/ModServerConnection.java), [packet](https://raw.githubusercontent.com/plasmoapp/plasmo-voice/2.1.17/protocol/src/main/java/su/plo/voice/proto/packets/tcp/clientbound/ConnectionPacket.java)

## Recuperação preservada

Leitura integral realizada de `instance-recovery.md`, `diagnostic-lab.md`, `aurora-ai-tools.md`, ADR-025/027/028 e task files `task-ai-tools.md`, `task-aur-1166-instance-recovery.md`, `task-aur-1167-doctor-lab.md`, além do prompt consolidado, AGENTS, SECURITY e core-ipc. Os resultados desses task files são **evidência histórica**, não testes repetidos neste incremento.

Inspeção estática de `engine/ai_tools.rs` confirma que `SetSetting` usa `serde_json` e um pointer de allowlist. Não é editor properties/TOML. `engine/recovery.rs::preview_selected_files` confere guard, fingerprint, escopo protegido, hashes e filtros de configuração sensível; é candidato ao fluxo nativo futuro, não uma permissão de escrita por si só. `diagnostic_lab.rs` expõe somente alterações suportadas pela allowlist existente.

Próximo executor de voz deve integrar identidade física da instância, jogo encerrado, lock comum, plano e diff locais, autorização específica, revalidação do SHA-256, snapshot antes da escrita, journal durável, retomada após crash e restauração que recuse sobrescrever edição posterior. Não ampliar snapshots para `saves/**/serverconfig`, mundos, segredos ou módulos administrados. O utilitário atual não chama recovery e não promete rollback de algo que não alterou.

## Demonstração reproduzida e testes

Diretório: `C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-r4-connect`. Windows, Node `v24.8.0`. Comandos realmente executados:

```powershell
node --test apps/connect-voice-tools/inspect.test.mjs
node --check apps/connect-voice-tools/inspect.mjs
git diff --check
node apps/connect-voice-tools/inspect.mjs --mod svc --version 2.6.22 --mode dedicated-aurora-only --config apps/connect-voice-tools/fixtures/svc.properties --guest-port 35554 --datagram-budget 1400 --transport-payload 1450 --probe-port true
node apps/connect-voice-tools/inspect.mjs --mod plasmo --version 2.1.17 --mode dedicated-aurora-only --config apps/connect-voice-tools/fixtures/plasmo.toml --guest-port 35555 --game-port 25565 --datagram-budget 1200 --transport-payload 1450 --probe-port true
```

Resultado: **14/14 testes aprovados**, nenhum ignorado; syntax check e diff check aprovados. As demonstrações produziram planos redigidos, `available_at_probe`, anúncio SVC `127.0.0.1:35554`, anúncio Plasmo `127.0.0.1:35555` e impedimento de artefato/runtime não verificado. 1400/1200/1450 são **entradas sintéticas de teste**, não medições do transporte/mod.

Fixture SVC: 155 bytes, SHA-256 `9eedfdbad4ea8d1b02213365e4f84d205c0852d88763dcc698c2a83c9dfc6c66`. Fixture Plasmo: 216 bytes, SHA-256 `0d0148a57b2f8746f5bc9764eafbf15ed50fa0fb03dd32aa6d9e9d6191a33509`. Teste em diretório temporário compara bytes antes/depois da CLI. Probe real testa porta ocupada e dois binds sucessivos após liberação. Erros da CLI não imprimem caminhos ou mensagem bruta do filesystem. Fixture representa chaves reais, com dados sintéticos; nenhum mod rodou.

Cobertura: separação de endereços, herança explícita de porta, config incompleta, ausência declarada do mod, versão desconhecida, LAN/misto bloqueados, orçamento insuficiente, MTU nunca reduzido, duplicações/sintaxe ambígua/UTF-8/tamanho/limites recusados, hash mudando após edição, preservação de bytes e redação de diagnóstico. O encerramento é automático após imprimir JSON; sockets e handles já estão fechados. Não há processo de fundo, configuração aplicada ou restauração pessoal necessária.

## Gates restantes, por mod

| Verificação | SVC | Plasmo |
| --- | --- | --- |
| Inspector de configuração e plano sintético | **PROTÓTIPO**, testado | **PROTÓTIPO**, testado |
| Artefato local/hash/metadados/licença exatos | **PRECISA DE TESTE** | **PRECISA DE TESTE** |
| Transporte UDP real do mod, autenticação e retorno | **PRECISA DE TESTE** | **PRECISA DE TESTE** |
| Endereçamento de dedicado só Aurora | Rascunho global, sem aplicação/runtime | Rascunho global, sem aplicação/runtime |
| LAN | **NÃO VERIFICADO**, sem override | **NÃO VERIFICADO**, sem override |
| Misto direto/Aurora e várias instâncias | **NÃO IMPLEMENTADO**, requer adaptador | **NÃO IMPLEMENTADO**, requer adaptador |
| Mutação confirmada, restauração concorrente e crash | **NÃO IMPLEMENTADO** neste adaptador | **NÃO IMPLEMENTADO** neste adaptador |
| Voz audível consentida, sem gravação, sob carga do jogo | **PRECISA DE TESTE** | **PRECISA DE TESTE** |

O ensaio audível futuro executa cada mod em separado: anfitrião com dedicado descartável, convidado com cliente descartável do mesmo artefato/par, autenticação oficial, serviço UDP autorizado e ponte loopback em cada participante. Registrar handshake próprio do mod e confirmação humana bidirecional, sem gravar áudio; repetir sob carga, porta ocupada, desconexão, revogação e encerramento. Status TCP ou echo UDP não demonstram voz. O procedimento com comandos de jogo e ponte só pode ser fechado depois de fixar runtime/artefatos/adaptador; não há comandos fictícios nesta documentação.

Até esses gates, recursos de voz permanecem experimentais e desligados. A indisponibilidade de teste audível não impede implementar os adaptadores e o executor local com fixtures; nenhuma intervenção humana foi exigida por este incremento. Nenhum push, deploy, release, instalador, cobrança ou alteração pessoal foi efetuado. Rollback deste incremento consiste em retirar o utilitário/documento da branch; nenhum estado externo depende deles.

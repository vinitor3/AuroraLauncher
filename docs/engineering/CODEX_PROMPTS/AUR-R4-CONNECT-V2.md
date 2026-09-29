# Prompt executável — Aurora Connect V2

Copie o conteúdo a partir de “Tarefa” para um chat de implementação do Codex. Este prompt autoriza uma implementação incremental local; a revisão documental que o produziu não executou esse código. Não depende da conversa original.

## Tarefa

Atue como engenheiro responsável pelo primeiro incremento seguro do Aurora Connect V2. Audite o código real, reconcilie a documentação, refine a decisão e implemente uma mudança pequena, efetiva e verificável. Não entregue apenas sugestões. Preserve o produto existente e encerre com evidências, limitações e próximo bloqueio real.

### 1. Base, fontes e trabalho local

O repositório Aurora é `C:/Users/usuario/Desktop/AuroraLauncher`. Não use `C:/Users/usuario/Desktop/TESTE-main/aurora-skins-runtime`, que é histórico. Na revisão de 29/09/2026 o checkout principal ainda apontava a `dc2ec5a`/alpha.13, mas a versão atual **alpha.14** estava comprovada em `C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-1167-doctor-lab`, HEAD `6a107b87adc4882bd464909e22650185f862ea5e`; implementação/versão `89211c735093d2a310eb5ef60eb2c10f33a4c61e`, incluindo recuperação `be3a1c4`. Não comece de alpha.13 deixando entregas posteriores para trás. Reconfirme todos os SHAs/versões atuais; esses valores não fixam versões futuras.

A revisão V2 foi produzida na branch `codex/aur-r4-connect-v2-review`, worktree `C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-r4-connect-v2-review`, baseada na alpha.14. Se seus documentos ainda não estiverem integrados no checkout escolhido, leia essa cópia e integre somente os documentos concluídos relevantes sem substituir arquivos novos por versões antigas. Há outro chat/branch de revisão (`codex/aur-r4-connect-v2-docs`): conferir ownership e divergências, não mesclar documentos concorrentes automaticamente nem sobrescrever trabalho em andamento.

Antes de editar:

1. Leia o `AGENTS.md` do repositório e os aplicáveis ao caminho. Execute `git status --short`, `git rev-parse HEAD`, `git branch --show-current`, `git log -5 --oneline` e `git worktree list --porcelain` no repositório correto. Registre alterações preexistentes sem expor conteúdo sensível.
2. Leia integralmente `docs/engineering/MASTER_ROADMAP.md` e `docs/task-aur-r4-connect-v2-review.md`, `docs/engineering/AURORA_CONNECT_SPEC.md`, `docs/engineering/AURORA_CONNECT_DEPENDENCIES.md`, `docs/engineering/ADR/PROPOSAL-aurora-connect.md`, `docs/engineering/CODEX_TASKS/AUR-R4-CONNECT-BACKLOG.md`. Leia a matriz, planos paralelo/integração, `SECURITY.md`, ADR-010 com ressalva V2 e ADR-018. Referência não equivale a leitura; registre o que de fato abriu.
3. Leia manifests/lockfiles reais: `package.json`, `apps/desktop/package.json`, `apps/desktop/package-lock.json`, `apps/desktop/src-tauri/Cargo.toml`, `apps/desktop/src-tauri/Cargo.lock` e `apps/desktop/src-tauri/tauri.conf.json`. Confirme scripts e dependências; Quinn transitivo não prova feature de rede.
4. Leia `scripts/active-mod-matrix.json` e `scripts/skins-feature-contract.json`, `docs/core-ipc.md`, `docs/instance-recovery.md`, `docs/diagnostic-lab.md`, `docs/aurora-ai-tools.md`, ADR-025/027/028 e task AUR-1167. Inspecione módulos relevantes e consumidores sob `apps/desktop/src-tauri/src/`: `engine/mod.rs`, `engine/ipc.rs`, `engine/launch.rs`, `launcher_sessions.rs`, `commands.rs`, registro Tauri, recuperação e autorização de instância. Faça busca de transporte existente antes de criar um novo. Não declare uma auditoria integral sem realizá-la.
5. Trabalhe em branch `codex/<id>-<slug>` e worktree exclusiva com base atual reconciliada. Use ferramenta gerenciada de worktree se puder operar o repositório correto; fallback Git só se indisponível. Preserve mudanças locais, trabalho de outros agentes e releases. Sem reset destrutivo, clean forçado, force checkout, push, PR/publicação/deploy/instalador ou faturamento nesta tarefa. Não alterar tags/artefatos históricos. Um owner por `App.tsx`, `commands.rs`, `edge-proxy/src/index.ts` e manifests compartilhados.

Classifique separado: requisitos novos, texto original, proposta, código observado, teste executado e pendência. Use os estados do AGENTS: CONCLUÍDO, PARCIAL, PROTÓTIPO, IMPLEMENTADO MAS NÃO HOMOLOGADO, NÃO IMPLEMENTADO, LEGADO, RISCO, HIPÓTESE, NÃO VERIFICADO, PRECISA DE TESTE. Build/mock não é homologação.

### 2. Preservação obrigatória

R0 segurança/Firestore/CI/licença/proveniência e distribuição pública permanecem gates. R1 preserva Assistente desktop, modularização, limites HTTP/IPC e homologação dos nove pares Core/Skins. A matriz lida na alpha.14 tem Core 1.1.5 e Skins 0.3.1: Forge 1.12.2 e Fabric/Forge 1.16.5, 1.19.2, 1.20.1 e 1.21.1; confirme loader/Java na matriz, sem ampliar ou inventar homologação. Companion/HUD foi retirado pela ADR-018; nenhum adaptador de rede autoriza reintroduzi-lo ou criar Assistente no jogo.

Preserve updater reversível R1.5, snapshots e configurações, download CurseForge somente permitido/oficial, diferença implementação/runtime. Doctor determinístico continua somente leitura; ações confirmadas são outro fluxo. **Já existem** na alpha.14 recuperação manual com CAS SHA-256/SQLite limitado, prévia, proteção anterior, journal/desfazer; observações por tentativa; cópias independentes de diagnóstico e aplicação seletiva; Tools confirmadas e histórico. Não refazer nem remover esses recursos. Mundos/segredos/módulos administrados não entram automaticamente no recovery; GC, migração geral de executores e pontos automáticos gerais ainda exigem trabalho próprio.

R2 mantém restore bit a bit e GC seguro, R2.5 plano/diff/confirmação/auditoria. R3 mantém manifesto/Creator; R4 mantém convites autorizados e sync por manifesto, R5 produção/privacidade/zero-spend. A PoC isolada pode investigar rede sem lançar o produto; não libera distribuição nem sync antes dos gates. Corrija documentação obsoleta de forma rastreável, sem apagar evidências históricas úteis nem confundir falta de referência com arquivo inútil.

### 3. Resultado do produto e limites

Aurora Connect é nome de trabalho. Queremos compartilhar servidor/mundo para convidados entrarem com pouca configuração e jogo/serviços TCP/UDP homologados, inicialmente Simple Voice Chat e Plasmo Voice. Minecraft continua autoritativo; P2P é conectividade. Host permanece disponível; hospedagem 24 horas está fora.

Separe servidor público já acessível (conexão normal), servidor privado com agente autorizado, dedicado local e singleplayer aberto para LAN. Transporte, descoberta de porta, abrir LAN, auto-join e sync são capacidades distintas. Não prometa acesso a servidor privado sem responsável. Fluxo final sem VPN externa manual, driver, admin permanente ou portas manuais. Configuração explícita de laboratório é permitida para PoC e deve ser rotulada como tal.

### 4. Relay e tecnologia: política corrigida

Priorize direto; **relay gratuito é permitido como alternativa verificada**, não há veto geral a candidatos relayed. Não force falha se houver relay gratuito autorizado disponível. Uma biblioteca pode iniciar por relay autorizado e tentar direto em paralelo. Não prometer migração sem interrupção antes de medir.

Não contratar, habilitar billing, usar custo obrigatório/automático, VPS obrigatória, créditos temporários, rotação de contas, burla de limites ou relays aleatórios. Código open source não é infraestrutura grátis. Para cada dependência externa registre operador, finalidade, autenticação, licença aplicável, termos, quotas, regiões, limites, cobrança e falha, URL oficial/data. Serviço com excedente fica desabilitado sem bloqueio efetivo comprovado de cobrança; alerta, cap não garantido ou contador local não bastam. Não distribuir segredo global/admin no launcher; emissão segura de credenciais individuais requer mecanismo real e dependências verificadas.

Avalie Iroh primeiro sem decisão antecipada. A pesquisa de 29/09 observou 1.3.0 em latest; relays públicos gratuitos de desenvolvimento/hobby sem SLA e limites numéricos não confirmados **não são autorização de distribuição**. Termos integrais ainda precisam de verificação. Oferta paga/trial não atende. Desabilite defaults externos e portmapper na PoC local até autorização técnica aplicável; não presuma que o modo direto não usa descoberta auxiliar.

Compare com Quinn + descoberta/traversal compatíveis. Pion ICE + quic-go só em processo auxiliar se vantagem concreta: provar reaproveitamento do caminho/socket negociado e demultiplexação de leitura, além de build, assinatura, updater, IPC e encerramento. Não implementar todas alternativas nem adicionar Go automaticamente. Não impor ICE/STUN externo a Iroh. Fixe versão após conferir MSRV, plataformas, licença e APIs correspondentes; nunca copiar latest para lockfile antigo.

Avalie LAN, IPv6 com firewall, IPv4 alcançável, hole punching e PCP/NAT-PMP/UPnP opcionais com consentimento, duração e limpeza. Não garantir CGNAT. UDP externo bloqueado requer relay realmente alcançável; no Iroh pesquisado há HTTPS/WebSocket TCP443, sujeito a bloqueio e retenção por retransmissão, afetando áudio. Não prometer ping equivalente. Preserve comparação e4mc/World Host pelo escopo TCP/UDP/custo; e4mc tem proposta direta Dialtone, mas isso não comprova voz. Relay próprio obrigatório permanece adiado. Sem caminho direto/relay permitido, informe falha clara, nunca mude para pago.

### 5. Contrato mínimo TCP/UDP e segurança

Separe identidade/autorização, sinalização, descoberta/traversal, transporte, registro de serviços, integração e UI. Evite monólito/framework desnecessário. Caminho **novo proposto**: `apps/desktop/src-tauri/src/engine/connect/`, a confirmar após auditoria; testes **novos propostos** no módulo e/ou `apps/desktop/src-tauri/tests/connect_*.rs`.

TCP: Minecraft/mod → listener loopback convidado → stream confiável autenticado → agente host → endpoint TCP aprovado. Um TCP corresponde a um stream e conexão host próprios; status e jogo independentes. Preservar bytes/ordem/integridade/bidirecionalidade/backpressure/half-close/cancelamento. Nada de VPN improvisada ou bytes TCP em UDP sem confiabilidade. Auth e serviço aprovados antes de abrir destino/encaminhar bytes.

UDP: mod → listener UDP loopback convidado → DATAGRAM/equivalente autenticado → socket host ligado ao endpoint autorizado. Respostas retornam ao mesmo participante/sessão/serviço/associação/generation. Padrão inicial: socket UDP conectado **por associação** no host; socket compartilhado só com demultiplexação comprovada. Preserve fronteira/conteúdo, sem concatenar/truncar, sem prometer ordem. Escolher QUIC não captura UDP do mod automaticamente. Áudio não usa o stream confiável ordenado do jogo por padrão.

Proponha registro versionado pequeno, confirmando nomes existentes: ID, TCP/UDP, endpoint host, permissões, limites, capability e adaptador. Convidado solicita ID aprovado, jamais IP/porta arbitrário. Manifesto é dado não confiável. Não criar SOCKS/proxy aberto/acesso geral à LAN/execução remota/RCON/SSH/bancos/painéis.

Use limites explícitos de sessões, streams, associações, memória, tamanho, frequência/banda e expiração. Spec propõe laboratório de 1 sessão/8 convidados, 8 serviços, 16 streams/peer e 64 totais, 16 associações UDP/peer e 128 totais; valide/ajuste com testes, não como promessa de capacidade. Filas TCP limitadas/backpressure; voz curta com descarte de vencidos; outros protocolos UDP têm política própria. Congestionamento/criptografia ficam com bibliotecas mantidas.

Negocie suporte DATAGRAM, meça limite dinâmico e subtraia enquadramento (24 bytes na proposta v1; validar antes de fixar wire format). Defina B como bytes disponíveis ao envelope, H como cabeçalho e S como teto de payload do serviço: exigir B >= H antes de usar min(S, B-H), respeitando o comprimento serializado e família UDP. Datagramas dos mods podem exceder MTU útil; medir tamanho integral sem guardar áudio, tratar redução de MTU/troca de caminho. Primeiro incremento sem fragmentação; declarar incompatibilidade/erro explícito em oversize. Fragmentação futura exige desenho autenticado, bounded, timeout de remontagem e testes de perda/abuso; nunca truncamento silencioso. Teste contenção jogo/voz e filas; conexão compartilhada é hipótese, não obrigação.

Vincule chave/identidade peer, sessão, convite e serviços com validação criptográfica, expiração, revogação e proteção replay. Pin confiável ou cadeia validada; nunca aceitar qualquer certificado. E2E inclusive relay; autenticação não basta sem confidencialidade. Não expor token Microsoft/Minecraft/Firebase a peer/relay/log. Sessões independentes do launcher permanecem isoladas. Loopback sozinho não autentica processo local.

Sinalização só metadados mínimos por participante, acesso limitado, expiração lógica e retenção com rules testáveis. Firestore/Worker apenas após contratos/plano gratuitos conferidos; emulador/manual é suficiente na PoC, não UX automática. Nada de voz/jogo/modpack em Firestore; sem novo backend legado por conveniência. Não registrar/analisar áudio. Redigir endereços/segredos em diagnósticos; direto não garante privacidade de IP.

### 6. Compatibilidade de voz é parte de R4

Não criar voice chat próprio. SVC e Plasmo ficam explicitamente no backlog e na arquitetura, mesmo que primeiro incremento só transporte bytes. Homologar separadamente, sem exigir ambos na mesma instância. Fixar Minecraft, loader, mod cliente/servidor, API e hashes; porta padrão/nome do mod não provam suporte. TCP/UDP de mesmo número são serviços distintos.

Resolver duas partes: encaminhar pacotes e informar destino ao cliente. Separe bind host, endereço anunciado e ponte loopback individual. SVC: investigar `voice_host`, configuração dedicada versus LAN e `VoiceHostEvent.getPlayer()/setVoiceHost()` na API efetiva; Plasmo: `[host.public]` e evento `UdpClientConnectEvent` da API 2.1.17 pesquisada como candidato cliente. Essas APIs não foram integradas/homologadas no Aurora e não devem ser presumidas em toda versão. A API SVC pesquisada é variável; fixá-la para o alvo permitido.

Não mudar globalmente para 127.0.0.1 sem ponte correspondente em cada participante. Diretos+Aurora precisam anúncio por participante/API cliente comprovada ou restrição explícita do modo misto. Descobrir porta real, reservar listener sem corrida, lidar com conflitos e várias instâncias. Configuração reversível/API oficial primeiro; hook mínimo no Core só justificado com contrato local e matriz afetada, sem Companion e sem imposição à PoC inicial. Não reescrever mensagens no TCP Minecraft cifrado.

Preserve autenticação, secrets e criptografia nativas dos mods e online-mode. Documente efeito do IP da ponte; não invente identidade encaminhada aceita pelo servidor. Configuração usa plano/diff, autorização, snapshot, revalidação, escrita transacional e restore por comparação: edições concorrentes bloqueiam restauração automática. Não ampliar silenciosamente recovery para configs excluídas. Não instalar/redistribuir mod ignorando licença/proveniência/updater/CurseForge.

### 7. Execute o menor incremento que avance

1. Termine inventário AUR-R4-001 e registre decisão provisória, ameaça, dependências e limites do experimento. Não espere arquitetura completa para realizar um incremento local seguro quando não existir bloqueio material.
2. Se não há transporte, implemente a menor ponte **TCP autenticada e cifrada** entre dois agentes/processos/endpoints de teste, destinos allowlisted e chave esperada. Use configuração explícita local, sem nuvem/faturamento e desabilitada por padrão. Não abra listener wildcard nem use certificado aceito indiscriminadamente. Não adicionar UI/Java/voz se não necessários ao teste.
3. Deixe UDP concretamente especificado e testável: tipos/contrato de sessão-serviço-associação, enquadramento, limites/erros/cancelamento e cenários automatizáveis; se útil no incremento, testes de validação de contrato. Não simular êxito de voz nem dizer UDP completo por haver enum. Prove fronteiras que permitem o incremento UDP seguinte.
4. Se TCP já funciona com evidência, não refaça. Avance para bloqueio real: UDP bidirecional e retorno isolado, teste simultâneo, endereçamento do mod ou falha de auth/limite comprovada. Ajuste IDs/escopo sem apagar obrigações das tarefas restantes.
5. Nenhum teste externo fora de ambiente autorizado/zero-spend. Nenhum deploy, alteração de rules em produção, port mapping sem consentimento, instalador ou publicação. Impedimento real: preserve o que ficou verificável, detalhe a lacuna e próximo teste; não substitua autenticação por mock de sucesso.

### 8. Testes e comandos

Primeiro confirme os comandos dos manifests e testes existentes. Comandos reais presentes na base (rodar apenas os pertinentes ao incremento e todos os exigidos antes de integração):

```powershell
npm --prefix apps/desktop run build
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
npm --prefix apps/edge-proxy run check
npm --prefix apps/edge-proxy run test
npm --prefix functions run lint
npm run core:build
npm run skins:build
npm run release:verify-mods
git diff --check
```

`edge-proxy check` é dry-run na base lida; reconfirme antes de executar. Não rodar `deploy`, `desktop:build` ou `release:package` nesta tarefa. Builds Java são sequenciais quando necessários. Dependência nova exige lockfile/licença/MSRV e comando real reprodutível. Não inventar `npm test` global nem declarar sucesso de comando não executado. Firestore tests requerem emulador configurado, nunca projeto vivo por engano. Registre nomes exatos de testes novos e seus comandos após criá-los.

Cobertura de aceite do incremento: integridade TCP bidirecional e payloads maiores que buffer, múltiplos streams/status/jogo, slow reader, encerramento/cancelamento, auth negativa/certificado errado, convite replay/expirado/revogado quando implementado, destino fora da allowlist e zero encaminhamento antes de autorização. UDP quando implementado: datagramas vazios/limites/oversize, retorno A≠B, várias instâncias, perda/reordenação/duplicação, redução MTU, saturação, limpeza/expiração e TCP+UDP juntos. Restaurar configurações com divergência e conflito de portas nos adaptadores.

Plano restante obrigatório: cada mod isolado, dedicado/LAN suportados, participantes mistos se anunciados; relay ausente/quota, IPv4/IPv6, NAT duplo/CGNAT, UDP bloqueado, redes distintas BR–BR/BR–EUA. Medir conexão/RTT/jitter/perda/CPU/RAM/banda/carga/estabilidade com método e amostras; áudio e TPS/MSPT são métricas distintas, sem produtor falso. Não inventar taxa de sucesso/ganho de ping. Loopback/LAN/mock não provam NAT; TCP aprovado não aprova UDP/voz/relay/R4.

### 9. UX e relatório final

Quando houver UI, eventos vêm do backend: preparar → sessão → convidado → transporte → validar serviços → entrar. Mostre transporte, caminho selecionado, servidor e voz separadamente. Handshake túnel não é jogador no mundo; IPC conectado não é runtime de jogo; UDP recebido não é voz audível. Falha da voz preserva jogo. Informar mod ausente/versão não homologada/porta ocupada/serviço ausente/credencial inválida/quota/falha de conectividade apenas com evidência. Timeout não diagnostica CGNAT.

Relatório em português, autossuficiente:

- Base/HEAD/branch/worktree, versões reais e mudanças locais preservadas; o que foi lido e limites da auditoria.
- Inventário do que já existia, decisão/alternativas e motivo do incremento escolhido.
- Arquivos alterados e comportamento antes/depois; caminhos novos identificados; nenhuma feature futura apresentada pronta.
- Comandos/testes executados, resultados e logs redigidos; testes não executados e motivo. Separar local, runtime, redes externas e homologação.
- Segurança/custos/dependências/termos verificados; capacidades experimentais desligadas; riscos e limites.
- Rollback concreto sem apagar dados do usuário; efeitos de desligar a feature.
- Atualização do task file, roadmap/matriz/ADR pertinentes, IDs rastreáveis e próximo incremento que realmente falta. Não abrir nova numeração ADR sem conferir colisão.
- SHA se houver commit local, sem push; nenhuma publicação, contratação ou cobrança.

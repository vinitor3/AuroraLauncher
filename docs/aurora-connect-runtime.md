# Aurora Connect — laboratório descartável de Minecraft

Estado: **PROTÓTIPO**, separado do launcher/UI e sem distribuição. Alvo existente na matriz: Minecraft **1.21.1**, Fabric **0.16.14**, Java **21**, protocolo **767**. Nenhum par Core/Skins é acrescentado ou alterado. A prova inicial é STATUS/pong por Host/Guest autenticados; não comprova login, entrada no mundo ou voz. SVC/Plasmo terão provas separadas depois disso.

## Preparação implementada

`apps/connect-runtime-tools/runtime.mjs` usa somente APIs nativas do Node, sem npm install ou atualização de lock. `plan` lê o `release` e `bin/java.exe` do JDK explicitamente selecionado, sem executá-los, e devolve JSON/hash do plano. Exige Windows/x86_64, versão 21, marcador `MZ`, limites de leitura e ausência observada de links/hardlinks. O marcador não valida integralmente o formato PE. Hash do executável e do release detecta mudança desses arquivos; não é assinatura do fornecedor nem validação de todo o JDK. O JDK local continua sendo uma dependência confiada pelo operador.

`stage` reconstrói o plano a partir do código fixado e do JDK atual. Exige o hash exato e a declaração explícita de aceitação dos termos pelo usuário **antes de criar pasta ou acessar a rede**. Não carrega URLs/comandos de JSON fornecido. Cria somente uma pasta nova em `.aurora/connect-runtime/prepared-*` nesta worktree; não acessa instâncias pessoais. Os nove downloads totalizam **55.582.631 bytes**. Cada um exige HTTPS no domínio fixado, status 200, sem redirect/credenciais/compressão, tamanho exato e digest publicado. Limites: 90 s por artefato, 180 s totais, sem retry. Só renomeia `.part` após verificação e flush; publica `ready.json` depois de todos os artefatos e nova conferência dos arquivos Java.

O servidor Mojang é fixado pelo SHA-1 publicado; SHA-256 adicional é calculado dos bytes baixados e registrado como observação, sem alegar um SHA-256 independente oficial. Bibliotecas usam SHA-256 publicado. Pins completos e URLs estão em [pins.mjs](../apps/connect-runtime-tools/pins.mjs); [fontes/licenças](engineering/AURORA_CONNECT_DEPENDENCIES.md#runtime-descartável--décimo-quarto-incremento). Falha conserva somente a nova pasta incompleta, com `failed.json` e parciais limitados, nunca `ready.json`; não retoma nem apaga recursivamente. Nenhuma pasta existente é reutilizada. Não é sandbox contra substituição concorrente de diretórios por outro processo do mesmo usuário; manter o laboratório exclusivo durante a operação.

Os arquivos preparados mantêm `eula=false`, loopback `127.0.0.1:43001`, **`online-mode=false` por preferência explícita do usuário**, whitelist ativa, RCON/query/JMX desligados, dois jogadores e mundo novo próprio. Isso desliga a verificação de contas pelo servidor descartável; autenticação e autorização de serviço no Connect continuam obrigatórias. Não altera instâncias pessoais, gates de entitlement/distribuição ou prova compatibilidade entre clientes originais/offline. A porta não é reservada antecipadamente; ocupação deve falhar sem encerrar seu dono. Nenhum mod, Core ou Skins é instalado nesta etapa. Não há regra de firewall, router, relay, serviço, conta ou cobrança criada. Downloads dependem somente dos servidores oficiais; falha não muda para mirror ou plano pago.

## Comandos de preparação

Executar no diretório desta worktree. O usuário declarou ter lido e aceitado EULA/MSA e autorizou os testes locais. Os dois comandos abaixo foram executados no décimo oitavo incremento; não repetir a solicitação de consentimento já resolvida.

```powershell
node apps/connect-runtime-tools/runtime.mjs plan 'C:/Program Files/Eclipse Adoptium/jdk-21.0.12.101-hotspot'
```

Plano observado em 29/09/2026: Java `21.0.12.1`, digest `f6ab6d12656111a7fcea8d490c86b76535885e7278571e15de4610a4f31773c1`, nove artefatos, 55.582.631 bytes. Cópia local ignorada: `.aurora/connect-validation/runtime-eighteen-plan.json`. O digest identifica o objeto `plan` serializado pelo helper, não o envelope de apresentação dessa cópia. Recalcular após qualquer mudança de código/JDK e revisar antes de aprovar. O plano online anterior e seu hash permanecem somente no registro histórico do incremento 14.

Com a aceitação explícita recebida e o plano revisado, foi executado:

```powershell
node apps/connect-runtime-tools/runtime.mjs stage 'C:/Program Files/Eclipse Adoptium/jdk-21.0.12.101-hotspot' f6ab6d12656111a7fcea8d490c86b76535885e7278571e15de4610a4f31773c1 --user-accepted-minecraft-eula
```

Resultado: `runtime_staged`, exit 0, nove artefatos verificados em `.aurora/connect-runtime/prepared-KekiXy`. SHA-256 observado do servidor: `e3bc55693e93cda0188f2e60aea28113fc647c5e85a15fa3d1b347349231b4bb`. Evidência redigida: `.aurora/connect-validation/runtime-eighteen-stage.json`. Esse comando **não executa Java**, não torna `eula=true` e não inicia o servidor. `runtime_staged` indica apenas artefatos verificados e arquivos próprios preparados. O argumento registra uma autorização externa já obtida; sua existência não autoriza o agente a aceitá-la pelo usuário.

## Execução e limites

O plano contém vetor de argumentos revisável para `KnotServer`, oito JARs de classpath explícitos e `-Dfabric.gameJarPath=server.jar`. Fonte fixada confirma que o lado servidor usa o diretório corrente; o executor define cwd para a pasta descartável. O bundler executa código durante a inicialização; não usar Knot como inspeção sem consentimento. `JAVA_TOOL_OPTIONS`, `_JAVA_OPTIONS`, `JDK_JAVA_OPTIONS` e `CLASSPATH` são removidos somente no ambiente do filho. Memória: 256 MiB iniciais, teto 1536 MiB. O ensaio do incremento 18 iniciou esse servidor e observou seu bind/STATUS.

O roteiro de transporte foi testado com fixture independente e depois com o servidor real: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/connect-minecraft-status-smoke.ps1 -GamePort 43001 -SkipBuild`. Usa somente serviço `game/tcp`, dois agentes reais e `guest-tcp`, sem autorizar UDP de aplicação. Confere protocolo 767, status/pong e saída dos agentes; não inicia Java nem encerra o destino. `-SkipBuild` foi usado após conferir o binário já testado; omitir quando precisar recompilar.

Antes de iniciar: verificar novamente os artefatos/configuração, registrar a autorização específica de execução/EULA, confirmar porta livre e espaço/memória, preparar encerramento do filho e trocar apenas o `eula.txt` da pasta própria. Observar bind loopback real; esse bind limita entrada, não garante ausência de conexões externas do servidor. A configuração offline foi expressamente solicitada; não acessar tokens/cache de contas ou alterar configurações pessoais. Para STATUS, usar a [consulta existente](aurora-connect.md#consulta-de-status-de-jogo-pelo-listener-tcp--experimental) através da ponte, além de evidência independente da lease/serviço. Encerrar com `stop` no stdin, aguardar saída e só terminar à força o filho próprio em falha com prazo. Não matar Java por nome/PID encontrado em inventário.

Não há restauração pessoal: a pasta inteira é nova. Conservar evidências redigidas; logs crus do jogo não são relatórios e não devem ser enviados ao chat/Git. Uma limpeza futura só pode remover a pasta exata pertencente ao ensaio, depois de confirmar ausência do processo e o confinamento à raiz. Rollback de código: revert local revisado; nenhum instalador ou publicação.

## Supervisor de processo próprio — décimo sexto incremento

`owned-process.mjs` oferece `withOwnedProcess(options, operation)`, usado explicitamente pelo caller: executável/cwd locais absolutos, vetor de argumentos, `shell=false`, janela oculta e um único filho. Não executa nada ao importar e não aprova EULA/artefato ou atesta readiness. Ambiente fornecido substitui a herança; para Java usar ambiente mínimo sem credenciais. Quando omitido, copia o ambiente atual. Em ambos os casos remove as quatro variáveis de injeção Java de qualquer casing, sem alterar o ambiente global. Recusa lexicalmente UNC/devices/ADS, sem consultar a rede; validação de links/artefatos continua responsabilidade do caller.

Prazo global padrão 60 s (máximo uma hora), stop padrão/máximo 5 s, saída stdout+stderr padrão 1 MiB (máximo 16 MiB), até 256 argumentos/64 KiB. Drena ambos os pipes sem guardar conteúdo. A operação recebe AbortSignal; ao terminar, o helper envia `stop\n`, aguarda saída e evento `close` (pipes fechados). Prazo de stop excedido termina somente o objeto ChildProcess criado e devolve erro com `forced=true`; nunca PASS normal. Falha de operação, saída precoce, cancelamento, flood e erro de spawn permanecem falhas mesmo se cleanup sair com zero. Deadline inclui startup/operação/stop, mas o recolhimento pelo sistema pode terminar depois; não relata close antes de observá-lo. Erros têm código fixo e contagens, sem conteúdo, PID, caminho, argumentos ou causa bruta.

**18/18 testes** com filhos Node próprios cobrem sucesso/erro, deadline inclusive durante stop, cancelamento antes/durante/imediatamente após spawn, stdout/stderr/limite agregado, EPIPE, callback tardio, ambiente e preservação de outro filho concorrente. A API não supervisiona descendentes nem consegue interromper JavaScript arbitrário: callback deve cooperar e não criar trabalho solto. Fonte oficial consultada: [ChildProcess, close e stdio](https://nodejs.org/api/child_process.html#event-close). Isso prepara lifecycle, mas não conecta automaticamente Java, EULA, artefatos, configuração e status em um executor de produto. Os testes não executaram Java/Minecraft. [Comandos e revisão](task-aur-r4-connect.md#décimo-sexto-incremento--supervisor-de-filho-com-encerramento-verificado).

## Consentimento recebido e execução — décimo oitavo incremento

Preparação posterior19: cliente de protocolo em pacote privado separado, dependências fixadas/auditadas e FrameGuard real com limites/backpressure. O incremento20 integra o cliente restrito descrito abaixo; a evidência histórica de STATUS permanece separada. [Fontes, vulnerabilidade corrigida e limites](engineering/AURORA_CONNECT_DEPENDENCIES.md#cliente-de-protocolo-somente-para-laboratório--incremento-19).

O usuário declarou ter lido e aceitado a [EULA oficial](https://www.minecraft.net/en-us/eula) e o [Microsoft Services Agreement](https://www.microsoft.com/servicesagreement), autorizando download/execução e testes locais. O impedimento do incremento 14 está resolvido. O agente executa os comandos; não precisa de credenciais ou de nova aceitação. Dois computadores, clientes reais e cada mod de voz mantêm provas próprias.

`server.mjs run-plan <java-home> <prepared-basename>` reconstrói/verifica os nove artefatos, hashes observados, consentimento, JDK e bytes das configurações. Recusa links, extras, mods e estágio usado. Inclui hashes dos executáveis Connect/PowerShell; esses dois arquivos somente de leitura admitem hardlinks legítimos Cargo/WinSxS, sem estender a exceção aos artefatos/configs. Hash/MZ não são verificação de assinatura ou de confiança. O plano explicita argumentos, ambiente mínimo, diff EULA e limites.

`run <java-home> <prepared-basename> <run-plan-sha> --user-accepted-minecraft-eula` exige hash atual antes de mutação. Consulta a porta 43001, reconfere o plano e cria marcador exclusivo sem permissão de reuso. Salva snapshot de EULA antes de false→true, executa somente o filho próprio e restaura os bytes originais após seu encerramento. Alteração concorrente recusa sobrescrita e deixa snapshot/registro de recuperação. Isso não é recuperação automática após crash do supervisor; o marcador impede relançar o estágio. Mundo e arquivos gerados permanecem apenas na nova pasta.

PowerShell usa perfil/temp em nova pasta irmã `probe-*`: módulos do Windows podem criar AppData. A primeira tentativa revelou esse efeito e recusou o estágio alterado antes de Java/EULA; ele foi preservado, e a nova tentativa usa estágio novo. Durante execução, consulta somente sockets Listen da porta fixa, mantém endereços/PIDs fora do relatório e exige um único bind `127.0.0.1` cujo PID corresponde ao filho Java vivo. O predicado do supervisor não expõe PID/handle. STATUS/pong 767 e nova checagem de ownership precedem `server_ready`.

Limites: readiness 180 s, vida total 300 s, stop 5 s, stdout+stderr 4 MiB drenados sem retenção; Java Xmx1536M. CLI aguarda `stop` completo ou EOF vazio. Entrada inválida, prazo, saída precoce e kill forçado são falhas. Consultas auxiliares têm timeout/saída limitados. Não há callback de shell arbitrário ou execução de comando contido em JSON. O callback de biblioteca deve respeitar AbortSignal; a API não supervisiona descendentes.

O smoke de dois agentes é uma operação separada enquanto o servidor está pronto; não encerrar seu PowerShell pai para simular cleanup dos agentes. Após o smoke, enviar `stop`, aguardar `server_stopped`, conferir EULA, porta livre e processos. As suites que comparam conteúdo da raiz de staging devem executar com `--test-concurrency=1`: fixtures paralelas na mesma raiz interferem nessa observação. [Resultados e comandos exatos](task-aur-r4-connect.md).

Execução real aprovada em `prepared-0QnAmb`, plano `1bae08c9e734e9c1b0d839402f2db35d23e1d9e7cfc913f3d98d5b342684f877`: `server_ready`, smoke PASS com dois agentes/protocolo 767/zero jogadores e stop exit 0, sem kill, EULA original restaurada. Conferência posterior observou zero listeners 43001, Java e agentes; configuração offline/whitelist/loopback preservada. Reuso foi recusado. O estágio anterior `prepared-KekiXy` permanece como evidência da recusa pré-Java por AppData. Esses diretórios não servem para nova execução; criar outro estágio e revisar seu plano. Mundo/logs do laboratório permanecem ignorados e não devem ser publicados.

## Dois clientes de protocolo — vigésimo incremento

O perfil separado `play` conserva online-mode=false, bind127.0.0.1:43001 e whitelist ativa. Acrescenta somente duas identidades sintéticas fixas, desliga compressão e exigência de perfil assinado na pasta nova. Não usa login Microsoft/Mojang, tokens ou cache de conta. Essas identidades não comprovam contas originais ou clientes gráficos mistos; a autenticação Connect continua obrigatória.

`runtime.mjs plan-play/stage-play` e `server.mjs run-plan-play/run-play` usam a mesma sintaxe dos comandos STATUS, com hashes próprios. Um hash do perfil anterior não autoriza play. O run-plan explicita readiness180s, consulta de bind10s, STATUS6s, vida300s e stop5s. A CLI aceita a autorização de termos já recebida; não é necessário perguntá-la novamente. Criar estágio novo para cada execução Java e revisar o plano atual; não copiar hashes de stages usados.

Quando o servidor emitir server_ready, executar em outro terminal local:

~~~powershell
node --max-old-space-size=512 apps/connect-client-lab/smoke.mjs --offline-lab
~~~

O processo cria quatro agentes: dois pares Host/Guest independentes, com sessões/leases distintas e somente game/tcp, até o mesmo servidor descartável. Não é prova de um único Host com dois peers. Cada cliente conecta somente a seu listener loopback. O helper verifica hash do binário Connect, eventos, plano e autorização; pins ficam em RAM/stdin. SIGINT/prazo/falha encerram recursos próprios. O servidor permanece sob seu supervisor: enviar stop e aguardar server_stopped/EULA restaurada; nunca matar Java por nome.

PASS exige, nos dois clientes, identidade offline prevista, LOGIN, término de CONFIGURATION, PLAY, posição com confirmação de teleport, chunk e batch reconhecidos, informação e spawn do outro participante. Consulta independente deve observar 2 jogadores durante e 0 após desconexão. Só então encerra os quatro agentes com eventos stopped/exit0 e nenhum kill. O relatório contém somente estados/contagens, sem nomes, UUIDs, coordenadas, chunks ou texto do servidor. Isso é prova de protocolo e pacotes de mundo, não renderização, movimentação humana, Core/Skins ou voz.

GuardedClient instala FrameGuard antes do splitter e recusa compressão, criptografia inesperada, transferência de destino, recursos externos, aceitação automática de novos termos e reconfiguração. O parser deve consumir o frame integral, sem ignorar partialReadError; exceções síncronas dos handlers viram erro fixo. Não flexibiliza limites para aceitar pacote malformado. Máximos por cliente: frame21bits, fio32MiB,65536pacotes,60s. O limite Node512MiB é de old-space, não de toda alocação nativa. O guard não valida sozinho NBT/schema/CPU; este cliente não deve ser usado como implementação de produto ou com servidores arbitrários.

Encerramento fecha serializer→framer→socket e verifica close; prazo1s excedido permanece falha. A causa inicial e a observação parcial sobrevivem a falha adicional de cleanup. O ensaio real encontrou um travamento do FrameGuard ao completar frame grande com entrada aberta; regressão determinística confirmou callback interno de Transform retido. A correção chama super._read após drenar o frame. Testes anteriores que encerravam a entrada cedo não cobriam esse caso.

## Boot Simple Voice Chat — vigésimo primeiro incremento

Perfil `svc` separado: `runtime.mjs plan-svc/stage-svc`, seguido de `server.mjs run-plan-svc/run-svc`, com a mesma sintaxe e gates dos perfis anteriores. Acrescenta somente SVC2.6.22 ao Fabric1.21.1 já fixado; dez artefatos,60.952.106B. O downloader admite seu CDN Modrinth somente para pin completo de arquivo/URL/tamanho/SHA256. Não resolve mods dinamicamente ou executa instalador. Verificador confere árvore exata também em mods/config; arquivos extras, links e perfil/hash diferentes recusam antes da execução.

O baseline próprio define UDP43002, bind127.0.0.1 e voice_host vazio. Antes de Java, TCP43001 e UDP43002 precisam estar livres. Readiness exige STATUS/pong e um único endpoint UDP pertencente ao mesmo filho vivo. Windows observou `::ffff:127.0.0.1` para o UDP Java: o plano aprova essa forma mapeada exata, além de127.0.0.1, somente no check UDP. Não aceita wildcard, endereço externo, ::1, outro processo ou múltiplos binds; TCP mantém seu critério anterior. [Representação IPv4 mapeada](https://www.rfc-editor.org/rfc/rfc4291.html#section-2.5.5.2).

Depois do stop normal e da restauração da EULA, lê apenas a configuração de voz, até256KiB, com parser limitado/UTF8 estrito. Exige bind/porta/anúncio previstos e mudança de hash/mais de três chaves para comprovar writer; retorna somente hash/tamanho/contagem/booleanos. Não persiste esse conteúdo em Git, não aplica anúncio da ponte e não executa restore nativo nessa etapa. Os arquivos gerados ficam na pasta descartável preservada.

Execução real aprovada em `prepared-Ll1oDQ`, plano `580fba02338074f48be4b7cfbf31bb468e2585b9755a057d98285f7d371dfc7a`: bind TCP/UDP próprio, STATUS por dois agentes PASS, stop exit0 semkill, EULA restaurada e conferência posteriorzeroTCP43001/UDP43002/Java/agentes. Writer passou de176B/3chaves para3081B/19chaves, SHA256 `9ceff6b64b73ad52f4c5f843e01aa027b237cf8f44aebabe53d60ce831582fe4`. Os stages anteriores recusados por representação IPv4 mapeada permanecem preservados. [Comandos, testes e falhas](task-aur-r4-connect.md#vigésimo-primeiro-incremento--boot-e-writer-svc-reais).

Estado: **PROTÓTIPO** de boot/writer. Porta aberta não comprova handshake, anúncio recebido, retorno UDP pelo Connect ou áudio. Nenhum cliente de voz participou. Plasmo tem perfil/dependências/gates separados; não deduzir sua compatibilidade desse resultado.

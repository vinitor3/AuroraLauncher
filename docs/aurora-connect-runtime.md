# Aurora Connect — laboratório descartável de Minecraft

Estado: **PROTÓTIPO**, separado do launcher/UI e sem distribuição. Alvo existente na matriz: Minecraft **1.21.1**, Fabric **0.16.14**, Java **21**, protocolo **767**. Nenhum par Core/Skins é acrescentado ou alterado. A primeira prova será STATUS/pong por Host/Guest autenticados; não comprova login, entrada no mundo ou voz. SVC/Plasmo terão provas separadas depois disso.

## Preparação implementada

`apps/connect-runtime-tools/runtime.mjs` usa somente APIs nativas do Node, sem npm install ou atualização de lock. `plan` lê o `release` e `bin/java.exe` do JDK explicitamente selecionado, sem executá-los, e devolve JSON/hash do plano. Exige Windows/x86_64, versão 21, marcador `MZ`, limites de leitura e ausência observada de links/hardlinks. O marcador não valida integralmente o formato PE. Hash do executável e do release detecta mudança desses arquivos; não é assinatura do fornecedor nem validação de todo o JDK. O JDK local continua sendo uma dependência confiada pelo operador.

`stage` reconstrói o plano a partir do código fixado e do JDK atual. Exige o hash exato e a declaração explícita de aceitação dos termos pelo usuário **antes de criar pasta ou acessar a rede**. Não carrega URLs/comandos de JSON fornecido. Cria somente uma pasta nova em `.aurora/connect-runtime/prepared-*` nesta worktree; não acessa instâncias pessoais. Os nove downloads totalizam **55.582.631 bytes**. Cada um exige HTTPS no domínio fixado, status 200, sem redirect/credenciais/compressão, tamanho exato e digest publicado. Limites: 90 s por artefato, 180 s totais, sem retry. Só renomeia `.part` após verificação e flush; publica `ready.json` depois de todos os artefatos e nova conferência dos arquivos Java.

O servidor Mojang é fixado pelo SHA-1 publicado; SHA-256 adicional será calculado dos bytes baixados e registrado como observação, sem alegar um SHA-256 independente oficial. Bibliotecas usam SHA-256 publicado. Pins completos e URLs estão em [pins.mjs](../apps/connect-runtime-tools/pins.mjs); [fontes/licenças](engineering/AURORA_CONNECT_DEPENDENCIES.md#runtime-descartável--décimo-quarto-incremento). Falha conserva somente a nova pasta incompleta, com `failed.json` e parciais limitados, nunca `ready.json`; não retoma nem apaga recursivamente. Nenhuma pasta existente é reutilizada. Não é sandbox contra substituição concorrente de diretórios por outro processo do mesmo usuário; manter o laboratório exclusivo durante a operação.

Os arquivos preparados mantêm `eula=false`, loopback `127.0.0.1:43001`, `online-mode=true`, whitelist ativa, RCON/query/JMX desligados, dois jogadores e mundo novo próprio. A porta não é reservada antecipadamente; ocupação deve falhar sem encerrar seu dono. Nenhum mod, Core ou Skins é instalado nesta etapa. Não há regra de firewall, router, relay, serviço, conta ou cobrança criada. Downloads dependem somente dos servidores oficiais; falha não muda para mirror ou plano pago.

## Comandos de preparação

Executar no diretório desta worktree. **Somente o primeiro comando já foi executado com o JDK real**; `stage` ainda depende da aceitação pessoal dos termos, conforme abaixo.

```powershell
node apps/connect-runtime-tools/runtime.mjs plan 'C:/Program Files/Eclipse Adoptium/jdk-21.0.12.101-hotspot'
```

Plano observado em 29/09/2026: Java `21.0.12.1`, digest `e2b6da51503e1078173cf14fa10cc5c206b405dead59d7e2662c2d1084fcb353`, nove artefatos, 55.582.631 bytes. Cópia local ignorada: `.aurora/connect-validation/runtime-fourteen-plan.json`. O digest identifica o objeto `plan` serializado pelo helper, não o envelope de apresentação dessa cópia. Recalcular após qualquer mudança de código/JDK e revisar antes de aprovar.

Depois da aceitação explícita pelo usuário e da revisão do mesmo plano, o agente pode executar, sem pedir ao usuário para programar ou operar o terminal:

```powershell
node apps/connect-runtime-tools/runtime.mjs stage 'C:/Program Files/Eclipse Adoptium/jdk-21.0.12.101-hotspot' e2b6da51503e1078173cf14fa10cc5c206b405dead59d7e2662c2d1084fcb353 --user-accepted-minecraft-eula
```

Esse comando **não executa Java**, não torna `eula=true` e não inicia o servidor. `runtime_staged` indica apenas artefatos verificados e arquivos próprios preparados. O argumento registra uma autorização externa já obtida; sua existência não autoriza o agente a aceitá-la pelo usuário.

## Execução futura e limites

O plano contém vetor de argumentos revisável para `KnotServer`, oito JARs de classpath explícitos e `-Dfabric.gameJarPath=server.jar`. Fonte fixada confirma que o lado servidor usa o diretório corrente; o futuro executor precisa definir `WorkingDirectory` para a pasta descartável. O bundler executa código durante a inicialização; não usar Knot como inspeção sem consentimento. Limpar `JAVA_TOOL_OPTIONS`, `_JAVA_OPTIONS`, `JDK_JAVA_OPTIONS` e `CLASSPATH` somente no ambiente do filho. Memória proposta: 256 MiB iniciais, teto 1536 MiB. Esses argumentos ainda não são evidência de launch bem-sucedido.

O roteiro de transporte já está implementado e testado com fixture independente: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/connect-minecraft-status-smoke.ps1 -GamePort 43001`. Usa somente serviço `game/tcp`, dois agentes reais e `guest-tcp`, sem autorizar UDP de aplicação. Confere protocolo 767, status/pong e saída dos agentes; não inicia Java nem encerra o destino. A execução sobre servidor Minecraft real permanece pendente.

Antes de iniciar: verificar novamente os artefatos/configuração, registrar a autorização específica de execução/EULA, confirmar porta livre e espaço/memória, preparar encerramento do filho e trocar apenas o `eula.txt` da pasta própria. Observar bind loopback real; esse bind limita entrada, não garante ausência de conexões externas do servidor. Manter autenticação online; não contornar conta/licença para teste de login. Para STATUS, usar a [consulta existente](aurora-connect.md#consulta-de-status-de-jogo-pelo-listener-tcp--experimental) através da ponte, além de evidência independente da lease/serviço. Encerrar com `stop` no stdin, aguardar saída e só terminar à força o filho próprio em falha com prazo. Não matar Java por nome/PID encontrado em inventário.

Não há restauração pessoal: a pasta inteira é nova. Conservar evidências redigidas; logs crus do jogo não são relatórios e não devem ser enviados ao chat/Git. Uma limpeza futura só pode remover a pasta exata pertencente ao ensaio, depois de confirmar ausência do processo e o confinamento à raiz. Rollback de código: revert local revisado; nenhum instalador ou publicação.

## Supervisor de processo próprio — décimo sexto incremento

`owned-process.mjs` oferece `withOwnedProcess(options, operation)`, usado explicitamente pelo caller: executável/cwd locais absolutos, vetor de argumentos, `shell=false`, janela oculta e um único filho. Não executa nada ao importar e não aprova EULA/artefato ou atesta readiness. Ambiente fornecido substitui a herança; para Java usar ambiente mínimo sem credenciais. Quando omitido, copia o ambiente atual. Em ambos os casos remove as quatro variáveis de injeção Java de qualquer casing, sem alterar o ambiente global. Recusa lexicalmente UNC/devices/ADS, sem consultar a rede; validação de links/artefatos continua responsabilidade do caller.

Prazo global padrão 60 s (máximo uma hora), stop padrão/máximo 5 s, saída stdout+stderr padrão 1 MiB (máximo 16 MiB), até 256 argumentos/64 KiB. Drena ambos os pipes sem guardar conteúdo. A operação recebe AbortSignal; ao terminar, o helper envia `stop\n`, aguarda saída e evento `close` (pipes fechados). Prazo de stop excedido termina somente o objeto ChildProcess criado e devolve erro com `forced=true`; nunca PASS normal. Falha de operação, saída precoce, cancelamento, flood e erro de spawn permanecem falhas mesmo se cleanup sair com zero. Deadline inclui startup/operação/stop, mas o recolhimento pelo sistema pode terminar depois; não relata close antes de observá-lo. Erros têm código fixo e contagens, sem conteúdo, PID, caminho, argumentos ou causa bruta.

**18/18 testes** com filhos Node próprios cobrem sucesso/erro, deadline inclusive durante stop, cancelamento antes/durante/imediatamente após spawn, stdout/stderr/limite agregado, EPIPE, callback tardio, ambiente e preservação de outro filho concorrente. A API não supervisiona descendentes nem consegue interromper JavaScript arbitrário: callback deve cooperar e não criar trabalho solto. Fonte oficial consultada: [ChildProcess, close e stdio](https://nodejs.org/api/child_process.html#event-close). Isso prepara lifecycle, mas não conecta automaticamente Java, EULA, artefatos, configuração e status em um executor de produto. Os testes não executaram Java/Minecraft. [Comandos e revisão](task-aur-r4-connect.md#décimo-sexto-incremento--supervisor-de-filho-com-encerramento-verificado).

## Intervenção necessária para baixar/executar Minecraft

A [EULA oficial](https://www.minecraft.net/en-us/eula) associa compra, download ou uso à concordância com os termos e remete ao [Microsoft Services Agreement](https://www.microsoft.com/servicesagreement). O prompt consolidado proíbe aceitar contratos em nome do usuário. Portanto, manter `eula=false` não é autorização para baixar o JAR Mojang. Até resposta explícita, somente fontes/metadados e fixtures próprias são utilizados.

| Item | Procedimento preparado |
| --- | --- |
| Impedimento observado | Falta aceitação pessoal explícita para download/uso do servidor Minecraft; nenhum JAR Mojang foi baixado neste laboratório |
| Por que o agente não resolve | Não pode declarar concordância contratual em nome do usuário; restrição expressa do prompt |
| Ação mínima e computador | Neste PC Windows, ler os termos oficiais e informar se aceita e autoriza o servidor descartável local descrito; não exige conta nova para a prova STATUS |
| Procedimento do agente após resposta | Recalcular/revisar plano, executar `stage`, validar resultados; preparar/iniciar somente o filho na pasta própria com bind loopback e encerramento controlado |
| Resultado esperado | Download íntegro; em etapa seguinte, STATUS/pong real através do túnel, com EULA/config/vida do processo registradas separadamente |
| Dados não secretos a devolver | Apenas a decisão sobre aceitação/autorização; nenhuma senha, token, chave ou log pessoal |
| Etapa desbloqueada | Ensaio local de Minecraft real; não substitui segundo computador, login/mundo ou voz consentida |

Sem resposta, os testes do downloader/planos, preparação do executor e demais trabalhos locais independentes continuam autorizados. Dois computadores e cada mod de voz mantêm os roteiros/gates próprios; não são pré-condição para escrever e testar este helper.

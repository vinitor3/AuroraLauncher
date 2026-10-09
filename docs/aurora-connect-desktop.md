# Aurora Connect no desktop — laboratório experimental

Este documento descreve o caller local acrescentado no incremento46, a descoberta de portas do incremento48, o perfil de sessão do incremento56 e o transporte TCP+UDP explícito do incremento57. O [task](task-aur-r4-connect.md) registra os comandos executados e seus resultados; instruções abaixo não constituem prova de execução gráfica. Todos os gates de [R4](engineering/MASTER_ROADMAP.md), nove pares Core/Skins, Assistente desktop e recovery/CAS/Doctor permanecem válidos.

## Perfil explícito TCP+UDP — incremento57

O painel oferece **Jogo + UDP experimental**, escolhido antes da preparação. Autoriza somente `game`/TCP e `aux`/UDP fragmentado, com destinos loopback manuais e prazo único de1–120s. O perfil de sessão56 permanece separado e somente TCP; nenhum parser faz fallback para outro perfil. O convidado revisa os serviços, os limites UDP e o destino local fixo `127.0.0.1:35554`, sem receber o destino privado do anfitrião. [Contrato, comandos, evidências e limites57](aurora-connect-desktop-udp.md).

A conexão TCP e a consulta de servidor ficam disponíveis independentemente da preparação UDP. UDP só aparece disponível depois de abrir a porta e negociar a primeira associação com capacidade4096B; falha de porta/capacidade/encerramento aparece separadamente e não desliga o TCP. A capacidade é da associação inicial e não promete ausência de perda ou suporte a toda mensagem de um mod. Expiração, revogação e encerramento geral continuam fechando ambos os transportes.

Não há alteração ou anúncio automático das configurações de voz, descoberta LAN neste perfil, porta alternativa, relay ou teste de áudio implícito. A autorização de retorno limita16384B/s e20fragmentos/s por serviço, incluindo headers; não homologa tráfego de áudio. Host e Guest têm papéis diferentes: o host autoriza o destino UDP, mas não recebe uma ponte local de convidado. Simple Voice Chat e Plasmo Voice conservam validações separadas.

## Perfil explícito de sessão — incremento56

O painel experimental oferece também **Sessão de jogo**, selecionada antes de preparar anfitrião ou convidado. Usa o [contrato52](aurora-connect-session-lifetime.md), somente `game`/TCP, com prazo para entrar de1–600s e encerramento absoluto de até3600s desde a emissão. O perfil curto anterior permanece selecionado por padrão e conserva seus comandos, DTOs, assinaturas e limite600s. Nenhum prazo é ampliado silenciosamente ao colar um convite.

A revisão mostra e vincula ao digest os dois prazos. A preparação continua limitada a60s; exportar deixa de ser permitido quando o convite é consumido ou o ingresso vence. Depois de uma admissão válida, a ponte permite TCP existente e novas conexões até o vencimento da sessão, revogação ou encerramento anterior. Sem convidado admitido, o host encerra no fim do ingresso. O tempo máximo é contado desde emissão, sem renovação por atividade; limites de inatividade continuam independentes.

Preparação, fonte descoberta, consulta STATUS, cancelamento, logout e encerramento preservam os gates anteriores. Duração não autoriza iniciar/prolongar Java nem habilita voz ou relay. A emissão por duração usa uma única amostra de relógio para os campos assinados; o desktop aceita segundos inteiros exatos, recusando intervalos fracionários em convites schema2. [Contrato nativo, interface, provas e limites56](aurora-connect-desktop-session-lifetime.md). Uma amostra curta não homologa uma hora de estabilidade ou entrada gráfica.

## Disponibilidade e limites

A feature Rust `connect-lab` é opcional e desligada por padrão. O build normal registra os comandos estreitos, mas não compila nem instancia o transporte; `connect_capabilities.available=false` mantém a navegação Connect oculta. A feature independente `connect-voice-lab` não habilita voz neste painel. Não foi criado instalador ou alterada versão de release.

O caller usa diretamente a biblioteca real `apps/connect-lab`, sem subprocesso PowerShell, chave privada no frontend, Firebase social ou persistência de convites. Os perfis curto e de sessão autorizam somente `game/tcp`, revisão1 e `reliableStream`; o opt-in57 acrescenta `aux/udp` conforme a seção própria. Destinos loopback são selecionados explicitamente pelo anfitrião. Digitar uma porta não comprova que pertence ao Minecraft ou a uma instância gerenciada. A alternativa Windows de descoberta confere o processo filho já iniciado pelo Aurora, conforme o contrato abaixo. Connect não inicia Java, abre o mundo para LAN ou entra automaticamente no jogo.

Há uma tentativa por processo/janela principal do launcher. As chaves privadas e o estado de consumo permanecem somente na memória do backend. O proprietário deriva da janela Tauri, não de argumento enviado pelo frontend. Essa propriedade local não equivale a autenticação Microsoft, identidade de jogador ou amizade Aurora.

Os modos são loopback e IPv6 externo explicitamente selecionado, conforme [spec8.1.1](engineering/AURORA_CONNECT_SPEC.md#811-modo-externo-ipv6-explícito--incremento45). Não há fallback de rede, DNS, relay, portmapper ou mudança em firewall/roteador. O filtro inicial IPv6 não comprova todos os caminhos internos do Iroh ou alcance entre redes.

## Fluxo implementado

1. O anfitrião seleciona endereço inicial, destino de jogo loopback e prazo até600s. Preparar gera a identidade e um identificador de operação, sem abrir transporte.
2. O convidado seleciona o modo e informa o código público recebido diretamente do anfitrião. Preparar gera sua identidade, sem conectar. As identidades públicas podem ser copiadas por ação explícita.
3. O anfitrião recebe o código do convidado e revisa destinatário, endereço, serviço, destino, prazo e digest. Somente a confirmação da revisão vigente inicia o Host real; então o convite pode ser exportado/copied explicitamente.
4. O convidado cola o convite. O backend confere assinatura, pin independente do anfitrião, destinatário, modo e serviço; apresenta nova revisão. Somente sua confirmação inicia o Guest e a ponte TCP em porta loopback efêmera.
5. O convidado pode consultar STATUS/pong pela ponte e copiar o endereço para Conexão direta no Minecraft. O painel não inicia o jogo nem afirma entrada no mundo.
6. Encerrar cancela a operação e aguarda recursos próprios. A mesma ação serve durante a preparação, conexão e consulta. Operações antigas não podem encerrar uma nova tentativa pelo mesmo identificador.

No perfil curto original, preparação/revisão expiram após60s; o convite tem prazo independente de até600s desde emissão, que também limita a sessão inteira. A interface distingue esses relógios. O perfil56 separa ingresso e sessão conforme a seção acima. Não há renovação implícita. Reiniciar exige novas chaves/sessão/convite; não se reaproveita a autorização consumida. O prazo900s do laboratório gráfico histórico não altera esses contratos.

STATUS deste incremento usa Minecraft1.21.1/protocolo767, handshake `localhost:43001`, explicitamente identificado na interface. A ponte TCP continua opaca. A resposta de STATUS tem parser limitado e expõe apenas resumo de protocolo, população declarada e tempos; não apresenta MOTD, favicon, nomes ou payload bruto. Há até30 consultas, intervalo mínimo20s, limite próprio e cancelamento. Outros protocolos/virtual hosts exigem contrato de seleção posterior.

## Estados e encerramento

Transporte autenticado, servidor respondendo e entrada no mundo são estados separados. A população declarada pelo STATUS não identifica jogador. `worldState=notVerified`, Simple Voice Chat/Plasmo desativados e caminho não verificado permanecem explícitos. Não usar uma admissão QUIC como prova de mundo, mod, conta ou áudio.

Navegar para outra seção desmonta o painel, sem encerrar a sessão; ao retornar, ele recupera a preparação/revisão/snapshot do backend. Copiar depende de ação explícita e pode usar seleção manual caso o clipboard não esteja disponível. Respostas antigas de operações assíncronas não substituem uma revisão ou cancelamento mais recente.

Sair da conta aguarda o encerramento da tentativa atual. Fechar a janela pela barra mantém o comportamento de bandeja existente. Sair efetivamente do aplicativo aguarda `shutdown_all`, bloqueia preparações novas e encerra somente recursos Connect. Nenhum desses fluxos mata Java do usuário ou restaura configuração de mod automaticamente. Encerramento forçado do processo perde chaves em RAM e os sockets são liberados pelo sistema; não há retomada de convite consumido.

## Descoberta de porta da instância — incremento48

No Windows, a mesma feature experimental oferece **Jogo aberto pelo Aurora · mundo LAN**. O usuário abre o mundo para LAN no Minecraft e pede a busca. A lista vem dos filhos vivos mantidos por esta execução do launcher; servidores iniciados fora dele conservam a seleção manual. Não há busca por nome de processo, PID fornecido pelo frontend, leitura de linha de comando, broadcast, varredura de rede ou alteração da configuração do jogo.

A camada nativa duplica um handle do `Child` já próprio e vincula PID, horário de criação, instância e execução. Consulta as tabelas TCP de escuta IPv4/IPv6 e conserva somente linhas daquele processo. São aceitos binds loopback e wildcard; wildcard é apresentado como tal e convertido em destino loopback para a consulta/ponte, sem mudar o bind original. Inventário não envia pacotes. Há no máximo32candidatos, tabelas limitadas a1MiB/16384linhas e duas tentativas de tamanho.

Cada escolha produz revisão própria. **Autorizar consulta e selecionar porta** consome seu digest, confere processo/escuta antes e depois de STATUS/pong e retorna uma seleção opaca de uso único. A consulta continua restrita ao protocolo767/1.21.1, agora com a porta descoberta no handshake; resposta não comprova compatibilidade com outro protocolo, mundo ou jogador. Não é leitura/análise de logs do Minecraft.

Inventário/seleção valem30s. Preparar o anfitrião consome a seleção dentro desse prazo e conserva a prova de processo; não recebe um PID arbitrário nem troca o destino. Depois valem os prazos próprios de preparação60s e convite/sessão600s. A revisão do anfitrião identifica origem, instância e execução; digest também vincula a identidade privada observada do processo. Antes de abrir o transporte e, durante a sessão, em verificações a cada1s, o backend reconfere a escuta e o processo. Perda da origem encerra Connect com `connect_source_changed`, aguardando liberação. Java permanece independente e nunca é encerrado pelo Connect.

Limite: observação antes/depois e monitor periódico não tornam atômicas a identidade do processo e uma conexão TCP. Outro processo local pode disputar a porta no intervalo; não há proteção prometida contra usuário local comprometido. Porta escutando também não significa mundo pronto, e uma consulta bem-sucedida não é homologação de LAN. Testes usam filho Rust próprio com servidor STATUS sintético; ensaio de LAN real, janela Tauri e os nove pares permanecem separados.

O painel invalida seleção ao trocar de papel, ignora respostas tardias após desmontagem e exige nova escolha após revisão vencida. Desligar a feature mantém a descoberta indisponível. Não há arquivos/configurações pessoais a recuperar; o rollback operacional encerra somente Connect e preserva o jogo.

## Desenvolvimento e validação

Com dependências locais e nesta worktree, o comando preparado para desenvolvimento é:

```powershell
npm --prefix apps/desktop run tauri -- dev --features connect-lab
```

Não executar o empacotamento oficial para testar esse painel. O comando acima inicia o aplicativo completo e usa o diretório desktop normal; qualquer ensaio gráfico com contas/configurações pessoais requer o fluxo de autorização aplicável. Testes nativos usam dois managers reais e endpoints controlados; os testes DOM usam fixtures e não comprovam invocação Tauri, interface nativa, internet, jogo ou voz.

Rollback operacional: Encerrar Connect, aguardar recursos liberados e sair pelo menu do launcher quando necessário. Rollback de código: revert local revisado do incremento, preservando demais entregas. O build sem a feature mantém Connect indisponível. Não remover dados pessoais, worktrees, snapshots ou kits históricos.

## Continuidade das seis sequências

| Sequência solicitada | Este incremento | Validação ainda necessária |
| --- | --- | --- |
| Convites entre duas internets | APIs assinadas45 reutilizadas | Entrada, expiração, revogação com host vivo e replay da mesma identidade no novo fluxo externo |
| Integração no launcher | Caller nativo e painel com revisão, conectar, STATUS e encerrar | Exercício gráfico Tauri e entre computadores; estado de jogador depende de produtor de jogo confiável |
| Ciclo de jogo | Porta manual ou descoberta por processo próprio com revisão/STATUS, monitor de origem e cleanup; perfil56 separa ingresso e sessão | Minecraft LAN real, dedicado supervisionado, estabilidade prolongada e exercício gráfico do perfil56 |
| Simple Voice Chat | Controles reais53 com V2; transporte genérico TCP+UDP57 no painel, sem aplicar configurações | Áudio real, anúncio por participante, recovery integrado ao painel e limites dos pacotes reais de áudio |
| Plasmo Voice | Controles reais53 separados com V2; transporte genérico57 no painel, sem identificar mod | Ensaio separado de áudio/anúncio/recovery e limites dos pacotes reais de áudio |
| Redes e conjunto | Amostra IPv4 próprio documentada separadamente | LAN/IPv4 externo/NAT/CGNAT/relay elegível; dois jogadores, contas mistas, Core/Skins, quedas/estabilidade; manifesto/updater antes de sincronização |

Ausência de participantes remotos não bloqueia a implementação local independente. Testes audíveis e ações no outro PC serão preparados separadamente, sem solicitar credenciais ou gravações.

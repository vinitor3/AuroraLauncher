# Aurora Connect no desktop — laboratório experimental

Este documento descreve o caller local acrescentado no incremento46. O [task](task-aur-r4-connect.md) registra os comandos executados e seus resultados; instruções abaixo não constituem prova de execução gráfica. Todos os gates de [R4](engineering/MASTER_ROADMAP.md), nove pares Core/Skins, Assistente desktop e recovery/CAS/Doctor permanecem válidos.

## Disponibilidade e limites

A feature Rust `connect-lab` é opcional e desligada por padrão. O build normal registra os comandos estreitos, mas não compila nem instancia o transporte; `connect_capabilities.available=false` mantém a navegação Connect oculta. A feature independente `connect-voice-lab` não habilita voz neste painel. Não foi criado instalador ou alterada versão de release.

O caller usa diretamente a biblioteca real `apps/connect-lab`, sem subprocesso PowerShell, chave privada no frontend, Firebase social, persistência de convites ou processo Java. Autoriza somente `game/tcp`, revisão1 e `reliableStream`, com destino loopback selecionado explicitamente pelo anfitrião. Digitar uma porta não comprova que pertence ao Minecraft ou a uma instância gerenciada. Descoberta de porta, mundo LAN, início do servidor e entrada automática no jogo permanecem incrementos próprios.

Há uma tentativa por processo/janela principal do launcher. As chaves privadas e o estado de consumo permanecem somente na memória do backend. O proprietário deriva da janela Tauri, não de argumento enviado pelo frontend. Essa propriedade local não equivale a autenticação Microsoft, identidade de jogador ou amizade Aurora.

Os modos são loopback e IPv6 externo explicitamente selecionado, conforme [spec8.1.1](engineering/AURORA_CONNECT_SPEC.md#811-modo-externo-ipv6-explícito--incremento45). Não há fallback de rede, DNS, relay, portmapper ou mudança em firewall/roteador. O filtro inicial IPv6 não comprova todos os caminhos internos do Iroh ou alcance entre redes.

## Fluxo implementado

1. O anfitrião seleciona endereço inicial, destino de jogo loopback e prazo até600s. Preparar gera a identidade e um identificador de operação, sem abrir transporte.
2. O convidado seleciona o modo e informa o código público recebido diretamente do anfitrião. Preparar gera sua identidade, sem conectar. As identidades públicas podem ser copiadas por ação explícita.
3. O anfitrião recebe o código do convidado e revisa destinatário, endereço, serviço, destino, prazo e digest. Somente a confirmação da revisão vigente inicia o Host real; então o convite pode ser exportado/copied explicitamente.
4. O convidado cola o convite. O backend confere assinatura, pin independente do anfitrião, destinatário, modo e serviço; apresenta nova revisão. Somente sua confirmação inicia o Guest e a ponte TCP em porta loopback efêmera.
5. O convidado pode consultar STATUS/pong pela ponte e copiar o endereço para Conexão direta no Minecraft. O painel não inicia o jogo nem afirma entrada no mundo.
6. Encerrar cancela a operação e aguarda recursos próprios. A mesma ação serve durante a preparação, conexão e consulta. Operações antigas não podem encerrar uma nova tentativa pelo mesmo identificador.

Preparação/revisão expiram após60s; o convite tem prazo independente de até600s desde emissão, que também limita a sessão inteira. A interface distingue esses relógios. Não há renovação implícita. Reiniciar exige novas chaves/sessão/convite; não se reaproveita a autorização consumida. O prazo900s do laboratório gráfico histórico não altera esse contrato.

STATUS deste incremento usa Minecraft1.21.1/protocolo767, handshake `localhost:43001`, explicitamente identificado na interface. A ponte TCP continua opaca. A resposta de STATUS tem parser limitado e expõe apenas resumo de protocolo, população declarada e tempos; não apresenta MOTD, favicon, nomes ou payload bruto. Há até30 consultas, intervalo mínimo20s, limite próprio e cancelamento. Outros protocolos/virtual hosts exigem contrato de seleção posterior.

## Estados e encerramento

Transporte autenticado, servidor respondendo e entrada no mundo são estados separados. A população declarada pelo STATUS não identifica jogador. `worldState=notVerified`, Simple Voice Chat/Plasmo desativados e caminho não verificado permanecem explícitos. Não usar uma admissão QUIC como prova de mundo, mod, conta ou áudio.

Navegar para outra seção desmonta o painel, sem encerrar a sessão; ao retornar, ele recupera a preparação/revisão/snapshot do backend. Copiar depende de ação explícita e pode usar seleção manual caso o clipboard não esteja disponível. Respostas antigas de operações assíncronas não substituem uma revisão ou cancelamento mais recente.

Sair da conta aguarda o encerramento da tentativa atual. Fechar a janela pela barra mantém o comportamento de bandeja existente. Sair efetivamente do aplicativo aguarda `shutdown_all`, bloqueia preparações novas e encerra somente recursos Connect. Nenhum desses fluxos mata Java do usuário ou restaura configuração de mod automaticamente. Encerramento forçado do processo perde chaves em RAM e os sockets são liberados pelo sistema; não há retomada de convite consumido.

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
| Ciclo de jogo | Destino explicitamente aprovado, sessão limitada e cleanup | Descoberta por processo/instância, dedicado/LAN, sessões longas com contrato próprio |
| Simple Voice Chat | Mantém adaptador, controles e provas locais anteriores | Tamanho UDP integral, áudio real, anúncio por participante e recovery integrado |
| Plasmo Voice | Mantém adaptador, controles e provas locais anteriores | Ensaio separado de áudio/tamanho/anúncio/recovery |
| Redes e conjunto | Amostra IPv4 próprio documentada separadamente | LAN/IPv4 externo/NAT/CGNAT/relay elegível; dois jogadores, contas mistas, Core/Skins, quedas/estabilidade; manifesto/updater antes de sincronização |

Ausência de participantes remotos não bloqueia a implementação local independente. Testes audíveis e ações no outro PC serão preparados separadamente, sem solicitar credenciais ou gravações.

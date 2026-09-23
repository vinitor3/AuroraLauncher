# ADR-023 — Contas independentes em vários launchers

- Data: 2026-09-23
- Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**
- Escopo: autenticação no desktop e proteção das instâncias locais compartilhadas.

## Contexto

O Firebase Auth usava o mesmo nome de aplicativo e a persistência local padrão em todas as aberturas do launcher. As janelas compartilhavam o estado de autenticação: entrar ou sair de uma conta podia trocar a conta nas demais. O launcher já inicia processos Minecraft separados e mantém um servidor IPC próprio por jogo, mas esse compartilhamento impedia testar contas diferentes de forma previsível.

## Decisão

1. Antes de inicializar Firebase Auth, consultar o comando nativo `launcher_auth_scope`. O primeiro processo obtém um bloqueio exclusivo do sistema operacional em `launcher-sessions/primary.lock`, no diretório de dados do launcher, e continua usando o nome Firebase e a persistência existentes. O login lembrado permanece disponível nessa janela principal.
2. Aberturas adicionais recebem um identificador aleatório próprio, estável durante a execução, e uma sessão temporária em um nome Firebase diferente. Cada janela pode entrar ou sair de uma conta sem sincronizar esse estado com as demais. Uma eventual janela adicional no mesmo processo também recebe um identificador separado.
3. O bloqueio permanece aberto durante a vida do processo principal. Nunca excluir ou substituir o arquivo de bloqueio durante a eleição: o sistema libera a posse ao encerrar o processo, inclusive em crash. As janelas temporárias existentes não são promovidas automaticamente quando a principal termina. A próxima abertura disponível pode assumir a sessão principal.
4. Falhar explicitamente se a preparação do escopo nativo não for possível. Não iniciar silenciosamente uma autenticação compartilhada como fallback. O arquivo de eleição é vazio e não recebe dados de conta, tokens ou senhas.
5. Preservar os diretórios de instâncias, downloads, configurações e aparência. Jogos de duas contas devem usar instâncias Minecraft diferentes. Não executar dois jogos na mesma pasta: lançamento, instalação e alterações usam o bloqueio nativo de mutação; renomear e excluir também respeitam esse bloqueio entre processos. O arquivo fica em `.aurora/instance-locks`, fora das instâncias, com identidade derivada do caminho canônico, para permitir renomear/excluir uma instância ociosa no Windows sem soltar o bloqueio. Não seguir links nesse diretório nem apagar arquivos de bloqueio ao final da operação.
6. Preservar o Assistente desktop e o comportamento da bandeja. O atalho global pode pertencer a um único launcher; o botão do Assistente permanece disponível nas demais janelas.

## Comportamento e limites

Para testar duas contas, abrir o launcher duas vezes, entrar com outra conta na janela adicional e iniciar uma instância Minecraft diferente em cada janela. As configurações das instâncias continuam compartilhadas no computador; o isolamento introduzido aqui é da sessão de autenticação.

A sessão adicional não lembra o login após encerrar o processo. O botão **X** mantém o comportamento anterior de ocultar na bandeja; para encerrar de fato, usar **Fechar o launcher** no menu da bandeja. Fechar uma janela não encerra os processos Minecraft que ela iniciou.

O escopo secundário usa um identificador novo a cada execução e não altera a conta persistida pela principal. Esta decisão não implementa um gerenciador de perfis com várias contas permanentemente salvas, clonagem de modpacks ou autenticação Microsoft.

No Windows, lançamento, renomeação e exclusão também verificam o diretório de jogo dos processos Java: um Minecraft pode sobreviver ao launcher e já não ter seu bloqueio mantido. O probe retorna somente um estado controlado, sem registrar linhas de comando, e recusa a operação quando não consegue confirmar a disponibilidade. Nenhum Minecraft é encerrado automaticamente. Os bloqueios e a checagem entre processos devem ser usados por todas as janelas da versão integrada; a operação simultânea com executáveis antigos do launcher não foi homologada.

A preparação de **Jogar** verifica a JVM antes de criar um novo registro Doctor ou iniciar uma atualização automática do loader; o lançamento verifica novamente antes de instalar Core/Skins. Instalações e alterações manuais fora desse fluxo continuam com o bloqueio de mutação existente, sem ampliar a detecção de JVM órfã nesta entrega.

A validação automatizada deve cobrir independência de escopos, estabilidade após nova consulta, exclusão mútua real entre processos, recuperação após crash e preservação do login principal. Homologação com duas contas reais, duas janelas e dois clientes Minecraft continua **PRECISA DE TESTE** até existir evidência específica do conjunto integrado.

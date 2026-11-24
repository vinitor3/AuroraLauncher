# Aurora Skins

O módulo aplica as skins e capas do guarda-roupa do Aurora no cliente. No launcher, abra o guarda-roupa, escolha ou importe uma skin PNG e equipe-a. O modelo pode usar braços clássicos ou finos.

## Multiplayer

Cada jogador precisa do Core e de Skins para ver as aparências Aurora. Para retransmiti-las no multiplayer, instale no servidor o JAR de Skins da mesma versão de Minecraft e do mesmo loader. O servidor vincula as mudanças ao jogador conectado e guarda esses dados em memória durante a sessão. A biblioteca pessoal permanece no cliente.

## Versões e build

O código compartilhado fica em `core` e os adaptadores ficam em `minecraft`. Execute `npm run skins:build` a partir da raiz e `npm run release:verify-mods` para verificar os JARs de distribuição. Confira as combinações de Minecraft, Java e loader na [matriz ativa](../../scripts/active-mod-matrix.json).

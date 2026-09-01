# Tarefa — Aurora Skins e compatibilidade do Cave Horror

## Estado

**PARCIAL** em 1º de setembro de 2026: código, matrizes de mods, catálogo de
produção e teste visual do menu estão concluídos. O instalador oficial
`0.1.1-alpha.5` ainda será gerado depois do commit e do preflight limpo.

## Entrega

- Core 1.1.2, Skins 0.2.1 e Companion 0.2.3 publicados como novas matrizes,
  sem alterar releases anteriores.
- `pack.mcmeta` compatível com cada Minecraft em todos os 27 JARs; o gate de
  release recusa um módulo que não contenha esse metadado.
- SLF4J do Core isolado no namespace privado e gate contra `org/slf4j/**` ou
  `net/minecraft/**` indevidos.
- `${version_name}` dos argumentos Forge resolvido para o JAR cliente físico,
  impedindo que Minecraft bruto e transformado entrem juntos no module path.
- Skin 3D em pé, painel ancorado à direita, botão diretamente sob o avatar e
  hit-test próprio para continuar clicável em menus modificados pelo FancyMenu.
- Catálogo HTTPS próprio com pesquisa, paginação, cache e fallback offline,
  publicado no Worker `aurora-api`.

## Evidência runtime

O Cave Horror Project foi iniciado com Minecraft 1.20.1, Forge 47.4.21 e Java
17. O launcher instalou por troca atômica os três JARs cujos SHA-256 coincidem
com os releases. O jogo chegou ao menu, registrou Core 1.1.2, Skins 0.2.1 e
Assistente 0.2.3, conectou o IPC e não registrou os erros de pacote duplicado ou
resource pack inválido. Visualmente, a skin apareceu em pé, o botão Skins ficou
alinhado sob o avatar, abriu a tela e a aba Explore mostrou catálogo online e
paginação. Um mundo/servidor não foi aberto; essa parte ainda **PRECISA DE
TESTE**.

## Distribuição automática

Ao iniciar uma instância suportada, o launcher escolhe a combinação exata de
Minecraft/loader, valida o Core assinado e instala ou substitui atomicamente
Core, Skins e Companion na pasta `mods`. A imagem de uma skin do catálogo só é
baixada quando a pessoa a seleciona/aplica e depois fica no cache local; o
catálogo possui skins originais embutidas para funcionar sem rede.

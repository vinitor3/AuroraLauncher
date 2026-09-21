# Aurora Skins

Aurora Skins 0.2.5 é o módulo oficial, independente e client-side para aparência dentro do Minecraft. A fonte do Launcher instala a build correta automaticamente depois do Aurora Core selecionado (candidato 1.1.5; API mínima 1.1.3) sem instalar Companion. O Assistente desktop permanece no Launcher. A distribuição ao usuário depende da próxima integração/compilação do instalador.

## Estado

**PARCIAL**: catálogo em grade integrado nos nove alvos. Builds, testes e observações gráficas desta versão estão no [registro da grade](../../docs/task-skins-grid.md); a homologação completa por loader exige os cenários indicados nesse registro.

Nesta versão estão implementados:

- módulo registrado pela API do Core, sem duplicar sessão ou IPC;
- biblioteca local endereçada por SHA-256, favoritos e histórico limitado;
- importação validada de PNG real 64x64 e 64x32 de até 2 MB;
- seleção manual Classic/Wide ou Slim e conversão visual correta de 64x32;
- aplicação dinâmica da skin do jogador local;
- fallback assíncrono para a skin HTTPS do perfil do Launcher;
- catálogo HTTPS oficial com pesquisa e paginação, cache local e fallback original embutido quando a rede estiver indisponível;
- página Skins nas Opções Aurora;
- botão alinhado dinamicamente e personagem 3D na tela principal;
- preview com duas camadas e cabeça/corpo suavizados em direção ao mouse;
- layout responsivo para resoluções lógicas pequenas e diferentes GUI scales;
- grade Explorar de 4 × 3, ou 6 × 2 em áreas baixas, sem texto dentro dos cards;
- seleção por identidade, loading/falha por slot e descarte de respostas antigas;
- prévias deduplicadas, cache limitado em memória e reaproveitamento do PNG validado em disco.

O catálogo agora agrega MCStat e os desenhos originais Aurora, e a busca de jogadores usa mc-api com fallback MCStat. Digite `@nome`/`@UUID` em Explorar, ou `#tag` no catálogo. Explorar mostra 12 slots com modelos de corpo inteiro. Clique numa prévia pronta para selecionar; **Usar skin** aplica explicitamente. As setas trocam lotes inteiros de 12; slots sem resultado ficam vazios e inativos. Veja [fontes, configuração, limites e exemplos](../../docs/skin-sources.md).

Permanecem **NÃO IMPLEMENTADOS** nesta entrega: sincronização de favoritos/skin ativa pela API Aurora e propagação de skins entre jogadores no multiplayer. No desktop existe grade de resultados. Não há scraping de sites: os mods usam somente o Worker Aurora e este consulta as APIs oficiais.

## Matriz

| Minecraft | Loader de build | Java runtime | Build JDK | Core mínimo | Skins |
| --- | --- | ---: | ---: | --- | --- |
| 1.12.2 | Forge 14.23.5.2860 | 8 | 17, release 8 | 1.1.3 | 0.2.5 |
| 1.16.5 | Fabric 0.16.14 | 8 | 17, release 8 | 1.1.3 | 0.2.5 |
| 1.16.5 | Forge 36.2.42 | 8 | 17, release 8 | 1.1.3 | 0.2.5 |
| 1.19.2 | Fabric 0.16.14 | 17 | 17 | 1.1.3 | 0.2.5 |
| 1.19.2 | Forge 43.4.16 | 17 | 17 | 1.1.3 | 0.2.5 |
| 1.20.1 | Fabric 0.16.14 | 17 | 17 | 1.1.3 | 0.2.5 |
| 1.20.1 | Forge 47.4.23 | 17 | 17 | 1.1.3 | 0.2.5 |
| 1.21.1 | Fabric 0.16.14 | 21 | 21 | 1.1.3 | 0.2.5 |
| 1.21.1 | Forge 52.1.12 | 21 | 21 | 1.1.3 | 0.2.5 |

As versões de loader acima são as dependências de compilação. Versões mínimas selecionadas ficam em `scripts/active-mod-matrix.json`; os ambientes realmente executados estão no registro da grade. NeoForge está fora do escopo atual.

## Organização

- `core`: modelo, catálogo online/fallback, cache, validação, favoritos, histórico e geometrias responsivas;
- `minecraft/1.16.5`, `1.19.2`, `1.20.1`, `1.21.1`: adaptadores Fabric/Forge;
- `minecraft/legacy/forge-1.12.2`: adaptador Forge legado isolado;
- `releases/skins/0.2.5`: artefatos imutáveis referenciados pela fonte do Launcher.

Para compilar e testar toda a matriz sem alterar releases:

```powershell
npm run skins:build
```

O comando também gera cópias de conferência ignoradas pelo Git em `.aurora/build-matrix/skins`. Antes do instalador, `npm run release:verify-mods` recompila Core e Skins e exige correspondência normalizada com os 18 JARs selecionados, além de tamanho/hash, metadata e assinatura aplicáveis. Releases históricas não são sobrescritas.


O wrapper Gradle 8.4 compartilhado fica em `tools/gradle/8.4`; a geração 1.21.1 preserva o wrapper 9.7.1 próprio do Core, usado também pelo Skins. Execute builds sequencialmente para evitar concorrência no cache Loom.

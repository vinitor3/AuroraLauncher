# Aurora Core

O Core conecta o jogo ao launcher por WebSocket local e oferece a API de módulos, eventos, configuração e menu Aurora. Cada execução usa uma porta e uma credencial temporárias.

## Estrutura

- `api` contém os contratos Java de módulos.
- `runtime` gerencia a sessão, as mensagens e a configuração.
- `minecraft` contém cada adaptador de versão e loader, inclusive o Forge legado.

## Compilar e validar

Na raiz, execute `npm run core:build`. Use `npm run release:verify-mods` para conferir os artefatos de distribuição. As versões de Minecraft, loader, Java e módulo selecionadas para esta edição ficam na [matriz ativa](../../scripts/active-mod-matrix.json).

O jogo carrega o JAR correspondente ao seu Minecraft e loader. Os termos das bibliotecas distribuídas com o Core estão nos [avisos de terceiros](../../THIRD_PARTY_NOTICES.md).

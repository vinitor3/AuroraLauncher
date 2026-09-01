# Tarefa — catálogo e instalação CurseForge

## Estado

**CONCLUÍDO no código em 31 de agosto de 2026.** A publicação do Worker e um
novo instalador ficam para a próxima entrega pública.

## Escopo entregue

- A tela Descobrir pesquisa modpacks no Modrinth ou CurseForge e usa os mesmos
  filtros de versão, Forge/Fabric, ordenação e paginação.
- As abas de mods, shaders e pacotes de recursos mantêm os dois provedores,
  mostram os mais baixados sem busca e aplicam versão, loader e ordenação.
- Um modpack CurseForge tem o ZIP validado por SHA-1 e o `manifest.json` lido
  localmente. O Aurora aceita somente Forge/Fabric, instala o Minecraft/loader,
  baixa os arquivos obrigatórios e aplica `overrides` com proteção contra
  caminhos inseguros e limites de extração.
- O Worker resolve arquivos e projetos em lotes. A chave CurseForge continua
  fora do desktop.
- URLs diretas são baixadas em paralelo. Quando a distribuição direta está
  bloqueada, a página oficial é aberta dentro do Aurora, o arquivo disparado
  pela página é validado e a fila segue automaticamente.
- Conteúdo avulso CurseForge instala também sua árvore de dependências
  obrigatórias, limitada e sem ciclos.
- Mods avulsos do Modrinth também resolvem recursivamente as dependências
  marcadas como obrigatórias e baixam os arquivos em lote.
- Capas e nomes dos projetos instalados pelo CurseForge são guardados nos
  metadados locais para aparecer na lista da instância.

## Limites

- Arquivos sem SHA-1 não são instalados automaticamente.
- Downloads bloqueados continuam respeitando a página oficial do CurseForge;
  o launcher não constrói URLs de CDN para contornar a escolha do autor.
- NeoForge e outros loaders permanecem fora do escopo; somente Forge e Fabric.
- A homologação contra as novas ações do catálogo público depende da publicação
  do Worker atualizado; o dry-run local não altera produção.
- Nenhum instalador foi gerado nesta tarefa.

## Evidência

- Build React/Vite e verificação TypeScript concluídos.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test`
  concluídos: 34 testes aprovados e 1 teste online opcional ignorado.
- `wrangler deploy --dry-run` e lint TypeScript das Functions concluídos.
- Build completo do Companion Forge/Fabric concluído.

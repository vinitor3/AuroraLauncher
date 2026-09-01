# Tarefa — catálogo e instalação CurseForge

## Estado

**IMPLEMENTADO MAS NÃO HOMOLOGADO.** O código e o Worker foram publicados em
1º de setembro de 2026, mas a chave de produção é recusada com HTTP 403 na
busca oficial e precisa ser renovada pelo proprietário. Nenhum novo instalador
foi gerado nesta tarefa.

## Escopo entregue

- A tela Descobrir pesquisa modpacks no Modrinth ou CurseForge e usa filtros de
  versão, Forge/Fabric, categoria dinâmica, ordenação e paginação.
- As abas de mods, shaders e pacotes de recursos mantêm os dois provedores,
  mostram os mais baixados sem busca e aplicam versão, loader, categoria e
  ordenação. Categorias como **Aventura** são lidas da taxonomia de cada
  provedor, agrupadas e exibidas em português quando há tradução conhecida.
- Um modpack CurseForge tem o ZIP validado por SHA-1 e o `manifest.json` lido
  localmente. O Aurora aceita somente Forge/Fabric, instala o Minecraft/loader,
  baixa os arquivos obrigatórios e aplica `overrides` com proteção contra
  caminhos inseguros e limites de extração.
- O Worker resolve arquivos e projetos em lotes. A chave CurseForge continua
  fora do desktop.
- Downloads diretos do CurseForge usam tickets temporários assinados e passam
  pelo Worker com autenticação do CDN; a chave não aparece na URL nem no
  instalador. Os tickets duram oito horas para comportar filas grandes, mas têm
  host restrito, limite de uso e rate limit por IP.
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
- O Worker atualizado está em produção e `/health` confirma sua revisão. O
  smoke real `/health/curseforge` falha corretamente enquanto a chave atual for
  recusada; não há fallback para API não oficial.
- Nenhum instalador foi gerado nesta tarefa.

## Evidência

- Build React/Vite e verificação TypeScript concluídos.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test`
  concluídos: 47 testes aprovados e 2 online ignorados por padrão; o teste real
  de categorias Modrinth foi executado separadamente e confirmou `adventure`.
- A suíte Node do Worker cobre 8 casos de ticket, allowlist/redirecionamento,
  JSON inválido, retomada HTTP 416, retry/429 e normalização de categorias; ela
  também verifica a propagação de `categoryId`, passou e agora faz parte do CI.
- Worker validado em dry-run e publicado. `/health` responde HTTP 200; o smoke
  CurseForge identifica a credencial inválida com HTTP 503 e mensagem segura.
  A publicação final desta tarefa é `68f079bf-d148-4a07-ab3e-cec8a51af8fd`.

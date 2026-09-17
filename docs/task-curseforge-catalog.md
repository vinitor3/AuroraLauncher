# Tarefa — catálogo e instalação CurseForge

## Estado

**IMPLEMENTADO MAS NÃO HOMOLOGADO.** Em 17 de setembro de 2026 a nova chave
de produção foi validada: `/health/curseforge` retorna HTTP 200, um resultado
de busca e 52 categorias. O Worker integrado inclui catálogo, skins e updater;
a release desktop unificada é a `0.1.1-alpha.6`. Instalação real de cada tipo de
conteúdo e execução dentro do Minecraft ainda exigem homologação.

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
  Na fonte posterior à alpha.6, o launcher tenta primeiro uma cópia oficial
  Modrinth de SHA-1 idêntico e corrige redirecionamentos/cancelamentos da fila;
  consulte `task-external-downloads.md`. Nenhum instalador novo foi gerado.
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
- O Worker atualizado está em produção e `/health` confirma sua revisão.
  A chave canônica `CURSEFORGE_API_KEY` está operacional; `_2` não é utilizada.
  Não há fallback para API não oficial.

## Validação operacional em 17/09/2026

- Revisão: `2026-09-17-integrated-catalog-skins-updates-v1`.
- Deployment: `d9669ec0-50ed-4eec-a59f-337b13244a7d`.
- `/health` e `/health/curseforge`: HTTP 200 após o deploy integrado.
- 56 testes do Worker aprovados, incluindo os fluxos CurseForge e skins.
- Build integrado do frontend aprovado; release coordenada conforme
  `release-workflow.md`. Consulte o manifesto do instalador para a proveniência.

## Evidência histórica de 01/09/2026

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

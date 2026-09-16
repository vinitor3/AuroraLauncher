# AUR-SKINS-API — mc-api + MCStat

Data: 2026-09-16. Branch: `codex/mcstat-skins-catalog`, base `b13df76`.

## Entrega

- HTTP, DTOs, modelos, cache e fallback separados no Worker; chave opcional por header, sem credencial nos clientes.
- Busca por prefixo, perfil nome/UUID, extração de imagem, histórico opcional e relevância local.
- MCStat para temas/tags e fallback de jogador; catálogo original mantém fluxo sem upstream.
- Desktop: autocomplete 250 ms, cancelamento, grade com rosto/nome e seleção para prévia 3D.
- Nove mods: pesquisa `@nome`/`@UUID`/`#tag`, debounce, prévia da seleção e integração existente ao Core.
- Configuração, três cURLs, exemplos backend/Java/UI e ajuste de cotas em `docs/skin-sources.md`.

## Evidências

- Worker: 43 testes aprovados, check de empacotamento aprovado, TypeScript estrito dos módulos de skins aprovado.
- QA independente: `docs/qa-mcstat-catalog.md`; paginação, 429, PNG inválido, cotas JSON/PNG, alternativa de textura e limites TSV/URL.
- Java: 13 testes compartilhados aprovados, incluindo debounce e busca → perfil → prévia → biblioteca com servidor HTTP local.
- `npm run skins:build` com JDK21: 9/9 JARs; bytecode e ausência de pacotes SLF4J/Minecraft indevidos verificados pelo teste Rust, inclusive Jar-in-Jar de 1.21.1.
- Desktop build aprovado; Rust fmt/clippy aprovados, 48 testes aprovados e 2 testes de rede ignorados por design.
- Worker publicado: versão Cloudflare `da01b3c6-c1a1-4319-a28a-43af9970e0b9`, revisão `2026-09-16-player-skins-v1`.
- Smoke real: nome/UUID Notch, 12 sugestões, perfil mc-api, histórico 1 entrada, PNG HTTP200/526 bytes e SHA-256 igual ao perfil. Catálogo página1: 12 itens (6 Aurora +6 MCStat); `#ninja` respondeu pelas duas fontes.
- Exemplo local `apps/desktop/examples/skins.html`: UI real consultou Notch, carregou avatar3D; catálogo exibiu miniaturas e carregou skin MCStat selecionada. Não houve login, alteração de conta ou aplicação no jogo.

## Limites e pendências

Estado do runtime dos mods: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. A compilação e os testes de protocolo não comprovam abertura de mundo/servidor nem interação visual nas nove combinações.

O secret `mcstat_API_KEY` já existe; `MC_API_KEY` não foi encontrado na lista de nomes dos secrets e o mc-api está em modo anônimo. Nenhum valor foi lido ou registrado.

Instalador alpha.5 preservado. A worktree `codex-aur-content-updater` da tarefa “Refine Aurora visual design” contém alterações não concluídas sobre `8967688`, incluindo UI, versões e backend. Ela não foi modificada, descartada nem mesclada silenciosamente. Um instalador completo exige integrar/validar esse trabalho e usar nova versão de release, pelo gate oficial. Nenhum push foi solicitado.

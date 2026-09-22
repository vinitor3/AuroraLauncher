# AUR-SKINS-API — mc-api + MCStat

## Correção de busca e primeiro uso — 22/09/2026

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO** no instalador. Integração e deploy
pertencem à tarefa de release alpha.9; esta entrega não publica isoladamente.

- Evidência anterior à correção: `GET /v1/skins/profile/Notch` no Worker de
  produção respondeu HTTP 404 (`Jogador não encontrado.`). Na mesma rodada,
  mc-api `/v1/profile/Notch`, Mojang `/users/profiles/minecraft/Notch` e
  sessionserver `/session/minecraft/profile/069a79f444e94726a5befca90e38aaf5`
  responderam HTTP 200. A causa exata da divergência entre o edge e a consulta
  local é **NÃO VERIFICADO**; um índice de terceiros ausente não pode determinar
  sozinho que uma conta Java não existe.
- Fallback oficial Mojang em caso de 404/indisponibilidade do mc-api; MCStat
  continua como alternativa em paralelo. UUID/nome/textura são validados; as
  chaves dos fornecedores não chegam aos endpoints oficiais. 429 mantém
  cooldown, respostas inválidas não liberam downloads e existe deadline total.
- O catálogo abre com skins prontas para o primeiro uso; Buscar perfil pode
  ser acionado durante o autocomplete e cancela sua resposta antiga. Falha de
  sugestão atrasada não substitui o sucesso da busca explícita.
- Seleção online é integrada no App com cache local do PNG, botão **Aplicar
  skin** e avatar a partir da imagem equipada. `SkinFace` compartilha o desenho
  com o avatar; skins legadas não exibem o preenchimento opaco como face preta.
- Testes focados: nove regressões Mojang, rota completa Worker → perfil → PNG
  com SHA-256, três testes de concorrência/UI e três de face/hat legado. Build
  desktop, dry-run Worker e TypeScript estrito dos módulos de skins aprovados.
  Testes automatizados não homologam o Minecraft nem o dispositivo do amigo.
- A revisão independente encontrou e corrigiu dois casos adicionais: cooldown
  de 401/403 conserva status de indisponibilidade (503), permitindo fallback
  para outro jogador; lista MCStat vazia não cancela uma correspondência oficial
  ainda em andamento. As 25 verificações focadas Worker passaram após esses
  reparos; a suíte completa anterior passou em 77 testes.

Decisão: [ADR-020](engineering/ADR/ADR-020-mojang-player-fallback.md).

Smoke da integração detectou que o primeiro deploy do fallback ainda devolvia
503 ao buscar Notch por nome. A consulta por UUID no mesmo Worker respondeu
200, assim como as sugestões de nomes. O resolver passou para o endpoint
oficial atual `api.minecraftservices.com/minecraft/profile/lookup/name/`,
recomendado no WEB-8043 para substituir o endpoint legado com 403 intermitente.
Diagnósticos agora registram somente host do provedor e status/categoria de
falha, sem nomes de jogador, caminhos, corpos ou credenciais. As 25 regressões
focadas passaram após a troca; a integração repetirá o smoke real após deploy.

## Histórico da entrega original

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
- Java: 14 testes compartilhados aprovados, incluindo debounce, busca → perfil → prévia → biblioteca com servidor HTTP local e regressão de metadados da skin ativa.
- `npm run skins:build` com JDK21: 9/9 JARs; bytecode e ausência de pacotes SLF4J/Minecraft indevidos verificados pelo teste Rust, inclusive Jar-in-Jar de 1.21.1.
- Desktop build aprovado; Rust fmt/clippy aprovados, 48 testes aprovados e 2 testes de rede ignorados por design.
- Na rodada final 0.2.3, o teste IPC `rejects_invalid_nonce_or_protocol` teve falha intermitente no handshake local (Windows 10053/WouldBlock). A repetição completa com `--test-threads=1` aprovou os 48 testes; não houve alteração no IPC. Registrar como instabilidade do teste, não como prova de homologação.
- Worker publicado: versão Cloudflare `da01b3c6-c1a1-4319-a28a-43af9970e0b9`, revisão `2026-09-16-player-skins-v1`.
- Smoke real: nome/UUID Notch, 12 sugestões, perfil mc-api, histórico 1 entrada, PNG HTTP200/526 bytes e SHA-256 igual ao perfil. Catálogo página1: 12 itens (6 Aurora +6 MCStat); `#ninja` respondeu pelas duas fontes.
- Exemplo local `apps/desktop/examples/skins.html`: UI real consultou Notch, carregou avatar3D; catálogo exibiu miniaturas e carregou skin MCStat selecionada. Não houve login, alteração de conta ou aplicação no jogo.

## Limites e pendências

Na revisão focal independente, os 14 testes Java foram confirmados sem erro. O desktop também passou a descartar carregamentos antigos de imagem antes de trocar a textura 3D; consulta UUID e avatar foram revalidados visualmente, com build aprovado. A matriz final 0.2.3 foi recompilada integralmente e seus hashes ficam em `releases/skins/0.2.3/manifest.json`.

Revisão final do armazenamento: o teste novo `previewOfSameTextureMustNotChangeSavedModelOrName` reproduziu alteração indesejada de metadados ao visualizar uma textura já salva. Corrigido movendo a gravação de metadados exclusivamente para a importação explícita. **Skins 0.2.3** é a matriz final; 0.2.2 permanece imutável no commit `1b3d9a3`. Nenhuma correção de API mudou depois do deploy descrito acima.

Estado do runtime dos mods: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. A compilação e os testes de protocolo não comprovam abertura de mundo/servidor nem interação visual nas nove combinações.

O secret `mcstat_API_KEY` já existe; `MC_API_KEY` não foi encontrado na lista de nomes dos secrets e o mc-api está em modo anônimo. Nenhum valor foi lido ou registrado.

Instalador alpha.5 preservado. A worktree `codex-aur-content-updater` da tarefa “Refine Aurora visual design” contém alterações não concluídas sobre `8967688`, incluindo UI, versões e backend. Ela não foi modificada, descartada nem mesclada silenciosamente. Um instalador completo exige integrar/validar esse trabalho e usar nova versão de release, pelo gate oficial. Nenhum push foi solicitado.

# Skins: mc-api, MCStat e Aurora

## Como usar

- Launcher: Guarda-roupa → Buscar online → Jogadores. Digite pelo menos três caracteres; selecione uma sugestão ou envie um nome/UUID completo em **Buscar perfil**. A prévia muda; equipar continua sendo uma ação separada.
- Launcher: **Catálogo MCStat + Aurora** pesquisa temas/nomes. Use `#ninja` para filtrar pela tag.
- Minecraft: Skins → Explorar. `@Not` sugere jogadores, `@Notch` pesquisa o nome, `@069a79f444e94726a5befca90e38aaf5` consulta UUID diretamente. Sem `@`, pesquisa catálogo; `#tag` filtra MCStat. Use as setas para selecionar e **Usar skin** para aplicar.
- A prévia do resultado não altera a skin ativa nem adiciona skins à biblioteca. Aplicar/favoritar materializa um PNG validado. Todas as edições suportadas usam a mesma lógica compartilhada.

## API confirmada

Documentação consultada em 16/09/2026: [mc-api](https://mc-api.bisai.dev/docs), [OpenAPI](https://mc-api.bisai.dev/v1/openapi.json), [MCStat](https://mcstat.org/api-docs).

mc-api oferece busca por prefixo de nomes observados (atuais e antigos), perfil por nome/UUID, textura e histórico observado. Não é uma listagem completa de todas as contas. A documentação não oferece catálogo de skins por categoria/tag; `/v1/tags` representa dados do Minecraft, não temas de skins. Temas/tags são atendidos pela API oficial MCStat, com catálogo Aurora original como fallback. Um nome fora do índice ainda pode ser consultado diretamente por perfil no launcher ou via método Java `getPlayerProfile`.

## Arquitetura e credenciais

A implementação principal é **TypeScript/fetch no Worker**. Os mods Fabric/Forge consomem o Worker público, sem segredo e sem dependência HTTP/JSON adicional embutida. Isto mantém Java 8 no legado e evita conflitos de dependências nos modpacks.

- `apps/edge-proxy/src/skins/http.ts`: HTTP limitado, timeout, status, corpo limitado e cooldown.
- `McApiClient.ts`: API oficial `https://mc-api.bisai.dev`; `searchPlayers`, `getPlayerProfile`, `autocompleteSkinNames`, `getSkinImageUrlFromProfile`.
- `dtos.ts` e `models.ts`: DTOs não confiáveis, validação, `SkinResult`, `PlayerSuggestion`.
- `SkinService.ts`: caches em memória, coalescência de chamadas simultâneas, fallback MCStat e último perfil em cache em caso de falha.
- Desktop: `PlayerSkinsClient` e `OnlineSkinSearch`. Debounce de 250 ms e cancelamento de resultados antigos.
- Mod: `AuroraCatalogProvider`, `PlayerSuggestion`, `SearchDebouncer`; transporte HTTPS limitado e protocolo TSV V1 preservado. Busca/download não bloqueiam a thread de render.

No Worker, `MC_API_KEY` é opcional. Sem ela, o mc-api é consultado anonimamente. A chave existente `mcstat_API_KEY` pertence ao MCStat e **não é reutilizada no mc-api**. `MCSTAT_API_KEY` também é aceita como alias. Chaves não são enviadas ao launcher, Minecraft, logs ou URLs de imagem.

Produção (terminal em `apps/edge-proxy`):

```powershell
npx wrangler secret put MC_API_KEY
# Cole a chave somente no prompt privado. Não a inclua no comando.
# MCStat já configurado: não é necessário recriar seu secret.
npm run deploy
```

Local: copie `.dev.vars.example` para `.dev.vars`, preencha apenas nesse arquivo ignorado pelo Git e use `npm run dev`. Nunca coloque a chave em variável `VITE_*`, argumentos JVM ou código Java.

## Três chamadas cURL / Postman

Exemplos PowerShell usando variável de ambiente opcional (chave vazia resulta em chamada pública). No Postman, importe cada cURL e configure `MC_API_KEY` em um ambiente privado, com header `X-API-Key: {{MC_API_KEY}}`; remova o header para modo anônimo.

```powershell
curl.exe --fail-with-body -H "X-API-Key: $env:MC_API_KEY" "https://mc-api.bisai.dev/v1/players/search?q=Not&limit=12"
curl.exe --fail-with-body -H "X-API-Key: $env:MC_API_KEY" "https://mc-api.bisai.dev/v1/profile/Notch"
curl.exe --fail-with-body -H "X-API-Key: $env:MC_API_KEY" "https://mc-api.bisai.dev/v1/profile/069a79f444e94726a5befca90e38aaf5/skins?limit=12"
```

## Uso no backend e no mod

```ts
const skins = new SkinService(env);
const suggestions = await skins.autocompleteSkinNames("Not");
const profile = await skins.getPlayerProfile(suggestions[0].uuid ?? suggestions[0].name, true);
const head = skins.getSkinImageUrlFromProfile(profile, "head"); // PNG de rosto
const texture = skins.getSkinImageUrlFromProfile(profile, "skin"); // atlas para modelo 3D
```

```java
runtime.catalogProvider().searchDebounced("@Not", 0, 12).thenAccept(page -> {
    // Volte à thread do Minecraft antes de atualizar widgets.
    // As telas AuroraSkinsScreen/LegacySkinsUi fazem isso e renderizam nome + avatar.
});
runtime.catalogProvider().getPlayerProfile("Notch").thenCompose(
    entry -> runtime.catalogProvider().preview(entry)
); // sem aplicar: PNG local validado disponível em entry.file()
```

As rotas Aurora são `GET /v1/skins/players?query=Not`, `GET /v1/skins/profile/Notch?history=1` e `GET /v1/skins/catalog?format=json&query=%23ninja`. O perfil retorna a textura original e `skinUrl` pública do próprio Worker, com SHA-256 verificado. Histórico é opcional e uma falha nele não impede carregar a skin atual. Renders head/body são URLs públicas sem chave; o avatar local usa a textura, não depende de um render remoto.

## Limites e falhas

- HTTP mc-api: 3 s; MCStat: 2,5 s por chamada; catálogo inteiro: orçamento de 9 s; leitor desktop/mod: 12 s.
- Cache em memória: busca 60 s, perfil 5 min, último perfil em falha até 1 h; máximo 256 registros. Catálogo MCStat 5 min e 256 páginas, imagens 1 h e 64 entradas. Cache é por processo/isolate, não persistente/global.
- `SKIN_API_REQUESTS_PER_MINUTE` (Worker var) ajusta o orçamento das consultas de jogador, padrão 30/min, máximo 120. No código, o catálogo MCStat tem orçamento separado de 30/min em `fetchMcstatPage`; rotas de jogador têm proteção por IP de 60/min. Respeite também as cotas do seu plano externo.
- HTTP 429 respeita `Retry-After`; não há repetição imediata em autocomplete. 401/403 entram em espera, JSON/URLs inválidas são rejeitados. Logs contêm operação/status/cache hit/miss, nunca credencial.
- Texturas: host/path permitidos, HTTPS, sem redirect, limite 256 KiB no Worker, dimensões 64×64/64×32, CRC e dados PNG íntegros, hash conferido antes de servir/aplicar.
- Sem rede: catálogo original e biblioteca local continuam disponíveis. Nome não encontrado/limite/falha aparece como mensagem; não se aplica uma skin aleatória.

## Verificação

`npm --prefix apps/edge-proxy test`, `npm --prefix apps/edge-proxy run check`, `npm --prefix apps/desktop run build` e `npm run skins:build`.
Execute o script de matriz com **JDK 21** em JAVA_HOME (os projetos antigos selecionam toolchain 17 e bytecode Java 8). O exemplo visual do desktop em `examples/skins.html` usa as mesmas peças de busca e não salva conta/skin.

Estado de gameplay do Skins 0.2.2: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. As correções anteriores de avatar, botão e SLF4J permanecem; build não substitui teste visual/mundo/servidor nas nove combinações.

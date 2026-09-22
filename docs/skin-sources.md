# Skins: mc-api, MCStat e Aurora

## Como usar

- Launcher: Guarda-roupa → Adicionar skin abre **Catálogo de skins**, com opções MCStat + Aurora prontas para selecionar. Use `#ninja` para filtrar pela tag. **Aplicar skin** guarda a aparência neste computador sem exigir que o Minecraft esteja aberto.
- Launcher: **Jogadores** aceita o nome ou UUID da conta Minecraft Java. Sugestões aparecem a partir de três caracteres; **Buscar perfil** continua disponível enquanto elas carregam. A seleção baixa e guarda o PNG; aplicar continua sendo uma ação separada.
- Minecraft: Skins → Explorar. `@Not` sugere jogadores, `@Notch` pesquisa o nome, `@069a79f444e94726a5befca90e38aaf5` consulta UUID diretamente. Sem `@`, pesquisa catálogo; `#tag` filtra MCStat. A grade mantém 12 slots: clique numa skin pronta para selecionar e use **Usar skin** para aplicar. As setas navegam páginas inteiras; uma pesquisa nova volta à primeira página sem seleção.
- Selecionar um resultado não altera a skin ativa. No desktop, a seleção online já guarda o PNG na biblioteca para permitir aplicar sem uma nova consulta remota. No mod, aplicar/favoritar materializa o PNG validado.

## Biblioteca desktop e recuperação da equipada

Correção de 17/09/2026, posterior ao instalador alpha.6:

- **Minhas skins** mostra a biblioteca; **Equipada** abre a imagem e o modelo
  efetivamente aplicados. Selecionar uma prévia não equipa automaticamente.
- Arquivo e URL são guardados como PNG no IndexedDB, separado por conta. As
  escritas só concluem após o commit da transação. URLs `blob:` não persistem.
- Na reabertura, o PNG nativo equipado é comparado byte a byte com a biblioteca;
  seu ID e modelo ficam persistidos separadamente. Se faltar a cópia nativa,
  ela pode ser restaurada pelo PNG do ID equipado no IndexedDB.
- Registros antigos com apenas URL são recuperados quando a origem responde,
  com duas consultas simultâneas no máximo. Falha não remove registros nem
  substitui um PNG válido por uma imagem vazia. Sem arquivo local nem origem
  disponível, é necessário importar o PNG novamente.
- URLs temporárias só são liberadas após publicar a nova lista/prévia. A skin
  equipada usa uma imagem independente das URLs temporárias da biblioteca.
  Miniatura e 3D mostram um aviso quando a imagem falha; o 3D tem limite de 15 s.
- Iniciar Minecraft aguarda a recuperação inicial e usa o modelo equipado local,
  não um modelo remoto antigo. Migrações de avatar já em andamento terminam
  antes do envio explícito de uma nova equipagem, evitando sobrescrita atrasada.
- Para remover a skin em uso, equipe outra primeiro. Demais skins não são
  apagadas pela recuperação. Respostas de uma conta anterior não mudam a atual.

Fixture reproduzível: `npm --prefix apps/desktop run dev -- --host 127.0.0.1 --port 1428`,
abrir `/examples/wardrobe-recovery.html`. Usa somente PNG sintético e proprietário
`qa-wardrobe`; recarregar verifica IndexedDB real, e os botões simulam falha e
restauração da imagem. Não autentica nem altera dados pessoais. A navegação
Equipada e o envio ao launch são cobertos por testes das funções reais do App,
não por uma cópia da interface no fixture.

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO** no launcher empacotado. Build,
testes automatizados e smoke visual dos componentes passaram; reabrir a próxima
build nativa com conta real e confirmar no Minecraft ainda **PRECISA DE TESTE**.

## API confirmada

Documentação consultada em 16/09/2026: [mc-api](https://mc-api.bisai.dev/docs), [OpenAPI](https://mc-api.bisai.dev/v1/openapi.json), [MCStat](https://mcstat.org/api-docs).

mc-api oferece busca por prefixo de nomes observados (atuais e antigos), perfil por nome/UUID, textura e histórico observado. Não é uma listagem completa de todas as contas. A documentação não oferece catálogo de skins por categoria/tag; `/v1/tags` representa dados do Minecraft, não temas de skins. Temas/tags são atendidos pela API oficial MCStat, com catálogo Aurora original como fallback. Um nome fora do índice ainda pode ser consultado diretamente por perfil no launcher ou via método Java `getPlayerProfile`.

## Arquitetura e credenciais

A implementação principal é **TypeScript/fetch no Worker**. Os mods Fabric/Forge consomem o Worker público, sem segredo e sem dependência HTTP/JSON adicional embutida. Isto mantém Java 8 no legado e evita conflitos de dependências nos modpacks.

- `apps/edge-proxy/src/skins/http.ts`: HTTP limitado, timeout, status, corpo limitado e cooldown.
- `McApiClient.ts`: API oficial `https://mc-api.bisai.dev`; `searchPlayers`, `getPlayerProfile`, `autocompleteSkinNames`, `getSkinImageUrlFromProfile`.
- `dtos.ts` e `models.ts`: DTOs não confiáveis, validação, `SkinResult`, `PlayerSuggestion`.
- `SkinService.ts`: caches em memória, coalescência de chamadas simultâneas, fallback MCStat/Mojang e último perfil em cache em caso de falha.
- `MojangClient.ts`: consulta oficial de nome exato em `api.mojang.com` e textura pública em `sessionserver.mojang.com`, sem chave. Valida UUID do perfil e da propriedade `textures`, nome, modelo e origem `textures.minecraft.net`.
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
    // As telas AuroraSkinsScreen/LegacySkinsUi desenham 12 modelos; metadados ficam fora dos cards.
});
runtime.catalogProvider().getPlayerProfile("Notch").thenCompose(
    entry -> runtime.catalogProvider().preview(entry)
); // sem aplicar: PNG local validado disponível em entry.file()
```

As rotas Aurora são `GET /v1/skins/players?query=Not`, `GET /v1/skins/profile/Notch?history=1` e `GET /v1/skins/catalog?format=json&query=%23ninja`. O perfil retorna a textura original e `skinUrl` pública do próprio Worker, com SHA-256 verificado. Histórico é opcional e uma falha nele não impede carregar a skin atual. Renders head/body são URLs públicas sem chave; o avatar local usa a textura, não depende de um render remoto.

## Limites e falhas

- HTTP mc-api: 3 s; MCStat: 2,5 s; Mojang: 2 s por chamada; busca/perfil: orçamento total de 7,5 s; catálogo/PNG da rota: orçamento de 9 s; leitor desktop/mod: 12 s.
- Se mc-api responder 404 ou estiver indisponível, Mojang e MCStat (quando configurado) são consultados em paralelo; a primeira resposta válida vence e cancela a outra. O fallback oficial resolve nomes exatos/UUID; não oferece busca por prefixo. Um prefixo ausente retorna sugestões vazias, sem afirmar que a conta não existe. Falha de rede oficial é apresentada como indisponibilidade, e não como jogador inexistente.
- Cache em memória: busca 60 s, perfil 5 min, último perfil em falha até 1 h; máximo 256 registros. Catálogo MCStat 5 min e 256 páginas, imagens 1 h e 64 entradas. Cache é por processo/isolate, não persistente/global.
- `SKIN_API_REQUESTS_PER_MINUTE` (Worker var) ajusta o orçamento das consultas de jogador, padrão 30/min, máximo 120. No código, o catálogo MCStat tem orçamento separado de 30/min em `fetchMcstatPage`; rotas de jogador têm proteção por IP de 60/min. Respeite também as cotas do seu plano externo.
- HTTP 429 respeita `Retry-After`; não há repetição imediata em autocomplete. 401/403 entram em espera, JSON/URLs inválidas são rejeitados. Logs contêm operação/status/cache hit/miss, nunca credencial.
- Texturas: host/path permitidos, HTTPS, sem redirect, limite 256 KiB no Worker, dimensões 64×64/64×32, CRC e dados PNG íntegros, hash conferido antes de servir/aplicar.
- Sem rede: catálogo original e biblioteca local continuam disponíveis. Nome não encontrado/limite/falha aparece como mensagem; não se aplica uma skin aleatória.

Correção desktop de 22/09/2026: o rosto da conta prioriza o PNG equipado local,
inclusive quando a sincronização online falha. Miniaturas e avatar usam a mesma
regra de transparência para o chapéu de skins legadas 64×32; o preenchimento
opaco da área não utilizada não cobre mais a face. O atlas original é preservado.

## Verificação

### Reparos nos mods — Skins 0.2.4 / Core 1.1.4

O cliente Java envia `User-Agent: AuroraSkins/0.2.4` em catálogo, busca,
perfil e PNG. Foi reproduzido HTTP 403 com o identificador Java padrão e
sucesso com o identificador do produto, sem relaxar TLS ou enviar secrets.
Reutilizar uma página válida do cache também restaura o estado online.

Prévia e skin ativa possuem ciclos de vida separados: fechar um seletor não
libera a textura de outro; respostas antigas não substituem a seleção atual.
Na 1.12.2, o slot SKIN real de `NetworkPlayerInfo` substitui campos antigos
inexistentes, preserva capa/elytra e restaura textura/modelo como um par.
O legado usa os nomes SRG corretos dos 12 componentes do modelo e restaura
o estado OpenGL ao terminar, inclusive em falha. O Core cede a tela ao módulo
durante resize, evitando renderizadores e botões duplicados.

Removidos os painéis roxos atrás da prévia, preservando o botão do menu.
O conteúdo da UI 1.21 não recebe uma segunda passagem de blur. A rotação
converte a pose do mouse para os eixos do modelo antes de desenhar.

Sondagem opcional da classe Java real (pasta nova de cache, nunca biblioteca pessoal):

```powershell
javac --release 8 -cp apps/aurora-skins/core/build/libs/aurora-skins-core-0.2.4.jar -d .aurora/catalog-smoke scripts/SkinCatalogSmoke.java
java -cp '.aurora/catalog-smoke;apps/aurora-skins/core/build/libs/aurora-skins-core-0.2.4.jar' SkinCatalogSmoke .aurora/catalog-smoke-new-run
```

Em 19/09, Java 8 validou catálogo e PNG MCStat; a primeira consulta de perfil
retornou 404, e a repetição controlada passou. A API pública respondeu 200 na
checagem direta. Isso registra intermitência externa, não garantia de
disponibilidade. Não houve alteração/deploy do Worker nesta entrega.

**PARCIAL**: testes gráficos focados em quatro combinações e skin dentro de
um mundo legado isolado. [Evidências e limites](task-skins-runtime-repair.md).

`npm --prefix apps/edge-proxy test`, `npm --prefix apps/edge-proxy run check`, `npm --prefix apps/desktop run build` e `npm run skins:build`.
Execute o script de matriz com **JDK 21** em JAVA_HOME (os projetos antigos selecionam toolchain 17 e bytecode Java 8). O exemplo visual do desktop em `examples/skins.html` usa as mesmas peças de busca e não salva conta/skin.

Estado de gameplay do Skins 0.2.3: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. As correções anteriores de avatar, botão e SLF4J permanecem; build não substitui teste visual/mundo/servidor nas nove combinações.

## Grade in-game — Skins 0.2.5

Explorar mantém 12 slots, mesmo com resposta curta ou vazia. Apenas `hasMore` habilita Próxima; não há total de páginas inferido nem retorno circular. Loading desabilita as duas setas, e a página visível só muda ao receber sua resposta. Falhas oferecem nova tentativa fora da grade. Seleção começa vazia e acompanha a identidade da entrada em atualizações da mesma página.

Cada slot possui renderer/textura próprios; o preview global do Core permanece independente. A pose dos cards é fixa. `catalogProvider.preview(entry)` usa os dois workers existentes, deduplica PNGs em andamento, mantém até 96 prévias em memória e reutiliza cache validado em disco. PNG compartilhado não mistura os modelos Slim/Classic. Resize recalcula apenas geometria; sair da aba ou fechar libera as texturas. Prévia não importa nem equipa; Aplicar, Favoritar, Modelo e Importar mantêm suas ações explícitas anteriores.

Evidências, comandos e limitações: [registro da grade](task-skins-grid.md).

# QA independente — catálogo MCStat e busca de jogadores

Data: 2026-09-16. Revalidação: **CONCLUÍDO** no escopo testado. Todos os achados reproduzidos nesta revisão foram corrigidos e revalidados. Produto: **IMPLEMENTADO MAS NÃO HOMOLOGADO**; gameplay não foi verificado.

## Resultado final

`npm --prefix apps/edge-proxy test`: **43 testes passaram, 0 falharam**, sem skips ou TODOs, código de saída 0. Composição: 35 testes anteriores + 7 testes da revisão independente + 1 novo teste de cota compartilhada JSON/PNG.

`npm --prefix apps/edge-proxy run check`: **CONCLUÍDO**, Wrangler `4.127.0`, bundle 91,36 KiB (gzip 22,84 KiB), encerrado em `--dry-run: exiting now`. Nenhum deploy.

A execução anterior de 42 testes com quatro falhas é histórica e foi substituída por esta revalidação. Os quatro achados de players e os achados anteriores de catálogo passaram nos testes atuais.

## Escopo e limites

Referência operacional: `AGENTS.md`. Revisados os módulos de `apps/edge-proxy/src/skins/`, suas rotas em `index.ts`, `playerSkins.ts`, `OnlineSkinSearch.tsx` e os contratos relevantes do consumidor Java.

Nesta rodada foram editados somente `apps/edge-proxy/src/players.test.ts` e este relatório, via `apply_patch`. Nenhum código de produção, `package.json` ou `mcstat.test.ts` foi alterado pelo QA. Os oito testes de integração do QA em `players.test.ts` exercitam `worker.fetch` com fetch mockado; o cliente desktop real também é exercitado para perfil/cache e autocomplete. Credenciais são placeholders. Sem secrets reais, chamadas MCStat reais, commit ou deploy.

A consulta pública à [documentação mc-api](https://mc-api.bisai.dev/docs) e à [OpenAPI oficial](https://mc-api.bisai.dev/v1/openapi.json) ocorreu na revisão anterior. Esta revalidação é local.

Node `v24.8.0`. Check executado com a instalação existente de Wrangler em `C:/Users/usuario/Desktop/AuroraLauncher/apps/edge-proxy/node_modules/.bin` no PATH do processo, com telemetria desabilitada; sem instalar dependências.

O proprietário informou frontend build e compilação estrita do backend aprovados, e transporte público compatível com Java 8. A revisão conferiu `setReadTimeout(12000)` na fonte Java, em `AuroraCatalogProvider.java:279`. Build/compilação não foram tratados como homologação. Cloudflare distribuído, UI real em navegador/Tauri, instalação/equipamento da skin e gameplay permanecem **NÃO VERIFICADO** por este QA.

## Achados de players encerrados

### QA-PLAYER-QUOTA — JSON e PNG compartilham a cota

**Estado: CONCLUÍDO** no escopo dos testes. Correção em `skins/http.ts:50`–`82`, `skins/McApiClient.ts:58`–`64`, `skins/SkinService.ts:92`–`93` e `index.ts:1579`–`1580`.

O teste em `players.test.ts:179` confirma que, com limite de duas chamadas, três URLs distintas iniciam somente dois downloads; o terceiro pedido retorna 429. Um hash inventado não contorna o limite.

O novo teste em `players.test.ts:301` comprova compartilhamento: uma consulta JSON de perfil e seu PNG esgotam a mesma cota de dois. Uma busca JSON posterior e outro PNG são bloqueados antes do fetch. Perfil e imagem em cache continuam disponíveis sem gasto adicional. Após avançar o relógio em 60.001 ms, uma nova consulta funciona.

O cenário roda sem `MC_API_KEY` e com chave fictícia, conferindo ausência/presença do header. Inclui, portanto, o modo anônimo informado pelo proprietário. A recuperação de cooldown após 429 em `players.test.ts:160` também passa. Evidência por instância do serviço/isolate; coordenação entre instâncias Cloudflare não foi demonstrada.

### QA-PLAYER-ASSET — PNG alternativo é usado quando o primário falha

**Estado: CONCLUÍDO**. Correção em `skins/McApiClient.ts:38` e `index.ts:1772`–`1776`; teste em `players.test.ts:197`.

A fonte primária retorna 404, enquanto `renders.skin_png` fornece imagem válida. A rota de perfil agora retorna 200 e o download do asset publicado também retorna 200. O adaptador preserva a alternativa; a rota tenta utilizá-la sob deadline de 9 segundos. O código impede fallback após 429 ou deadline expirado.

### QA-PLAYER-TSV — metadados úteis preservados em até 12 KiB

**Estado: CONCLUÍDO**. Correção em `index.ts:1740`–`1750`, chamada em `index.ts:1763`; teste ampliado em `players.test.ts:213`.

O teste usa doze sugestões com campos extensos antes dos campos importantes. Confirma preservação de `current=false`, `last_seen`, `name`, `uuid` e um campo adicional de 11.600 caracteres que ainda cabe. O campo excedente é omitido e `truncated=true` sinaliza a redução.

Cada objeto decodificado cabe em **12.288 bytes**, abaixo dos **16.384 bytes** aceitos pelo Java em `AuroraCatalogProvider.java:92`. O TSV completo, com base64url e doze linhas, mediu **188.494 bytes**, abaixo de **262.144 bytes**. Nenhuma sugestão foi perdida.

### QA-PLAYER-URL — origem extensa rejeitada antes de baixar ou publicar

**Estado: CONCLUÍDO**, com a decisão explícita do proprietário de rejeitar origens acima de 1.000 caracteres. Correção em `skins/models.ts:46`–`47`; teste ajustado em `players.test.ts:245`.

Justificativa: o teste anterior exigia aceitar uma origem de aproximadamente 1.500 caracteres e gerar um asset utilizável. Rejeitar antecipadamente essa origem também satisfaz o requisito original de não publicar URLs inutilizáveis. Manter a exigência de aceitação contrariaria o contrato escolhido.

A fixture conserva a origem extensa, define `renders: {}` e não configura fallback MCStat, eliminando qualquer PNG alternativo válido. O teste exige:

- rejeição da origem por `skinImageUrl`;
- perfil responder **HTTP 502** com erro de perfil inválido;
- resposta não conter `skinUrl` nem `contentHash`;
- exatamente **uma chamada upstream**, apenas ao JSON do perfil, sem download da imagem.

O limite deixa o transporte base64url abaixo de 1.800 caracteres e margem para o campo Java de 2.048 bytes. O teste verifica a rejeição antes de qualquer asset ser baixado ou publicado.

## Catálogo e orçamento revalidados

| Verificação | Evidência atual | Estado no escopo testado |
| --- | --- | --- |
| PAG-01: posições após filtragem | Teste original passa, sem repetição da primeira skin da página seguinte. | **CONCLUÍDO** |
| QUOTA-01: cooldown MCStat após 429 | Quatro consultas idênticas geram somente uma chamada upstream durante a espera. | **CONCLUÍDO** |
| PNG-01: cabeçalho truncado de 24 bytes | Catálogo descarta; asset retorna 502. | **CONCLUÍDO** |
| PNG-02: IDAT inválido com CRC correto | Decodificação limitada rejeita a imagem. | **CONCLUÍDO** |
| JAVA-01: leitura anterior de 4 segundos | Fonte Java usa 12 segundos; cenário saudável terminou em 5.142 ms. | **CONCLUÍDO** para configuração e cenário testado |
| Orçamento global máximo de 10 segundos | Deadline de 9 segundos interrompeu uma imagem em 9.005 ms. | **CONCLUÍDO** no teste de cancelamento |

O teste em `players.test.ts:263` usa duas listagens e treze imagens, cada fetch programado para 2.300 ms, abaixo do timeout individual. Sem deadline, as etapas e duas ondas de imagens levariam cerca de 9.200 ms. O worker abortou a última imagem, respondeu HTTP 200 com os resultados disponíveis e terminou dentro dos 10 segundos solicitados.

Também permaneceram verdes os testes de paginação 7/12/24, coalescência concorrente, TTLs, rejeição de hash antigo com 409, limites de 256 KiB com cancelamento de streaming, bloqueio de redirect/HTML/dimensões inválidas e campos TSV multibyte compatíveis com Java.

## Cliente e UI

`PlayerSkinsClient` continua aprovado nos cenários mockados de perfil, hash, mesma origem, asset em cache e autocomplete. Nenhum código frontend foi modificado pelo QA.

A fonte atual de `OnlineSkinSearch.tsx` foi relida: possui debounce, cancelamento de busca/seleção e proteção contra atualizar resultados de busca abortada. O proprietário também adicionou prévia com `SkinFace`; seu comportamento visual não foi homologado nesta rodada. Não há novo achado de UI demonstrado nesta revisão; teste visual e gameplay permanecem **NÃO VERIFICADO**.

## Reprodução

Na raiz da worktree:

```powershell
npm --prefix apps/edge-proxy test
npm --prefix apps/edge-proxy run check
```

Resultado final: **43/43 testes** e check de empacotamento aprovados. Os arquivos de produção permaneceram sob autoria exclusiva do proprietário.

## Snapshot revalidado

Branch: `codex/mcstat-skins-catalog`. HEAD de referência: `b13df763ef6b8778613ecfccaac077ca607a403d`. SHA-256 observados na execução final:

| Arquivo | SHA-256 |
| --- | --- |
| `apps/edge-proxy/src/index.ts` | `8B813890D43F27A7A60992CEC63C767D7725509C2E4AD670D2EF5376C14DAC21` |
| `skins/http.ts` | `636009121A037645E4E197D248A825146329A9DECD3CCB7F70B2FCCD2E35A0CC` |
| `skins/SkinService.ts` | `E1A41989F8F372D345EF001589808DF63B35C89D8551DDD82C6EF3134CDA39C0` |
| `skins/McApiClient.ts` | `4D43ECB3188CB987D63C3A2D47DA6CC821016DA212B27B653FA5459A1CA9D305` |
| `skins/models.ts` | `F4C5C330BB803545417F5A7558DF896D31A54BA66F2119AF099930FB9ECB1F6B` |
| `apps/desktop/src/lib/playerSkins.ts` | `5373DA8D17E681B8C0640EEFFD35A47FFC7888B3F360A8A26F2F2771D330352E` |
| `apps/desktop/src/features/skins/OnlineSkinSearch.tsx` | `A6942A49B09117DE6F22D617BDF8E62F8559E95DE039C101396D9A7C1C98BAD2` |

Os caminhos `skins/*.ts` abreviados pertencem a `apps/edge-proxy/src/skins/`. Alterações posteriores a esse snapshot exigem revalidar os trechos afetados.

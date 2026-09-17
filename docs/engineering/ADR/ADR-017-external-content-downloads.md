# ADR-017 — Download externo autorizado por identidade do arquivo

Data: 17/09/2026. Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**.

## Contexto

Arquivos CurseForge sem URL de distribuição exigiam uma página oficial no
WebView2. A navegação aceitava o site, mas não seu CDN; cancelamento e conclusão
de tentativas podiam também interferir na fila seguinte.

## Decisão

Manter a rota direta autenticada pelo Worker. Quando indisponível, reutilizar
um arquivo local verificado ou consultar o SHA-1 exato na API oficial Modrinth,
baixando somente o arquivo correspondente de seu CDN HTTPS. Não inferir
equivalência pelo nome, projeto, loader ou número de versão. O ZIP de manifesto
CurseForge permanece excluído dessa alternativa.

Sem alternativa exata, deixar a página visível executar seu próprio fluxo
oficial, aceitando seus redirecionamentos HTTPS para `forgecdn.net`. Capturar
somente o download da sessão, validar o SHA-1 e publicar com rename atômico.
Não fabricar URL, contornar confirmação/CAPTCHA ou ocultar publicidade.

A fila frontend serializa tentativas por UUID. O registro nativo mantém a
exclusão mútua da instância até a publicação/cancelamento e a libera antes do
evento terminal. Cancelamentos anteriores ao registro são lembrados de forma
limitada; finalizações são idempotentes e não publicam downloads cancelados.

## Consequências

O caminho sem interação cobre arquivos publicados também no Modrinth com o
mesmo hash. Outras páginas podem continuar exigindo confirmação. A consulta
adicional tem timeout de oito segundos; sessões têm limite de dez minutos.
O cancelamento da transferência Modrinth é cooperativo e não interrompe
imediatamente uma leitura de socket já bloqueada. Nenhuma chave nova ou
alteração do Worker é necessária. Testes automatizados não substituem
homologação do fluxo visual WebView2 e do modpack no Minecraft.

Referências: [API Modrinth por hash](https://docs.modrinth.com/api/operations/versionfromhash/),
[API CurseForge](https://docs.curseforge.com/rest-api/).

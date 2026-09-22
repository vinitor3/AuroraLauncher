# ADR-020 — Recuperar perfis de skin pela fonte oficial Mojang

Data: 2026-09-22. Estado: aceita para implementação. Complementa ADR-016.

## Contexto

A rota de produção Aurora devolveu `Jogador não encontrado` para Notch, enquanto
consultas locais ao mc-api e às duas APIs oficiais Mojang retornaram o perfil
existente. O motivo exato da diferença no edge não foi comprovado. Tratar a falha
ou ausência em um índice de terceiros como inexistência da conta impede usuários
novos de escolher uma skin sem buscar/importar um arquivo manualmente.

## Decisão

Preservar mc-api como primeira consulta e histórico opcional. Em HTTP 404 ou
indisponibilidade, consultar a API oficial Mojang em paralelo ao fallback MCStat
quando este estiver configurado. Nome exato resolve em `api.mojang.com`; UUID e
textura resolvem em `sessionserver.mojang.com`. A primeira resposta válida vence
e cancela a requisição alternativa. Para nomes, usar a rota atual
`api.minecraftservices.com/minecraft/profile/lookup/name/`, recomendada no
[WEB-8043](https://bugs.mojang.com/browse/WEB-8043), em vez da rota legada
`api.mojang.com/users/profiles/minecraft/`. Mojang nunca recebe os headers de chave dos
outros fornecedores. Limite por endpoint, timeout de 2 s e orçamento total de
7,5 s limitam a operação; não repetir imediatamente respostas 429.

O UUID da resposta e o UUID da propriedade `textures` precisam corresponder ao
solicitado; nome consultado deve corresponder ao perfil oficial. A textura fica
restrita ao host/path Minecraft já permitido, passa pelo mesmo validador PNG e
SHA-256 do Worker e é entregue pela origem Aurora. Falha de rede oficial aparece
como indisponibilidade. Um 404 oficial pode confirmar ausência; no autocomplete,
um prefixo ausente apenas produz lista vazia.

## Consequências

Clientes continuam usando JSON/TSV V1 e URLs Aurora; `mojang` é apenas mais um
valor de origem textual. Não há novos segredos ou dependências nos JARs. Há no
máximo duas fontes de fallback simultâneas, cada uma com sua cota. Mojang não
fornece autocomplete por prefixo nem catálogo de temas; mc-api/MCStat/Aurora
mantêm essas funções. Operações reais após deploy e aplicação da skin no jogo
continuam **PRECISA DE TESTE** até registro pela integração.

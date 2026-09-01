# Catálogo CurseForge protegido

O Aurora consulta o CurseForge pelo Cloudflare Worker público
`https://aurora-api.aurora-edge-proxy.workers.dev`. A chave pertence ao projeto
Aurora e fica no secret `CURSEFORGE_API_KEY` do Worker; ela não entra no código
desktop, no instalador ou na conta dos jogadores. Portanto, quem instala o
launcher não precisa criar uma chave CurseForge.

## Configuração do proprietário

Na pasta `apps/edge-proxy`, autentique o Wrangler, configure os secrets
`CURSEFORGE_API_KEY`, `GEMINI_API_KEY`, `FIREBASE_PROJECT_ID` e, quando exigido
pela validação Firebase, `FIREBASE_WEB_API_KEY`; depois publique com
`npm run deploy`. Nunca coloque os valores em `wrangler.toml` ou em arquivos
versionados. Uma chave já exposta deve ser revogada e substituída.

## Limites do proxy

- rotas privadas exigem um Firebase ID token válido;
- são permitidas apenas operações específicas de catálogo, arquivos,
  dependências e download;
- não existe proxy de URL livre;
- referências de um modpack são resolvidas em lotes pelo Worker, sem expor a
  chave ao cliente e sem enviar arquivos do jogador ao serviço;
- arquivos que o CurseForge não autoriza baixar diretamente nunca são obtidos
  por uma URL construída ou contorno do bloqueio. O launcher abre a página
  oficial, captura o download iniciado por ela, valida o SHA-1 publicado e
  continua automaticamente com o próximo item da fila.

## Fluxos disponíveis no launcher

- busca de modpacks, mods, shaders e pacotes de recursos, com versão do
  Minecraft, Forge/Fabric, ordenação e paginação quando aplicável;
- listagem inicial pelos mais baixados mesmo com a busca vazia;
- instalação de modpacks pelo `manifest.json` oficial, incluindo loader,
  arquivos obrigatórios e a pasta `overrides`;
- instalação automática das dependências obrigatórias de um mod, shader ou
  pacote de recursos adicionado pelo editor;
- downloads diretos em paralelo e verificados por SHA-1; arquivos sem hash são
  recusados;
- somente Forge e Fabric são aceitos como loaders de modpack.

O código do Worker precisa ser publicado antes de distribuir uma versão do
launcher que use as ações em lote `resolveFiles` e `resolveProjects`.

Firebase Functions e Secret Manager do Google não são necessários para esta
integração; Firebase continua responsável apenas pela identidade e pelos dados
do perfil.

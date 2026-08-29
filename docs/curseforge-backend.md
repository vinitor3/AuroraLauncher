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
- são permitidas apenas operações específicas de catálogo e download;
- não existe proxy de URL livre;
- arquivos que o CurseForge não autoriza baixar diretamente são apresentados
  como restrição do provedor, sem expor a chave ao cliente.

Firebase Functions e Secret Manager do Google não são necessários para esta
integração; Firebase continua responsável apenas pela identidade e pelos dados
do perfil.

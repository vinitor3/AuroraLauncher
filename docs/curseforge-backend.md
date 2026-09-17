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
- categorias são consultadas dinamicamente por classe (modpack, mod, shader ou
  resource pack) e a busca aceita somente o identificador numérico retornado
  pelo CurseForge;
- referências de um modpack são resolvidas em lotes pelo Worker, sem expor a
  chave ao cliente e sem enviar arquivos do jogador ao serviço;
- a ação autenticada `updates` identifica o arquivo instalado por SHA-1 ou
  `fileId`, percorre a listagem compatível e só oferece uma release estável mais
  nova para o mesmo Minecraft, classe e loader;
- URLs de CDN nunca são entregues diretamente ao desktop. O Worker assina em um
  ticket HMAC de oito horas os IDs e a URL retornada pela API, revalida a
  assinatura/expiração/host e transmite somente URLs HTTPS em `forgecdn.net`,
  enviando `x-api-key` sem expô-la. Há limites por ticket e IP, e as tabelas
  locais têm poda para não crescerem indefinidamente. Isso atende à
  [autenticação exigida para downloads do CDN](https://blog.curseforge.com/introducing-api-key-authentication-for-curseforge-file-downloads/);
- arquivos que o CurseForge não autoriza baixar diretamente nunca são obtidos
  por uma URL construída ou contorno do bloqueio. O desktop primeiro reutiliza
  o arquivo local com SHA-1 válido ou procura esse SHA-1 exato na API oficial
  Modrinth. Sem cópia idêntica, abre a página oficial CurseForge, captura o
  download iniciado por ela, valida o SHA-1 e continua com o próximo item.
- falhas de filtro, limite, indisponibilidade e autorização recebem mensagens
  distintas; consultas têm timeout e repetem uma vez somente em `429`/`5xx`.

## Fluxos disponíveis no launcher

- busca de modpacks, mods, shaders e pacotes de recursos, com versão do
  Minecraft, Forge/Fabric, categoria, ordenação e paginação quando aplicável;
- listagem inicial pelos mais baixados mesmo com a busca vazia;
- instalação de modpacks pelo `manifest.json` oficial, incluindo loader,
  arquivos obrigatórios e a pasta `overrides`;
- instalação automática das dependências obrigatórias de um mod, shader ou
  pacote de recursos adicionado pelo editor;
- detecção e revisão de atualização para conteúdo instalado; o desktop pede
  novamente o alvo ao Worker imediatamente antes de aplicar e rejeita qualquer
  resposta diferente da que o usuário revisou;
- downloads diretos em paralelo e verificados por SHA-1; arquivos sem hash são
  recusados;
- somente Forge e Fabric são aceitos como loaders de modpack.

O código do Worker precisa ser publicado antes de distribuir uma versão do
launcher que use as ações em lote `resolveFiles`, `resolveProjects` e
`updates`.

## Verificação operacional

- `GET /health` confirma a revisão publicada do Worker sem consultar terceiros;
- `GET /health/curseforge` faz uma busca mínima e lê categorias reais, portanto
  também valida a credencial e o acesso ao catálogo;
- em 1º de setembro de 2026, o Worker atualizado foi publicado, mas a chave
  presente em `CURSEFORGE_API_KEY` recebeu `403 API Key missing or invalid` na
  busca oficial. O proprietário precisa renovar a chave aprovada pelo
  [processo oficial do CurseForge](https://support.curseforge.com/support/solutions/articles/9000208346-about-the-curseforge-api-and-how-to-apply-for-a-key)
  e atualizar o secret com `wrangler secret put CURSEFORGE_API_KEY`.
- em 17 de setembro de 2026, após a configuração da nova chave, o Worker de
  revisão `2026-09-16-player-skins-v1` retornou HTTP 200 em `/health/curseforge`,
  com um resultado de busca e 52 categorias reais. A credencial está operacional.
  O binding utilizado é exatamente `CURSEFORGE_API_KEY`; um secret adicional
  chamado `CURSEFORGE_API_KEY_2` não é utilizado e não ativa rotação automática.
  Este teste confirma acesso ao catálogo; não substitui instalação real de cada
  tipo de conteúdo nem homologação dentro do Minecraft.

Firebase Functions e Secret Manager do Google não são necessários para esta
integração; Firebase continua responsável apenas pela identidade e pelos dados
do perfil.

## Automação de arquivos externos (fonte posterior à alpha.6)

**IMPLEMENTADO MAS NÃO HOMOLOGADO.** Sem instalador novo nesta etapa.

- A alternativa Modrinth usa `version_file/{sha1}?algorithm=sha1`, nunca busca
  por nome nem troca de versão. Somente HTTPS `cdn.modrinth.com` é aceito,
  inclusive nos redirecionamentos; tamanho e hash são conferidos antes do rename.
- O ZIP de manifesto CurseForge não é substituído por um `.mrpack` Modrinth.
- Na página oficial, navegação para HTTPS `*.forgecdn.net` é permitida, corrigindo
  o bloqueio que poderia impedir o download. Não há URLs de CDN construídas,
  scripts de contorno, ocultação da página ou bypass de CAPTCHA/confirmação.
- A fila é serial e cada tentativa tem UUID próprio. Sucesso libera o bloqueio
  de alteração da instância antes de notificar a próxima tentativa. Eventos
  antigos, duplicados ou de downloads rejeitados não concluem outro arquivo.
- Fechar a janela, desmontar a interface ou atingir dez minutos encerra a
  sessão e impede publicação tardia. Cancelamento do mirror é cooperativo
  entre blocos; uma leitura de rede já bloqueada pode demorar até seu timeout,
  mas não pode instalar um arquivo depois do cancelamento.
- Arquivos temporários ficam na mesma pasta do destino e são publicados por
  rename somente após validação. Falha de hash/publicação preserva o arquivo
  existente. Links, junções e caminhos fora da instância são recusados.
- A página ainda pode exigir interação humana. A instalação completa pela
  página real/WebView2 e dentro do Minecraft continua **PRECISA DE TESTE**.

Evidências e comandos: [tarefa de downloads externos](task-external-downloads.md).

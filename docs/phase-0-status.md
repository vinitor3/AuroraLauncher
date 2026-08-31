# Status da Fundação e da Fase 1

Atualizado em 30 de agosto de 2026.

## Fase 0 — Fundação & Identidade

**Concluída no código e no instalador.** O Aurora possui aplicativo Tauri v2 +
React, núcleo Rust, instâncias isoladas, resolução e download do Java correto,
login por nick/senha no Firebase Auth, perfis no Firestore, biblioteca local de
skins e instalador Windows.

O nick aceito segue as regras do Minecraft: 3 a 16 caracteres, apenas letras
ASCII, números e `_`. O mesmo nick autenticado é passado à JVM; o placeholder
`--username` não é mais usado como nome do jogador.

O Firebase fica responsável por Auth e Firestore. O Supabase Storage é o
destino principal de skins/capas pelo Cloudflare Worker; Firebase Storage fica
como fallback transitório. Catálogo CurseForge e Assistente usam o mesmo Worker
`https://aurora-api.aurora-edge-proxy.workers.dev`; as chaves privadas ficam em
secrets do Worker e nunca são distribuídas no launcher.

## Fase atual — 1.1, estabilização do Launcher

Esta etapa está **implementada e compilada**, mas ainda precisa de homologação
visual dentro do Minecraft para ser considerada encerrada:

- downloads com até oito transferências concorrentes, retomada, tentativas,
  hash e publicação atômica;
- deduplicação por arquivo de destino. Isso corrige a falha intermitente
  `os error 2` dos assets quando nomes lógicos diferentes apontam para o mesmo
  hash da Mojang;
- duas barras de progresso: instalação total e arquivo em andamento, mantendo
  nome, bytes, velocidade e quantidade de downloads ativos;
- botão **Abrir pasta** na edição de instância;
- seleção múltipla de mods, shaders e pacotes de recursos para ativar,
  desativar ou desinstalar;
- fallback interno para arquivos CurseForge sem URL direta: o Aurora abre a
  página oficial em uma janela WebView2, aguarda o disparo da própria página,
  captura em arquivo temporário, valida o SHA-1 e instala na pasta correta;
- skin local para o Companion 1.20.1 e conversão correta do layout legado
  64x32 para 64x64;
- foto pública do perfil renderizada a partir da face e da segunda camada da
  única skin equipada online, com biblioteca não equipada mantida apenas localmente;
  somente o documento mínimo `publicProfiles/{uid}` é compartilhado entre contas;
- cards de instância com capa do modpack, início direto, seleção visual e
  estado de processo em execução;
- renomeação segura de instância pelo editor;
- gerenciador de Java com inventário dos runtimes encontrados e instalação
  verificada das linhas 8, 17 e 21;
- normalização segura de Markdown/HTML legado do Modrinth, convertendo imagens,
  links e vídeos sem mostrar tags cruas nem executar iframes.

## Fase 1.2 — Companion e Assistente nativos

Implementado como infraestrutura/protótipo:

- Companion empacotado para Forge 1.12.2 e Fabric/Forge 1.16.5, 1.19.2,
  1.20.1 e 1.21.1; NeoForge não faz parte do escopo;
- IPC WebSocket bidirecional limitado a `127.0.0.1`, com porta efêmera e nonce
  novo por execução;
- painel do Assistente dentro do launcher;
- `AltGr + /`, pedidos por texto/voz, respostas, legendas e Edge TTS pelo IPC;
- conversa Gemini autenticada no Worker, usando Flash-Lite no jogo e fallback
  automático para modelos disponíveis;
- fala em português por `edge-tts-rust`, com legendas de limite de sentença no
  launcher e encaminhamento de legendas para o painel in-game;
- captura de tela opcional: ocorre somente pelo botão **Analisar tela**, depois
  de confirmação explícita a cada captura; nunca é feita silenciosamente;
- upload de skin/capa preparado no Supabase Storage, sempre depois de validar o
  token Firebase no Worker; a Secret key nunca entra no launcher;
- skin/capa por URL HTTPS nas nove combinações e skin por arquivo local no
  Companion 1.20.1;
- o launcher valida o conteúdo remoto antes de salvar: skin precisa ser um PNG
  64x64 ou 64x32 de até 5 MB. Links de páginas, inclusive páginas do NovaSkin
  que não entregam o PNG, são recusados com mensagem curta.

Ainda não implementado nesta fase:

- substituir a janela Swing externa mostrada sobre o jogo por uma tela/HUD
  nativa do Minecraft, sem pausar o mundo e sem impedir o controle normal do
  mouse;
- adaptar essa nova interface às nove combinações de versão/loader;
- homologar visualmente as correções de skin em um mundo real.

## Fase 1.3 — Atualizações de conteúdo

Ainda não implementada:

- identificar mods desatualizados;
- trocar a versão de um mod preservando compatibilidade de Minecraft e loader;
- detectar e atualizar a versão de um modpack.

## Evidências e pendências de validação

Os nove JARs disponíveis foram inspecionados e são arquivos válidos. Os dois
JARs 1.20.1 foram recompilados com o fallback local e a conversão de skin
legada; as outras sete combinações ainda carregam a implementação anterior.
Os testes Rust do IPC validam handshake,
pedido e resposta em ambas as direções. O teste online do Edge TTS produziu MP3
e eventos de sentença. Uma conversa autenticada completa confirmou o Worker e
o modelo `gemini-3.5-flash-lite` no modo in-game.

Em uma homologação anterior, Minecraft 1.20.1 Fabric confirmou carregamento do Companion,
autenticação IPC, tick de cliente, abertura/fechamento do painel e registro de
uma skin PNG 64x64 dentro do gerenciador de texturas do jogo. Minecraft 1.12.2
Forge também foi homologado em execução real com Java 8, nick correto, skin do
perfil, handshake IPC autenticado e abertura/fechamento do painel. O JAR legado
isola WebSocket/SLF4J no namespace do Aurora e remove classes Java 9, evitando
conflitos com o `LaunchClassLoader` e o ASM 5 do Forge antigo. A validação
runtime das outras sete combinações, da capa e a inspeção visual da correção de
skin num mundo ainda são testes pendentes; compilação não deve ser confundida
com validação dentro do jogo.

O armazenamento online de aparências está **ativo**. Em 28 de agosto de 2026,
o Worker publicado confirmou `SUPABASE_API_KEY_SERVICE_ROLE`, o bucket público
`aurora-appearance` e o acesso administrativo. O launcher sincroniza a skin no
Supabase e mantém a cópia local do Companion 1.20.1 como proteção contra falhas
temporárias de rede.

O perfil usado durante o teste continha uma URL NovaSkin que retornava HTTP
403/HTML, e não um PNG. Por isso essa skin específica não pôde ser usada como
prova visual; agora o launcher impede que esse tipo de endereço seja equipado.

## Otimização de downloads

O instalador usa um gerenciador único para Minecraft, Fabric, Forge, assets,
modpacks, conteúdo individual e Java. Lotes independentes fazem até oito
transferências simultâneas, com pool de conexões, retomada de arquivo parcial,
três tentativas, gravação temporária e validação SHA-1/SHA-256/SHA-512 antes da
troca atômica. A interface mostra separadamente a porcentagem da instalação e a
do arquivo atual, preservando o nome, a contagem de arquivos ativos, bytes e
velocidade. Um teste HTTP local comprova concorrência real e integridade.

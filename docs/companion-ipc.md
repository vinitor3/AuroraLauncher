# Canal IPC do Aurora Core

O Launcher abre um WebSocket exclusivamente em `127.0.0.1`, numa porta
efêmera. A cada execução ele cria um nonce aleatório e passa ao Minecraft:

- `-Daurora.ipc.port=<porta>`;
- `-Daurora.session.nonce=<nonce-efêmero>`.

O nonce não é senha, JWT ou token Firebase. Ele não é persistido e expira ao
encerrar a execução. O jogo nunca recebe credenciais Firebase ou chaves Gemini,
CurseForge e Cloudflare.

## Handshake

O Aurora Core no jogo inicia a conexão com `hello`, incluindo nonce, loader,
versão do Minecraft, versão do Core e protocolo. O Launcher encerra a conexão
quando o nonce não corresponde e responde `accepted` somente após autenticá-lo.
Em seguida envia `session`, uma projeção com ids, username, estado e scopes que
deliberadamente não possui tokens. O Companion utiliza esta mesma conexão pela
API do Core.

## Mensagens bidirecionais

Core/módulos para Launcher:

- `telemetry`: FPS, MSPT disponível, memória usada e dimensão;
- `assistantRequest`: identificador, pergunta de até 2.000 caracteres e, de
  forma opcional, a captura confirmada pela pessoa;
- `assistantListen`: solicita ao launcher uma transcrição de voz em português,
  sem entregar ao jogo credenciais do serviço;
- `overlay`: compatibilidade com o pedido antigo de abertura do painel.

Launcher para Companion:

- `toggleAssistant`: abre ou fecha a interface do Assistente;
- `assistantResponse`: resposta final vinculada ao identificador do pedido;
- `assistantCaption`: trecho de legenda gerado durante a fala.
- `assistantTranscript`: texto reconhecido (ou erro curto) vinculado ao pedido
  de voz; uma transcrição válida é enviada automaticamente como pergunta.
- `session`: projeção pública da conta e do perfil Minecraft;
- eventos de módulos com nomes limitados a 64 caracteres e payload limitado.

O servidor mantém filas limitadas, valida nomes/tamanhos, recusa campos com
aparência de credencial e não aceita conexões fora do loopback. O protocolo não
transmite senha, token Firebase, IP de servidor, chat do Minecraft ou arquivos
pessoais.

## Atalho e captura

No Windows, `AltGr + /` é registrado pelo launcher como `Ctrl + Alt + /`. Com um
jogo suportado conectado, o comando é enviado ao Companion; sem jogo conectado,
o mesmo atalho abre o painel interno do launcher. O Companion ainda usa uma
janela Swing externa. A tela/HUD nativa dentro do Minecraft, com controle de
mouse correto, é trabalho pendente da Fase 1.2.

A imagem da tela não faz parte da telemetria. O botão **Analisar tela** mostra
uma confirmação antes de cada captura, reduz a imagem e envia JPEG somente
naquele pedido ao Worker autenticado.

# Desenvolvimento do Aurora Core

O Aurora Core 1.0.0 foi introduzido como mod-base obrigatório para as instâncias Fabric/Forge suportadas pelo Aurora. O Launcher escolhe o artefato pela matriz central, valida assinatura e hash, instala o Core antes do Companion e só então inicia o IPC.

## APIs públicas

- módulos: metadados SemVer, versão mínima do Core e lifecycle isolado;
- eventos: login, logout, alteração de perfil/skin, conexão do Launcher e configurações;
- sessão: id Aurora, UUID Minecraft, username, estado e scopes, sem credenciais;
- configurações: documento JSON por módulo, schema, migração e backup;
- UI: páginas dinâmicas e `AuroraPlayerPreview` version-neutral;
- IPC: envio de mensagens tipadas/extensíveis pelo único socket do Core.

## UI no jogo

O menu padrão de opções recebe o botão **Aurora Options**. A tela central lista páginas registradas dinamicamente, mostra a sessão e renderiza o jogador local em 3D com skin atual, modelo classic/slim, camadas externas e movimento suavizado em direção ao mouse. Se o render não estiver disponível, somente a prévia é omitida; a tela e o jogo continuam utilizáveis.

O botão de Skins aparece apenas quando `aurora_skins` estiver registrado. Assim, Skins continua um módulo independente e não uma dependência obrigatória do Core.

## Inicialização

1. O Launcher resolve Minecraft, loader e Java.
2. Seleciona Core e Companion pela versão exata.
3. Verifica o Core assinado e instala ambos por troca atômica.
4. Abre um WebSocket loopback com nonce efêmero.
5. Passa somente porta, nonce e loader como propriedades JVM.
6. O Core autentica o `hello`, recebe a projeção pública da sessão e publica eventos aos módulos.

O perfil atual do Launcher ainda usa identidade Minecraft offline; por isso a sessão enviada ao Core tem estado `offline`. A futura autenticação oficial pode fornecer estado `authenticated` sem mudar o contrato e sem transmitir tokens ao jogo.

## Estado de validação

Todos os nove alvos compilam e os artefatos 1.0.0 têm tamanho, SHA-256 e assinatura verificados por testes Rust. Os testes Java cobrem compatibilidade SemVer, suavização do avatar, isolamento de listeners, rollback de módulo e migração de configuração. Isso não substitui homologação runtime: cada linha permanece **IMPLEMENTADO MAS NÃO HOMOLOGADO** até executar o roteiro da matriz em uma instância limpa.

# ADR-027 — Temas locais declarativos e recuperação independente

Data: 28/10/2026. Estado: IMPLEMENTADO MAS NÃO HOMOLOGADO.

## Decisão

O Aurora interpreta documentos JSON v1 de aparência. O programa não chama uma IA para gerar temas. A pessoa pode copiar instruções e o documento visual para uma IA externa; somente o resultado que satisfaz o formato fechado é aceito. Texto de tema nunca vira markup, CSS livre, expressão, comando ou permissão.

React reconstrói dados tipados e só oferece componentes internos, cores hexadecimais, fontes do sistema, posições previstas, ícones, imagens PNG, vídeo local silencioso e efeitos internos. Layout permite reordenar os quatro itens de navegação e alternar o lado da barra lateral. Elementos extras são imagens e textos em fluxo normal, sem links, botões funcionais ou sobreposições. Ações, credenciais, autenticação e controles nativos continuam sob responsabilidade do launcher.

O documento inclui recursos base64. URLs, caminhos, SVG enviado pelo usuário, HTML, CSS, scripts e plugins não são aceitos. O seletor de arquivos normaliza imagens para PNG; documentos importados precisam da mesma validação nativa. Vídeo exige contêiner permitido e tamanho limitado; o WebView valida reprodução/metadados antes da prévia. O suporte a codec depende do WebView instalado.

O schema é validado no frontend e no Rust. Limites: documento 34 MiB; quatro recursos; PNG 2 MiB/4096 pixels por lado/8 milhões de pixels; vídeo 20 MiB/Full HD/180 segundos; recursos somados 24 MiB; biblioteca de 32 temas e 96 MiB. PNG animado é recusado. Contraste de texto/painel e texto de ação/destaque precisa atingir 4,5:1. Cenas têm animação delimitada, doze pétalas no máximo, preferência de movimento reduzido e pausa quando o documento está oculto.

## Prévia e recuperação

A prévia só começa após preparar a mídia e obter um token do núcleo nativo. Rust usa relógio monotônico e prazo de dez segundos; confirmação expirada, repetida ou de outra prévia não persiste a escolha. Sem confirmação, o padrão oficial volta, preservando a biblioteca. A associação de tema a uma instância/modpack é local e exige a mesma confirmação. A seleção da instância apenas ativa associações já confirmadas; ela nunca recebe tema de um modpack remoto automaticamente.

Confirmação, erros e restauração ficam fora da árvore tematizada. Há restauração por botão, Ctrl+Shift+F12 e menu na bandeja. O núcleo monitora heartbeat e pode recarregar a WebView travada com a configuração padrão. Ausência de heartbeat por aproximadamente cinco segundos em janela visível tematizada dispara recuperação; prévia expira também com a janela oculta. Não depender apenas de setTimeout do React.

Um marcador durável precede qualquer aplicação personalizada. Encerramento inesperado deixa marcador e desabilita tema geral/associação automática na próxima inicialização. Encerramento normal conserva escolhas confirmadas e cancela prévia pendente. Falha ao persistir restauração mantém a recuperação pendente; não limpar o marcador antes de salvar a configuração segura. Biblioteca/associações continuam disponíveis para nova aplicação explícita.

Armazenamento confinado a visual-themes no diretório de dados do app, com lock de proprietário, arquivos temporários, backup recuperável e rejeição de links. Uma segunda sessão lê os temas disponíveis mas começa no padrão e faz alterações somente em memória; não modifica dados/marcadores do proprietário.

## Consequências e validação

Novos tipos de componentes/efeitos exigem alteração versionada no renderer. Uma IA externa pode gerar documentos inválidos; o erro deve orientar o usuário, sem executar o conteúdo. A validação não promete funcionamento de todo codec ou desempenho constante em todo hardware. Testes sintéticos de formato, DOM, disco e relógio não substituem homologação visual do WebView, de GPU fraca, bandeja e reprodução de mídias reais.

Integração e evidências: [tarefa](../../task-launcher-themes.md), [uso](../../launcher-themes.md). Não altera Worker, autenticação, Core, Skins, Minecraft ou permissões do Tauri. Não gera instalador nem versão de release nesta tarefa.

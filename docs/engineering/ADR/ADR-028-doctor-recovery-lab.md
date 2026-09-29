# ADR-028 — Observações do Doctor e cópias de diagnóstico

Estado: aceita. Implementação: **IMPLEMENTADO MAS NÃO HOMOLOGADO**, validação registrada em AUR-1167.

## Contexto

A recuperação manual já possui identidade verificada, inventário restrito, CAS, prévias persistidas, proteção do estado anterior, journal transacional e desfazer. O Doctor já possui tentativas, diagnóstico local, histórico e orientação Gemini. As próximas entregas precisam preservar esses mecanismos e permitir experimentar mudanças sem editar a origem.

## Decisão

- Compartilhar a identidade registrada da recuperação e uma observação de inventário verificado. Cada observação do Doctor associa explicitamente UUID da instância, UUID da tentativa, fase e fingerprint. Datas e nomes são apresentação, não autoridade de associação.
- Exibir diferenças entre estados efetivamente observados. Limites de captura e ausência de histórico são explícitos. Uma diferença anterior a uma falha é associação temporal, não evidência de causalidade.
- Manter os detalhes de arquivos e perfis locais fora do relatório sanitizado enviado aos provedores de IA.
- Criar cópias com identidade, diretório de jogo, logs e histórico próprios. A origem é bloqueada durante captura, e cópias físicas verificadas impedem compartilhamento mutável por links. A seleção explícita de um mundo cria outra cópia independente; o padrão exclui mundos.
- Persistir proveniência e estado de criação fora do diretório do jogo. Cópias incompletas não podem iniciar. Interrupções conservam evidência suficiente para reconhecer e limpar somente a cópia.
- Aplicação é uma seleção restrita de diferenças suportadas. Comparar a origem atual com a base, validar conflitos e mudanças após prévia, proteger o estado anterior e usar o executor da recuperação. Sem proteção, nenhuma alteração na origem.
- Sugestões de IA não autorizam criação, restauração ou aplicação. Os comandos não executam scripts propostos por IA nem busca automática ilimitada de combinações de mods.

## Consequências

O recurso é uma cópia de teste, não uma sandbox de segurança do sistema operacional. Código de mods continua com as permissões normais do processo. Não há promessa de correção a partir de uma tentativa que deixou de apresentar o erro. Não há ampliação da matriz de Minecraft/loaders, reinstalação do Companion ou alteração de autenticação/sessões. Mundos e dados fora do escopo não são transferidos ao aplicar uma solução. Proteção automática geral antes de updates continua fora desta decisão.

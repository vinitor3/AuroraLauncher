# Correção da tradução — alpha.10

Estado: **CONCLUÍDO** para as correções e testes automatizados abaixo.
Tradução real autenticada da nova versão: **NÃO VERIFICADO** nesta sessão.

O usuário relatou ausência de tradução em Descobrir, no modpack Zombie Invade
100 Days do Modrinth (`l9m9tuPN`, `zombie-invade-100-days`). A página pública foi
obtida em 22/09/2026 e processada pelo normalizador real do App e ReactMarkdown:
168 trechos, 4.091 caracteres traduzíveis, 710 caracteres Han, 38 links e 12
elementos de código. Mistura chinês e inglês. Os novos limites geram quatro
lotes (48/48/48/24 trechos), em vez dos lotes antigos de até 96 trechos.
O segundo modpack mencionado por posição não foi identificado inequivocamente.

Defeitos reproduzidos e corrigidos:

- Uma configuração Firebase oficial antiga sem `workerUrl` perdia o endpoint
  padrão e falhava antes de enviar tradução. A fábrica agora recupera o endpoint
  do projeto correspondente, preservando configurações explicitamente customizadas.
- A resposta da página A podia liberar `detailLoading` da página B ainda pendente.
  A tradução começava no resumo e era descartada quando o corpo chegava. Os
  handlers reais reproduziram `{ activeProject: 'qa-b', detailLoading: false,
  activeProjectFetchPending: true }`. A resposta agora só modifica o pedido ativo.
- Respostas válidas fora de ordem eram rejeitadas. IDs únicos agora determinam
  a associação com o nó original; IDs inválidos/duplicados continuam rejeitados.
- JSON incompleto, truncamento e orçamento de raciocínio podiam encerrar toda
  a tradução. Há fallback limitado, orçamento CJK maior e lotes menores.
- Falha em qualquer lote descartava toda a tradução. Lotes concluídos aparecem
  progressivamente, sobrevivem a falhas/cancelamento e não são reenviados ao retomar.
- Texto CJK sem espaços unia palavras portuguesas em fronteiras de trechos ou
  elementos inline. Separadores são inseridos nos textos traduzidos, mantendo
  marcação, atributos, código protegido e restauração exata do original.

O modelo recebe instruções explícitas para detectar idiomas por trecho e traduzir
páginas mistas, preservando nomes próprios. Prosa CJK copiada integralmente provoca
fallback, em vez de sucesso silencioso. A UI informa quando nenhum texto mudou,
quando não há texto traduzível e que palavras desenhadas em imagens ficam originais.

Evidência automatizada: testes React reais/linkedom com serviço simulado para
progresso, falha de segundo lote, retomada, cancelamento, resposta tardia,
troca de página, desmontagem, marcação e identidades. Testes Worker simulam Gemini,
incluindo mistura PT/EN/JA/ZH, expansão de CJK, truncamento e schema inválido.
Também foram processadas descrições públicas JP/EN (`Eunc4UW0`) e ZH/EN (`mEn7eS3l`).
Fixtures públicas e relatórios de exploração ficam ignorados em `.aurora/`.

Limite desta evidência: a revisão automática bloqueou abrir o launcher com
depuração, com a razão genérica “blocked by policy”. Não houve tentativa de
contornar esse bloqueio. Não foram extraídos tokens nem capturadas telas. Testes
com serviços simulados e build não demonstram tradução real no Gemini; a pequena
página autenticada da alpha.9 não foi reaproveitada como homologação desta correção.

O contrato e os limites permanentes estão em `task-project-translation.md`.
O instalador alpha.10 usa o fluxo oficial, mantendo releases anteriores imutáveis.

Validação antes do empacotamento:

- `node --test apps/desktop/tests/*.test.mjs apps/desktop/src/lib/*.test.mjs`: **140/140**.
- `node --test apps/edge-proxy/src/*.test.ts`: **90/90**.
- Build desktop e `wrangler deploy --dry-run`: aprovados.
- `git diff --check`: aprovado.
- CORS de produção: OPTIONS de origens Tauri retorna 204; POST sem sessão
  retorna 401. Isso valida a barreira de autenticação, não uma tradução autenticada.

# Tarefa — tradução das páginas de projetos

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. A implementação e os testes locais
foram concluídos em 22 de setembro de 2026. A tradução online depende da publicação
do Worker integrado e de uma sessão Aurora; tradução real pelo Gemini ainda
**PRECISA DE TESTE**.

- Um botão circular com ícone de tradução aparece na descrição de mods, modpacks,
  shaders e pacotes de recursos do Modrinth e CurseForge. Ele traduz para português,
  cancela a solicitação em andamento ou restaura o original. Alternar novamente
  reutiliza a tradução enquanto essa página permanece aberta.
- O launcher copia o DOM já renderizado e envia somente os textos ao serviço.
  O retorno ocupa nós de texto, sem substituir títulos, negrito, tabelas, imagens,
  atributos, links ou estilos por HTML gerado. Espaços nas bordas dos trechos são
  preservados. Código, comandos em elementos de código, imagens, SVG, fórmulas,
  URLs isoladas e elementos marcados `translate=no`/`notranslate` não são traduzidos.
  Links das descrições mantêm `target=_blank` e `rel=noopener noreferrer` após a
  sanitização. Texto desenhado dentro de banners/imagens permanece original.
- Textos separados por formatação são enviados juntos, com IDs, para dar contexto
  à tradução. A naturalidade continua sujeita ao modelo; nomes próprios e versões
  têm instrução explícita de preservação. O botão permite conferir o original.
- `/v1/translate` exige Firebase Auth, usa a chave Gemini já existente no Worker
  e aplica limite próprio de 24 solicitações/minuto por conta. Não há nova chave
  no desktop nem tradução automática sem clique.
- Lotes têm até 96 trechos/7.000 caracteres; cada trecho tem até 3.500 caracteres.
  Páginas acima de 60.000 caracteres traduzíveis/1.600 trechos mostram um erro.
  Cada chamada tem duas tentativas de até 20 segundos, apenas para modelo ausente,
  indisponibilidade ou rede; quota não provoca repetição. Corpos de entrada/saída,
  schema JSON, quantidade e ordem dos IDs são verificados. Erros ou respostas
  incompletas mantêm a página original inteira.

Validação local:

```powershell
node --test apps/desktop/src/lib/projectTranslation.test.mjs apps/desktop/src/lib/edge.test.mjs
node --test apps/edge-proxy/src/translation.test.ts
```

Os 17 testes cobrem estrutura do DOM real (linkedom), espaçamento, URLs, código,
tentativa de HTML no texto traduzido, limites de lote/Unicode, resposta incompleta,
cancelamento, autenticação, schema, quota e fallback limitado. O integrador executa
o build desktop e o dry-run do Worker no conjunto final.

A API usa o formato estruturado documentado pelo Google:
https://ai.google.dev/gemini-api/docs/migrate-to-interactions
(campos de `generateContent`: `generationConfig.responseMimeType` e
`generationConfig.responseSchema`).

# Tarefa — tradução das páginas de projetos

Estado: **CONCLUÍDO** para implementação e regressões automatizadas da alpha.10.
Homologação da nova versão com Gemini autenticado: **NÃO VERIFICADO** nesta tarefa.
A alpha.9 havia passado apenas por uma página pequena no WebView2; essa evidência
não demonstrava funcionamento em páginas grandes ou com idiomas misturados.
As correções e a evidência de reprodução estão em `task-translation-repair.md`.

- Um botão circular com ícone de tradução aparece na descrição de mods, modpacks,
  shaders e pacotes de recursos do Modrinth e CurseForge. Ele traduz para português,
  cancela a solicitação em andamento ou restaura o original. O progresso aparece
  por porcentagem e cada lote concluído é exibido. Uma falha/cancelamento conserva
  os lotes prontos; “Continuar tradução” retoma os restantes. Alternar novamente
  reutiliza a tradução enquanto essa página permanece aberta. Respostas tardias
  de uma página fechada ou trocada não alteram a página atual.
- O launcher copia o DOM já renderizado e envia somente os textos ao serviço.
  O retorno ocupa nós de texto, sem substituir títulos, negrito, tabelas, imagens,
  atributos, links ou estilos por HTML gerado. Espaços nas bordas dos trechos são
  preservados; fronteiras sem espaço de textos CJK recebem separadores quando
  necessários entre as palavras em português, inclusive junto de negrito/links.
  Código, comandos em elementos de código, imagens, SVG, fórmulas,
  URLs isoladas e elementos marcados `translate=no`/`notranslate` não são traduzidos.
  Links das descrições mantêm `target=_blank` e `rel=noopener noreferrer` após a
  sanitização. Texto desenhado dentro de banners/imagens permanece original.
- Textos separados por formatação são enviados juntos, com IDs, para dar contexto
  à tradução. O serviço detecta o idioma de cada trecho e aceita chinês, japonês,
  inglês e outros idiomas misturados na mesma página. A naturalidade continua
  sujeita ao modelo; nomes próprios e versões
  têm instrução explícita de preservação. O botão permite conferir o original.
- `/v1/translate` exige Firebase Auth, usa a chave Gemini já existente no Worker
  e aplica limite próprio de 24 solicitações/minuto por conta. Não há nova chave
  no desktop nem tradução automática sem clique.
- O desktop envia até 48 trechos/2.500 caracteres por lote; cada trecho tem até
  2.400 caracteres, ou 800 quando contém CJK. O Worker continua aceitando o
  contrato da alpha.9 (96/7.000/3.500) para compatibilidade.
  Páginas acima de 60.000 caracteres traduzíveis/1.600 trechos mostram um erro.
  Cada chamada tem até duas tentativas de 20 segundos, incluindo modelo ausente,
  indisponibilidade, rede e resposta truncada/incompleta. Quota não provoca
  repetição automática. Corpos de entrada/saída, schema JSON, quantidade e
  unicidade dos IDs são verificados. Respostas fora de ordem são associadas pelos
  IDs. Erros conservam o lote original, sem descartar os lotes já traduzidos.
  O orçamento de saída admite expansão de CJK para português, e raciocínio
  mínimo/desligado evita gastar esse orçamento antes de retornar o texto.

Validação local:

```powershell
node --test apps/desktop/src/lib/projectTranslation.test.mjs apps/desktop/src/lib/edge.test.mjs apps/desktop/tests/projectDescriptionFlow.test.mjs apps/desktop/tests/projectDetailLoading.test.mjs apps/desktop/tests/firebaseConfig.test.mjs
node --test apps/edge-proxy/src/translation.test.ts
```

As regressões cobrem DOM/React reais com linkedom, espaçamento CJK, URLs, código,
tentativa de HTML no texto traduzido, Unicode, resposta incompleta, retomada,
cancelamento, configuração antiga, corrida no carregamento do projeto,
autenticação, schema, quota e fallback limitado. Serviços externos são simulados
nesses testes; isso não comprova qualidade linguística nem acesso Gemini real.

A API usa o formato estruturado documentado pelo Google:
https://ai.google.dev/gemini-api/docs/migrate-to-interactions
(campos de `generateContent`: `generationConfig.responseMimeType` e
`generationConfig.responseSchema`). Configuração de raciocínio:
https://ai.google.dev/gemini-api/docs/generate-content/thinking .

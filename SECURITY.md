# Segurança

## Relatando uma vulnerabilidade

Não publique detalhes de vulnerabilidades em issues. Entre em contato de forma privada com o mantenedor do repositório, descrevendo o componente afetado, impacto, passos mínimos para reprodução e uma prova de conceito segura, se houver.

Evite acessar contas, instâncias ou dados que não pertençam a você. Não inclua senhas, tokens, cookies, chaves privadas, mundos ou arquivos pessoais no relatório.

## Princípios do projeto

- segredos de Gemini, CurseForge e Supabase permanecem no backend;
- JWTs Firebase não são enviados pela linha de comando do jogo;
- o IPC do Companion é restrito a `127.0.0.1` e protegido por nonce efêmero;
- downloads são validados por hash antes da instalação;
- análise de tela depende de ação e confirmação explícitas do usuário;
- logs e mensagens de erro devem redigir credenciais.

## Versões suportadas

Correções de segurança são priorizadas na branch `main` durante a alpha. Builds antigas podem deixar de receber correções sem aviso; use sempre o release mais recente disponibilizado pelo mantenedor.


# ADR-008 — Broker tipado para Gemini Tools níveis 0–2

## Contexto

O chat recebe texto, logs e screenshot. Google deixa a execução da function call sob responsabilidade da aplicação; OWASP recomenda least privilege e aprovação humana.

## Opções avaliadas

1. modelo executar comandos/shell;
2. funções tipadas diretamente em módulos existentes;
3. broker de capabilities entre modelo e domínio.

## Vantagens e desvantagens

Shell é flexível e inaceitável. Chamadas diretas espalham política. Broker centraliza schema/autorização/auditoria, com maior trabalho inicial.

## Riscos

Prompt injection indireta, path traversal, confused deputy, replay de confirmação, loops/custo e log sensível.

## Decisão recomendada

**ADIAR mutação** e criar opção 3. Nível 0 lê dados redigidos; nível 1 produz plano; nível 2 prepara diff/snapshot e só executa após confirmação vinculada ao digest do plano. Não oferecer fetch URL, shell ou escrita arbitrária. Nível 3 permanece fora do roadmap ativo.

## Consequências

Doctor/CAS precisam contratos determinísticos primeiro. Cada tool valida novamente usuário, path e precondição.

## Reversibilidade

Alta: tools são adapters sobre domínio, e Gemini pode ser substituído.


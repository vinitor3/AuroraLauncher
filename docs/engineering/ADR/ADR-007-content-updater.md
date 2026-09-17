# ADR-007 — Updater planificado, compatível e reversível

**Estado:** aceita e implementada no código em 2 de setembro de 2026; homologação runtime pendente.

## Contexto

Instalação existe, mas não há detecção/troca segura de versões. Modrinth fornece IDs/hashes/dependências; CurseForge pode negar download de terceiros.

## Opções avaliadas

1. comparar filename/versão;
2. substituir automaticamente pelo “latest”;
3. inventário normalizado + resolver puro + executor transacional.

## Vantagens e desvantagens

Automação simples é rápida e perigosa. A opção 3 exige metadados persistidos, mas distingue seguro/ambíguo/bloqueado e suporta rollback.

## Riscos

Trocar loader/game, dependência incompatível, downgrade, perda de config e arquivo bloqueado pelo autor.

## Decisão

Adotar a opção 3. A varredura e o plano não tocam nos arquivos instalados. O
usuário confirma a troca; o executor revalida o estado, faz staging, valida
hash/tamanho e só então realiza o commit com journal e snapshot mínimo.

Modrinth é resolvido diretamente pelos endpoints oficiais. CurseForge é
resolvido pelo Worker autenticado, que preserva a chave privada e só emite um
ticket para arquivos autorizados. Quando `downloadUrl` não estiver disponível,
o Aurora mantém o fluxo oficial/manual. Somente Forge e Fabric entram no plano.

## Consequências

O lote separa provedores e exige revisão. Componentes Aurora são excluídos do
updater de conteúdo. Modpacks Modrinth usam o mesmo modelo transacional;
modpacks CurseForge são detectados, mas não aplicados automaticamente enquanto
não houver executor transacional próprio para seu manifesto. CAS pode entrar
depois sem alterar o contrato do resolver.

## Reversibilidade

Alta via journal, manifesto anterior e snapshot mínimo. A implementação ainda
precisa de homologação em instâncias reais antes de ser classificada como
produção.

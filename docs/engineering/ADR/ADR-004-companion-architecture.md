# ADR-004 — Companion com core compartilhado e adaptadores

## Contexto

Nove combinações atravessam Java 8/17/21, LWJGL2/3, mappings e APIs Fabric/Forge incompatíveis. O compartilhamento atual usa reflexão e módulos isolados.

## Opções avaliadas

1. nove implementações independentes;
2. um JAR universal por reflexão;
3. core de protocolo/estado + adaptadores de plataforma/versão.

## Vantagens e desvantagens

Independentes maximizam compatibilidade e duplicação. JAR universal simplifica distribuição, mas aumenta branches/reflection e falhas runtime. Adaptadores equilibram reuso e APIs reais.

## Riscos

Abstração “comum” vazar tipos Minecraft; toolchain único quebrar Java 8.

## Decisão

**MELHORAR** para core Java mínimo sem classes Minecraft: DTO IPC, estado do Assistente, filas e sanitização. Keybind, screen/HUD, render, textura e lifecycle ficam em adaptadores. 1.12.2 continua projeto isolado.

## Consequências

Testes do core sem jogo; cada adaptador ainda exige runtime. Protocolo IPC ganha versão/capabilities.

## Reversibilidade

Média; o protocolo compartilhado reduz custo de trocar API de render.

## Estado

Aceita e parcialmente implementada pelo Companion 0.2.0 + Aurora Core 1.0.0.
Os JARs publicados ainda têm defeitos de metadata/ordem de inicialização e a UI
nativa continua pendente; consulte o dossiê e a matriz antes de promover o
estado para homologado.

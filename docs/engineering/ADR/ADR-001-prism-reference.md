# ADR-001 — Prism como referência, não componente

## Contexto

Aurora já possui engine Rust própria. Prism é GPL-3.0-only e MultiMC é Apache-2.0 com branding/API keys fora da licença de código.

## Opções avaliadas

1. incorporar/forkar Prism;
2. manter engine própria e estudar comportamento/formato;
3. substituir pelo core XMCL/JavaScript.

## Vantagens e desvantagens

Fork entrega maturidade, mas reabre arquitetura C++/Qt, migração e atribuição. Engine própria preserva produto atual, porém assume compatibilidade. XMCL tem partes MIT, mas mudaria fronteiras e alguns cores estão arquivados.

## Riscos

Copiar código sem rastrear licença/copyright ou continuar divergindo silenciosamente de formatos reais.

## Decisão recomendada

**MANTER** engine Rust. Usar Prism/MultiMC/Modrinth App/ATLauncher como referência de teste e, quando necessário, portar apenas trechos identificados por arquivo, licença, commit e notice. Nunca reutilizar branding/API keys/CDN.

## Consequências

Criar fixtures de compatibilidade e atribuição por contribuição. A descrição pública não chamará Prism de engine do Aurora.

## Reversibilidade

Alta antes do manifesto/CAS estabilizar; depois, migração de instâncias exigirá conversor.


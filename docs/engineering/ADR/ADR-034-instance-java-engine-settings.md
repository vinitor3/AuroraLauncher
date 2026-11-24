# ADR-034 — Configurações locais de Java e engine por instância

Data: 28/10/2026. Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**.

## Contexto

JVM-01/OPEN-07 documentam RAM fixa no launch apesar da fórmula existente. O usuário pediu RAM, inicialização personalizada e argumentos JVM. Já existem gerenciador Java, requisitos por Minecraft/Fabric, IPC, Doctor e recuperação alpha.14. A wave de temas possui `App.tsx`; configurações de execução não podem ser conteúdo de tema.

## Decisão

1. Persistir um schema fechado v1 no caminho fixo `.aurora/launch-settings.json` por instância, com publicação por substituição, bloqueio de mutação e validação nativa. Ausência de campos conhecidos conserva 4096 MB manual/GC padrão/sem flags.
2. Resolver RAM no backend; modo automático usa RAM física e contagem limitada de JARs. Falha de inventário não inventa zero ou uma medição. Registrar a memória efetiva no Doctor.
3. Tratar a entrada avançada como uma lista limitada de argumentos de desempenho, sem shell/argfiles/agentes/entrypoint/classpath/propriedades livres. Heap, identidade, diretórios e IPC continuam sob controle do engine.
4. Oferecer GC padrão ou G1 explícito para Java >=17; preservar padrão abaixo disso. Evitar pacotes de flags experimentais sem especificação ou medição. O ganho de desempenho continua **NÃO VERIFICADO**.
5. Remover variáveis de ambiente que injetam opções na JVM do jogo e manter a API antiga `prepare_launch` como wrapper compatível. Não alterar probes/installers Java nesta entrega.
6. Entregar painel isolado e patch pequeno de montagem para o dono de `App.tsx`; consumir o contrato de tokens de temas sem incorporar configurações no JSONC nem copiar alterações em andamento.

## Consequências

RAM total não mede disponibilidade no momento do spawn; a reserva é uma heurística. O inventário físico é Windows; modo automático em outros sistemas precisa de implementação posterior. Flags permitidas podem não existir na JVM escolhida. Inicialização real, interface nativa e compatibilidade por Minecraft/loader ainda exigem prova. Sem hooks ou comandos pré/pós-jogo porque não há especificação para eles. Sem instalador, release ou mudança em Core/Skins.

Detalhes, limites e testes: [Java/engine](../../java-engine.md) e [tarefa](https://github.com/vinitor3/AuroraLauncher/blob/088bf5b435b1b63e842c11ff1c01648b5e38bced/docs/task-java-engine.md).

# Fixtures sintéticas do Aurora Doctor

Todos os arquivos deste diretório foram criados para testes automatizados. Não são logs de jogadores ou evidência de homologação do Minecraft.

- `missing-dependency.txt`: declaração explícita do loader sobre dependência obrigatória ausente.
- `ambiguous-stack.txt`: nome de mod em stack trace sem diagnóstico causal suficiente.
- `normal-exit.txt`: encerramento normal, sem erro explícito.
- `memory-failure.txt`: falha explícita da JVM por falta de memória.

Os testes também geram bytes UTF-16, conteúdo inválido, arquivos grandes e valores pessoais fictícios dentro de diretórios temporários próprios. Esses casos verificam os limites de leitura e do relatório compartilhável.

# Aurora Doctor

O Aurora Doctor analisa localmente uma instância selecionada e explica problemas sustentados pelos dados disponíveis. O diagnóstico básico funciona offline, sem Gemini e sem transmitir logs. A análise preserva mods, configurações e mundos e registra somente um histórico local resumido. Uma consulta Gemini opcional pode explicar o resultado estruturado após ação explícita do jogador; ela não substitui os fatos locais. Propostas de alteração seguem um fluxo separado de revisão e confirmação descrito em [Aurora AI Tools](aurora-ai-tools.md).

## Fluxo e evidências

Abra o gerenciamento da instância e a área **Diagnóstico**, então use **Analisar instância**. O launcher destaca o problema principal e o próximo passo. Evidências, demais problemas, gravidade, confiança qualitativa, dados da instância e limites ficam em detalhes expansíveis. O relatório sanitizado pode ser copiado ou exportado.

O registro de inicialização identifica a tentativa, sua configuração e seu resultado. Uma falha de preparação ou um processo encerrado com erro permite abrir diretamente o diagnóstico daquela execução. Encerramento normal e cancelamento voluntário são resultados distintos. Uma execução cujo resultado não pode ser recuperado não é promovida automaticamente a crash.

Os registros ficam em `doctor-runs` no diretório de dados do launcher, fora da instância. A última tentativa usada para análise de logs é associada à identidade física persistente da pasta: novos registros acompanham renomeações e não são herdados por instâncias apagadas e recriadas com o mesmo nome. Registros legados mantêm sua leitura compatível, sem inventar um inventário verificável. Solicitar análise de logs de uma tentativa já substituída pede nova análise do estado atual.

Na alpha.14, a seção **Mudanças recentes e recuperação** mantém observações locais por UUID da tentativa e identidade de recuperação, com hashes de mods/configurações e campos permitidos do perfil. Ela mostra motivos de relevância dos pontos e abre a prévia existente em **Revisar restauração**. Ausência de histórico, ponto indisponível e mudanças posteriores são explícitos. A associação temporal não comprova causalidade. A área **Cópias de teste** permite experimentar em arquivos independentes e consultar resultados por tentativa; suas mudanças históricas são comparadas com o estado da criação, mesmo após novas edições. Esses detalhes privados não entram no relatório sanitizado enviado ao Gemini. Escopo, aplicação seletiva e limites: [Doctor e cópias de teste](diagnostic-lab.md).

Se esse histórico estiver corrompido, inacessível ou não puder ser gravado, a
execução continua sendo acompanhada em memória. A persistência dessa instância
fica desativada até o launcher reiniciar; o arquivo problemático é preservado.
O diagnóstico não bloqueia o jogo por falha de armazenamento do histórico.
Os bloqueios de inicialização concorrente e as verificações de caminhos da
própria instância continuam ativos. Nesse modo, o histórico novo não sobrevive
ao encerramento do launcher.

Logs já existentes antes de uma tentativa precisam de evidência de alteração para serem associados a ela. Conteúdo histórico, ausência de logs e associação incompleta são apresentados como limites da análise. Nomes de mods em stack traces, sozinhos, não demonstram responsabilidade. Quando não existe uma regra sustentada por evidência suficiente, o resultado informa **causa não identificada**.

## Regras e limites

As regras reconhecem declarações explícitas de Java ausente/incompatível, dependência obrigatória ausente, incompatibilidade de Minecraft/loader/mod, identificador de mod duplicado, memória insuficiente, arquivo ausente/corrompido e falha de download/rede. O incremento de 23/09/2026 acrescenta categorias de falha de aplicação de Mixin, falha gráfica explícita, falta de espaço em disco e acesso a arquivo negado. Uma falha de Mixin não identifica, sozinha, o mod responsável. A verificação reaproveita o perfil e os requisitos de Java do launcher. Ela não é um resolvedor geral de dependências ou compatibilidade de mods.

A análise não executa comandos retirados dos logs, não inicia o Java, não baixa arquivos e não faz correções automáticas. A presença de um executável, por si só, não comprova seu funcionamento. Sem uma versão Java verificada na tentativa registrada, a limitação deve permanecer explícita.

A leitura considera `logs/aurora-java.log`, `logs/latest.log`, `logs/debug.log` e relatórios `crash-*.txt` em `crash-reports`. São selecionados no máximo 12 arquivos, com enumeração de até 256 entradas de crash. O conteúdo analisado usa o final de cada arquivo, até 128 KiB por log e 1 MiB no conjunto; a identificação dos arquivos lê também prefixos limitados a 4 KiB. O perfil JSON selecionado fica limitado a 2 MiB. Cobertura parcial é explicitada.

A leitura trata arquivos ausentes, parcialmente escritos, UTF-8 inválido e UTF-16 com BOM ou estrutura reconhecível. Não é um decodificador geral de todas as codificações. Links simbólicos e junctions, inclusive nos ancestrais da instância, são recusados. Arquivos que não podem ser lidos com segurança são ignorados ou causam uma falha de análise compreensível.

O relatório compartilhável usa evidências resumidas e controladas: não é uma cópia dos logs. Caminhos pessoais, credenciais, tokens, contas, nomes de jogador e trechos arbitrários de mensagens não devem sair no relatório. Essa redução pode omitir informação útil; a UI deve deixar claras as limitações, sem prometer um diagnóstico completo.

Sem uma tentativa registrada, logs existentes ficam sem associação e não são usados como causa de uma falha atual. O Doctor ainda verifica fatos da configuração, como executável selecionado ausente e perfil ausente ou inválido. Logs de execuções normais, canceladas, em andamento ou de resultado desconhecido não são convertidos em relatos de crash. A versão Java mostrada provém apenas de uma observação anterior com o mesmo perfil e executável; o requisito exibido é o requisito **base** do Minecraft.

## Explicação opcional com Gemini

Após a análise local, **Explicar com Gemini** solicita uma orientação curta. O botão exige uma sessão Aurora e informa que envia categorias de erro, versões e memória sanitizadas. Abrir a área Diagnóstico ou analisar a instância não aciona Gemini automaticamente.

O payload contém apenas o estado do diagnóstico, versão numérica de Minecraft, categoria do loader, versões principais de Java, memória máxima, resultado categórico da execução, IDs de problemas com confiança/tipo e dois indicadores de cobertura. Campos ausentes ficam nulos ou desconhecidos. Não são enviados logs, títulos/explicações/evidências livres, nomes, caminhos, contas, IDs da execução, horários ou o relatório JSON integral. A autenticação identifica a sessão no Worker; o token não é encaminhado ao Gemini.

O Worker valida o contrato antes da consulta e acrescenta apenas instruções e rótulos fixos. O transporte Gemini compartilhado tenta `gemini-flash-latest`, com no máximo um fallback para `gemini-3.8-flash` se o alias estiver ausente ou retornar 503; quota e autorização recusada não geram repetição. O orçamento total é 35 segundos no Worker, incluindo a validação de sessão, e 40 segundos no cliente, incluindo a obtenção do token. Entrada Doctor: até 8 KiB; resposta do provedor e resposta lida pelo cliente: até 32 KiB. Doctor e Tools compartilham seis consultas por conta por minuto na memória do Worker; não é uma quota global persistente entre todas as réplicas.

A resposta validada tem resumo de até 160 caracteres, explicação de até 240, confiança qualitativa, até três categorias de evidência presentes no relatório e de um a três IDs de próximos passos permitidos. O texto dos passos vem do launcher, não do modelo. URLs, comandos e padrões conhecidos de conteúdo inadequado são recusados nos campos livres; essa filtragem não comprova a correção semântica do texto do modelo. A UI identifica a orientação como **Sugestão do Gemini**, preserva as evidências locais e não executa ações automaticamente a partir da resposta.

Sem evidência específica, o resultado é normalizado para **causa não identificada**. Rede e Mixin isolados não podem ser promovidos a causa confirmada. Cancelar a consulta, trocar o relatório, a instância ou a conta invalida a resposta anterior. Falhas de rede, autenticação, timeout, limite ou resposta inválida mantêm o diagnóstico local disponível. O compartilhamento do relatório local não incorpora automaticamente a orientação Gemini.

O uso online depende de publicação da rota `/v1/doctor/advice` no Worker configurado. Build local e testes com provedor simulado não comprovam essa publicação ou o funcionamento real do Gemini. Evidência inicial: [task-doctor-gemini.md](task-doctor-gemini.md), [ADR-024](engineering/ADR/ADR-024-doctor-gemini-guidance.md). Estado atual do transporte e implantação: [task-ai-tools.md](task-ai-tools.md).

## Ações confirmadas e histórico

O incremento de 28/09 oferece remoção reversível de mod identificado e ajuste de opções JSON permitidas, com prévia, confirmação, exclusão mútua e restauração dos bytes originais. O histórico local associa categorias de diagnóstico e tentativas a resultados posteriores, sem converter saída normal em prova de correção. Essa restauração é limitada às ações registradas; CAS, backup geral, modo seguro e correções automáticas permanecem **NÃO IMPLEMENTADO**. Contrato, limites e homologação em [Aurora AI Tools](aurora-ai-tools.md).

Evidências executadas e limitações de homologação: [registro da entrega](task-aurora-doctor.md).

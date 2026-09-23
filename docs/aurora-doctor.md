# Aurora Doctor

O Aurora Doctor analisa localmente uma instância selecionada e explica problemas sustentados pelos dados disponíveis. O diagnóstico básico funciona offline, sem Gemini e sem transmitir logs. A análise não modifica mods, configurações, mundos ou outros arquivos da instância. Uma consulta Gemini opcional pode explicar o resultado estruturado após ação explícita do jogador; ela não substitui os fatos locais.

## Fluxo e evidências

Abra o gerenciamento da instância e a área **Diagnóstico**, então use **Analisar instância**. O launcher destaca o problema principal e o próximo passo. Evidências, demais problemas, gravidade, confiança qualitativa, dados da instância e limites ficam em detalhes expansíveis. O relatório sanitizado pode ser copiado ou exportado.

O registro de inicialização identifica a tentativa, sua configuração e seu resultado. Uma falha de preparação ou um processo encerrado com erro permite abrir diretamente o diagnóstico daquela execução. Encerramento normal e cancelamento voluntário são resultados distintos. Uma execução cujo resultado não pode ser recuperado não é promovida automaticamente a crash.

Os registros ficam em `doctor-runs` no diretório de dados do launcher, fora da instância. Apenas a última tentativa é conservada por identificador de instância. A associação também verifica a criação da pasta, para evitar herdar a execução de uma instância apagada e recriada com o mesmo nome. Renomear uma instância não transfere o registro anterior para o novo nome. Solicitar pelo atalho uma tentativa já substituída pede uma nova análise do estado atual.

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

O Worker valida o contrato antes da consulta, acrescenta apenas instruções e rótulos fixos e usa uma única chamada ao provedor, sem retry automático. O orçamento total é 20 segundos no Worker, incluindo a validação de sessão, e 25 segundos no cliente, incluindo a obtenção do token. Entrada: até 8 KiB; resposta do provedor: até 24 KiB; resposta lida pelo cliente: até 8 KiB. A rota tem orçamento de seis consultas por conta por minuto na memória do Worker; não é uma quota global persistente entre todas as réplicas.

A resposta validada tem resumo de até 160 caracteres, explicação de até 240, confiança qualitativa, até três categorias de evidência presentes no relatório e de um a três IDs de próximos passos permitidos. O texto dos passos vem do launcher, não do modelo. URLs, comandos e padrões conhecidos de conteúdo inadequado são recusados nos campos livres; essa filtragem não comprova a correção semântica do texto do modelo. A UI identifica a orientação como **Sugestão do Gemini**, preserva as evidências locais e não executa ações a partir da resposta.

Sem evidência específica, o resultado é normalizado para **causa não identificada**. Rede e Mixin isolados não podem ser promovidos a causa confirmada. Cancelar a consulta, trocar o relatório, a instância ou a conta invalida a resposta anterior. Falhas de rede, autenticação, timeout, limite ou resposta inválida mantêm o diagnóstico local disponível. O compartilhamento do relatório local não incorpora automaticamente a orientação Gemini.

O uso online depende de publicação da rota `/v1/doctor/advice` no Worker configurado. Build local e testes com provedor simulado não comprovam essa publicação ou o funcionamento real do Gemini. Estado da entrega, testes e limitações: [task-doctor-gemini.md](task-doctor-gemini.md); decisão: [ADR-024](engineering/ADR/ADR-024-doctor-gemini-guidance.md).

## Próximo incremento

Um próximo incremento de recuperação pode propor uma ação estreita a partir de um diagnóstico confirmado, apresentar o efeito ao jogador e exigir confirmação antes de mutações. Sua implementação deverá incluir exclusão mútua, backup/rollback e auditoria recuperável próprios. CAS, backup geral, restauração, modo seguro e correções automáticas permanecem **NÃO IMPLEMENTADO** nesta entrega.

Evidências executadas e limitações de homologação: [registro da entrega](task-aurora-doctor.md).

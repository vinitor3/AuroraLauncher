# ADR-024 — Orientação Gemini opcional no Aurora Doctor

- Data: 2026-09-23
- Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**
- Complementa: [ADR-021](ADR-021-aurora-doctor-local.md).

## Contexto

O Doctor local já associa evidências limitadas a uma tentativa e produz um relatório compartilhável. Uma explicação curta pode ajudar o jogador a interpretar esse resultado, mas enviar o relatório inteiro aumentaria a exposição de contexto e permitiria que texto de logs ou metadados chegasse ao modelo como instruções.

## Decisão

O diagnóstico determinístico continua sendo a fonte dos fatos. A consulta Gemini é uma ação opcional e explícita na área Diagnóstico, usando a autenticação e o Worker existentes. Falha ou indisponibilidade dessa consulta não bloqueia a análise local.

O contrato enviado ao serviço aceita apenas códigos de diagnóstico/contexto e números dentro de limites definidos. Não envia texto bruto, nomes, caminhos, identificação da execução ou relatório integral. Campos desconhecidos e valores fora do contrato são recusados antes de consultar Gemini. Instruções do modelo são fixas, controladas pelo Worker e separam os dados de entrada das instruções.

A resposta deve obedecer a JSON estrito, limites de tamanho e uma lista permitida de próximos passos, com no máximo três ações manuais. Ela é validada antes de chegar à UI e tratada como dados; não gera invocações de ferramentas, shell, downloads ou alterações na instância. Campos livres recebem filtragem de padrões conhecidos, sem promessa de validação semântica perfeita. A orientação é rotulada como sugestão; uma resposta inválida é descartada.

As chamadas têm orçamento temporal, cancelamento e limites de corpo. Mudanças de instância, execução, relatório ou sessão invalidam respostas antigas na interface. O relatório local compartilhável permanece separado da orientação Gemini.

## Consequências e limites

O contexto reduzido favorece privacidade e previsibilidade, mas impede uma investigação de logs pelo modelo. O texto explica somente indicadores estruturados e não pode confirmar qual mod causou a falha. A UI mantém fatos locais e orientação opcional separados.

A funcionalidade depende de uma rota Worker implantada e de acesso autorizado ao serviço para funcionamento online. Testes com respostas sintéticas comprovam contratos e falhas controladas, não a qualidade de respostas reais nem disponibilidade da implantação.

Ferramentas de recuperação, correções automáticas, CAS, modo seguro e backups gerais permanecem fora deste incremento. Evidências: [registro da entrega](https://github.com/vinitor3/AuroraLauncher/blob/088bf5b435b1b63e842c11ff1c01648b5e38bced/docs/task-doctor-gemini.md).

# ADR-026 — Failover Gemini limitado e Flash-Lite opcional

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. Data: 29/09/2026. Complementa o transporte da ADR-025; preserva contratos, autenticação e confirmação local de ações.

## Contexto

Consultas reais alternaram entre respostas válidas, indisponibilidade e credencial recusada. O cliente alpha.13 já possui prazo total e tratamento de erros públicos. O usuário autorizou um modelo Lite somente como último recurso. A mudança deve aproveitar esse contrato sem exigir novo instalador ou apresentar uma chave alternativa como solução de quota.

## Decisão

Compartilhar o transporte entre Assistente, Doctor, Tools e tradução. Aceitar três secrets opcionais, com trim/deduplicação, e uma ordem explícita estrita de slots. A configuração publicada é `GEMINI_KEY_ORDER=1,2`; o terceiro slot recusado permanece excluído até validação de sua autorização.

Tentar Flash primeiro e limitar a consulta a quatro chamadas. Quando `GEMINI_FALLBACK_MODEL=gemini-flash-lite-latest` estiver habilitado, limitar a fase Flash a 20 segundos e até três chamadas, reservando orçamento para Lite no prazo global de 35 segundos. Aliases ausentes podem usar os modelos explícitos permitidos. Configuração de modelo não permitida falha antes do provedor.

Trocar credencial somente para falha transitória de rede, HTTP 408/5xx ou prazo de tentativa, com espera limitada e cancelável. Lite atende indisponibilidade, modelo ausente ou expiração da fase Flash. Quota, autorização, configuração recusada e conteúdo inválido não autorizam esse fallback. A tradução conserva uma segunda validação semântica dentro do mesmo orçamento, sem publicar resultados incompletos.

Limitar corpos de erro a 8 KiB e um segundo; cancelar imediatamente corpos de status terminais conhecidos. Registrar apenas metadados operacionais de slot/modelo/tentativa/status/código/duração. Nunca registrar chaves, perguntas ou conteúdo retornado. Nenhuma resposta do modelo concede autorização para executar uma ação.

## Consequências

O fallback pode recuperar indisponibilidade transitória, mas não garante disponibilidade ou equivalência entre Flash e Lite. Quotas por projeto e recusas de autorização exigem correção própria. O limite temporal pode encerrar uma consulta antes de usar todas as chaves. Respostas válidas continuam passando pelos mesmos validadores e ações pelo popup de confirmação existente.

A alteração é somente do Worker, compatível com alpha.13. Não muda Core/Skins, arquivos de instâncias ou a matriz Minecraft. Provas pontuais de preview não homologam estabilidade em produção. Evidências e publicação: [task-gemini-failover.md](https://github.com/vinitor3/AuroraLauncher/blob/088bf5b435b1b63e842c11ff1c01648b5e38bced/docs/task-gemini-failover.md).

# Aurora Doctor — orientação Gemini opcional

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. Há implementação real no engine, frontend e Worker; a consulta online requer a nova rota no serviço configurado. Esta tarefa não publica o Worker.

## Base e escopo

Referência: repositório ativo `AuroraLauncher`, base `6f3a13bd78c6b511ddc5b18f9eafaf14ee08ab1d`, launcher `0.1.1-alpha.11`, Aurora Core `1.1.5` e Aurora Skins `0.3.1`. A matriz ativa foi conferida antes do trabalho. Branch `codex/aur-doctor-gemini`, em worktree exclusiva de mesmo nome.

O incremento mantém o diagnóstico determinístico local e adiciona uma consulta Gemini opcional para explicar o resultado e sugerir poucos próximos passos manuais. Regras locais podem reconhecer novas evidências explícitas; isso não transforma uma stack trace ambígua em culpa de um mod nem associa crashes antigos a uma execução nova.

A consulta exige ação do jogador e sessão Aurora. Seu contrato é reduzido a códigos permitidos e números limitados. Não leva linhas de log, mensagens livres, caminhos pessoais, nomes de instância/jogador, identificador da execução ou o relatório JSON integral. A resposta não autoriza mutações nem execução de comandos. O Doctor local continua utilizável quando Gemini/rede/autenticação não estão disponíveis.

Core, Skins, grade/cache/multiplayer, sessões independentes e Assistente desktop permanecem na base atual. Não amplia versões/loaders, não recria Companion, não implementa correções automáticas, CAS, backup ou modo seguro. Não publica Worker, release ou instalador nesta tarefa.

## Responsabilidade e verificação

Ownership: integração e contrato/Worker; interface; regras Rust; QA independente e documentação. Os arquivos monolíticos têm proprietários exclusivos. Sem commit separado de QA; integração em commit coeso ao final.

| Verificação | Resultado |
| --- | --- |
| `node --test apps/edge-proxy/src/doctor.adversarial.test.ts` | **CONCLUÍDO**: 14 testes independentes aprovados. |
| Smoke DOM React do Doctor | **CONCLUÍDO**: 21 grupos aprovados pelo agente de UI, incluindo cancelamento e mudança de contexto/conta. |
| Testes Node de transporte Doctor, edge e preservação do Assistente | **CONCLUÍDO**: 19 testes aprovados. Inclui sessões independentes e requisito Java desconhecido. |
| `npm --prefix apps/desktop run build` | **CONCLUÍDO** no estado integrado final; aviso existente de bundle acima de 500 KiB. |
| `cargo fmt --all -- --check` e `cargo clippy --all-targets -- -D warnings` | **CONCLUÍDO**, usando o manifest do desktop. |
| `cargo test` completo do desktop | **CONCLUÍDO**: 158 aprovados, 5 testes opt-in ignorados. Inclui 12 novos cenários de identificação e 15 QA anteriores. |
| `cargo build` debug do desktop | **CONCLUÍDO**; binário local de teste, sem pacote/instalador. |
| `npm --prefix apps/edge-proxy run check` e `npm --prefix apps/edge-proxy test` | **CONCLUÍDO**: dry-run do Worker e 104 testes aprovados. Após a correção final da projeção, dry-run e 14 testes Doctor repetidos com sucesso. |
| `scripts/doctor-native-smoke.mjs` com WebView isolado | **CONCLUÍDO** no comando nativo: inventário de 11 instâncias instaladas e relatório real válido/sanitizado de uma instância. UI autenticada: **NÃO VERIFICADO** nessa execução, sem sessão na janela de teste. |
| `git diff --check` | **CONCLUÍDO**, sem erro de whitespace na revisão independente. |
| Nova rota publicada e consulta Gemini real | **NÃO VERIFICADO**; não houve deploy nesta tarefa. |

Os testes independentes usam tokens Firebase RSA gerados para fixtures e interceptam os serviços de chave/Gemini. Exercitam a rota HTTP real do Worker: autenticação ausente/expirada, campos extras e instruções nos dados, privacidade da projeção, limites e formato, evidência inventada, ações inválidas, normalização de causa desconhecida, orçamento por conta, quota/falha do provedor sem retry, cancelamento e corpos lentos de entrada/JWKS/provedor. Confirmam que erros não devolvem corpos ou credenciais do serviço. As respostas são **sintéticas** e não comprovam a qualidade de respostas Gemini reais.

Os grupos de UI verificam que abrir/analisar não envia dados automaticamente; clique explícito; etapas fixas; cancelamento mesmo quando um provedor simulado ignora o abort; relatório novo no mesmo contexto; mudança de instância/Java/execução/conta; evento de nova tentativa; desmontagem; e fallback local após falha ou resposta inválida.

As novas fixtures Rust em `apps/desktop/src-tauri/tests/fixtures/doctor_detection` são **sintéticas**, confrontadas com formatos publicados pelo Fabric/Forge, Mixin, GLFW e JVMS. As fontes estão no README dessas fixtures. A revisão corrigiu a distinção entre `UnsupportedClassVersionError` da JVM e `Unsupported class file major version` de um leitor de bytecode: o segundo não prova incompatibilidade do Java selecionado.

A revisão também preservou o diagnóstico de dependência salvo pela alpha.11, sem promover seu texto amplo a ausência confirmada, e impediu que o fallback Java 8 de um Minecraft não identificado seja enviado como requisito verificado ao Gemini. Há regressões específicas para ambos os casos.

O smoke nativo usou um perfil temporário do WebView para preservar duas janelas instaladas já abertas. A primeira tentativa de conexão de teste não disponibilizou CDP no perfil compartilhado; o perfil isolado permitiu validar o comando nativo. A instância real retornou `no_issues`, duas fontes, zero bytes novos legíveis e uma execução registrada: isso comprova integração/leitura do relatório, não reconhecimento de um crash real. Nenhum jogo foi iniciado nem configuração alterada. Os controles Gemini e detalhes compactos foram validados pelos grupos DOM; o script nativo agora também verifica esses controles quando há uma sessão autenticada.

## Contrato e arquivos principais

- `shared/doctorAdvice.ts`: projeção explícita, schema estrito e validação das ações/evidências.
- `shared/doctorTransport.ts`: orçamento e leitura de corpos limitada/cancelável.
- `apps/edge-proxy/src/doctor.ts` e integração em `index.ts`: `/v1/doctor/advice` autenticada, uma chamada Gemini, erros públicos fixos.
- `apps/desktop/src/lib/edge.ts` e `features/doctor/GeminiAdvice.tsx`: transporte autenticado, cancelamento e orientação opcional; `DoctorView.tsx` apresenta o essencial com detalhes expansíveis.
- `apps/desktop/src-tauri/src/engine/doctor_rules.rs`: declarações explícitas, negações e contexto de loader; `doctor.rs` conserva associação, leitura e relatório.
- `apps/edge-proxy/src/doctor.adversarial.test.ts`, smoke React e `tests/doctor_detection.rs`: verificação do incremento.

O Worker usa uma chamada `generateContent` com `responseMimeType: application/json` e `responseSchema`; o cliente e o servidor ainda validam a resposta, sem confiar apenas no schema do provedor. Referências oficiais: [saída estruturada](https://ai.google.dev/gemini-api/docs/structured-output), [generateContent](https://ai.google.dev/api/generate-content) e [modelos Gemini](https://ai.google.dev/gemini-api/docs/models). Foi reutilizado `gemini-3.5-flash-lite`, já selecionado pelo projeto para fluxos curtos.

## Limitações e continuidade

A explicação Gemini é orientação opcional, não uma confirmação causal ou homologação da instância. A ausência de evidência permanece explícita. A resposta não substitui os fatos locais nem altera seu grau de confiança. Ações continuam manuais e estreitas; uma futura recuperação automática exigiria contrato, confirmação, exclusão mútua e rollback próprios.

O orçamento de 20 segundos no servidor inclui sessão, entrada e saída do provedor; o cliente usa 25 segundos incluindo obtenção de token. Os limites são 8 KiB de entrada, 24 KiB de resposta do provedor, 8 KiB lidos pelo cliente, resumo de 160 caracteres e explicação de 240. O orçamento de seis consultas/minuto por conta usa memória do Worker e não representa quota persistente global. A filtragem dos campos de texto bloqueia padrões conhecidos; não é prova de exatidão semântica da orientação.

Sem publicar a rota nova, o botão deve informar indisponibilidade e preservar a análise local. Uma futura publicação e consulta real controlada devem ser registradas separadamente, sem promover os mocks atuais a homologação online. Nenhum instalador, release ou JAR foi gerado por esta entrega.

Decisão: [ADR-024](engineering/ADR/ADR-024-doctor-gemini-guidance.md). Comportamento do domínio: [Aurora Doctor](aurora-doctor.md).

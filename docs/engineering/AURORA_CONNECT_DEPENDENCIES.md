# Aurora Connect V2 — dependências e zero-spend

Revisão: 29/09/2026. Base documental: alpha.14, `6a107b87adc4882bd464909e22650185f862ea5e`, Core 1.1.5, Skins 0.3.1. Connect: **NÃO IMPLEMENTADO** na inspeção limitada desta revisão. Este cadastro avalia candidatos; não autoriza ativar contas, serviços, faturamento, deploy ou tráfego externo. [Especificação](AURORA_CONNECT_SPEC.md), [ADR proposta](ADR/PROPOSAL-aurora-connect.md), [tarefas e evidências exigidas](CODEX_TASKS/AUR-R4-CONNECT-BACKLOG.md).

## Política e gate de habilitação

Caminho direto é preferido; relay gratuito é permitido quando as condições do uso pretendido forem verificadas. Não há regra de rejeitar todo candidato relayed ou de falhar obrigatoriamente quando o direto não estiver disponível. O fallback somente utiliza dependência elegível. Nenhum relay gratuito sustentável de produção foi aprovado nesta revisão.

Um serviço não se torna elegível por funcionar tecnicamente, ter software livre, oferecer trial/crédito, não pedir cartão hoje ou possuir alerta de orçamento. Serviço que cobre excedentes permanece desabilitado sem bloqueio efetivo e comprovado de cobrança; um limite local é proteção de recursos, não garantia de fatura. Não habilitar faturamento. Não exigir VPS/relay do usuário, rotacionar contas, contornar quotas nem escolher relay público aleatório.

Antes de habilitar uma dependência: fixar versão/configuração; revisar fonte oficial vigente; preencher os campos abaixo; comprovar escopo permitido, autenticação, limites e custo; identificar dados expostos; testar indisponibilidade/limites em ambiente autorizado sem custo. Decisões e evidências ficam no backlog. Mudança de termos/configuração exige nova avaliação. Sem alternativa elegível, oferecer direto nas condições verificadas e falha clara nos demais casos, sem fallback pago silencioso.

`NÃO VERIFICADO` significa ausência de comprovação nesta revisão, não inexistência do recurso ou gratuidade. `NÃO IMPLEMENTADO` descreve integração Aurora; recursos upstream não mudam esse estado. Licenças indicadas por páginas oficiais devem ser conferidas nos artefatos e dependências da versão selecionada. Todas as fontes externas efetivamente lidas abaixo têm data de consulta **29/09/2026**; links marcados como pendentes não são leitura integral.

## Cadastro de infraestrutura

### DEP-IROH-PUBLIC — relays públicos n0

| Campo | Registro |
| --- | --- |
| Operador/finalidade | n0.computer; introdução/travessia de NAT e fallback de tráfego cifrado entre endpoints |
| Autenticação | Acesso ao relay público sem credencial; identidade/autorização entre agentes Aurora continuam obrigatórias |
| Licença/termos | Software Iroh MIT/Apache-2.0; não equivale a direito ilimitado sobre o serviço. Documentação limita públicos a desenvolvimento/hobby. Termos integrais aplicáveis: **NÃO VERIFICADO**; ver DEP-TERMS |
| Quotas/limites | Rate limiting por conexão; números exatos não publicados; capacidade compartilhada; sem SLA; somente stable atual oficialmente suportada; sem version locking |
| Regiões | Hostnames publicados: `use1-1`, `usw1-1`, `euc1-1`, `aps1-1` em `relay.n0.iroh.link`. Localização contratual, residência e região Brasil: **NÃO VERIFICADO**; não inferir por prefixo |
| Cobrança | Documentado sem custo; não exige plano pago. Não converter indisponibilidade em upgrade. Sustentabilidade/autorização para distribuição do Aurora: **NÃO VERIFICADO** e não aprovada |
| Dados/segredos | Tráfego E2E; operador observa metadados/IP/volume e monitora abuso. Não enviar tokens Microsoft/Minecraft ou logs de áudio |
| Falha | Se caminho direto já estiver funcionando, avaliar continuidade; novas sessões e fallback podem falhar. Throttling/indisponibilidade precisam de estado real, timeout/cancelamento e erro claro; não prometer quota detectável sem evento |
| Decisão | Candidato para desenvolvimento controlado após gate; distribuição pública desabilitada |
| Fonte/data | [Públicos](https://docs.iroh.computer/iroh-services/relays/public), [limites](https://docs.iroh.computer/relays/rate-limiting), [rede](https://docs.iroh.computer/configuring-networks), 29/09/2026 |

### DEP-IROH-DISCOVERY — DNS/Pkarr e reflexão de endereço

| Campo | Registro |
| --- | --- |
| Operador/finalidade | n0 opera `dns.iroh.link`; publica/resolve endereço de endpoint. Reflexão de endereço/NAT traversal integra o sistema de conectividade Iroh; é responsabilidade distinta de sinalização Aurora |
| Autenticação | Registros Pkarr assinados pelo endpoint. A assinatura autentica publicação, não convite nem permissão de serviço |
| Licença/termos | Componentes Iroh MIT/Apache-2.0; termos do serviço/defaults para distribuição Aurora: **NÃO VERIFICADO** |
| Quotas/limites | Publicação/lookup sujeitos a disponibilidade de DNS/HTTPS; quotas, retenção, TTL efetivo e compromisso de suporte: **NÃO VERIFICADO**. DHT e mDNS são opções explícitas, não presumidas |
| Regiões | **NÃO VERIFICADO**; não inferir residência do endereço do domínio |
| Cobrança | Não foi verificado contrato específico de cobrança/gratuidade sustentável do lookup. Não ativar plano/credencial; PoC pode usar endereços explícitos sem esse serviço |
| Dados/segredos | Endpoint ID e informação de localização necessária; evitar publicação de IPs internos ou metadados de sessão desnecessários. Fonte afirma filtragem de internos no padrão; verificar versão/configuração |
| Falha | Distinguir falha de resolução, caminho e autenticação; endereço explícito pode permitir conexão, sem garantir NAT traversal. Troca manual é experiência de desenvolvimento |
| Decisão | Configuração externa explícita; uso de produção pendente. Não chamar modo direto de independente de infraestrutura se lookup/reflexão externos estiverem ativos |
| Fonte/data | [Address Lookup](https://docs.iroh.computer/concepts/address-lookup), [segurança](https://docs.iroh.computer/concepts/security-privacy), 29/09/2026 |

### DEP-IROH-MANAGED — Shared/Pro e Dedicated

| Campo | Registro |
| --- | --- |
| Operador/finalidade | n0; relay gerenciado para produção, compartilhado ou dedicado |
| Autenticação | Capability vinculada ao endpoint assinada por API key global de projeto; fluxo de emissão separada por endpoint consta ainda não implementado. Key global jamais vai no launcher distribuído |
| Licença/termos | Software livre separado de contrato de serviço; contrato integral **NÃO VERIFICADO**. Oferta paga não autorizada |
| Quotas/limites | Shared observado: US$19/mês, 100 GB/mês, 10 mil conexões, 5 MB/s; excedentes US$0,09/GB e US$0,003/endpoint/mês conforme página. Trial 30 dias não é base sustentável; Dedicated exige cadastro próprio |
| Regiões | Shared anuncia quatro relays globais; localização contratual exata **NÃO VERIFICADO**. Dedicated pode oferecer regiões customizadas, sem escolha/cotação nesta tarefa |
| Cobrança | Obrigatória e com excedentes. Spend cap documentado não é teto garantido; atraso de apuração, taxas recorrentes/impostos e Dedicated podem continuar cobrando |
| Dados/segredos | E2E preserva conteúdo; metadados externos. Emissão de credenciais teria de permanecer em backend autorizado, sem fingir proteção de segredo no cliente |
| Falha | Não usar como fallback. Se existir configuração histórica, mantê-la desligada para Connect e registrar risco; nunca criar/alterar faturamento |
| Decisão | **RISCO** de gasto; desabilitado nesta proposta, inclusive trial |
| Fonte/data | [Shared](https://docs.iroh.computer/iroh-services/relays/shared), [API keys](https://docs.iroh.computer/iroh-services/access), [spend cap](https://docs.iroh.computer/iroh-services/billing/control-costs), 29/09/2026 |

### DEP-IROH-OBS — métricas e diagnósticos gerenciados

| Campo | Registro |
| --- | --- |
| Operador/finalidade | n0; receber métricas e relatórios de conectividade |
| Autenticação | API key global de projeto; não pode ser distribuída no aplicativo |
| Licença/termos | Licença do SDK e termos da versão selecionada **NÃO VERIFICADO**; não confundir licença do transporte com este serviço |
| Quotas/regiões/limites | Plano, limites, retenção e residência específicos **NÃO VERIFICADO** |
| Cobrança | Plano efetivo e proteção de cobrança **NÃO VERIFICADO**; nenhum plano habilitado |
| Dados/segredos | Diagnósticos podem conter endereços/identidade de endpoint; exigir minimização e redação. Voz não é telemetria |
| Falha | Diagnóstico local redigido continua; indisponibilidade não impede jogo nem força upload posterior ilimitado |
| Decisão | Desabilitado; desnecessário para primeira PoC. Observabilidade local é proposta separada |
| Fonte/data | [API Keys](https://docs.iroh.computer/iroh-services/access), 29/09/2026 |

### DEP-FIREBASE-SIGNAL — candidato de sinalização Aurora

| Campo | Registro |
| --- | --- |
| Operador/finalidade | Google Firebase/Firestore; candidato a metadados mínimos de sessão/convite por participante, reutilizando infraestrutura existente somente após auditoria |
| Autenticação | Firebase Auth/regras existentes a verificar; sessão/peer/convite exigem vinculação própria. Não inferir autorização pelo documento existir |
| Licença/termos | SDK/termos oficiais da versão e configuração selecionadas: **NÃO VERIFICADO** nesta pesquisa. Fonte local a auditar: `firebase/`, frontend e contratos de autenticação |
| Quotas/limites | Documentação consultada: uma base gratuita/projeto, 1 GiB, 50 mil leituras/dia, 20 mil escritas/dia, 20 mil exclusões/dia e 10 GiB de saída/mês. Plano real/consumo existente e margem Connect: **NÃO VERIFICADO**. Exclusões TTL exigem billing; expiração lógica em regras não equivale a remoção física |
| Regiões | Região do projeto real e circulação dos metadados: **NÃO VERIFICADO** |
| Cobrança | Billing/plano real e bloqueio efetivo de excedentes: **NÃO VERIFICADO**. Não habilitar faturamento; validar inicialmente com emulador/local sem conexão ao projeto de produção |
| Dados/segredos | Somente metadados necessários; nunca áudio, jogo, arquivos de modpack ou credenciais Microsoft/Minecraft. Escopo por participante, expiração e retenção limitada |
| Falha | Convite/presença deixam de atualizar com erro real; retries limitados e sem polling ilimitado. Conexões existentes têm política explícita de validade/revogação, não autorização eterna |
| Decisão | **HIPÓTESE**; regras testadas no emulador antes de qualquer uso; produção sem aprovação |
| Fonte/data | [Quotas e recursos pagos](https://firebase.google.com/docs/firestore/quotas), consultados em 29/09/2026. [Emulator Suite](https://firebase.google.com/docs/emulator-suite) é referência operacional a conferir na implementação, não teste executado nesta revisão |

### DEP-WORKER-SIGNAL — candidato de controle no Worker existente

| Campo | Registro |
| --- | --- |
| Operador/finalidade | Cloudflare; somente possível controle/autorização de metadados no Worker Aurora existente, não relay de jogo/voz |
| Autenticação | Contratos Firebase/JWT existentes a auditar por rota; credenciais administrativas só no backend; nenhum segredo global no cliente |
| Licença/termos | Código Aurora separado de termos Cloudflare/SDK; termos integrais de uso pretendido e licença de dependências selecionadas **NÃO VERIFICADO** |
| Quotas/limites | Free documentado: 100 mil requests/dia, 10 ms CPU/invocação. Plano efetivo/consumo do Worker e bindings adicionais **NÃO VERIFICADO**; não somar quotas de outros produtos como se fossem um pacote ilimitado |
| Regiões | Rede distribuída; residência/configuração efetiva de metadados **NÃO VERIFICADO** |
| Cobrança | Plano pago tem cobrança/uso excedente; não habilitar. Contador/rate limit local não garante ausência de fatura |
| Falha | Erro de controle explícito, retry limitado e respeito à expiração; não contratar alternativa. Nenhum tráfego sensível em logs |
| Decisão | Candidato condicionado à auditoria; nenhuma rota/deploy criada. Preferir PoC local/emulada |
| Fonte/data | [Preços Workers](https://developers.cloudflare.com/workers/platform/pricing/), consulta 29/09/2026; scripts reais em `apps/edge-proxy/package.json` |

### DEP-RELAY-OTHER — relay próprio, TURN/STUN e outros provedores

| Campo | Registro |
| --- | --- |
| Operador/finalidade | Nenhum operador selecionado. TURN/STUN somente se necessários à alternativa escolhida; relay próprio continua adiado |
| Autenticação | A definir por protocolo/serviço; nunca segredo compartilhado global embutido. STUN não fornece autorização de sessão |
| Licença/termos | **NÃO VERIFICADO**; não usar servidores encontrados aleatoriamente ou presumir termos a partir da licença do binário |
| Quotas/regiões/limites | **NÃO VERIFICADO**; incluir suporte UDP e eventual TCP/TLS, sem presumir que relay UDP resolve UDP externo bloqueado |
| Cobrança | **NÃO VERIFICADO**; VPS/egress próprio não é grátis por ser open source; créditos temporários e rotação de contas proibidos |
| Dados/segredos | E2E entre agentes obrigatório; operadores podem observar metadados; autenticação do relay não equivale à confidencialidade |
| Falha | Apenas direto se possível ou erro claro; nenhuma contratação automática |
| Decisão | Desabilitado até cadastro completo e teste autorizado; não é requisito imposto ao usuário para cumprir MVP |
| Fonte/data | Nenhum provedor adicional foi selecionado/validado; registro de lacuna em 29/09/2026 |

## Cadastro de bibliotecas e integrações comparadas

Bibliotecas locais não são operadoras de infraestrutura: quotas/regiões de hospedagem são “não aplicável” ao binário, mas continuam obrigatórias para cada serviço que ele usar.

| ID / componente | Mantenedor e finalidade | Versão/licença observadas | Autenticação | Termos, quotas e regiões | Risco de cobrança / comportamento na falha | Decisão e fonte (29/09/2026) |
| --- | --- | --- | --- | --- | --- | --- |
| DEP-IROH-LIB / Iroh | n0/comunidade; transporte e conectividade Rust | API latest exibiu 1.3.0; MIT ou Apache-2.0. Pin/lock Aurora **NÃO VERIFICADO** | Identidade criptográfica de endpoint; autorização Aurora própria | Licença da versão/transitivas a conferir; limites configuráveis/MTU a medir. Infraestrutura nos cadastros acima | Biblioteca sem tarifa indicada; presets podem ativar serviços externos. Falha deve liberar recursos e informar estado | Primeiro candidato condicional. [Repo](https://github.com/n0-computer/iroh), [API](https://docs.rs/iroh/latest/iroh/endpoint/struct.Connection.html) |
| DEP-QUINN / Quinn | Quinn project; QUIC Rust | 0.11.12 observado; MIT/Apache-2.0; versão final a selecionar | TLS; verificar identidade do peer vinculada ao convite, nunca aceitar qualquer certificado | Sem serviço de sinalização/relay incluído; NAT traversal necessita desenho compatível. Plataformas/pacote efetivo precisam de teste | Biblioteca não comprova infraestrutura grátis. Falha direta sem fallback elegível é erro claro | Alternativa Rust, não implementação paralela. [API](https://docs.rs/quinn/latest/quinn/), [repo](https://github.com/quinn-rs/quinn) |
| DEP-GO-ALT / Pion ICE + quic-go | Comunidades Pion e quic-go; alternativa em processo auxiliar | Pion v4.4.4 e quic-go v0.63.0 observados, MIT; páginas alertam não necessariamente latest; pin não selecionado | ICE não substitui TLS/autorização de serviço; provar reaproveitamento do caminho/socket | Sem serviço gratuito incluído; operador STUN/TURN teria cadastro próprio. Limites e performance DATAGRAM a testar | Sem tarifa da biblioteca; sidecar adiciona build/assinatura/updater/IPC/shutdown e pode depender de serviço pago, não aprovado | Somente com vantagem comprovada. [ICE](https://pkg.go.dev/github.com/pion/ice/v4), [quic-go](https://pkg.go.dev/github.com/quic-go/quic-go), [composição](https://quic-go.net/docs/quic/transport/) |
| DEP-E4MC / e4mc e infraestrutura associada | Projeto vgskye/e4mc; abrir LAN/domínio, relay e Dialtone direto | `rererewrite` declara 6.2.2; mod MIT; relay repo MIT/Apache-2.0. Não é versão homologada Aurora | Protocolo e autorização de infraestrutura para integração Aurora **NÃO VERIFICADO** | Quotas, operador contratual por serviço, regiões e termos de distribuição/uso de terceiros **NÃO VERIFICADO**; UDP/voz não comprovados | Não foi aprovado compromisso de gratuidade sustentável. Indisponibilidade não pode exigir contratação ou relay do usuário | Comparar sem descartar por relay. [Repo](https://github.com/vgskye/e4mc-minecraft-architectury/), [changelog](https://github.com/vgskye/e4mc-minecraft-architectury/blob/rererewrite/CHANGELOG.md), [relay](https://github.com/vgskye/e4mc-quiclime) |
| DEP-WORLDHOST / World Host e serviço | Projeto Gaming32/World Host; sessão/friends, UPnP/Proxy e pesquisa de hole punching | `main` declara 0.5.2; MIT; release efetiva não selecionada | README/fluxo descrevem mediação/validação; contrato completo de identidade e integração **NÃO VERIFICADO** | Operador contratual, termos, quotas, regiões, UDP genérico/voz e estabilidade **NÃO VERIFICADO** | Gratuidade sustentável/cobrança **NÃO VERIFICADO**; indisponibilidade com erro, sem opção paga silenciosa | Comparar, não homologado. [Repo](https://github.com/Gaming32/world-host), [fluxo UDP](https://github.com/Gaming32/world-host/blob/main/UDP_HOLE_PUNCH_PACKET_FLOW.md) |
| DEP-SVC / Simple Voice Chat | Projeto Simple Voice Chat/modrepo; mod de voz existente, não serviço Aurora de voz | Versão/loader/artefato/licença final: registrar na matriz de mod; **NÃO VERIFICADO** neste cadastro | Preservar autenticação/identidade/criptografia do mod; túnel não as substitui | Endereços, porta efetiva e API da versão exigem verificação; `voice_host` é investigação. Região é a do host/relay escolhido, não do nome do mod | Mod não implica infraestrutura externa gratuita. Falha de voz não derruba jogo; instalação segue origem/licença/updater | Homologação própria obrigatória. [Proxy/configuração](https://modrepo.de/minecraft/voicechat/wiki/proxy_setup): pesquisa detalhada registrada na [especificação](AURORA_CONNECT_SPEC.md) |
| DEP-PLASMO / Plasmo Voice | Projeto Plasmo Voice; mod de voz existente | Versão/loader/artefato/licença final: registrar na matriz; **NÃO VERIFICADO** neste cadastro | Preservar mecanismos do mod; não inventar identidade encaminhada por IP | Configuração efetiva e API exigem verificação; `[host.public]` é investigação. Quotas/regiões de serviços extras **NÃO VERIFICADO** | Não aprova add-ons/proxies externos. Falha de voz independente do jogo; recuperação não apaga configuração concorrente | Homologar separadamente de SVC. [Endereçamento](https://plasmovoice.com/docs/server/advanced/): pesquisa detalhada na [especificação](AURORA_CONNECT_SPEC.md) |

## Complemento da pesquisa dos mods — versão não é homologação

DEP-SVC: o branch oficial `26.3` consultado declara mod `2.6.24+26.3`, API `2.6.24` e beta; **não foi selecionado para Aurora**, cuja matriz não inclui Minecraft 26.3. Licença lida nesse branch: **All Rights Reserved**. A FAQ admite uso em modpacks com crédito; isso não é licença irrestrita de cópia nem autorização automática para distribuição do launcher/API. Registrar licença/artefato por versão antes de instalar ou criar add-on. A API pública `VoiceHostEvent` permite investigar anúncio por participante; versão mínima/loader/LAN continuam pendentes. Não há operador de hospedagem adicional selecionado para o mod; tamanho real de UDP, quotas do host e regiões dependem da sessão/relay homologados. Fontes de 29/09/2026: [propriedades](https://raw.githubusercontent.com/henkelmax/simple-voice-chat/26.3/gradle.properties), [API](https://raw.githubusercontent.com/henkelmax/simple-voice-chat/26.3/api/gradle.properties), [licença](https://raw.githubusercontent.com/henkelmax/simple-voice-chat/26.3/license), [FAQ](https://modrepo.de/minecraft/voicechat/faq), [evento](https://voicechat.modrepo.de/de/maxhenkel/voicechat/api/events/VoiceHostEvent.html).

DEP-PLASMO: **2.1.17** tem release e API versionada consultadas; é candidata de pesquisa, não seleção/homologação Aurora. O projeto declara **LGPL-3.0**; conferir obrigações e licença do artefato/add-on escolhido. O evento cliente `UdpClientConnectEvent` permite investigar substituição do cliente UDP; não comprova redirecionamento pronto nem API servidor por jogador. Preservar autenticação/secret/criptografia nativa; não enviar secret à sinalização. Serviço local do mod não é infraestrutura externa gratuita fornecida pelo projeto; nenhum add-on/relay adicional está aprovado. Fontes de 29/09/2026: [release](https://github.com/plasmoapp/plasmo-voice/releases/tag/2.1.17), [API versionada](https://dokka.plasmovoice.com/older/2.1.17/index.html), [repositório](https://github.com/plasmoapp/plasmo-voice), [criptografia](https://plasmovoice.com/docs/api/encoding-and-encryption.html).

Para ambos, origem de download/termos do distribuidor, hashes e configuração efetiva ficam pendentes por linha da matriz. Download usa o updater/origem autorizados e o fluxo oficial CurseForge; não há mirror novo. Falha de autenticação, endpoint ou compatibilidade de voz mantém jogo disponível quando possível. Não são obrigatórios na mesma instância. Portas padrão, APIs em latest e suporte upstream a LAN não homologam todos os pares Aurora. Outros serviços TCP/UDP precisam de extensão explícita e cadastro próprio.

## DEP-TERMS — limitações das fontes e decisões pendentes

Foi lida documentação oficial de relays, descoberta, redes, custos, credenciais e APIs, além dos READMEs/arquivos indicados. A URL [Iroh Services Terms](https://services.iroh.computer/legal/terms) teve extrato indexado disponível com atualização declarada em 14/05/2025, mas a abertura direta retornou vazio. **Não foi lido integralmente o termo nem verificada sua aplicabilidade exata a cada serviço público.** Isso permanece gate; não se afirma autorização comercial/jurídica de distribuição.

Há divergência editorial upstream: uma seção de segurança chama relay-only de futuro, enquanto o guia de redes já mostra desativação de transportes IP. Usar código/documentação da versão fixada e teste, não a frase mais conveniente. URLs `latest` e branches móveis são evidência da consulta, não locks reproduzíveis. Nenhuma porcentagem de sucesso ou ganho de ping do fornecedor é métrica Aurora.

O [backlog](CODEX_TASKS/AUR-R4-CONNECT-BACKLOG.md) deve registrar, por dependência habilitada, versão/hash, configuração permitida, fonte/data, decisão teste/produção, evidência de custo, teste de falha/quota e responsável. Recursos que continuarem sem prova ficam experimentais e desativados. A ausência de um serviço elegível não autoriza enfraquecer autenticação, expor portas indiscriminadamente nem contratar infraestrutura.

# Aurora Connect V2 — dependências e zero-spend

Reconciliação: 29/09/2026, base alpha.14 `5ac2f7e` sobre `6a107b8`, Core 1.1.5, Skins 0.3.1. [Execução/artefatos atuais](../task-aur-r4-connect.md), [correspondência histórica](AURORA_CONNECT_REVIEW.md), [spec](AURORA_CONNECT_SPEC.md), [ADR](ADR/PROPOSAL-aurora-connect.md) e [backlog](CODEX_TASKS/AUR-R4-CONNECT-BACKLOG.md). O laboratório começa somente com endereços diretos manuais e nenhuma configuração de relay, lookup público, portmapper, telemetria gerenciada ou sinalização cloud. A pesquisa histórica abaixo não autoriza ativar contas, serviços, faturamento, deploy ou tráfego externo.

## Política e gate de habilitação

Evidência posterior21: SVC Fabric1.21.1/2.6.22 fixado foi executado em servidor descartável, com bindUDP próprio e writer reais; não foi redistribuído. O perfil acrescenta5.369.475B aos nove artefatos existentes, downloadpúblico sem conta/cobrança. Allowlist Modrinth é limitada ao pin completo; nenhuma descoberta automática de mod ou mudança de licença. [Resultados, fonte do fallback de bind e limites](../aurora-connect-runtime.md#boot-simple-voice-chat--vigésimo-primeiro-incremento). Esta prova não libera relay, anúncio, autenticaçãoUDP ou áudio.

Caminho direto é preferido; relay gratuito é permitido quando as condições do uso pretendido forem verificadas. Não há regra de rejeitar todo candidato relayed ou de falhar obrigatoriamente quando o direto não estiver disponível. O fallback somente utiliza dependência elegível. Nenhum relay gratuito sustentável de produção foi aprovado nesta revisão.

Um serviço não se torna elegível por funcionar tecnicamente, ter software livre, oferecer trial/crédito, não pedir cartão hoje ou possuir alerta de orçamento. Serviço que cobre excedentes permanece desabilitado sem bloqueio efetivo e comprovado de cobrança; um limite local é proteção de recursos, não garantia de fatura. Não habilitar faturamento. Não exigir VPS/relay do usuário, rotacionar contas, contornar quotas nem escolher relay público aleatório.

Antes de habilitar uma dependência: fixar versão/configuração; revisar fonte oficial vigente; preencher os campos abaixo; comprovar escopo permitido, autenticação, limites e custo; identificar dados expostos; testar indisponibilidade/limites em ambiente autorizado sem custo. Decisões e evidências ficam no backlog. Mudança de termos/configuração exige nova avaliação. Sem alternativa elegível, oferecer direto nas condições verificadas e falha clara nos demais casos, sem fallback pago silencioso.

`NÃO VERIFICADO` significa ausência de comprovação nesta revisão, não inexistência do recurso ou gratuidade. `NÃO IMPLEMENTADO` descreve integração Aurora; recursos upstream não mudam esse estado. Licenças indicadas por páginas oficiais devem ser conferidas nos artefatos e dependências da versão selecionada. Todas as fontes externas efetivamente lidas abaixo têm data de consulta **29/09/2026**; links marcados como pendentes não são leitura integral.

## Cadastro de infraestrutura

### DEP-IROH-LAB — configuração efetivamente selecionada

| Campo | Decisão/evidência de 29/09/2026 |
| --- | --- |
| Componente e versão | `iroh = 1.3.0` exato em `apps/connect-lab/Cargo.toml`, resolução em lock próprio; `publish = false`; não adicionado ao desktop |
| Mantenedor/licença | n0/comunidade; MIT OR Apache-2.0 no artefato confirmado por `cargo info iroh@1.3.0` |
| Toolchain | MSRV 1.91; Rust local 1.98.0; nenhuma atualização global ou do lock desktop |
| Hash de aprovação local | `sha2 =0.11.0`, sem default features, agora dependência direta; a mesma versão já existia transitivamente no lock. Manifest do pacote consultado: MIT OR Apache-2.0, MSRV 1.85. SHA-256 de bytes exatos do plano, sem implementação criptográfica própria |
| Autenticação/capacidade | Identidade criptográfica Iroh com pin esperado; lease host por peer/sessão/serviço/prazo. Streams confiáveis e DATAGRAM; autorização Aurora permanece própria |
| Defaults e serviços | `default-features = false`, `tls-ring`; preset `Minimal`, relay Disabled, lookup/relay defaults limpos, bind e destino manuais; sem portmapper/telemetria gerenciada |
| Operador/termos externos | Nenhum serviço remoto configurado para o ensaio direto local. Licença de biblioteca não aprova uso futuro de infraestrutura |
| Quotas/regiões | Não aplicáveis a operador no laboratório local; limites de recursos locais no código/task. Não é capacidade anunciada para oito jogadores/mods |
| Custo e falha | Nenhuma conta, cobrança, crédito ou fallback pago. Sem caminho manual alcançável, timeout/erro; não classifica NAT/CGNAT |
| Segurança de dependências | rustls 0.23.45 resolve o aviso RUSTSEC-2026-0285 conferido. Varredura Cargo Audit posterior: zero vulnerabilidades conhecidas na base consultada e um alerta de manutenção `paste 1.0.15`, preservado; detalhes abaixo |
| Metadados de licença | `cargo metadata --locked --format-version 1 --manifest-path apps/connect-lab/Cargo.toml` identificou 358 pacotes, incluindo o local, todos com campo de licença. Isso é inventário, não revisão jurídica ou prova de cumprimento de cada obrigação de distribuição |
| Plataformas/homologação | Testes Windows locais conforme task; outros sistemas, NAT, relay, Minecraft e voz exigem evidências próprias |
| Fonte/API | Release/tag/manifest oficiais exatos e código do pacote Cargo 1.3.0; docs.rs exata teve erro de acesso, mantido como limitação |

Fontes verificadas pelo agente de transporte: [release v1.3.0](https://github.com/n0-computer/iroh/releases/tag/v1.3.0), [Cargo.toml da tag](https://raw.githubusercontent.com/n0-computer/iroh/v1.3.0/iroh/Cargo.toml), [presets da tag](https://github.com/n0-computer/iroh/blob/v1.3.0/iroh/src/endpoint/presets.rs), [advisory rustls](https://rustsec.org/advisories/RUSTSEC-2026-0285.html), consulta 29/09/2026. `N0DisableRelay` ainda teria descoberta/publicação e não foi selecionado. A [ADR](ADR/PROPOSAL-aurora-connect.md) explica a escolha frente às alternativas. Pesquisas históricas abaixo continuam distintas desta configuração.

O grafo de features conferido por `cargo tree -e features -i iroh` contém apenas `tls-ring` na seleção direta; `cargo tree -i rustls` identifica 0.23.45 e `cargo tree -i portmapper` não encontra o pacote. A presença de código upstream de relay no artefato não significa ativação. O [cliente de relay da tag 1.3.0](https://raw.githubusercontent.com/n0-computer/iroh/v1.3.0/iroh-relay/src/client.rs) constrói conexão WebSocket com TLS para URL HTTPS: evidência estática de um candidato sob UDP externo bloqueado, sem execução/habilitação, autorização de operador ou homologação de voz nesta configuração.

### Auditoria automatizada local — 29/09/2026

A ausência inicial de `cargo audit` foi resolvida instalando **cargo-audit 0.22.2** somente em `.aurora/connect-tools`, com compilação em `.aurora/connect-tools-build`; não houve instalação global nem dependência nova do desktop. `cargo info` confirmou MIT OR Apache-2.0 e MSRV 1.88. Instalação com `--locked --no-default-features --debug`, sem conta/cobrança; arquivos auxiliares permanecem ignorados pelo Git. Comando executado pelo coordenador:

```powershell
./.aurora/connect-tools/bin/cargo-audit.exe audit --file apps/connect-lab/Cargo.lock --db .aurora/connect-advisory-db --json
```

Resultado conferido no relatório local `.aurora/connect-validation/audit-20260929.json`: exit 0; 358 dependências; 1277 advisories; base `f23b768236fe2880e4cfa167da662cad8ca79240`, atualizada em `2026-09-29T04:32:15-03:00`; zero vulnerabilidades e um warning. Lista de ignores vazia, sem silenciamento ou substituição automática.

Após o lock final do terceiro incremento, o coordenador repetiu a auditoria com `--no-fetch` contra a mesma base e atualizou o relatório: exit 0, mesmo resultado. SHA-256 de `apps/connect-lab/Cargo.lock`: `94D0D98CA1D8BE92FAB31B20AC598B8AF7296CBFA30056B7F09D5418F14B3AA9`, também conferido na revisão documental. A repetição usa o snapshot de advisories identificado acima; não afirma atualização posterior dessa base.

O warning [RUSTSEC-2024-0436](https://rustsec.org/advisories/RUSTSEC-2024-0436.html) informa ausência de manutenção de `paste 1.0.15`, não vulnerabilidade classificada nesse relatório. O grafo Windows consultado por `cargo tree --locked -i paste` não encontra uso; `--target all` mostra a proc-macro via `netlink-packet-core 0.8.2` → `netdev 0.45.1`/`netwatch 0.19.3` → Iroh 1.3.0. O alerta continua relevante para os alvos em que essa cadeia é compilada e exige acompanhamento upstream antes de ampliar a homologação. Zero vulnerabilidades conhecidas nesse snapshot não comprova segurança absoluta, revisão do código, cobertura de todas as falhas ou obrigações de licença.

### Correspondência dos registros anteriores

O pacote alpha.13 (`d6f31db`) usava IDs numéricos `DEP-01..17`; estes permanecem históricos, com fonte/data originais acessíveis por `git show d6f31db:docs/engineering/AURORA_CONNECT_DEPENDENCIES.md`. O cadastro canônico usa os IDs abaixo. Nenhuma consulta histórica foi promovida a revalidação atual de termos ou da conta.

| ID histórico | Registro canônico |
| --- | --- |
| DEP-01 / DEP-02 / DEP-03 / DEP-04 | DEP-IROH-LIB / DEP-IROH-PUBLIC / DEP-IROH-MANAGED / DEP-IROH-DISCOVERY |
| DEP-05 / DEP-06 | DEP-QUINN / DEP-GO-ALT |
| DEP-07 / DEP-08 | DEP-RELAY-OTHER / DEP-PORTMAP |
| DEP-09 / DEP-10 / DEP-11 | DEP-E4MC / DEP-WORLDHOST / DEP-PLAYIT |
| DEP-12 / DEP-13 / DEP-14 | DEP-FIREBASE-SIGNAL / DEP-WORKER-SIGNAL / DEP-SUPABASE |
| DEP-15 / DEP-16 / DEP-17a / DEP-17b | DEP-SVC / DEP-PLASMO / DEP-MODRINTH / DEP-CURSEFORGE |

Referência Iroh 1.1.0 do pacote alpha.13 foi consultada em documentação/tag versionadas (MIT OR Apache-2.0, MSRV 1.91, `noq` 1.2.0 naquela tag). Isso preserva uma fonte reproduzível da comparação, não um pin obrigatório. A divergência de `latest` 1.3.0 fica resolvida somente pelo artefato/API escolhidos no incremento. Fontes históricas: [manifest v1.1.0](https://raw.githubusercontent.com/n0-computer/iroh/v1.1.0/iroh/Cargo.toml), [Connection 1.1.0](https://docs.rs/iroh/1.1.0/iroh/endpoint/struct.Connection.html), [anúncio 1.1.0](https://www.iroh.computer/blog/iroh-1-1-0). Consultas registradas no pacote de 29/09/2026; avisos posteriores exigem verificação própria.

### DEP-IROH-PUBLIC — relays públicos n0

| Campo | Registro |
| --- | --- |
| Operador/finalidade | n0.computer; introdução/travessia de NAT e fallback de tráfego cifrado entre endpoints |
| Autenticação | Acesso ao relay público sem credencial; identidade/autorização entre agentes Aurora continuam obrigatórias |
| Licença/termos | Software Iroh MIT/Apache-2.0; não equivale a direito ilimitado sobre o serviço. Documentação autoriza desenvolvimento/hobby. Termos gerais lidos integralmente; aplicação da restrição a automação ao SDK/relay público permanece **NÃO VERIFICADO**; ver DEP-TERMS |
| Quotas/limites | Rate limiting por conexão; números exatos não publicados; capacidade compartilhada; sem SLA; somente stable atual oficialmente suportada; sem version locking |
| Regiões | Hostnames publicados: `use1-1`, `usw1-1`, `euc1-1`, `aps1-1` em `relay.n0.iroh.link`. Localização contratual, residência e região Brasil: **NÃO VERIFICADO**; não inferir por prefixo |
| Cobrança | Documentado sem custo; não exige plano pago. Não converter indisponibilidade em upgrade. Sustentabilidade/autorização para distribuição do Aurora: **NÃO VERIFICADO** e não aprovada |
| Dados/segredos | Tráfego E2E; operador observa metadados/IP/volume e monitora abuso. Não enviar tokens Microsoft/Minecraft ou logs de áudio |
| Falha | Se caminho direto já estiver funcionando, avaliar continuidade; novas sessões e fallback podem falhar. Throttling/indisponibilidade precisam de estado real, timeout/cancelamento e erro claro; não prometer quota detectável sem evento |
| Decisão | Gratuidade e finalidade de desenvolvimento confirmadas para candidato de ensaio sintético curto; conexão ainda não executada, pendente delimitação dos termos em DEP-TERMS. Distribuição pública desabilitada |
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
| DEP-IROH-LIB / Iroh | n0/comunidade; transporte e conectividade Rust | Histórico: API latest exibiu 1.3.0. Atual: pin exato 1.3.0 e lock independente verificados em DEP-IROH-LAB; MIT OR Apache-2.0 | Identidade criptográfica de endpoint; autorização Aurora própria | Metadados das transitivas inventariados; obrigações de distribuição e limites de produto continuam pendentes. Infraestrutura nos cadastros acima | Biblioteca sem tarifa indicada; presets podem ativar serviços externos. Laboratório usa somente configuração explícita sem serviços remotos | Selecionado apenas para laboratório. [Repo](https://github.com/n0-computer/iroh), [manifest exato](https://raw.githubusercontent.com/n0-computer/iroh/v1.3.0/iroh/Cargo.toml) |
| DEP-QUINN / Quinn | Quinn project; QUIC Rust | 0.11.12 observado; MIT/Apache-2.0; versão final a selecionar | TLS; verificar identidade do peer vinculada ao convite, nunca aceitar qualquer certificado | Sem serviço de sinalização/relay incluído; NAT traversal necessita desenho compatível. Plataformas/pacote efetivo precisam de teste | Biblioteca não comprova infraestrutura grátis. Falha direta sem fallback elegível é erro claro | Alternativa Rust, não implementação paralela. [API](https://docs.rs/quinn/latest/quinn/), [repo](https://github.com/quinn-rs/quinn) |
| DEP-GO-ALT / Pion ICE + quic-go | Comunidades Pion e quic-go; alternativa em processo auxiliar | Pion v4.4.4 e quic-go v0.63.0 são observações históricas, MIT; README Pion consultado depois aponta v5. Nenhum pin selecionado | ICE não substitui TLS/autorização de serviço; provar reaproveitamento do caminho/socket | Sem serviço gratuito incluído; operador STUN/TURN teria cadastro próprio. Limites e performance DATAGRAM a testar | Sem tarifa da biblioteca; sidecar adiciona build/assinatura/updater/IPC/shutdown e pode depender de serviço pago, não aprovado | Somente com vantagem comprovada. [README atual consultado](https://raw.githubusercontent.com/pion/ice/master/README.md), [quic-go](https://pkg.go.dev/github.com/quic-go/quic-go), [composição](https://quic-go.net/docs/quic/transport/) |
| DEP-E4MC / e4mc e infraestrutura associada | Projeto vgskye/e4mc; abrir LAN/domínio, relay e Dialtone direto | Leitura posterior de `rererewrite` mostra 6.2.2, enquanto outra resposta em cache mostrou 6.2.1; sem commit fixo, não selecionar artefato por branch/cache. Mod MIT; relay repo MIT/Apache-2.0 | Protocolo e autorização de infraestrutura para integração Aurora **NÃO VERIFICADO** | Quotas, operador contratual por serviço, regiões e termos de distribuição/uso de terceiros **NÃO VERIFICADO**; UDP/voz não comprovados | Não foi aprovado compromisso de gratuidade sustentável. Indisponibilidade não pode exigir contratação ou relay do usuário | Comparar sem descartar por relay. [Propriedades consultadas](https://raw.githubusercontent.com/vgskye/e4mc-minecraft-architectury/rererewrite/gradle.properties), [changelog](https://github.com/vgskye/e4mc-minecraft-architectury/blob/rererewrite/CHANGELOG.md), [relay](https://github.com/vgskye/e4mc-quiclime) |
| DEP-WORLDHOST / World Host e serviço | Projeto Gaming32/World Host; sessão/friends, UPnP/Proxy e pesquisa de hole punching | `main` declara 0.5.2; MIT; release efetiva não selecionada | README/fluxo descrevem mediação/validação; contrato completo de identidade e integração **NÃO VERIFICADO** | Operador contratual, termos, quotas, regiões, UDP genérico/voz e estabilidade **NÃO VERIFICADO** | Gratuidade sustentável/cobrança **NÃO VERIFICADO**; indisponibilidade com erro, sem opção paga silenciosa | Comparar, não homologado. [Repo](https://github.com/Gaming32/world-host), [fluxo UDP](https://github.com/Gaming32/world-host/blob/main/UDP_HOLE_PUNCH_PACKET_FLOW.md) |
| DEP-SVC / Simple Voice Chat | Projeto Simple Voice Chat/modrepo; mod de voz existente, não serviço Aurora de voz | Versão/loader/artefato/licença final: registrar na matriz de mod; **NÃO VERIFICADO** neste cadastro | Preservar autenticação/identidade/criptografia do mod; túnel não as substitui | Endereços, porta efetiva e API da versão exigem verificação; `voice_host` é investigação. Região é a do host/relay escolhido, não do nome do mod | Mod não implica infraestrutura externa gratuita. Falha de voz não derruba jogo; instalação segue origem/licença/updater | Homologação própria obrigatória. [Proxy/configuração](https://modrepo.de/minecraft/voicechat/wiki/proxy_setup): pesquisa detalhada registrada na [especificação](AURORA_CONNECT_SPEC.md) |
| DEP-PLASMO / Plasmo Voice | Projeto Plasmo Voice; mod de voz existente | Versão/loader/artefato/licença final: registrar na matriz; **NÃO VERIFICADO** neste cadastro | Preservar mecanismos do mod; não inventar identidade encaminhada por IP | Configuração efetiva e API exigem verificação; `[host.public]` é investigação. Quotas/regiões de serviços extras **NÃO VERIFICADO** | Não aprova add-ons/proxies externos. Falha de voz independente do jogo; recuperação não apaga configuração concorrente | Homologar separadamente de SVC. [Endereçamento](https://plasmovoice.com/docs/server/advanced/): pesquisa detalhada na [especificação](AURORA_CONNECT_SPEC.md) |

## Complemento da pesquisa dos mods — versão não é homologação

O [registro de preparação dos mods](../aurora-connect-voice.md) contém a seleção posterior dos candidatos Fabric 1.21.1 SVC 2.6.22 e Plasmo 2.1.17 e a verificação dos arquivos baixados dos distribuidores oficiais. SHA-256 local SVC `972a01592f670aa57f2dc2e3587a656b798106037468c61e4bfb49eba7d6ae36`, com SHA-512 igual ao publicado; Plasmo `46e1a4ae51786457712991ba05bf573e2d11331aeadb63b7d108839def022a2b`, igual ao digest publicado. Metadados e APIs relevantes foram conferidos diretamente nos JARs. Licenças declaradas: SVC All Rights Reserved, Plasmo LGPL-3.0; aquisição pública para inspeção não autoriza redistribuição genérica. Arquivos ficam em `.aurora` ignorado, sem execução ou instalação. São 31/31 testes Node finais, incluindo os dois JARs reais e guardas de leitura; nenhum Minecraft ou áudio. Os registros de branch abaixo são contexto histórico e não substituem esses artefatos nem ampliam a matriz. As linhas do cadastro geral acima permanecem sem seleção para distribuição/runtime.

DEP-SVC: o branch oficial `26.3` consultado declara mod `2.6.24+26.3`, API `2.6.24` e beta; **não foi selecionado para Aurora**, cuja matriz não inclui Minecraft 26.3. Licença lida nesse branch: **All Rights Reserved**. A FAQ admite uso em modpacks com crédito; isso não é licença irrestrita de cópia nem autorização automática para distribuição do launcher/API. Registrar licença/artefato por versão antes de instalar ou criar add-on. A API pública `VoiceHostEvent` permite investigar anúncio por participante; versão mínima/loader/LAN continuam pendentes. Não há operador de hospedagem adicional selecionado para o mod; tamanho real de UDP, quotas do host e regiões dependem da sessão/relay homologados. Fontes de 29/09/2026: [propriedades](https://raw.githubusercontent.com/henkelmax/simple-voice-chat/26.3/gradle.properties), [API](https://raw.githubusercontent.com/henkelmax/simple-voice-chat/26.3/api/gradle.properties), [licença](https://raw.githubusercontent.com/henkelmax/simple-voice-chat/26.3/license), [FAQ](https://modrepo.de/minecraft/voicechat/faq), [evento](https://voicechat.modrepo.de/de/maxhenkel/voicechat/api/events/VoiceHostEvent.html).

DEP-PLASMO: **2.1.17** tem release e API versionada consultadas; é candidata de pesquisa, não seleção/homologação Aurora. O projeto declara **LGPL-3.0**; conferir obrigações e licença do artefato/add-on escolhido. O evento cliente `UdpClientConnectEvent` permite investigar substituição do cliente UDP; não comprova redirecionamento pronto nem API servidor por jogador. Preservar autenticação/secret/criptografia nativa; não enviar secret à sinalização. Serviço local do mod não é infraestrutura externa gratuita fornecida pelo projeto; nenhum add-on/relay adicional está aprovado. Fontes de 29/09/2026: [release](https://github.com/plasmoapp/plasmo-voice/releases/tag/2.1.17), [API versionada](https://dokka.plasmovoice.com/older/2.1.17/index.html), [repositório](https://github.com/plasmoapp/plasmo-voice), [criptografia](https://plasmovoice.com/docs/api/encoding-and-encryption.html).

Para ambos, origem de download/termos do distribuidor, hashes e configuração efetiva ficam pendentes por linha da matriz. Download usa o updater/origem autorizados e o fluxo oficial CurseForge; não há mirror novo. Falha de autenticação, endpoint ou compatibilidade de voz mantém jogo disponível quando possível. Não são obrigatórios na mesma instância. Portas padrão, APIs em latest e suporte upstream a LAN não homologam todos os pares Aurora. Outros serviços TCP/UDP precisam de extensão explícita e cadastro próprio.

Proveniência adicional de 29/09/2026: SVC `ServerConfig.java` do commit `7d4317264596736e935cb5a5fbcf899376aa8b34` declara os defaults da linha 1.21.1/2.6.22 (fonte SHA-256 `10e1bc0195748417e4428a117dd63edad361f43ef384f4951da8800d7848543c`). Plasmo tag `2.1.17` em `2b4d22cf9333bc32dad183445b28a267cbf8145f` declara 2.2.0 no manifest da fonte; branch `v2.1.x` fixada em `4c717b85c4bbfa286c7809a4018137142798986b` declara 2.1.17, com campos compatíveis com a classe do JAR selecionado (fonte `VoiceServerConfig.java` SHA-256 `2d2c223eb30712b8b06371c5208712040e978d81e64e46a0e7dd3afa1b97327e`). São evidências estáticas, não vínculo de build reproduzível. [Fontes, divergência e implicação do writer sobre rollback](../aurora-connect-voice.md#defaults-completos-e-reserialização--nono-incremento). Nenhum pin binário, dependência, versão da matriz ou licença foi alterado por esse levantamento.

## DEP-TERMS — limitações das fontes e decisões pendentes

Reconsulta em 29/09/2026 após o baseline `b1c6871`: [Public Relays](https://docs.iroh.computer/iroh-services/relays/public), [rate limiting](https://docs.iroh.computer/relays/rate-limiting), [hosting](https://www.iroh.computer/services/hosting) e [preços](https://www.iroh.computer/pricing) confirmam gratuidade, uso para desenvolvimento/teste e ausência de conta/autenticação obrigatória no relay público. Quotas exatas não são publicadas; não há SLA. Portanto, o custo e a finalidade técnica de um ensaio sintético pequeno estão documentados; não se rejeita relay gratuito por princípio. Nenhuma conta, plano ou relay foi ativado.

Os [termos gerais resumidos](https://www.iroh.computer/legal) e os [termos completos de Iroh Services](https://services.iroh.computer/legal/terms), datados de 14/05/2025, foram agora lidos integralmente. O segundo frontend exige JavaScript; como o navegador não estava disponível, seu texto foi lido no [asset público identificado pelo HTML/roteamento oficial](https://services.iroh.computer/assets/page-D1XY7eeI.js), sem executar JavaScript. O texto contém aceite pelo uso e restrição ampla a acesso automatizado sem permissão escrita. Não foi localizada uma delimitação explícita dessa cláusula para o SDK/relay público. Essa lacuna de aplicabilidade permanece; não é evidência de cobrança nem conclusão de que todo teste seja proibido.

Ensaio candidato, **não executado**: dois endpoints efêmeros autenticados, somente dados sintéticos, até 1 MiB de payload total e menos de 60 segundos; um relay público explicitamente selecionado, sem descoberta pública, telemetria, conta, assinatura, trial ou fallback. Volume de payload não inclui handshake/overhead externo e não é garantia de quota. Antes de conectar, resolver a aplicabilidade dos termos e registrar decisão específica de teste. Se surgir aceite adicional, cobrança ou necessidade de conta, o ensaio não prossegue automaticamente. Produção continua sem aprovação.

Consulta preparada, **não enviada**, ao contato oficial `hello@n0.computer` publicado na página Legal:

> Please confirm which terms govern a one-off automated development test using the official Iroh 1.3.0 SDK through a public n0 relay: two ephemeral endpoints, synthetic data only, at most 1 MiB total application payload, under 60 seconds, no account, API key, subscription, trial or telemetry. Does the restriction on automated access in Iroh Services Terms §3 apply to this SDK test? Is this use permitted without fees or additional agreement? Please provide the applicable public terms or written clarification. This is not a request for production use.

Não é necessário abrir conta nem fornecer código, IP, chave ou logs para essa consulta. Uma resposta oficial identificando o escopo e a condição de gratuidade permite reavaliar o ensaio restrito de AUR-R4-004; não autoriza distribuição. O agente não pode obter esclarecimento privado sem enviar mensagem externa, ação que não foi solicitada. Esse gate bloqueia somente a conexão ao relay público; implementação local, testes, auditoria de artefatos e preparo dos adaptadores prosseguem.

Há divergência editorial upstream: uma seção de segurança chama relay-only de futuro, enquanto o guia de redes já mostra desativação de transportes IP. Usar código/documentação da versão fixada e teste, não a frase mais conveniente. URLs `latest` e branches móveis são evidência da consulta, não locks reproduzíveis. Nenhuma porcentagem de sucesso ou ganho de ping do fornecedor é métrica Aurora.

O [backlog](CODEX_TASKS/AUR-R4-CONNECT-BACKLOG.md) deve registrar, por dependência habilitada, versão/hash, configuração permitida, fonte/data, decisão teste/produção, evidência de custo, teste de falha/quota e responsável. Recursos que continuarem sem prova ficam experimentais e desativados. A ausência de um serviço elegível não autoriza enfraquecer autenticação, expor portas indiscriminadamente nem contratar infraestrutura.

## Auditoria desktop e correção pontual TLS — sétimo incremento

O lock desktop tem escopo diferente do lock `apps/connect-lab`. A auditoria com Cargo Audit 0.22.2/DB `f23b768236fe2880e4cfa167da662cad8ca79240` encontrou `rustls 0.23.43`/RUSTSEC-2026-0285 preexistente na alpha.14. [Aviso oficial](https://github.com/rustls/rustls/security/advisories/GHSA-2mjx-qc3c-rqvc) e [release corrigida 0.23.45](https://github.com/rustls/rustls/releases/tag/v/0.23.45) conferidos em 29/09/2026. A falha aceita mensagens TLS 1.3 em nível de criptografia indevido; o transcript continua autenticado, portanto não equivale a prova de interceptação arbitrária. Corrigida nesta branch com `cargo update --offline --manifest-path apps/desktop/src-tauri/Cargo.toml -p rustls@0.23.43 --precise 0.23.45`: apenas versão/checksum do pacote mudaram. Sem alteração de features, API pública, toolchain ou atualização ampla de dependências. `cargo info rustls@0.23.45 --offline`: Apache-2.0 OR ISC OR MIT, MSRV 1.71.

Nova auditoria `--no-fetch --json`: **exit 0, zero vulnerabilidades conhecidas, 546 dependências**, oito warnings mantidos sem ignore. Relatórios locais `.aurora/connect-validation/desktop-audit-20260929.json` (antes) e `desktop-audit-fixed-20260929.json` (depois). Lock final SHA-256 `74fc67f016a01c82c4b3cac885914e7f4e722cb5273e1333755f3b643a4d3784`. Nenhum resultado promove segurança absoluta ou distribuição multiplataforma.

| Aviso remanescente | Alcance verificado | Gate preservado |
| --- | --- | --- |
| Cinco crates `unic-* 0.9.0`, unmaintained | Presentes no alvo Windows via `unic-ucd-ident → urlpattern → tauri-utils → Tauri` | Manutenção/upstream da stack, sem trocar toda a árvore nesta correção |
| `proc-macro-error 1.0.4`, RUSTSEC-2024-0370 | Ausente no Windows atual; `--target all`: `glib-macros`/`gtk3-macros` | Cadeia GTK de Linux/BSD, não liberada por teste Windows |
| `glib 0.18.5`, RUSTSEC-2024-0429, unsound | Ausente no Windows atual; GTK/WebKitGTK/Wry/Tauri em Linux/BSD | Exige atualização compatível e testes nesses alvos antes de distribuição |
| `chacha20 0.10.1`, yanked | Ausente nas features atuais, inclusive `--target all`; lock liga a rand/Quinn opcional de HTTP/3 do reqwest | Reavaliar antes de habilitar HTTP/3; yanked sozinho não declara vulnerabilidade |

Cadeias conferidas por `cargo tree --locked --manifest-path apps/desktop/src-tauri/Cargo.toml -i PACOTE`, repetido com `--target all` para os ausentes, e manifestos locais Tauri/Wry/reqwest. `rustls` corrigido está no Windows via reqwest/hyper-rustls e edge-tts-rust/tokio-tungstenite. Teste HTTPS público de catálogo e suites nativas estão registrados no task; nenhum login, credencial, relay ou cobrança foi usado. R0 continua aberto para avisos/targets e gates restantes.

## Runtime descartável — décimo quarto incremento

Preparação local sem redistribuição: Node 24.8 já instalado, somente APIs nativas, nenhum pacote npm ou lock novo. Alvo Minecraft 1.21.1/Fabric 0.16.14/Java 21 já pertence à matriz. Java local observado por leitura do `release`: Eclipse Adoptium `21.0.12.1`, Windows/x86_64; nenhuma execução do binário nessa inspeção. Hash do launcher Java/release fixa os dois arquivos observados, não todas as bibliotecas do JDK. Os [pins compilados](../../apps/connect-runtime-tools/pins.mjs) fixam nove URLs, tamanhos e digests; [operação e gates](../aurora-connect-runtime.md).

Consulta oficial em 29/09/2026: [metadata Mojang 1.21.1](https://piston-meta.mojang.com/v1/packages/22a1966494dfa4eeb5ee778c8e6ed5b774839582/1.21.1.json), SHA-1 do metadata `22a1966494dfa4eeb5ee778c8e6ed5b774839582`; servidor 51.627.615 bytes, SHA-1 `59353fb40c36d304f2035d51e7d6e6baa98dc05c`. SHA-256 não é publicado nesse metadata; o downloader calculará e registrará o observado, sem inventar fonte independente. Somente a URL Mojang autorizada será usada. **Nenhum JAR Minecraft baixado ou executado até este registro.**

O [perfil Fabric](https://meta.fabricmc.net/v2/versions/loader/1.21.1/0.16.14/server/json) publica tamanho/SHA-256 de ASM 9.8 (cinco módulos) e sponge-mixin 0.15.5+mixin.0.8.7. Os outros dois foram obtidos por checksum publicado e HEAD HTTP, sem baixar JAR: [intermediary](https://maven.fabricmc.net/net/fabricmc/intermediary/1.21.1/intermediary-1.21.1.jar.sha256), 657.725 bytes, SHA-256 `6059157dfb4a536ec151697004f74f6004b7d346b289c2f89849e0e585aa36fa`; [loader](https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.16.14/fabric-loader-0.16.14.jar.sha256), 1.416.842 bytes, SHA-256 `9ed7b7f4197153ad97d71b4b9cd9148989e8f0c600934b035ce45f3ef003efc0`. Tamanho HEAD ainda será confrontado com os bytes. Checksum e download do mesmo servidor não são assinatura independente. `time/releaseTime` desse profile são gerados pelo serviço, não datas históricas do loader.

Auditoria de fonte fixada do loader `075dbfa476b1259f1d04a96c2b9afc6bd8f06c4e` confirma [KnotServer](https://raw.githubusercontent.com/FabricMC/fabric-loader/075dbfa476b1259f1d04a96c2b9afc6bd8f06c4e/src/main/java/net/fabricmc/loader/impl/launch/knot/KnotServer.java), [propriedade gameJarPath](https://raw.githubusercontent.com/FabricMC/fabric-loader/075dbfa476b1259f1d04a96c2b9afc6bd8f06c4e/src/main/java/net/fabricmc/loader/impl/util/SystemProperties.java) e [MinecraftGameProvider](https://raw.githubusercontent.com/FabricMC/fabric-loader/075dbfa476b1259f1d04a96c2b9afc6bd8f06c4e/minecraft/src/main/java/net/fabricmc/loader/impl/game/minecraft/MinecraftGameProvider.java). Servidor usa cwd; inicialização do bundler executa código, não é inspeção. Não se usa installer nem download transitivo genérico por POM; profile fixa oito bibliotecas externas. [Build fixado](https://raw.githubusercontent.com/FabricMC/fabric-loader/075dbfa476b1259f1d04a96c2b9afc6bd8f06c4e/build.gradle) incorpora tiny-remapper, mapping-io, access-widener, SAT4J e MixinExtras. Licenças lidas na auditoria: loader/mapping-io/access-widener Apache-2.0, intermediary CC0, ASM BSD-3-Clause, Mixin/MixinExtras MIT, tiny-remapper LGPL-3.0, SAT4J EPL-1.0/LGPL-2.1. Nenhuma distribuição desses binários foi autorizada; conferir também avisos incorporados antes de distribuição futura.

[EULA Minecraft](https://www.minecraft.net/en-us/eula) vincula download/uso à concordância e incorpora o Microsoft Services Agreement. O prompt proíbe aceitar contratos em nome do usuário: `stage` exige declaração de aceitação já obtida, além do hash do plano; `eula=false` continua no arquivo preparado. Não há serviço pago, conta nova, trial, Realms, credencial ou fallback comercial. Falha de rede/tamanho/hash preserva somente staging próprio incompleto e não executa artefato. Depois de consentimento, runtime precisará de autorização/validação próprias, bind observado e encerramento controlado. [Java 21](https://docs.oracle.com/en/java/javase/21/docs/specs/man/java.html) confirma influência de `JDK_JAVA_OPTIONS`; limpar variáveis somente no ambiente do futuro filho, nunca globalmente.

## Cliente de protocolo somente para laboratório — incremento 19

Pacote **privado e isolado** `apps/connect-client-lab`, sem dependência no desktop: `minecraft-protocol =1.68.0`, BSD-3-Clause, Node >=22, fonte `2ba67550867179f15bc0530a48ab56b8a5dabd9f`; `minecraft-data =3.109.0`, MIT, fonte `491852d95b32edc37547400f39090a15737baaf1`. Node local 24.8.0, alvo 1.21.1/767 já na matriz. Metadados npm/licenças e [manifest](https://github.com/PrismarineJS/node-minecraft-protocol/blob/2ba67550867179f15bc0530a48ab56b8a5dabd9f/package.json), [cliente](https://github.com/PrismarineJS/node-minecraft-protocol/blob/2ba67550867179f15bc0530a48ab56b8a5dabd9f/src/createClient.js), [play](https://github.com/PrismarineJS/node-minecraft-protocol/blob/2ba67550867179f15bc0530a48ab56b8a5dabd9f/src/client/play.js) e [protocolo767](https://github.com/PrismarineJS/minecraft-data/blob/491852d95b32edc37547400f39090a15737baaf1/data/pc/1.21.1/protocol.json) conferidos em 29/09/2026. Quando a ferramenta web recusou raw, a mesma fonte oficial foi lida via HTTPS/artefato, sem adivinhar APIs.

Operador de aquisição: registry público npm; 75 entradas finais com URL HTTPS registry.npmjs.org, SHA-512 integrity e licenças MIT/BSD-2/BSD-3/Apache-2/ISC/CC0. Sem scripts de instalação declarados; `npm ci --ignore-scripts --no-fund --no-audit`. Não há hospedagem, conta, Realms, trial, cobrança ou fallback comercial. O cliente proposto fixa auth offline/IP loopback/versão/porta; módulos de autenticação presentes na árvore não autorizam usá-los. Sem distribuição nesta tarefa; avisos/licenças continuam obrigatórios se ela for futuramente autorizada.

Primeiro audit: cinco entradas moderadas transitivas por uuid8.3.2/10.0.0. [Advisory do autor GHSA-w5hq-g745-h8pq / CVE-2026-41907](https://github.com/uuidjs/uuid/security/advisories/GHSA-w5hq-g745-h8pq) afeta bounds de saída v3/v5/v6. Override **uuid11.1.1**, MIT, corrigido e CommonJS: callers msal/yggdrasil usam v4, conferido na fonte e exercitado por testes, junto ao CryptoProvider e negativas v3/v5/v6. Não foi aplicado o downgrade antigo sugerido pelo npm. Audit final: **zero vulnerabilidades conhecidas**, 75 dependências reportadas, sem ignore. Não garante ausência de riscos sem advisory.

Fontes de [framing](https://github.com/PrismarineJS/node-minecraft-protocol/blob/2ba67550867179f15bc0530a48ab56b8a5dabd9f/src/transforms/framing.js) e [compressão](https://github.com/PrismarineJS/node-minecraft-protocol/blob/2ba67550867179f15bc0530a48ab56b8a5dabd9f/src/transforms/compression.js) revelaram acumulação sem teto e inflate sem maxOutputLength. `FrameGuard` próprio limita prefixo canônico a três bytes, payload1..2.097.151 bytes e wire total32MiB, libera só frames completos e aplica contrapressão/cancelamento. Não valida schema/NBT nem limita memória/CPU de qualquer parser; **não resolve compressão sozinho**. Integração seguinte precisa desabilitar compressão no perfil, recusar negociação inesperada antes do handler e usar prazo/processo limitado. Não expor o harness a servidor não controlado. Quatorze testes de frames e três de dependências passaram; nenhuma prova de jogador neste incremento.

## Registros complementares históricos

Atualização efetivamente executada no incremento 18, posterior à preparação histórica acima: usuário aceitou EULA/MSA e autorizou o ensaio. Os nove artefatos (55.582.631 bytes) foram baixados dos pins, com tamanho/hash publicados conferidos. SHA-256 local observado do servidor Mojang: `e3bc55693e93cda0188f2e60aea28113fc647c5e85a15fa3d1b347349231b4bb`; não atribuir esse SHA-256 ao metadata oficial. Servidor 1.21.1/Fabric 0.16.14/Java 21 foi executado em pasta nova e respondeu STATUS/pong por dois agentes; nenhum JAR foi redistribuído e nenhuma dependência/lock foi acrescentado. O usuário pediu `online-mode=false` no laboratório; isso não remove autenticação Connect ou gates de produto. [Execução, falha corrigida e encerramento](../task-aur-r4-connect.md#décimo-oitavo-incremento--minecraft-real-por-dois-agentes-autenticados).

Dependência local do sexto incremento: `toml =0.9.12`, resolvido `0.9.12+spec-1.1.0`, MIT OR Apache-2.0, MSRV 1.76; já presente no lock desktop, agora aresta direta opcional de `connect-voice-lab`, features parse/serde/std sem defaults. Fonte/Cargo.toml local e [API versionada](https://docs.rs/toml/0.9.12%2Bspec-1.1.0/toml/) conferidos em 29/09/2026. Finalidade: spans e validação do anúncio Plasmo, sem serializer geral, serviço remoto, conta, quota ou custo. Falha de parsing recusa plano e conserva arquivo. O lock desktop foi auditado separadamente e registrou rustls 0.23.43/RUSTSEC-2026-0285 preexistente; a correção pontual 0.23.45 e os warnings são registrados no task, sem confundir com o lock independente Connect.

Os itens seguintes eram detalhados no primeiro pacote e não podem desaparecer na reconciliação. São pesquisa histórica de 29/09/2026, sem nova ativação, instalação ou prova de conta. Termos, preços, versões e permissões precisam de reconsulta antes de uso; nenhum participa do laboratório direto inicial.

| ID | Operador/finalidade | Autenticação, licença/termos | Quotas/regiões/limites | Custo, indisponibilidade e decisão |
| --- | --- | --- | --- | --- |
| DEP-PORTMAP | Responsável pelo roteador; PCP/NAT-PMP/UPnP opcionais | Consentimento específico, biblioteca/versão a auditar; propriedade do mapping e lease curto | Só rede local; não remove CGNAT; quotas de nuvem não aplicáveis | Desabilitado; falha não autoriza administração permanente nem alterar mapping alheio; limpar apenas concessão própria |
| DEP-PLAYIT | Developed Methods LLC/playit.gg; túnel terceiro | Conta/agente; licença exata pendente. Termos §3.4 consultados condicionavam serviço agregado/redistribuído a consentimento escrito | Página histórica anunciava Java/SVC/UDP grátis e TCP genérico/TCP+UDP premium; quota/região gratuita efetiva não comprovadas | Não selecionado, sem bundling/instalação/conta. Falha não ativa premium/trial. Proxy público não prova E2E entre agentes Aurora |
| DEP-SUPABASE | Supabase; papel atual de aparência, sinalização só alternativa futura | Service role fica no Worker autorizado; RLS/credenciais/contrato novos teriam revisão própria; SDK/licença da versão pendentes | Free historicamente 500 MB DB, 1 GB Storage, 5 GB egress, 200 Realtime/2 milhões mensagens; plano/região/consumo Aurora não verificados | Não ampliar aparência para Connect. Free sujeito a pausa/quotas; Pro custa mesmo com cap; falha de Connect não altera fallback de skins |
| DEP-MODRINTH | Rinth, Inc.; catálogo/download autorizado | API pública e User-Agent; operações privadas exigem scopes próprios. Licença de cada artefato e termos são distintos | Histórico 300 requests/min/IP e headers de rate limit; região/SLA/condições da integração não verificados | 429/falha com retry idempotente limitado; sem Host pago, mirror ou transmissão de JAR por Connect |
| DEP-CURSEFORGE | Overwolf/CurseForge; catálogo/download permitido | `x-api-key` somente no backend; autorização do autor/licença e contrato terceiros independentes | Quota/região/plano da chave Aurora não verificados; não extrapolar de outra conta | Sem URL autorizada mantém fluxo oficial/manual; não inferir CDN/burlar distribuição. Sem nova instalação ou billing |

Fontes históricas oficiais: [playit planos](https://playit.gg/pricing), [playit termos](https://playit.gg/terms/), [Supabase termos](https://supabase.com/terms), [preços](https://supabase.com/pricing), [Modrinth API](https://docs.modrinth.com/api/), [termos](https://modrinth.com/legal/terms), [CurseForge API](https://docs.curseforge.com/rest-api/), [termos terceiros](https://support.curseforge.com/support/solutions/articles/9000207405-curseforge-3rd-party-api-terms-and-conditions). Consulta original registrada em 29/09/2026; não houve console administrativo nesta reconciliação.

SVC API 2.6.20 era referência do primeiro pacote, enquanto a outra revisão observou 2.6.24 em branch 26.3 fora da matriz Aurora. Essa divergência histórica não selecionava um JAR. O quarto incremento confirmou API 2.6.20 embutida no candidato SVC Fabric 1.21.1/2.6.22 e assinaturas da release Plasmo 2.1.17. Artefato/hash/licença/API efetivos permanecem obrigatórios e independentes; nenhuma homologação de mod decorre dessa inspeção ou do encaminhamento UDP sintético.

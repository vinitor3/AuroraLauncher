# Dependency Graph

```mermaid
flowchart TD
  R0[R0 Governança e baseline] --> R1[R1 Launcher + Companion]
  R0 --> OBS[Observabilidade local e fixtures]
  R1 --> U[R1.5 Inventário e updater]
  OBS --> D[R2 Crash parser + Doctor]
  U --> SNAP[Snapshot transacional mínimo]
  SNAP --> CAS[R2 CAS + rollback]
  D --> T0[R2.5 Tools nível 0/1]
  CAS --> T2[R2.5 Tools nível 2]
  T0 --> T2
  U --> M[R3 Manifesto v3]
  CAS --> M
  M --> CREATOR[R3 Creator/Server Pack]
  M --> POC[R4 Multiplayer PoC]
  R1 --> POC
  POC --> SOCIAL[R4 Convites/presença]
  SOCIAL --> SYNC[R4 Live sync consentido]
  M --> MASTER[R3 Master Panel]
  T2 --> MASTER
  R0 --> REL[R5 Release/updater do launcher]
  REL --> PROD[R5 Produção]
```

## Trilho crítico

`R0 → R1 → inventário/updater → snapshot/CAS → Tools mutáveis → manifesto → multiplayer sync`.

## Paralelizável agora — Wave 0

- regras/testes Firestore em `firebase/**`;
- licença/avisos em arquivos jurídicos dedicados;
- CI em `.github/workflows/**`;
- proveniência, versionamento e documentação pública, sem substituir binários históricos.

## Próxima — Wave 1

- timeout HTTP/IPC sem tocar no frontend;
- harness runtime sem reescrever adaptadores;
- decomposição do `App.tsx` com ownership exclusivo;
- substituição transacional com staging/rollback em domínio Rust isolado.

## Bloqueado

- Tools nível 2 por falta de snapshot/rollback;
- live mod sync por falta de manifesto assinado e updater;
- Master por falta de APIs/roles/manifests estáveis;
- auto-updater por falta de chave de assinatura e política de canal.
- lançamento público por falta de prova Microsoft/Xbox/Minecraft de entitlement; Firebase identifica a conta Aurora, não a posse do jogo.

## Deve esperar

- relay Aurora próprio, social completo, analytics remoto, cosméticos pagos, nível 3 de IA e suporte NeoForge.

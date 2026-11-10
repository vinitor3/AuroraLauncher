const origin = "https://aurora-connect-social.aurora-edge-proxy.workers.dev";

/** An explicit build option pairs the frontend/native gates; inherited flags never enable them. */
export function connectOnlineEnvironment(args, inherited = {}) {
  if (args.length > 1 || args.some(value => value !== "--private-invitations")) {
    throw new Error("Opção de build Connect desconhecida.");
  }
  const privateFlag = args.length === 1 ? "1" : "0";
  return { ...inherited, AURORA_SOCIAL_ORIGIN: origin, VITE_AURORA_SOCIAL_ORIGIN: origin,
    AURORA_SOCIAL_PRIVATE_ONLINE: privateFlag, VITE_AURORA_SOCIAL_PRIVATE_ONLINE: privateFlag };
}

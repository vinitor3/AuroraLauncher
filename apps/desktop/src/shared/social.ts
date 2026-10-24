/** Public social projections. Neither a profile nor a session advert grants transport access. */
export type SocialPerson = { uid: string; nickname: string; avatarUrl: string | null };
export type SocialPresence = { modpackName: string | null; updatedAtMs: number };
export type SocialSessionDraft = {
  title: string; modpackName: string; minecraftVersion: string; loader: string;
  imageUrl: string | null; players: number; maxPlayers: number;
  visibility: "private" | "friends" | "public"; joinPolicy: "open" | "approval";
};
export type SocialSession = SocialSessionDraft & { id: string; hostUid: string; expiresAtMs: number };
export type SocialFriend = SocialPerson & { presence: SocialPresence | null; session: SocialSession | null };
export type SocialRequest = SocialPerson & { expiresAtMs: number };
export type SocialSnapshot = {
  friends: SocialFriend[]; incoming: SocialRequest[]; outgoing: SocialRequest[]; blocked: SocialPerson[];
  ownSession: SocialSession | null; presenceSharing: "friends" | "hidden"; modpackName: string | null;
};
export type SocialPage = { items: SocialSession[]; nextCursor: string | null };
export type SocialService = {
  readonly capabilities: { friends: boolean; catalog: boolean; publication: boolean; join: boolean };
  searchByNickname(nickname: string): Promise<SocialPerson | null>;
  snapshot(): Promise<SocialSnapshot>;
  requestFriend(uid: string): Promise<void>;
  respondFriend(uid: string, decision: "accept" | "decline"): Promise<void>;
  cancelRequest(uid: string): Promise<void>;
  removeFriend(uid: string): Promise<void>;
  block(uid: string): Promise<void>;
  unblock(uid: string): Promise<void>;
  setPresence(value: { sharing: "friends" | "hidden"; modpackName: string | null }): Promise<void>;
  publishSession(value: SocialSessionDraft): Promise<void>;
  closeSession(): Promise<void>;
  listPublicSessions(cursor?: string | null): Promise<SocialPage>;
  prepareJoin(sessionId: string): Promise<never>;
};

/** Exact nick search only; never a query over all accounts. */
export function normalizeSocialNickname(value: string): string {
  const nick = value.trim().toLowerCase();
  if (!/^[a-z0-9_]{3,16}$/.test(nick)) throw new Error("Digite o nick Aurora completo, com 3 a 16 letras, números ou _. ");
  return nick;
}

/** Only existing catalog image CDNs. No arbitrary tracking/localhost URLs. */
export function socialImageUrl(value: unknown): string | null {
  if (value === null || value === "" || value === undefined) return null;
  if (typeof value !== "string" || value.length > 512) return null;
  try {
    const url = new URL(value);
    if (url.protocol !== "https:" || url.username || url.password || url.port || url.search || url.hash) return null;
    if (!["cdn.modrinth.com", "media.forgecdn.net", "mediafilez.forgecdn.net"].includes(url.hostname)) return null;
    if (!/^\/[A-Za-z0-9_./%+-]+\.(png|jpg|jpeg|webp)$/i.test(url.pathname)) return null;
    return url.href;
  } catch { return null; }
}

export function validateSocialSession(value: SocialSessionDraft): SocialSessionDraft {
  const limited = (text: string, min: number, max: number) => typeof text === "string" && text.trim().length >= min && text.length <= max && !/[\u0000-\u001f\u007f]/.test(text);
  if (!limited(value.title, 1, 64) || !limited(value.modpackName, 1, 64)
    || !limited(value.minecraftVersion, 1, 24) || !limited(value.loader, 1, 24)
    || !["private", "friends", "public"].includes(value.visibility) || !["open", "approval"].includes(value.joinPolicy)
    || !Number.isInteger(value.players) || !Number.isInteger(value.maxPlayers)
    || value.players < 0 || value.maxPlayers < 1 || value.maxPlayers > 100 || value.players > value.maxPlayers
    || (value.imageUrl !== null && socialImageUrl(value.imageUrl) !== value.imageUrl)) {
    throw new Error("Confira o nome, versão, imagem do catálogo e limite de participantes da sessão.");
  }
  return { ...value, title: value.title.trim(), modpackName: value.modpackName.trim() };
}

/** Private authorization projection. Inbox pointers never enter this boundary. */
import { SocialBindingError, type SocialPrivateAuthorization } from "./connectSocialBinding.ts";

type Field = { stringValue?: string; integerValue?: string; timestampValue?: string };
const keys = ["schema", "authorizationId", "revision", "sessionId", "hostUid", "recipientUid", "status", "createdAt", "updatedAt", "expiresAt"];
const uid = /^[A-Za-z0-9_-]{1,128}$/;
export function privateAuthorizationFields(fields: Record<string, Field>, sessionId: string, recipientUid: string, callerUid: string, now: number): SocialPrivateAuthorization {
  const denied = (): never => { throw new SocialBindingError("session_not_authorized"); };
  if (Object.keys(fields).length !== keys.length || keys.some(key => !Object.hasOwn(fields, key))
    || fields.schema?.integerValue !== "1" || fields.status?.stringValue !== "active") return denied();
  const authorizationId = fields.authorizationId?.stringValue, hostUid = fields.hostUid?.stringValue;
  const revisionText = fields.revision?.integerValue ?? "", revision = Number(revisionText);
  const created = Date.parse(fields.createdAt?.timestampValue ?? ""), updated = Date.parse(fields.updatedAt?.timestampValue ?? "");
  const expiresAtMs = Date.parse(fields.expiresAt?.timestampValue ?? "");
  if (!authorizationId || !/^[0-9a-f]{64}$/.test(authorizationId) || !/^[1-9][0-9]{0,9}$/.test(revisionText)
    || !Number.isSafeInteger(revision) || revision > 0xffff_ffff || fields.sessionId?.stringValue !== sessionId
    || fields.recipientUid?.stringValue !== recipientUid || !uid.test(recipientUid) || !hostUid || !uid.test(hostUid)
    || hostUid === recipientUid || callerUid !== hostUid && callerUid !== recipientUid
    || !Number.isFinite(created) || !Number.isFinite(updated) || !Number.isFinite(expiresAtMs)
    || created > updated || updated > now || updated >= expiresAtMs || expiresAtMs <= now) return denied();
  return { authorizationId, revision, sessionId, hostUid, recipientUid, expiresAtMs };
}

import { fragmentedVoiceContract } from './voice-fragmented-profile.mjs';

// The transport is an operator choice, never inferred from a packet, mod or
// environment variable. Resolve its immutable artifact pin before any I/O.
export function voiceSmokeOptions(profile, args) {
  if (!['svc', 'plasmo'].includes(profile)) throw new Error('client_voice_profile');
  if (!Array.isArray(args) || args[0] !== '--offline-lab') throw new Error(`client_${profile}_usage`);
  const fragmented = args[1] === '--fragmented-udp';
  const offset = fragmented ? 2 : 1;
  if (!(args.length === offset || (args.length === offset + 1 && args[offset] === '--keepalive'))) {
    throw new Error(`client_${profile}_usage`);
  }
  return Object.freeze({ keepalive: args.length === offset + 1, fragmented,
    ...(fragmented ? { transport: fragmentedVoiceContract() } : {}) });
}

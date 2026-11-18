import type { ProviderId } from './types';

const scoped = new Set(['refresh_models', 'select_model', 'connect_provider', 'cancel_provider_login']);
export function commandKey(command: string, args?: Record<string, unknown>): string {
  return scoped.has(command) && typeof args?.provider === 'string' ? `${command}:${args.provider}` : command;
}
export function commandPending(pending: Set<string>, command: string, provider?: ProviderId): boolean {
  if (pending.has(command)) return true;
  return provider ? pending.has(`${command}:${provider}`) : [...pending].some(key => key.startsWith(`${command}:`));
}

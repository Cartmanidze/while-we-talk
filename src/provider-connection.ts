import type { ProviderId, ProviderState } from './types';

export const providerIds: ProviderId[] = ['chatgpt', 'antigravity', 'claude'];
export const providerNames: Record<ProviderId, string> = { chatgpt: 'ChatGPT', antigravity: 'Antigravity', claude: 'Claude Code' };
export type ConnectionPhase = { key: 'login' | 'auth' | 'checking' | 'limits' | 'catalog' | 'model' | 'ready'; label: string; step: number };
export function connectionPhase(id: ProviderId, provider: ProviderState): ConnectionPhase {
  if (provider.authPending) return { key: 'auth', label: 'Жду входа', step: 1 };
  if (provider.loading) return { key: 'checking', label: 'Проверяю…', step: 2 };
  if (!provider.connected) return id === 'antigravity' && provider.models.length > 0
    ? { key: 'catalog', label: 'Список моделей получен', step: 2 }
    : { key: 'login', label: 'Нужен вход', step: 1 };
  if (id !== 'antigravity' && !provider.budgetConfirmed) return { key: 'limits', label: 'Проверь лимиты', step: 2 };
  if (!provider.models.some(model => model.id === provider.selectedModel)) return { key: 'model', label: 'Выбери модель', step: 3 };
  return { key: 'ready', label: 'Готово', step: 3 };
}

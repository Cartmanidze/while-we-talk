import { useEffect, useMemo, useState } from 'react';
import { Icon } from './icons';
import type { ProviderId, ProviderState, RunCommand } from './types';

export function ModelPicker({ id, label, providerId, provider, locked, run }: {
  id: string; label: string; providerId: ProviderId; provider: ProviderState; locked: boolean; run: RunCommand;
}) {
  const [query, setQuery] = useState('');
  useEffect(() => setQuery(''), [providerId]);
  const models = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    const filtered = provider.models.filter(model => `${model.name} ${model.id}`.toLocaleLowerCase().includes(normalized));
    const selected = provider.models.find(model => model.id === provider.selectedModel);
    return selected && !filtered.some(model => model.id === selected.id) ? [selected, ...filtered] : filtered;
  }, [provider.models, provider.selectedModel, query]);
  return <div className="connection-model-picker">
    <label className="model-picker-label" htmlFor={id}>{label}</label>
    {provider.models.length > 8 && <label className="search-field"><Icon name="search" size={15} /><input type="search" value={query} onChange={event => setQuery(event.target.value)} placeholder="Найти модель" aria-label={`Поиск модели: ${label}`} disabled={locked} /></label>}
    <div className="select-wrap model-select"><select id={id} value={provider.selectedModel ?? ''} disabled={locked || !provider.models.length} onChange={event => void run('select_model', { provider: providerId, modelId: event.target.value })}>
      <option value="" disabled>{provider.loading ? 'Загружаю модели…' : provider.models.length ? 'Выбери модель' : 'Пока нет моделей'}</option>
      {models.map(model => <option value={model.id} key={model.id}>{model.name}</option>)}
    </select><Icon name="chevron" size={15} /></div>
  </div>;
}

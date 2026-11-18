import { Icon } from './icons';
import type { AppState, AudioDevice, Settings } from './types';

function LevelMeter({ label, level }: { label: string; level: number }) {
  // A linear bar makes ordinary speech at −30 dB occupy only 3% of the track.
  const db = Math.min(0, Math.round(20 * Math.log10(Math.max(0.001, level))));
  return <div className="level-track" role="meter" aria-label={`Уровень: ${label.toLowerCase()}`} aria-valuemin={-60} aria-valuemax={0} aria-valuenow={db} aria-valuetext={db === -60 ? 'Ниже порога шкалы' : `${db} дБ относительно цифрового максимума`}>
    <span style={{ width: `${(db + 60) / 60 * 100}%` }} />
  </div>;
}

export function LiveSources({ settings, levels }: Pick<AppState, 'settings' | 'levels'>) {
  const sources = [
    { label: 'Микрофон', icon: 'mic', enabled: settings.micEnabled, level: levels.microphone },
    { label: 'Звук компьютера', icon: 'speaker', enabled: settings.systemEnabled, level: levels.system },
  ] as const;
  return <div className="live-sources">{sources.map(({ label, icon, enabled, level }) =>
    <div className="live-source" key={icon}><Icon name={icon} size={16} /><span>{label}</span>
      {enabled && level !== null ? <LevelMeter label={label} level={level} /> : <small>{enabled ? 'Жду звук' : 'Выключен'}</small>}
    </div>
  )}</div>;
}

function SourceControl({ label, icon, enabled, deviceId, devices, level, locked, onEnabled, onDevice }: {
  label: string; icon: 'mic' | 'speaker'; enabled: boolean; deviceId: string | null;
  devices: AudioDevice[]; level: number | null; locked: boolean;
  onEnabled: (value: boolean) => void; onDevice: (value: string | null) => void;
}) {
  return <div className={`source-control ${enabled ? 'is-enabled' : ''}`}>
    <div className="source-heading">
      <span className="source-symbol"><Icon name={icon} size={17} /></span><span>{label}</span>
      <label className="switch"><input type="checkbox" checked={enabled} disabled={locked} onChange={event => onEnabled(event.target.checked)} aria-label={`Использовать ${label.toLowerCase()}`} /><span /></label>
    </div>
    <div className="select-wrap"><select value={deviceId ?? ''} disabled={locked || !enabled || devices.length === 0} aria-label={`Устройство: ${label.toLowerCase()}`} onChange={event => onDevice(event.target.value || null)}>
      <option value="">{devices.length ? 'Устройство по умолчанию' : 'Нет доступных устройств'}</option>
      {devices.map(device => <option key={device.id} value={device.id}>{device.name}</option>)}
    </select><Icon name="chevron" size={14} /></div>
    {enabled && level !== null && <LevelMeter label={label} level={level} />}
  </div>;
}

export function SourcePanel({ state, locked, change }: { state: AppState; locked: boolean; change: (settings: Settings) => void }) {
  const settings = state.settings;
  return <section className="sidebar-section" aria-labelledby="sources-title">
    <div className="section-label" id="sources-title"><span>Источники звука</span><span className="quiet-label">На компьютере</span></div>
    <SourceControl label="Микрофон" icon="mic" enabled={settings.micEnabled} deviceId={settings.micDeviceId} devices={state.devices.microphones} level={state.levels.microphone} locked={locked} onEnabled={micEnabled => change({ ...settings, micEnabled })} onDevice={micDeviceId => change({ ...settings, micDeviceId })} />
    <SourceControl label="Звук компьютера" icon="speaker" enabled={settings.systemEnabled} deviceId={settings.systemDeviceId} devices={state.devices.system} level={state.levels.system} locked={locked} onEnabled={systemEnabled => change({ ...settings, systemEnabled })} onDevice={systemDeviceId => change({ ...settings, systemDeviceId })} />
    {state.listening && <p className="field-note">Чтобы сменить устройство, останови расшифровку.</p>}
  </section>;
}

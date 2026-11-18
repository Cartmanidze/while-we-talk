import { useEffect, useMemo, useRef, useState } from 'react';
import type { RunCommand, Settings } from './types';
import { WindowAppearanceController, type WindowAppearance } from './window-appearance-controller';

const adjustmentKeys = new Set(['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End', 'PageUp', 'PageDown', 'Enter']);

export function WindowAppearancePanel({ settings, applied, connected, run }: {
  settings: Settings; applied: boolean; connected: boolean; run: RunCommand;
}) {
  const current: WindowAppearance = { hideFromCapture: settings.hideFromCapture ?? true, windowOpacity: settings.windowOpacity ?? 100 };
  const mounted = useRef(true);
  const [view, setView] = useState({ ...current, saving: false });
  const controller = useMemo(() => new WindowAppearanceController(current,
    value => run('update_window_appearance', { ...value }),
    value => { if (mounted.current) setView(value); }), [run]);
  useEffect(() => {
    mounted.current = true;
    controller.sync(current);
    return () => { mounted.current = false; };
  }, [controller, current.hideFromCapture, current.windowOpacity]);

  function changeOpacity(windowOpacity: number) {
    controller.edit({ hideFromCapture: controller.snapshot().hideFromCapture, windowOpacity });
  }
  function commit() { if (connected) void controller.commit(!applied); }

  return <section className="window-appearance-panel" aria-labelledby="window-appearance-title">
    <div className="window-appearance-heading"><h3 id="window-appearance-title">Окно</h3>{view.saving && <span role="status">Применяю…</span>}</div>
    <label className="window-privacy-toggle"><input type="checkbox" checked={view.hideFromCapture} disabled={!connected} onChange={event => {
      controller.edit({ hideFromCapture: event.target.checked, windowOpacity: controller.snapshot().windowOpacity }); commit();
    }} /><span>Скрывать при демонстрации экрана</span></label>
    <p className="field-note">Некоторые программы записи всё равно показывают окно. Перед звонком проверь, что увидят участники.</p>
    {!applied && <div className="window-appearance-warning" role="alert"><span>Не удалось изменить вид окна. Попробуй ещё раз.</span><button className="text-button" type="button" disabled={!connected || view.saving} onClick={() => void controller.commit(true)}>Повторить</button></div>}
    <div className="window-opacity-label"><label htmlFor="window-opacity">Непрозрачность окна</label><output htmlFor="window-opacity">{view.windowOpacity}%</output></div>
    <input id="window-opacity" className="window-opacity-range" type="range" min={30} max={100} step={1} value={view.windowOpacity} disabled={!connected} aria-valuetext={`${view.windowOpacity}%`} onChange={event => changeOpacity(Number(event.target.value))} onPointerDown={event => event.currentTarget.setPointerCapture(event.pointerId)} onPointerUp={commit} onBlur={commit} onKeyUp={event => { if (adjustmentKeys.has(event.key)) commit(); }} />
    <div className="window-opacity-footer"><span>Чем меньше процент, тем лучше видно фон за окном.</span><button className="text-button" type="button" disabled={!connected || (view.windowOpacity === 100 && applied && !view.saving)} onClick={() => { changeOpacity(100); commit(); }}>Сделать окно непрозрачным</button></div>
  </section>;
}

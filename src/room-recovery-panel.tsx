import { useState } from 'react';
import { Icon } from './icons';
import { canConfirmRecovery, recoverySignature } from './room-recovery';
import type { RoomRecoveryInfo } from './types';

function dateLabel(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString('ru-RU', { day: 'numeric', month: 'short', year: 'numeric', hour: '2-digit', minute: '2-digit' });
}

export function RoomRecoveryPanel({ recoveries, notice, locked, recovering, onRecover }: {
  recoveries: RoomRecoveryInfo[]; notice: string | null; locked: boolean; recovering: boolean;
  onRecover: (roomId: string) => Promise<boolean>;
}) {
  const [confirmation, setConfirmation] = useState<string | null>(null);
  const [dismissedNotice, setDismissedNotice] = useState<string | null>(null);
  const visibleNotice = notice && notice !== dismissedNotice;
  if (!recoveries.length && !visibleNotice) return null;
  async function recover(item: RoomRecoveryInfo) {
    if (!canConfirmRecovery(item, confirmation, locked || recovering)) return;
    if (await onRecover(item.id)) setConfirmation(null);
  }
  return <section className="room-recovery-panel" aria-label="Восстановление комнат">
    {visibleNotice && <div className="recovery-notice" role="status"><Icon name="check" size={18} /><div><strong>Комната восстановлена</strong><p>{notice}</p></div><button className="icon-button" type="button" aria-label="Закрыть уведомление о восстановлении" onClick={() => setDismissedNotice(notice)}><Icon name="close" size={15} /></button></div>}
    {recoveries.length > 0 && <><div className="recovery-heading"><Icon name="shield" size={17} /><strong>Не удалось открыть историю</strong></div><div className="recovery-list">
      {recoveries.map(item => {
        const signature = recoverySignature(item);
        const available = item.available && item.backup !== null && item.backup.id === item.id;
        return <article className="recovery-item" key={item.id}>
          <div className="recovery-description"><strong>{item.backup?.title || `Комната ${item.id}`}</strong>{item.backup && <span>Копия от {dateLabel(item.backup.updatedAt)} · записей: {item.backup.entryCount}</span>}
            {item.primaryError && <p className="recovery-path-error">Основной файл: {item.primaryError}</p>}
            {item.backupError && <p className="recovery-path-error">Резервная копия: {item.backupError}</p>}
          </div>
          {!available ? <p className="recovery-unavailable">Не удалось восстановить историю комнаты. {item.backupError || (!item.backup ? 'Нет доступной резервной копии.' : item.backup.id !== item.id ? 'Копия относится к другой комнате.' : item.primaryError || 'Проверь путь к файлам комнаты.')}</p>
            : confirmation === signature ? <div className="recovery-confirm"><p>Восстановить предыдущую версию? Приложение сохранит повреждённый файл и несохранённый текст отдельно.</p><div><button className="button recovery-confirm-button" type="button" disabled={locked || recovering} onClick={() => void recover(item)}>{recovering ? 'Восстанавливаю…' : 'Восстановить'}</button><button className="text-button" type="button" disabled={recovering} onClick={() => setConfirmation(null)}>Отмена</button></div></div>
              : <button className="button button-secondary" type="button" disabled={locked || recovering} onClick={() => setConfirmation(signature)}>Восстановить из копии</button>}
        </article>;
      })}
    </div></>}
    {recoveries.length > 0 && locked && <p className="recovery-lock-note">Останови расшифровку и дождись окончания запроса.</p>}
  </section>;
}

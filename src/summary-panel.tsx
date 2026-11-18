import { Icon } from './icons';
import { MarkdownContent } from './markdown-content';

export function SummaryPanel({ text, stale, busy, enabled, blockedReason, contextStatus, onSummarize, onCancel }: {
  text: string; stale: boolean; busy: boolean; enabled: boolean; blockedReason: string; contextStatus: string;
  onSummarize: () => void; onCancel: () => void;
}) {
  return <section className="conversation-panel summary-panel" aria-labelledby="summary-title">
    <div className="panel-heading"><h2 id="summary-title">Итог комнаты</h2>{stale && text && <span className="stale-badge">Есть новые записи</span>}</div>
    <div className="summary-scroll" aria-busy={busy}>
      {text ? <div className="summary-text"><MarkdownContent text={text} /></div> : <div className="empty-summary"><h3>{busy ? 'Подвожу итог…' : 'Итог разговора'}</h3>{!busy && <p>Нажми «Подвести итог», чтобы собрать решения и открытые вопросы.</p>}</div>}
    </div>
    <div className="summary-bottom">
      {busy ? <><p className="field-note" role="status">{contextStatus || 'Модель читает разговор…'}</p><button className="button button-secondary" type="button" onClick={onCancel}><Icon name="stop" size={16} />Отменить подготовку итога</button></>
        : <>{stale && text && <p className="field-note">В комнате есть новые реплики. Обнови итог, чтобы их учесть.</p>}<button className={`button ${text ? 'button-secondary' : 'button-primary'}`} type="button" disabled={!enabled} onClick={onSummarize}>{text ? 'Обновить итог' : 'Подвести итог'}</button>{!enabled && blockedReason && <p className="field-note" role="status">{blockedReason}</p>}</>}
    </div>
  </section>;
}

import { useEffect, useRef, useState } from 'react';
import { Icon } from './icons';
import type { AppState } from './types';
import { MarkdownContent } from './markdown-content';

function duration(value: number | null) {
  return value === null ? '—' : `${new Intl.NumberFormat('ru-RU', { maximumFractionDigits: 2 }).format(value / 1000)} с`;
}

export function AnswerPanel({ answer, busy, enabled, setupRequired, setupBusy, blockedReason, contextStatus, onSetup, onAnswer, onCancel }: {
  answer: AppState['answer']; busy: boolean; enabled: boolean; setupRequired: boolean; setupBusy: boolean;
  blockedReason: string; contextStatus: string; onSetup: () => void; onAnswer: () => void; onCancel: () => void;
}) {
  const [copied, setCopied] = useState(false);
  const [copyError, setCopyError] = useState(false);
  const body = useRef<HTMLDivElement>(null);
  const follow = useRef(true);
  useEffect(() => { setCopied(false); setCopyError(false); if (!answer.text) follow.current = true; }, [answer.model, answer.text === '']);
  useEffect(() => { if (follow.current && body.current) body.current.scrollTop = body.current.scrollHeight; }, [answer.text]);
  useEffect(() => { if (copied) { const timer = setTimeout(() => setCopied(false), 2000); return () => clearTimeout(timer); } }, [copied]);
  async function copy() {
    try { await navigator.clipboard.writeText(answer.text); setCopied(true); setCopyError(false); }
    catch { setCopyError(true); }
  }

  return <section className="answer-panel" aria-labelledby="answer-title">
    <div className="panel-heading answer-heading">
      <h2 id="answer-title">Ответ</h2>
      {answer.text && <button className="icon-button" type="button" onClick={() => void copy()} title={copied ? 'Скопировано' : 'Скопировать ответ'} aria-label={copied ? 'Ответ скопирован' : 'Скопировать ответ'}><Icon name={copied ? 'check' : 'copy'} size={16} /></button>}
    </div>
    <div className="answer-scroll" ref={body} onScroll={() => { const element = body.current; if (element) follow.current = element.scrollHeight - element.scrollTop - element.clientHeight < 72; }} aria-busy={busy}>
      {answer.text ? <div className="answer-text"><MarkdownContent text={answer.text} />{answer.streaming && <span className="stream-cursor" aria-label="Модель отвечает…" />}</div> : <div className="empty-answer">
        <h3>{busy ? 'Готовлю ответ…' : setupRequired ? 'Выбери, кто ответит' : 'Ответ по разговору'}</h3>
        <p>{busy ? contextStatus || 'Можно остановить ответ в любой момент.' : setupRequired ? 'Для ответа подключи сервис и выбери модель. Расшифровка доступна и без входа.' : 'Добавь вопрос в разговор, затем нажми «Получить ответ».'}</p>
      </div>}
    </div>
    <div className="answer-bottom">
      {(answer.firstTokenMs !== null || answer.totalMs !== null) && <details className="answer-timing"><summary>Время ответа</summary><div className="answer-metrics"><div><span>До первых слов</span><strong>{duration(answer.firstTokenMs)}</strong></div><div><span>До конца ответа</span><strong>{duration(answer.totalMs)}</strong></div></div></details>}
      {copyError && <p className="field-note error-text" role="status">Выдели ответ и скопируй его вручную.</p>}
      {busy ? <button className="button answer-action cancel-answer" type="button" onClick={onCancel}><Icon name="stop" size={17} /><span>Остановить ответ</span></button>
        : setupRequired ? <button className="button button-primary answer-action answer-setup" type="button" disabled={setupBusy} onClick={onSetup}>{setupBusy ? 'Получаю список моделей…' : 'Подключить модель'}<Icon name="arrow" size={17} /></button>
        : <button className="button button-primary answer-action" type="button" onClick={onAnswer} disabled={!enabled}><Icon name="spark" size={18} /><span>Получить ответ</span><kbd>Ctrl ↵</kbd></button>}
      {!busy && !enabled && !setupRequired && <p className="answer-footnote" role="status">{blockedReason}</p>}
    </div>
  </section>;
}

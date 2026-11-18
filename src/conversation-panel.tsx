import { useMemo, useState } from 'react';
import { Icon, type IconName } from './icons';
import { historyWindow, MAX_MESSAGE_BYTES, messageBytes, sourceLabel } from './room-helpers';
import type { HistorySource, TranscriptEntry } from './types';
import { MarkdownContent } from './markdown-content';
import { useRoomDraft } from './use-room-draft';
import { useHistoryScroll } from './use-history-scroll';

const sourceIcons: Record<HistorySource, IconName> = { microphone: 'mic', system: 'speaker', user: 'edit', assistant: 'spark' };
const fullTime = new Intl.DateTimeFormat('ru-RU', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });
const shortTime = new Intl.DateTimeFormat('ru-RU', { hour: '2-digit', minute: '2-digit' });
function timeLabel(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? '' : fullTime.format(date);
}

function AssistantReply({ entry }: { entry: TranscriptEntry }) {
  const [open, setOpen] = useState(false);
  const date = new Date(entry.at);
  const time = Number.isNaN(date.getTime()) ? '' : shortTime.format(date);
  return <details className="history-reply" onToggle={event => setOpen(event.currentTarget.open)}>
    <summary><Icon name="chevron" size={15} />Ответ модели{time && <time dateTime={entry.at} title={timeLabel(entry.at)}> · {time}</time>}</summary>
    {open && <MarkdownContent text={entry.text} />}
  </details>;
}

const plural = new Intl.PluralRules('ru');
function newReplies(count: number) {
  const form = plural.select(count);
  return `${count} ${form === 'one' ? 'новая реплика' : form === 'few' ? 'новые реплики' : 'новых реплик'}`;
}

export function ConversationPanel({ roomId, visible = true, focusEntryId = null, focusRequestId = 0, focusPending = false, entries, listening, total, loadingEarlier, adding, addEnabled, blockedReason, contextStatus, onLoadMore, onAddMessage }: {
  roomId: string; visible?: boolean; focusEntryId?: string | null; focusRequestId?: number; focusPending?: boolean;
  entries: TranscriptEntry[]; listening: boolean; total: number;
  loadingEarlier: boolean; adding: boolean; addEnabled: boolean; blockedReason: string; contextStatus: string;
  onLoadMore: (before: number) => Promise<boolean>; onAddMessage: (text: string) => Promise<boolean>;
}) {
  const history = useHistoryScroll({ roomId, entries, total, visible, focusEntryId, focusRequestId, focusPending });
  const { text: draft, persisted, setText: setDraft, clear: clearDraft, clearIfUnchanged } = useRoomDraft(roomId);
  const bytes = useMemo(() => messageBytes(draft), [draft]);
  const oversized = bytes > MAX_MESSAGE_BYTES;
  const page = historyWindow(history.shownTotal, history.shown.length);

  async function addMessage() {
    if (!addEnabled || adding || oversized || !draft.trim()) return;
    const sentText = draft;
    if (await onAddMessage(sentText)) { clearIfUnchanged(sentText); history.followLatest(); }
  }

  return <section className="conversation-panel history-panel" aria-labelledby="conversation-title">
    <div className="panel-heading history-toolbar"><h2 id="conversation-title">История разговора</h2>
      {total > 0 && <span>{page.remaining > 0 ? `Показано ${history.shown.length} из ${total}` : `Записей: ${total}`}</span>}
    </div>
    <div className="history-viewport">
      <div className="transcript-scroll" ref={history.scroll} onScroll={history.trackScroll} role="log" aria-label="История комнаты" aria-live={loadingEarlier ? 'off' : 'polite'} aria-relevant="additions text" tabIndex={0}>
        {page.remaining > 0 && <div className="load-history"><button className="text-button" type="button" disabled={loadingEarlier || !addEnabled} onClick={() => void history.loadEarlier(onLoadMore)}>{loadingEarlier ? 'Загружаю историю…' : `Ещё записи · ${page.nextPage}`}</button></div>}
        {history.shown.length === 0 ? <div className="empty-conversation"><h3>{listening ? 'Слушаю…' : 'Пока нет реплик'}</h3><p>{listening ? 'После паузы здесь появится текст фразы.' : 'Включи расшифровку или напиши первую реплику ниже.'}</p></div>
          : <div className="transcript-list">{history.shown.map(entry => {
            const matched = entry.id === focusEntryId && !focusPending;
            return <article className={`transcript-entry source-${entry.source}${matched ? ' search-match' : ''}`} data-entry-id={entry.id} tabIndex={matched ? -1 : undefined} key={entry.id}>
              {matched && <span className="search-match-badge">Найденная запись</span>}
              {entry.source === 'assistant' ? <AssistantReply entry={entry} /> : <>
                <div className="entry-meta"><span className="entry-source"><Icon name={sourceIcons[entry.source]} size={13} />{sourceLabel(entry.source)}</span><time dateTime={entry.at}>{timeLabel(entry.at)}</time></div>
                <p>{entry.text}</p>
              </>}
            </article>;
          })}</div>}
        {listening && history.shown.length > 0 && history.unread === 0 && <div className="listening-tail"><span className="status-dot online" />Слушаю дальше…</div>}
      </div>
      {history.unread > 0 && <button type="button" className="jump-latest button button-primary" onClick={() => { history.followLatest(); history.scroll.current?.focus({ preventScroll: true }); }}><Icon name="arrow" size={16} />{newReplies(history.unread)}</button>}
    </div>
    <form className="message-composer" onSubmit={event => { event.preventDefault(); void addMessage(); }}>
      {listening && <label className="draft-label" htmlFor="message-draft">Черновик</label>}
      <textarea id="message-draft" value={draft} rows={2} disabled={adding || (!listening && !addEnabled)} onChange={event => setDraft(event.target.value)} placeholder="Напиши вопрос или заметку…" aria-label="Вопрос или заметка" aria-describedby="message-note" onKeyDown={event => { if (event.ctrlKey && event.key === 'Enter') { event.preventDefault(); event.stopPropagation(); void addMessage(); } }} />
      {(draft || !persisted) && <div className="draft-state"><span role="status">{persisted ? 'Черновик останется здесь, пока открыто окно' : 'Черновик может потеряться при закрытии окна'}</span><button className="text-button" type="button" disabled={adding || !draft} onClick={clearDraft}>Очистить черновик</button></div>}
      <div className="composer-footer"><span id="message-note" className={oversized ? 'error-text' : ''}>{oversized ? 'Текст больше 1 МБ. Раздели его на несколько реплик.' : listening ? 'Останови расшифровку, чтобы сохранить реплику.' : !addEnabled && blockedReason ? blockedReason : 'Ctrl+Enter сохраняет текст в комнате'}</span><button className="button button-secondary" type="submit" disabled={!addEnabled || adding || oversized || !draft.trim()}>{adding ? 'Сохраняю реплику…' : 'Сохранить реплику'}</button></div>
    </form>
    <div className="context-footer"><span>{contextStatus || 'Реплики сохраняются на этом компьютере'}</span></div>
  </section>;
}

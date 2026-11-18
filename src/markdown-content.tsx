import { memo, useEffect, useRef, useState, type ReactNode } from 'react';
import ReactMarkdown, { type Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { Icon } from './icons';
import { safeHttpUrl } from './markdown-security';

function CodeBlock({ children }: { children?: ReactNode }) {
  const code = useRef<HTMLPreElement>(null);
  const [copyState, setCopyState] = useState<'idle' | 'copied' | 'error'>('idle');
  useEffect(() => { if (copyState === 'copied') { const timer = setTimeout(() => setCopyState('idle'), 2000); return () => clearTimeout(timer); } }, [copyState]);
  async function copy() {
    try { await navigator.clipboard.writeText(code.current?.textContent ?? ''); setCopyState('copied'); }
    catch { setCopyState('error'); }
  }
  return <div className="md-code-block"><div className="md-code-toolbar"><span>Код</span><button className="text-button" type="button" aria-label="Копировать код" onClick={() => void copy()}><Icon name={copyState === 'copied' ? 'check' : 'copy'} size={13} />{copyState === 'copied' ? 'Скопировано' : 'Копировать'}</button></div><pre ref={code}>{children}</pre>{copyState === 'error' && <span className="md-copy-error" role="status">Выдели код и скопируй его вручную.</span>}</div>;
}

const components: Components = {
  a: ({ href, children }) => {
    const safe = safeHttpUrl(href ?? '');
    return safe ? <a href={safe} target="_blank" rel="noopener noreferrer" referrerPolicy="no-referrer" title={safe}>{children}</a> : <span className="md-disabled-link">{children}</span>;
  },
  img: ({ alt }) => <span className="md-image-placeholder" title="Изображения не загружаются автоматически">[Изображение: {alt || 'без описания'}]</span>,
  pre: CodeBlock,
  table: ({ children }) => <div className="md-table-scroll" role="region" aria-label="Таблица" tabIndex={0}><table>{children}</table></div>,
  h1: ({ children }) => <h3 className="md-major-heading">{children}</h3>,
  h2: ({ children }) => <h3>{children}</h3>,
};

export const MarkdownContent = memo(function MarkdownContent({ text }: { text: string }) {
  return <div className="markdown-content"><ReactMarkdown remarkPlugins={[remarkGfm]} skipHtml components={components} urlTransform={(url, key) => key === 'href' ? safeHttpUrl(url) : undefined}>{text}</ReactMarkdown></div>;
});

function PreviewBlock({ children }: { children?: ReactNode }) { return <>{children}{' '}</>; }
function PreviewInline({ children }: { children?: ReactNode }) { return <>{children}</>; }
const previewComponents: Components = {
  p: PreviewBlock, h1: PreviewBlock, h2: PreviewBlock, h3: PreviewBlock, h4: PreviewBlock, h5: PreviewBlock, h6: PreviewBlock,
  ul: PreviewBlock, ol: PreviewBlock, li: PreviewBlock, blockquote: PreviewBlock, pre: PreviewBlock,
  table: PreviewBlock, thead: PreviewBlock, tbody: PreviewBlock, tr: PreviewBlock, th: PreviewBlock, td: PreviewBlock,
  section: PreviewBlock, strong: PreviewInline, em: PreviewInline, del: PreviewInline, code: PreviewInline,
  a: PreviewInline, sup: PreviewInline, img: () => null, input: () => null, hr: () => null, br: () => <> </>,
};
export const MarkdownPreview = memo(function MarkdownPreview({ text }: { text: string }) {
  return <span className="room-preview"><ReactMarkdown remarkPlugins={[remarkGfm]} skipHtml components={previewComponents} urlTransform={() => undefined}>{text}</ReactMarkdown></span>;
});

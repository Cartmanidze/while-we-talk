import test, { after } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { createServer } from 'vite';
import react from '@vitejs/plugin-react';
import { safeHttpUrl } from './markdown-security.ts';

const server = await createServer({ configFile: false, cacheDir: 'node_modules/.vite-test-markdown', plugins: [react()], server: { middlewareMode: true, hmr: false, watch: null }, appType: 'custom', logLevel: 'silent' });
after(() => server.close());
const { MarkdownContent, MarkdownPreview } = await server.ssrLoadModule('/src/markdown-content.tsx');
const render = (text, component = MarkdownContent) => renderToStaticMarkup(createElement(component, { text }));

test('only explicit http(s) links without embedded credentials are clickable', () => {
  assert.equal(safeHttpUrl('https://example.com/docs?q=one'), 'https://example.com/docs?q=one');
  assert.equal(safeHttpUrl('http://example.com/'), 'http://example.com/');
  for (const link of ['javascript:alert(1)', 'data:text/html,hello', 'file:///C:/secret', '//example.com', '/local', '#fragment', 'mailto:a@example.com', 'https://user:password@example.com/']) assert.equal(safeHttpUrl(link), undefined, link);
});
test('headings, strong text, code and GFM tables are rendered as elements', () => {
  const html = render('### Итог\n\n**Вывод** и `код`.\n\n| Имя | Число |\n| --- | --- |\n| Один | 1 |');
  assert.match(html, /<h[1-6][^>]*>Итог<\/h[1-6]>/);
  assert.match(html, /<strong>Вывод<\/strong>/);
  assert.match(html, /<code>код<\/code>/);
  assert.match(html, /<table>/);
  assert.match(html, /md-table-scroll/);
});
test('raw HTML and images cannot introduce executable markup or remote loads', () => {
  const html = render('<script>alert(1)</script>\n\n<img src="https://tracker.example/pixel" onerror="alert(1)">\n\n![Схема](https://tracker.example/image.png)\n\n[опасно](javascript:alert(1))');
  assert.doesNotMatch(html, /<script|<img|<iframe|onerror=|src=/i);
  assert.doesNotMatch(html, /href="javascript:/i);
  assert.match(html, /Схема/);
});
test('literal HTML in fenced code stays escaped and has a copy control', () => {
  const html = render('```html\n<img src=x onerror=alert(1)>\n```');
  assert.match(html, /&lt;img/);
  assert.doesNotMatch(html, /<img/);
  assert.match(html, /Копировать код/);
});
test('safe links use a separate browsing context without opener or referrer', () => {
  const html = render('[Документация](https://example.com/docs)');
  assert.match(html, /href="https:\/\/example.com\/docs"/);
  assert.match(html, /target="_blank"/);
  assert.match(html, /rel="noopener noreferrer"/);
});
test('room preview contains plain readable text instead of markdown markers', () => {
  const html = render('### Итог\n\n**Процесс** и [поток](https://example.com).\n\n- `Код`', MarkdownPreview);
  assert.match(html, /Итог/);
  assert.match(html, /Процесс/);
  assert.doesNotMatch(html, /###|\*\*|href=|<img|<h[1-6]|<ul|<pre/);
});

export function safeHttpUrl(value: string): string | undefined {
  if (/[\u0000-\u001f\u007f]/.test(value)) return undefined;
  try {
    const url = new URL(value);
    if ((url.protocol === 'https:' || url.protocol === 'http:') && !url.username && !url.password) return url.href;
  } catch { /* Relative URLs and non-URL text remain plain text. */ }
  return undefined;
}

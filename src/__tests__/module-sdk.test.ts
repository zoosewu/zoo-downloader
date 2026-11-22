/**
 * The site module SDK types (src-tauri/crates/zoo-module/sdk) as module authors get them:
 * the template copied with the SDK into types/, checked by TypeScript.
 * The editor must accept the template and flag APIs the module host does not have.
 */
import { cpSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { afterAll, describe, expect, test } from 'vitest';
import { typeErrors } from './tsc';

const crate = resolve(import.meta.dirname, '../../src-tauri/crates/zoo-module');
const dir = mkdtempSync(join(tmpdir(), 'zoo-module-template-'));
cpSync(join(crate, 'template'), dir, { recursive: true });
cpSync(join(crate, 'sdk'), join(dir, 'types'), { recursive: true });
afterAll(() => rmSync(dir, { recursive: true, force: true }));

function typecheck(extra?: string): string[] {
  if (extra !== undefined) writeFileSync(join(dir, 'src', 'extra.ts'), extra);
  else rmSync(join(dir, 'src', 'extra.ts'), { force: true });
  return typeErrors(dir);
}

describe('module SDK types', () => {
  test('the template type-checks cleanly', () => {
    expect(typecheck()).toEqual([]);
  });

  test('every supported API type-checks', () => {
    const errors = typecheck(`
      import { defineModule, type Zoo } from 'zoo-module';
      export async function all(zoo: Zoo) {
        const r = await fetch(new URL('https://a/'), { method: 'POST', body: new URLSearchParams({ a: '1' }), redirect: 'manual', signal: AbortSignal.timeout(5) });
        const fd = new FormData(); fd.append('a', 'b');
        const h = new Headers({ a: 'b' }); h.getSetCookie();
        const bytes: Uint8Array = await r.bytes();
        new TextDecoder().decode(new TextEncoder().encode(await r.text()));
        crypto.getRandomValues(new Uint8Array(4)); crypto.randomUUID();
        await crypto.subtle.digest('SHA-256', bytes);
        const doc = new DOMParser().parseFromString('<p>', 'text/html');
        doc.querySelectorAll('a').length; doc.getElementById('x')?.closest('div')?.dataset.id;
        setTimeout(() => {}, 1); queueMicrotask(() => {}); structuredClone({}); atob(btoa('x'));
        console.log(performance.now());
        await zoo.wait(1, 'countdown');
        zoo.credentials.has('k') && zoo.credentials.placeholder('k');
        await zoo.captcha.recaptcha({ siteKey: 'k', pageUrl: 'https://a/' });
        await zoo.captcha.image('https://a/c.png');
        zoo.log.info('x'); throw zoo.fail('m', { hint: 'h' });
      }
      export const m = defineModule({ resolve: (url) => ({ url, size: 1, headers: { a: 'b' } }) });
    `);
    expect(errors).toEqual([]);
  });

  test.each([
    ['WebSocket', "new WebSocket('wss://a/');"],
    ['document', 'document.title;'],
    ['window', 'window.location;'],
    ['localStorage', "localStorage.getItem('a');"],
    ['XMLHttpRequest', 'new XMLHttpRequest();'],
    ['Blob', 'new Blob([]);'],
    ['require', "require('fs');"],
    ['process', 'process.env;'],
    ['Deno', 'Deno.exit();'],
  ])('%s is reported as missing', (name, code) => {
    const errors = typecheck(`export {};\n${code}`);
    expect(errors.some((e) => e.includes(`'${name}'`))).toBe(true);
  });

  test('wrong module shapes are reported', () => {
    const errors = typecheck(`
      import { defineModule } from 'zoo-module';
      export const a = defineModule({ resolve: () => ({ url: 1 }) });
      export const b = defineModule({ resolve: () => ({ url: 'u', fileName: 'x' }) });
      export const c = defineModule({});
    `);
    expect(errors).toHaveLength(3);
    // Typos name the field and list the valid ones.
    expect(errors[1]).toContain("'fileName' is not a Resolved field (url, filename, headers, size, maxConnections, maxSegmentSize, decrypt)");
  });

  test('list type-checks, and a file without a name is reported', () => {
    expect(
      typecheck(`
        import { defineModule } from 'zoo-module';
        export const m = defineModule({
          resolve: (url) => ({ url }),
          async list(url) { return [{ url: url + '#1', filename: 'a', size: 1, folder: 'x/y' }, { url: url + '#2', filename: 'b' }]; },
        });
      `),
    ).toEqual([]);
    // A file without a name. (Unknown fields are rejected by the host when the module runs.)
    const errors = typecheck(`
      import { defineModule } from 'zoo-module';
      export const b = defineModule({ resolve: (url) => ({ url }), list: (url) => [{ url }] });
    `);
    expect(errors).toHaveLength(1);
    expect(errors[0]).toContain("'filename'");
  });
});

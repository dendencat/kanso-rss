import { chromium } from '@playwright/test';
import { createServer } from 'node:http';
import { readFile, mkdir } from 'node:fs/promises';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const token = '0123456789abcdef0123456789abcdef';
const feeds = [
  { id: 'rust', title: 'Rust Blog', folder: 'テクノロジー', url: 'https://blog.rust-lang.org/feed.xml', unread: 2 },
  { id: 'design', title: 'Design Notes', folder: 'デザイン', url: 'https://example.org/design.xml', unread: 1 },
  { id: 'science', title: 'Science & Nature', folder: '世界を知る', url: 'https://example.org/science.xml', unread: 2 },
];
const titles = [
  '小さく始めて、長く育てる。Rustでつくる道具のかたち',
  '静かなインターフェースが、思考の余白をつくる',
  '街の小さな緑から、自然との新しい関係を考える',
  '所有するデータと、使い続けられるソフトウェア',
  '遠くを知るために、身近な世界を観察する',
  '<img src=x onerror="window.pwned=true">危険な記事もテキストとして表示',
];
const articles = titles.map((title, i) => ({ id: String(i + 1), feed_id: feeds[i % 3].id, feed_title: feeds[i % 3].title, title,
  url: i === 5 ? 'javascript:window.pwned=true' : `https://example.org/articles/${i}`,
  content: i === 5 ? '<script>window.pwned=true</script><img src="https://tracker.invalid/track"><p>安全な本文</p>' : '<p>情報は、私たちの世界の見方を少しずつ変えていきます。毎日の小さな発見を、落ち着いて読むための場所。</p><p>手に馴染む道具には、必要なものだけが残っています。使い続けられるソフトウェアについて、今日は少し立ち止まって考えてみましょう。</p>',
  published: new Date(Date.now() - i * 3600000 * 12).toISOString(), read: i === 3, starred: i === 1,
}));
function stats() { return { feeds: feeds.length, articles: articles.length, unread: articles.filter(a => !a.read).length, starred: articles.filter(a => a.starred).length }; }
let progress = { running: false, processed: 3, total: 3, added: 0, errors: 0 };
const server = createServer(async (req, res) => {
  try {
    const u = new URL(req.url, 'http://127.0.0.1');
    const json = (value, status = 200) => { res.writeHead(status, { 'content-type': 'application/json' }); res.end(JSON.stringify(value)); };
    if (!u.pathname.startsWith('/api/v1/')) {
      const file = u.pathname === '/' ? 'index.html' : u.pathname.slice(1);
      if (!['index.html', 'app.js', 'style.css', 'icon.svg', 'manifest.webmanifest', 'sw.js', 'licenses.html', 'third-party.html'].includes(file)) { res.writeHead(404); res.end(); return; }
      res.writeHead(200, { 'content-type': file.endsWith('.js') ? 'text/javascript' : file.endsWith('.css') ? 'text/css' : file.endsWith('.svg') ? 'image/svg+xml' : 'text/html',
        'content-security-policy': "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; worker-src 'self'; manifest-src 'self'" });
      res.end(await readFile(`${root}/web/${file}`)); return;
    }
    if (req.headers.authorization !== `Bearer ${token}`) { json({ error: 'Unauthorized' }, 401); return; }
    const chunks = []; for await (const chunk of req) chunks.push(chunk);
    const raw = Buffer.concat(chunks).toString();
    const body = req.headers['content-type'] === 'application/json' && raw ? JSON.parse(raw) : {};
    const path = u.pathname.slice('/api/v1/'.length);
    if (path === 'stats') { json(stats()); return; }
    if (path === 'feeds' && req.method === 'GET') { for (const f of feeds) f.unread = articles.filter(a => a.feed_id === f.id && !a.read).length; json(feeds); return; }
    if (path === 'feeds' && req.method === 'POST') { const f = { id: `new-${feeds.length}`, ...body, unread: 0 }; feeds.push(f); json(f, 201); return; }
    if (path.startsWith('feeds/') && req.method === 'DELETE') { const i = feeds.findIndex(f => f.id === path.split('/')[1]); feeds.splice(i, 1); res.writeHead(204); res.end(); return; }
    if (path === 'articles') {
      let rows = articles.filter(a => (!u.searchParams.has('feed_id') || a.feed_id === u.searchParams.get('feed_id')) && (!u.searchParams.has('unread') || !a.read) && (!u.searchParams.has('starred') || a.starred) && (!u.searchParams.has('q') || a.title.includes(u.searchParams.get('q'))));
      if (u.searchParams.has('folder')) rows = rows.filter(a => feeds.some(f => f.id === a.feed_id && f.folder === u.searchParams.get('folder')));
      json(rows.slice(Number(u.searchParams.get('offset') || 0), Number(u.searchParams.get('offset') || 0) + 50)); return;
    }
    if (path.startsWith('articles/')) { const a = articles.find(a => a.id === path.split('/')[1]); if (req.method === 'PATCH') Object.assign(a, body); json(a); return; }
    if (path === 'mark-read') { for (const a of articles) if ((!body.feed_id || a.feed_id === body.feed_id) && (!body.folder || feeds.some(f => f.id === a.feed_id && f.folder === body.folder))) a.read = true; json({ changed: articles.length }); return; }
    if (path === 'refresh') { if (req.method === 'POST') { progress = { ...progress, running: false }; json({ started: true }, 202); } else json(progress); return; }
    if (path === 'opml') { if (req.method === 'POST') json({ imported: 1 }); else { res.writeHead(200, { 'content-type': 'application/xml' }); res.end('<opml version="2.0"><body/></opml>'); } return; }
    json({ error: 'Not found' }, 404);
  } catch (error) { res.writeHead(500); res.end(String(error)); }
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1440, height: 1000 }, colorScheme: 'light' });
const errors = []; const trackers = [];
page.on('pageerror', error => errors.push(error.message));
page.on('request', request => { if (request.url().includes('tracker.invalid')) trackers.push(request.url()); });
try {
  await page.goto(`http://127.0.0.1:${server.address().port}`);
  await page.locator('#login-token').fill('wrongwrongwrongwrongwrongwrongwrong');
  await page.locator('#login-form button[type=submit]').click();
  await page.locator('#login-error').filter({ hasText: 'APIトークン' }).waitFor();
  await page.locator('#login-token').fill(token);
  await page.locator('#login-form button[type=submit]').click();
  await page.locator('.article-row').first().waitFor();
  assert.equal(await page.locator('.article-row').count(), 6);
  const [licensePage] = await Promise.all([page.waitForEvent('popup'), page.locator('.license-link').click()]);
  await licensePage.locator('a[href="third-party.html"]').click();
  await licensePage.locator('details').first().waitFor();
  assert.ok(await licensePage.locator('details').count() > 500);
  await licensePage.close();
  await mkdir(`${root}/artifacts`, { recursive: true });
  await page.screenshot({ path: `${root}/artifacts/desktop-library.png`, fullPage: true });
  await page.locator('.article-row').first().click();
  await page.locator('#detail-title').filter({ hasText: '小さく始めて' }).waitFor();
  assert.equal(await page.locator('#detail-body').textContent(), '情報は、私たちの世界の見方を少しずつ変えていきます。毎日の小さな発見を、落ち着いて読むための場所。\n\n手に馴染む道具には、必要なものだけが残っています。使い続けられるソフトウェアについて、今日は少し立ち止まって考えてみましょう。');
  await page.locator('#article-star').click();
  await page.locator('#article-star[aria-pressed=true]').waitFor();
  await page.screenshot({ path: `${root}/artifacts/desktop-reading.png`, fullPage: true });
  await page.locator('[data-view=starred]').click();
  await page.waitForFunction(() => document.querySelectorAll('.article-row').length === 2);
  await page.locator('[data-view=all]').click();
  await page.locator('#search').fill('危険');
  await page.waitForFunction(() => document.querySelectorAll('.article-row').length === 1);
  await page.locator('.article-row').click();
  await page.locator('#detail-body').filter({ hasText: '安全な本文' }).waitFor();
  assert.equal(await page.evaluate(() => window.pwned), undefined);
  assert.equal(await page.locator('#detail-body img, #detail-body script').count(), 0);
  assert.equal(await page.locator('#article-open').isDisabled(), true);
  assert.deepEqual(trackers, []);
  await page.locator('[data-view=all]').click();
  await page.locator('#add-feed').click();
  await page.locator('#feed-url').fill('https://example.org/new.xml');
  await page.locator('#feed-title').fill('New Feed');
  await page.locator('#feed-folder').fill('テスト');
  await page.locator('#feed-form button[type=submit]').click();
  await page.locator('#breadcrumb-title').filter({ hasText: 'New Feed' }).waitFor();
  await page.locator('#settings-button').click();
  await page.locator('#export-button').click();
  await page.locator('#export-dialog').waitFor({ state: 'visible' });
  assert.match(await page.locator('#export-content').inputValue(), /<opml/);
  await page.locator('[data-close=export-dialog]').click();
  await page.locator('[data-close=settings-dialog]').click();
  await page.locator('[data-view=all]').click();
  await page.locator('#mark-read').click();
  await page.locator('#confirm-ok').click();
  await page.waitForFunction(() => document.getElementById('count-unread').textContent === '0');
  await page.setViewportSize({ width: 390, height: 844 });
  await page.locator('.article-row').first().click();
  await page.locator('#article-detail').waitFor({ state: 'visible' });
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true);
  await page.screenshot({ path: `${root}/artifacts/mobile-reading.png`, fullPage: true });
  await page.locator('#reader-back').click();
  await page.locator('#menu-button').click();
  await page.locator('#settings-button').click();
  await page.locator('#logout-button').click();
  await page.locator('#login-dialog').waitFor({ state: 'visible' });
  assert.equal(await page.locator('.article-row').count(), 0);
  assert.deepEqual(await page.evaluate(() => Object.keys(localStorage)), []);
  assert.deepEqual(errors, []);
  console.log('PASS: login, article reading, read/star state, search, feed add, export, mark read, mobile layout, logout, CSP/XSS/tracking protections.');
} finally { await browser.close(); await new Promise(resolve => server.close(resolve)); }

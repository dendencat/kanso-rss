'use strict';

const $ = id => document.getElementById(id);
const native = Boolean(window.__TAURI__?.core?.invoke);
const invoke = (command, args) => window.__TAURI__.core.invoke(command, args);
const state = { token: '', feeds: [], articles: [], stats: {}, view: 'all', feed: null, folder: null, search: '', unreadOnly: false, selected: null, selectionSerial: 0, offset: 0, hasMore: false, generation: 0, session: 0, editing: null, remote: false, refreshing: false };
let toastTimer;
let searchTimer;
let refreshTimer;
let confirmAction;
const paths = {
  inbox: 'M3 4h18v16H3z M3 13h5l2 3h4l2-3h5',
  circle: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18',
  star: 'm12 3 2.8 5.7 6.2.9-4.5 4.4 1.1 6.2-5.6-3-5.6 3 1.1-6.2L3 9.6l6.2-.9z',
  settings: 'm10 3-.5 3-2 1-2.8-1-2 3 2.2 2v2l-2.2 2 2 3 2.8-1 2 1 .5 3h4l.5-3 2-1 2.8 1 2-3-2.2-2v-2l2.2-2-2-3-2.8 1-2-1-.5-3z M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6',
  import: 'M12 3v12 m-4-4 4 4 4-4 M4 15v6h16v-6',
  refresh: 'M20 8a8 8 0 0 0-14-2L3 9 M3 4v5h5 M4 16a8 8 0 0 0 14 2l3-3 M16 15h5v5',
  search: 'M10.5 3a7.5 7.5 0 1 0 0 15 7.5 7.5 0 0 0 0-15 M16 16l5 5',
  'check-all': 'm3 12 4 4 8-9 m-1 9 7-9',
  external: 'M14 3h7v7 M21 3l-11 11 M10 3H3v18h18v-7',
  close: 'm6 6 12 12 M18 6 6 18',
  folder: 'M3 5h7l2 3h9v12H3z',
  moon: 'M20 15a8.5 8.5 0 0 1-11-11 9 9 0 1 0 11 11',
  menu: 'M4 6h16 M4 12h16 M4 18h16',
  back: 'm14 5-7 7 7 7',
  check: 'm4 12 5 5L20 6',
};

function icon(name) {
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('viewBox', '0 0 24 24');
  svg.setAttribute('fill', 'none');
  svg.setAttribute('stroke', 'currentColor');
  svg.setAttribute('stroke-width', '1.5');
  svg.setAttribute('stroke-linecap', 'round');
  svg.setAttribute('stroke-linejoin', 'round');
  svg.setAttribute('aria-hidden', 'true');
  const path = document.createElementNS(svg.namespaceURI, 'path');
  path.setAttribute('d', paths[name] || paths.circle);
  svg.append(path);
  return svg;
}
document.querySelectorAll('[data-icon]').forEach(el => el.append(icon(el.dataset.icon)));
if (native) document.querySelectorAll('a[href="licenses.html"]').forEach(link => {
  link.removeAttribute('target');
});

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

// Convert feed markup to text without parsing it into a DOM. This never creates
// images, links, scripts, frames or other active/tracking content.
function plainText(value = '') {
  const entities = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'", nbsp: ' ' };
  return String(value).replace(/<(script|style)\b[^>]*>[\s\S]*?<\/\1\s*>/gi, '')
    .replace(/<\s*(br|\/p|\/div|\/li|\/h[1-6])\b[^>]*>/gi, '\n\n')
    .replace(/<[^>]*>/g, '')
    .replace(/&(#x[0-9a-f]+|#\d+|amp|lt|gt|quot|apos|nbsp);/gi, (match, name) => {
      if (!name.startsWith('#')) return entities[name.toLowerCase()] ?? match;
      const code = name[1].toLowerCase() === 'x' ? parseInt(name.slice(2), 16) : parseInt(name.slice(1), 10);
      return code > 0 && code <= 0x10ffff && !(code >= 0xd800 && code <= 0xdfff) ? String.fromCodePoint(code) : '';
    }).replace(/\n[\t ]+/g, '\n').replace(/\n{3,}/g, '\n\n').trim();
}

function safeLink(value) {
  try { const u = new URL(value); return ['https:', 'http:'].includes(u.protocol) && !u.username && !u.password ? u.href : null; }
  catch { return null; }
}

function toast(message) {
  $('toast').textContent = message;
  $('toast').hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { $('toast').hidden = true; }, 4500);
}

async function api(path, { method = 'GET', body, text = false } = {}) {
  const payload = body === undefined ? undefined : typeof body === 'string' ? body : JSON.stringify(body);
  let status, result;
  if (native) {
    const response = await invoke('api_request', { method, path: `/api/v1/${path}`, body: payload ?? null });
    status = response.status; result = response.text;
  } else {
    const response = await fetch(`/api/v1/${path}`, {
      method, headers: { Authorization: `Bearer ${state.token}`, ...(payload === undefined ? {} : { 'Content-Type': typeof body === 'string' ? 'application/xml' : 'application/json' }) },
      body: payload, cache: 'no-store', redirect: 'error', signal: AbortSignal.timeout(40000),
    });
    status = response.status; result = await response.text();
  }
  if (status < 200 || status >= 300) {
    const error = new Error(status === 401 ? 'APIトークンを確認してください。' : status === 409 ? '更新処理が進行中です。' : status === 502 ? 'フィードを取得できませんでした。公開されたRSS/AtomのURLを確認してください。' : `処理できませんでした (${status})。入力と上限を確認してください。`);
    error.status = status;
    throw error;
  }
  return text ? result : result ? JSON.parse(result) : null;
}

function handleError(error) {
  toast(error.message || '接続に失敗しました。');
  if (error.status === 401 && !native) {
    state.token = ''; state.session++; state.generation++;
    resetLibrary(); showLogin();
  }
}

function resetLibrary() {
  state.selectionSerial++;
  state.feeds = []; state.articles = []; state.stats = {}; state.selected = null;
  state.view = 'all'; state.feed = null; state.folder = null;
  state.search = ''; state.offset = 0; state.unreadOnly = false;
  $('search').value = ''; $('unread-toggle').setAttribute('aria-pressed', 'false');
  renderNavigation(); renderArticles(); renderReader(); renderHeading();
}

async function loadLibrary() {
  const session = state.session;
  const [feeds, stats] = await Promise.all([api('feeds'), api('stats')]);
  if (session !== state.session) return;
  state.feeds = feeds; state.stats = stats;
  renderNavigation(); renderHeading();
  await loadArticles();
  if (session === state.session) $('sync-label').textContent = state.remote ? 'サーバーに接続中' : native ? 'この端末に保存' : '接続済み';
}

function viewName() {
  if (state.feed) return state.feeds.find(f => f.id === state.feed)?.title || 'フィード';
  if (state.folder !== null) return state.folder || '未分類';
  return { all: 'すべての記事', unread: '未読の記事', starred: 'スター付き' }[state.view];
}

function renderHeading() {
  $('view-title').replaceChildren(document.createTextNode(viewName()), el('span', 'heading-dot', '.'));
  $('breadcrumb-title').textContent = viewName();
  const messages = { all: '気になる世界を、ひとつの場所に。', unread: 'まだ出会っていない、新しい視点。', starred: 'また読みたい記事を、大切に。' };
  $('view-subtitle').textContent = state.feed || state.folder !== null ? 'お気に入りの情報を、あなたのペースで。' : messages[state.view];
  $('date-label').textContent = new Intl.DateTimeFormat('en-US', { weekday: 'long', month: 'long', day: 'numeric' }).format(new Date()).toUpperCase();
  $('footer-count').textContent = `${state.feeds.length} フィードを購読中`;
  $('mode-label').replaceChildren(el('span', 'status-dot'), document.createTextNode(state.remote ? 'CONNECTED LIBRARY' : native ? 'LOCAL LIBRARY' : 'PRIVATE READER'));
}

function renderNavigation() {
  $('count-all').textContent = state.stats.articles || 0;
  $('count-unread').textContent = state.stats.unread || 0;
  $('count-starred').textContent = state.stats.starred || 0;
  document.querySelectorAll('[data-view]').forEach(button => {
    const selected = !state.feed && state.folder === null && state.view === button.dataset.view;
    button.classList.toggle('active', selected); button.setAttribute('aria-current', selected ? 'page' : 'false');
  });
  const navigation = $('feed-navigation'); navigation.replaceChildren();
  $('folder-suggestions').replaceChildren();
  const folders = [...new Set(state.feeds.map(f => f.folder))];
  if (!state.feeds.length) navigation.append(el('p', 'no-feeds', 'お気に入りのフィードを追加して、ライブラリを育てましょう。'));
  for (const folder of folders) {
    const heading = el('button', 'folder-heading', ''); heading.append(icon('folder'), document.createTextNode(folder || '未分類'));
    heading.addEventListener('click', () => setView('all', null, folder)); navigation.append(heading);
    if (folder) { const option = el('option'); option.value = folder; $('folder-suggestions').append(option); }
    for (const feed of state.feeds.filter(f => f.folder === folder)) {
      const button = el('button', `nav-item feed-item${state.feed === feed.id ? ' active' : ''}`);
      button.append(el('span', 'feed-avatar', Array.from(plainText(feed.title))[0] || 'R'), el('span', 'feed-name', plainText(feed.title)), el('span', `count${feed.last_error ? ' feed-error' : ''}`, feed.last_error ? '!' : feed.unread || ''));
      button.title = feed.last_error || feed.url;
      button.addEventListener('click', () => setView('all', feed.id)); navigation.append(button);
    }
  }
}

function articleParams(offset = 0) {
  const p = new URLSearchParams({ limit: '50', offset: String(offset) });
  if (state.feed) p.set('feed_id', state.feed);
  if (state.folder !== null) p.set('folder', state.folder);
  if (state.view === 'unread' || state.unreadOnly) p.set('unread', 'true');
  if (state.view === 'starred') p.set('starred', 'true');
  if (state.search) p.set('q', state.search);
  return p;
}

async function loadArticles(more = false) {
  const generation = ++state.generation;
  const offset = more ? state.articles.length : 0;
  const rows = await api(`articles?${articleParams(offset)}`);
  if (generation !== state.generation) return;
  state.articles = more ? [...state.articles, ...rows] : rows;
  state.offset = offset; state.hasMore = rows.length === 50;
  if (state.selected) state.selected = state.articles.find(a => a.id === state.selected.id) || null;
  renderArticles(); renderReader();
}

function relativeDate(value) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return '';
  const days = Math.max(0, Math.floor((Date.now() - date.getTime()) / 86400000));
  return days === 0 ? '今日' : days === 1 ? '昨日' : date.toLocaleDateString('ja-JP', { month: 'numeric', day: 'numeric' });
}

function renderArticles() {
  const list = $('article-list'); list.replaceChildren();
  $('list-caption').textContent = state.search ? `「${state.search}」の検索結果` : `${state.articles.length}${state.hasMore ? '+' : ''} 件の記事 · 新しい順`;
  $('load-more').hidden = !state.hasMore;
  if (!state.articles.length) {
    const empty = el('div', 'empty-list');
    empty.append(el('strong', '', state.feeds.length ? 'ひと息つける時間。' : 'あなたの読書空間を、ここから。'), el('p', '', state.feeds.length ? 'このビューには記事がありません。フィードを更新するか、別のビューを選んでください。' : 'ブログ、ニュース、気になるメディア。最初のフィードを追加してみましょう。'));
    if (!state.feeds.length) { const button = el('button', 'secondary-button', '＋ 最初のフィードを追加'); button.addEventListener('click', () => showFeed()); empty.append(button); }
    list.append(empty);
  }
  for (const article of state.articles) {
    const button = el('button', `article-row${article.read ? ' read' : ''}${state.selected?.id === article.id ? ' selected' : ''}`);
    button.setAttribute('role', 'listitem'); button.setAttribute('aria-label', `${article.read ? '既読' : '未読'}: ${plainText(article.title)}`);
    const source = el('div', 'row-source');
    source.append(el('span', 'feed-avatar', Array.from(plainText(article.feed_title))[0] || 'R'), el('span', 'row-source-name', plainText(article.feed_title)), el('time', '', relativeDate(article.published)));
    button.append(source, el('h3', 'row-title', plainText(article.title)), el('p', 'row-summary', plainText(article.content).replace(/\s+/g, ' ').slice(0, 160)));
    const bottom = el('div', 'row-bottom');
    if (!article.read) bottom.append(el('span', 'unread-dot'));
    bottom.append(el('span', '', article.read ? '既読' : '未読'));
    if (article.starred) bottom.append(el('span', 'row-star', '★'));
    button.append(bottom); button.addEventListener('click', () => selectArticle(article).catch(handleError)); list.append(button);
  }
}

function renderReader() {
  const a = state.selected;
  $('reader-empty').hidden = Boolean(a); $('article-detail').hidden = !a;
  for (const id of ['article-read', 'article-star', 'article-open']) $(id).disabled = !a;
  if (!a) { $('reader-position').textContent = 'READ AT YOUR OWN PACE'; $('app').classList.remove('reading'); return; }
  $('reader-position').textContent = `${state.articles.findIndex(row => row.id === a.id) + 1} / ${state.articles.length}`;
  $('detail-source').textContent = plainText(a.feed_title); $('detail-source-mark').textContent = Array.from(plainText(a.feed_title))[0] || 'R';
  const date = new Date(a.published);
  $('detail-date').textContent = Number.isNaN(date.getTime()) ? '' : date.toLocaleDateString('ja-JP', { year: 'numeric', month: 'long', day: 'numeric' });
  $('detail-title').textContent = plainText(a.title);
  const text = plainText(a.content);
  $('detail-body').textContent = text || '本文はフィードに含まれていません。元の記事を開いてお読みください。';
  $('detail-reading-time').textContent = `約 ${Math.max(1, Math.ceil(text.length / 600))} 分で読めます · ${a.read ? '既読' : '未読'}`;
  $('article-star').classList.toggle('starred', a.starred);
  $('article-star').setAttribute('aria-label', a.starred ? 'スターを外す' : 'スターを付ける'); $('article-star').title = $('article-star').getAttribute('aria-label');
  $('article-star').setAttribute('aria-pressed', String(a.starred));
  $('article-read').title = a.read ? '未読にする' : '既読にする'; $('article-read').setAttribute('aria-label', $('article-read').title);
  $('article-open').disabled = !safeLink(a.url); $('detail-original').hidden = !safeLink(a.url);
}

async function selectArticle(article) {
  const session = state.session;
  const selection = ++state.selectionSerial;
  const generation = state.generation;
  const detail = await api(`articles/${encodeURIComponent(article.id)}`);
  if (generation !== state.generation || selection !== state.selectionSerial || session !== state.session) return;
  Object.assign(article, detail);
  state.selected = article; $('app').classList.add('reading');
  renderArticles(); renderReader(); $('reader').scrollTop = 0;
  if (!article.read) {
    await api(`articles/${encodeURIComponent(article.id)}`, { method: 'PATCH', body: { read: true } });
    if (session !== state.session) return;
    article.read = true;
    state.stats.unread = Math.max(0, (state.stats.unread || 0) - 1);
    const f = state.feeds.find(f => f.id === article.feed_id); if (f) f.unread = Math.max(0, f.unread - 1);
    renderNavigation(); renderArticles(); renderReader();
  }
}

async function patchSelected(kind) {
  const session = state.session;
  const a = state.selected; if (!a) return;
  const value = !a[kind];
  await api(`articles/${encodeURIComponent(a.id)}`, { method: 'PATCH', body: { [kind]: value } });
  if (session !== state.session) return;
  a[kind] = value;
  const [stats, feeds] = await Promise.all([api('stats'), api('feeds')]);
  if (session !== state.session) return;
  state.stats = stats; state.feeds = feeds;
  renderNavigation(); renderArticles(); renderReader();
}

async function setView(view, feed = null, folder = null) {
  state.selectionSerial++;
  state.view = view; state.feed = feed; state.folder = folder; state.selected = null;
  state.search = ''; $('search').value = ''; clearTimeout(searchTimer);
  $('app').classList.remove('menu-open', 'reading');
  renderHeading(); renderNavigation(); renderReader();
  try { await loadArticles(); } catch (error) { handleError(error); }
}

function showLogin() { if (!$('login-dialog').open) $('login-dialog').showModal(); }
$('login-dialog').addEventListener('cancel', event => event.preventDefault());
$('login-form').addEventListener('submit', async event => {
  event.preventDefault(); const button = event.submitter; button.disabled = true; $('login-error').textContent = '';
  state.token = $('login-token').value.trim(); state.session++;
  try { await loadLibrary(); $('login-token').value = ''; $('login-dialog').close(); }
  catch (error) { state.token = ''; $('login-error').textContent = error.message || 'サーバーに接続できません。'; }
  finally { button.disabled = false; }
});

function showFeed(feed = null) {
  state.editing = feed?.id || null; $('feed-form').reset();
  $('feed-url').value = feed?.url || ''; $('feed-url').disabled = Boolean(feed);
  $('feed-title').value = feed?.title || ''; $('feed-folder').value = feed?.folder || '';
  $('feed-error').textContent = ''; $('feed-dialog').querySelector('h2').textContent = feed ? '購読フィードを編集。' : '新しいフィードを購読。';
  $('feed-form').querySelector('[type=submit]').textContent = feed ? '変更を保存' : '購読を追加 ＋';
  $('feed-dialog').showModal();
}
$('add-feed').addEventListener('click', () => showFeed());
$('feed-form').addEventListener('submit', async event => {
  event.preventDefault(); const button = event.submitter; button.disabled = true; $('feed-error').textContent = '';
  try {
    const body = { title: $('feed-title').value.trim(), folder: $('feed-folder').value.trim(), url: $('feed-url').value.trim() };
    if (state.editing) { await api(`feeds/${encodeURIComponent(state.editing)}`, { method: 'PATCH', body }); }
    else { const feed = await api('feeds', { method: 'POST', body }); state.feed = feed.id; state.folder = null; state.view = 'all'; }
    $('feed-dialog').close(); await loadLibrary(); toast(state.editing ? '変更を保存しました。' : '購読を追加しました。記事を取得しています。');
    if (!state.editing) { await refreshFeeds(); }
    if ($('settings-dialog').open) renderSettings();
  } catch (error) { $('feed-error').textContent = error.message; toast(error.message); }
  finally { button.disabled = false; }
});
document.querySelectorAll('[data-close]').forEach(button => button.addEventListener('click', () => $(button.dataset.close).close()));
document.querySelectorAll('[data-view]').forEach(button => button.addEventListener('click', () => setView(button.dataset.view)));
$('menu-button').addEventListener('click', () => $('app').classList.toggle('menu-open'));
document.querySelector('.workspace').addEventListener('click', event => { if (!event.target.closest('#menu-button')) $('app').classList.remove('menu-open'); });
$('reader-back').addEventListener('click', () => $('app').classList.remove('reading'));
$('article-star').addEventListener('click', () => patchSelected('starred').catch(handleError));
$('article-read').addEventListener('click', () => patchSelected('read').catch(handleError));
function openOriginal() {
  const url = safeLink(state.selected?.url); if (!url) return;
  if (native) invoke('open_article', { url }).catch(handleError);
  else window.open(url, '_blank', 'noopener,noreferrer');
}
$('article-open').addEventListener('click', openOriginal); $('detail-original').addEventListener('click', openOriginal);
$('search').addEventListener('input', () => { clearTimeout(searchTimer); searchTimer = setTimeout(() => { state.search = $('search').value.trim(); state.selected = null; loadArticles().catch(handleError); }, 300); });
$('unread-toggle').addEventListener('click', () => { state.unreadOnly = !state.unreadOnly; $('unread-toggle').setAttribute('aria-pressed', String(state.unreadOnly)); loadArticles().catch(handleError); });
$('load-more').addEventListener('click', async () => { $('load-more').disabled = true; try { await loadArticles(true); } catch (error) { handleError(error); } finally { $('load-more').disabled = false; } });

function confirm(title, description, action) {
  $('confirm-title').textContent = title; $('confirm-description').textContent = description; confirmAction = action; $('confirm-dialog').showModal();
}
$('confirm-cancel').addEventListener('click', () => { confirmAction = null; $('confirm-dialog').close(); });
$('confirm-ok').addEventListener('click', async () => { const action = confirmAction; confirmAction = null; $('confirm-dialog').close(); try { await action?.(); } catch (error) { handleError(error); } });
$('mark-read').addEventListener('click', () => confirm('すべて既読にしますか？', `${viewName()}の全記事を既読にします。検索とスターの絞り込みは対象に含まれません。`, async () => {
  await api('mark-read', { method: 'POST', body: { feed_id: state.feed, folder: state.folder } }); await loadLibrary(); toast('既読にしました。');
}));

async function refreshFeeds() {
  if (state.refreshing) return;
  state.refreshing = true; $('refresh-button').classList.add('spinning'); $('refresh-button').disabled = true;
  try { await api('refresh', { method: 'POST' }); }
  catch (error) { if (error.status !== 409) { stopRefresh(); throw error; } }
  await pollRefresh();
}
function stopRefresh() { state.refreshing = false; clearTimeout(refreshTimer); $('refresh-button').classList.remove('spinning'); $('refresh-button').disabled = false; }
async function pollRefresh() {
  const session = state.session;
  try {
    const progress = await api('refresh');
    if (session !== state.session) return;
    $('sync-label').textContent = `更新中 ${progress.processed} / ${progress.total}`;
    if (progress.running) { refreshTimer = setTimeout(pollRefresh, 1200); return; }
    stopRefresh(); await loadLibrary();
    toast(progress.errors ? `${progress.added} 件を追加。${progress.errors} フィードで取得エラーがありました。` : `${progress.added} 件の新しい記事を取得しました。`);
  } catch (error) { if (session === state.session) { stopRefresh(); handleError(error); } }
}
$('refresh-button').addEventListener('click', () => refreshFeeds().catch(handleError));

function renderSettings() {
  $('remote-form').hidden = !native;
  $('connection-description').textContent = native ? state.remote ? '共有サーバーのライブラリを表示しています。' : 'ライブラリはこの端末に保存されています。' : 'このサーバーのライブラリを表示しています。';
  $('logout-button').textContent = native ? 'この端末のライブラリに戻る' : 'ログアウト';
  $('logout-button').disabled = native && !state.remote;
  const list = $('manage-feeds'); list.replaceChildren();
  if (!state.feeds.length) list.append(el('p', 'field-note', '購読フィードはまだありません。'));
  for (const feed of state.feeds) {
    const row = el('div', 'manage-feed');
    const edit = el('button', '', '編集'); edit.addEventListener('click', () => showFeed(feed));
    const remove = el('button', '', '削除'); remove.addEventListener('click', () => confirm('購読を削除しますか？', `「${feed.title}」と保存済みの記事・スターを削除します。`, async () => {
      await api(`feeds/${encodeURIComponent(feed.id)}`, { method: 'DELETE' }); if (state.feed === feed.id) { state.feed = null; state.selected = null; }
      await loadLibrary(); renderSettings(); toast('購読を削除しました。');
    }));
    row.append(el('span', 'feed-avatar', Array.from(plainText(feed.title))[0] || 'R'), el('span', 'manage-feed-name', plainText(feed.title)), edit, remove); list.append(row);
  }
}
$('settings-button').addEventListener('click', () => { renderSettings(); $('settings-dialog').showModal(); });
$('remote-form').addEventListener('submit', async event => {
  event.preventDefault(); event.submitter.disabled = true; $('remote-error').textContent = '';
  try {
    await invoke('connect_remote', { endpoint: $('remote-url').value.trim(), token: $('remote-token').value.trim() });
    state.remote = true; state.session++; state.generation++; resetLibrary();
    await loadLibrary(); $('remote-token').value = ''; $('settings-dialog').close(); toast('サーバーに接続しました。');
  } catch (error) {
    await invoke('disconnect_remote').catch(() => {}); state.remote = false; state.session++; resetLibrary(); await loadLibrary().catch(handleError);
    $('remote-error').textContent = error.message || String(error);
  } finally { event.submitter.disabled = false; }
});
$('logout-button').addEventListener('click', async () => {
  stopRefresh(); state.token = ''; state.remote = false; state.session++; state.generation++;
  resetLibrary(); $('settings-dialog').close();
  if (native) { await invoke('disconnect_remote'); await loadLibrary().catch(handleError); }
  else showLogin();
});

function chooseImport() { $('opml-file').value = ''; $('opml-file').click(); }
$('import-button').addEventListener('click', chooseImport); $('settings-import').addEventListener('click', chooseImport);
$('opml-file').addEventListener('change', async () => {
  const file = $('opml-file').files[0]; if (!file) return;
  if (file.size > 1024 * 1024) { toast('OPMLは1 MiB以下にしてください。'); return; }
  try { const result = await api('opml', { method: 'POST', body: await file.text() }); await loadLibrary(); if ($('settings-dialog').open) renderSettings(); toast(`${result.imported} フィードをインポートしました。`); }
  catch (error) { handleError(error); }
});
$('export-button').addEventListener('click', async () => {
  try { $('export-content').value = await api('opml', { text: true }); $('export-download').hidden = native; $('export-dialog').showModal(); }
  catch (error) { handleError(error); }
});
$('export-copy').addEventListener('click', async () => {
  try { await navigator.clipboard.writeText($('export-content').value); toast('OPMLをコピーしました。'); }
  catch { $('export-content').focus(); $('export-content').select(); toast('テキストを選択しました。コピーして保存できます。'); }
});
$('export-download').addEventListener('click', () => {
  const url = URL.createObjectURL(new Blob([$('export-content').value], { type: 'application/xml' }));
  const a = el('a'); a.href = url; a.download = 'kanso.opml'; a.click(); setTimeout(() => URL.revokeObjectURL(url), 1000);
});

let theme = 'light';
try { theme = localStorage.getItem('kanso-theme') || (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'); } catch {}
document.documentElement.dataset.theme = theme;
$('theme-button').addEventListener('click', () => { theme = theme === 'dark' ? 'light' : 'dark'; document.documentElement.dataset.theme = theme; try { localStorage.setItem('kanso-theme', theme); } catch {} });
document.addEventListener('keydown', event => {
  if (event.ctrlKey || event.metaKey || event.altKey || document.querySelector('dialog[open]') || ['INPUT', 'TEXTAREA'].includes(event.target.tagName)) return;
  if (event.key === '/') { event.preventDefault(); $('search').focus(); return; }
  if (event.key.toLowerCase() === 's') { event.preventDefault(); patchSelected('starred').catch(handleError); return; }
  if (['j', 'k'].includes(event.key.toLowerCase())) {
    event.preventDefault(); const index = state.articles.findIndex(a => a.id === state.selected?.id);
    const next = index < 0 ? 0 : Math.max(0, Math.min(state.articles.length - 1, index + (event.key.toLowerCase() === 'j' ? 1 : -1)));
    if (state.articles[next]) selectArticle(state.articles[next]).catch(handleError);
  }
});
window.addEventListener('offline', () => { $('sync-label').textContent = 'オフライン'; toast('ネットワークに接続されていません。'); });
window.addEventListener('online', () => { if (native || state.token) loadLibrary().catch(handleError); });

renderHeading(); renderNavigation(); renderArticles();
if (native) loadLibrary().catch(handleError); else showLogin();
if (!native && 'serviceWorker' in navigator && window.isSecureContext) navigator.serviceWorker.register('/sw.js').catch(() => {});

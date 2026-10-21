// Outlooker variant-1 prototype logic. Static mock, no network.
const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const esc = s => String(s).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const ic = (id, cls = 's16') => `<svg class="ic ${cls}"><use href="#i-${id}"/></svg>`;
const fmt = n => n.toLocaleString('en-US');

const S = {
  lang: 'zh', data: 'full', screen: 'search',
  q: '', facetPst: null, facetFolder: null, quickAtt: false, quick90: false, selMail: 1048213,
  dsl: { open: true, join: 'AND', groups: [
    { join: 'AND', conds: [{ f: 'date', v: '2024-07-01', v2: '2024-09-30' }, { f: 'att', v: 'yes' }] },
    { join: 'OR', conds: [{ f: 'body', v: '合約' }, { f: 'body', v: 'invoice' }] },
  ] },
  savedId: 's1', ran: true, selRow: 0, sqlMail: null,
  psts: PSTS.slice(), queue: QUEUE.map(q => ({ ...q })), selQ: 'q2',
  ob: 0, obDir: 'default', obDb: 0,
};

function t(k, vars) {
  let s = (I18N[S.lang][k] ?? I18N.zh[k] ?? k);
  if (vars) for (const [a, b] of Object.entries(vars)) s = s.replace('{' + a + '}', b);
  return s;
}
const empty = (icon, title, body, action = '') =>
  `<div class="empty"><div class="art">${ic(icon, '')}</div><h3>${esc(title)}</h3><p>${esc(body)}</p>${action}</div>`;
const AV = ['#0f6cbd', '#5c2e91', '#00666d', '#a4262c', '#3b6a0e', '#8a3707', '#4f52b2', '#7a1f6e'];
function avatar(name, lg) {
  const h = [...name].reduce((a, c) => a + c.codePointAt(0), 0);
  const init = /^[\x00-\x7f]/.test(name) ? name.split(/\s+/).map(w => w[0]).join('').slice(0, 2).toUpperCase() : [...name].slice(-2).join('');
  return `<span class="avatar ${lg ? 'lg' : ''}" style="background:${AV[h % AV.length]}" aria-hidden="true">${esc(init)}</span>`;
}
const strip = h => h.replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim();
function hl(text, terms) {
  let out = esc(text);
  for (const term of terms.filter(Boolean)) {
    const re = new RegExp(esc(term).replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'gi');
    out = out.replace(re, m => `<mark>${m}</mark>`);
  }
  return out;
}
const mails = () => (S.data === 'empty' ? [] : MAILS);

/* ---------------- i18n + shell ---------------- */
function applyI18n() {
  document.documentElement.lang = S.lang === 'zh' ? 'zh-Hant-TW' : 'en';
  $$('[data-i18n]').forEach(el => { el.textContent = t(el.dataset.i18n); });
  $$('[data-i18n-ph]').forEach(el => { el.placeholder = t(el.dataset.i18nPh); });
  $$('[data-i18n-aria]').forEach(el => { el.setAttribute('aria-label', t(el.dataset.i18nAria)); });
  $$('#langSeg button').forEach(b => b.setAttribute('aria-pressed', b.dataset.lang === S.lang));
  $$('input[name=lang]').forEach(r => { r.checked = r.value === S.lang; });
}
function renderAll() { applyI18n(); renderCrumb(); renderSearch(); renderSql(); renderImport(); renderSettings(); renderOnboarding(); }
function renderCrumb() { $('#crumb').textContent = '· ' + t('nav.' + S.screen); }
function go(screen) {
  S.screen = screen;
  $$('.screen').forEach(s => s.classList.toggle('active', s.id === 'screen-' + screen));
  $$('#rail button').forEach(b => b.dataset.screen === screen ? b.setAttribute('aria-current', 'page') : b.removeAttribute('aria-current'));
  renderCrumb();
}
$('#rail').addEventListener('click', e => { const b = e.target.closest('button'); if (b) go(b.dataset.screen); });
$('#rail').addEventListener('keydown', e => {
  const bs = $$('#rail button'); const i = bs.indexOf(document.activeElement); if (i < 0) return;
  const j = { ArrowDown: i + 1, ArrowUp: i - 1, Home: 0, End: bs.length - 1 }[e.key];
  if (j !== undefined) { e.preventDefault(); bs[(j + bs.length) % bs.length].focus(); }
});
$('#langSeg').addEventListener('click', e => { const b = e.target.closest('button'); if (b) { S.lang = b.dataset.lang; renderAll(); } });
$('#dataSeg').addEventListener('click', e => {
  const b = e.target.closest('button'); if (!b) return;
  S.data = b.dataset.data;
  $$('#dataSeg button').forEach(x => x.setAttribute('aria-pressed', x === b));
  S.psts = S.data === 'empty' ? [] : PSTS.slice();
  S.queue = S.data === 'empty' ? [] : QUEUE.map(q => ({ ...q }));
  renderAll();
});
function toast(msg) { $('#toastMsg').textContent = msg; $('#toast').classList.add('show'); clearTimeout(toast.h); toast.h = setTimeout(() => $('#toast').classList.remove('show'), 2400); }

/* ---------------- 1. Search ---------------- */
const FIELDS = ['sender', 'recipient', 'date', 'folder', 'att', 'pst', 'body'];
const OPS = { sender: 'contains', recipient: 'contains', body: 'contains', date: 'between', folder: 'is', att: 'is', pst: 'is' };
function condOk(m, c) {
  const v = (c.v || '').toLowerCase();
  switch (c.f) {
    case 'sender': return !v || (m.from + ' ' + m.email).toLowerCase().includes(v);
    case 'recipient': return !v || (m.to + ' ' + m.cc).toLowerCase().includes(v);
    case 'body': return !v || strip(m.body).toLowerCase().includes(v);
    case 'date': { const d = m.date.slice(0, 10); return (!c.v || d >= c.v) && (!c.v2 || d <= c.v2); }
    case 'folder': return !c.v || m.folder === c.v || m.folder.startsWith(c.v + '/');
    case 'att': return !c.v || (m.atts.length > 0) === (c.v === 'yes');
    case 'pst': return !c.v || m.pst === c.v || m.also.includes(c.v);
  }
  return true;
}
function dslOk(m) {
  const gs = S.dsl.groups.filter(g => g.conds.length);
  if (!gs.length) return true;
  const gOk = g => g.join === 'AND' ? g.conds.every(c => condOk(m, c)) : g.conds.some(c => condOk(m, c));
  return S.dsl.join === 'AND' ? gs.every(gOk) : gs.some(gOk);
}
function results() {
  const q = S.q.trim().toLowerCase();
  return mails().filter(m =>
    (!q || [m.from, m.email, m.to, m.cc, m.subject, strip(m.body)].join(' ').toLowerCase().includes(q)) &&
    (!S.facetPst || m.pst === S.facetPst || m.also.includes(S.facetPst)) &&
    (!S.facetFolder || m.folder === S.facetFolder || m.folder.startsWith(S.facetFolder + '/')) &&
    (!S.quickAtt || m.atts.length) && (!S.quick90 || m.date >= '2024-06-15') && dslOk(m));
}
function renderFacets() {
  const all = mails();
  const cnt = f => all.filter(f).length;
  const item = (label, sel, attrs, n, l2, icon) => `<button type="button" class="tree-item ${l2 ? 'l2' : ''}" aria-selected="${sel}" ${attrs}>${ic(icon)}<span>${esc(label)}</span><span class="n">${n}</span></button>`;
  $('#facets').innerHTML = `
    <div class="facet-title">${t('s.sources')}</div>
    ${item(t('s.allMail'), !S.facetPst, 'data-pst=""', fmt(all.length), false, 'pst')}
    ${S.psts.map(p => item(p.name, S.facetPst === p.name, `data-pst="${esc(p.name)}"`, cnt(m => m.pst === p.name || m.also.includes(p.name)), true, 'pst')).join('')}
    <div class="facet-title">${t('s.folders')}</div>
    ${all.length ? FOLDERS.map(f => item(f.split('/').pop(), S.facetFolder === f, `data-folder="${esc(f)}"`, cnt(m => m.folder === f || m.folder.startsWith(f + '/')), f.includes('/'), 'folder')).join('') : ''}
    <div class="facet-title">${t('s.quick')}</div>
    <label class="check"><input type="checkbox" id="qAtt" ${S.quickAtt ? 'checked' : ''}>${ic('clip')}<span>${t('s.hasAtt')}</span></label>
    <label class="check"><input type="checkbox" id="q90" ${S.quick90 ? 'checked' : ''}><span>${t('s.last90')}</span></label>`;
}
$('#facets').addEventListener('click', e => {
  const b = e.target.closest('.tree-item'); if (!b) return;
  if ('pst' in b.dataset) S.facetPst = b.dataset.pst || null;
  if ('folder' in b.dataset) S.facetFolder = S.facetFolder === b.dataset.folder ? null : b.dataset.folder;
  renderSearch();
});
$('#facets').addEventListener('change', e => { if (e.target.id === 'qAtt') S.quickAtt = e.target.checked; if (e.target.id === 'q90') S.quick90 = e.target.checked; renderSearch(); });

function condRow(c, gi, ci) {
  const sel = (opts, val, k) => `<select class="input" data-g="${gi}" data-c="${ci}" data-k="${k}">${opts.map(([v, l]) => `<option value="${esc(v)}" ${v === val ? 'selected' : ''}>${esc(l)}</option>`).join('')}</select>`;
  const inp = (val, k, type = 'text') => `<input class="input" type="${type}" data-g="${gi}" data-c="${ci}" data-k="${k}" value="${esc(val || '')}" aria-label="${t('s.f.' + c.f)}">`;
  let vals;
  if (c.f === 'date') vals = inp(c.v, 'v', 'date') + inp(c.v2, 'v2', 'date');
  else if (c.f === 'att') vals = sel([['yes', t('s.yes')], ['no', t('s.no')]], c.v, 'v');
  else if (c.f === 'folder') vals = sel([['', t('s.any2')], ...FOLDERS.map(f => [f, f])], c.v, 'v');
  else if (c.f === 'pst') vals = sel([['', t('s.any2')], ...PSTS.map(p => [p.name, p.name])], c.v, 'v');
  else vals = inp(c.v, 'v');
  return `<div class="cond">${sel(FIELDS.map(f => [f, t('s.f.' + f)]), c.f, 'f')}<span class="op">${t('s.op.' + OPS[c.f])}</span><div class="vals">${vals}</div>
    <button type="button" class="btn small subtle" data-del="${gi}:${ci}" aria-label="Remove condition">${ic('x')}</button></div>`;
}
function joinToggle(val, attr) {
  return ['AND', 'OR'].map(j => `<button type="button" aria-pressed="${val === j}" ${attr}="${j}">${j === 'AND' ? t('s.all') : t('s.any')}</button>`).join('');
}
function renderDsl() {
  const d = S.dsl;
  $('#dsl').hidden = !d.open;
  $('#dslToggle').setAttribute('aria-expanded', d.open);
  const nc = d.groups.reduce((a, g) => a + g.conds.length, 0);
  $('#dslCount').textContent = nc;
  $('#dslSum').textContent = nc ? t('s.condCount', { g: d.groups.length, c: nc }) : '';
  $('#topJoin').innerHTML = joinToggle(d.join, 'data-top');
  $('#dslGroups').innerHTML = d.groups.map((g, gi) => `
    ${gi ? `<span class="badge brand dsl-join">${d.join}</span>` : ''}
    <div class="dsl-group" role="group" aria-label="${t('s.group')} ${gi + 1}">
      <div class="ghead">${t('s.group')} ${gi + 1}<span class="muted">· ${t('s.match')}</span><div class="toggle" role="group">${joinToggle(g.join, `data-gj="${gi}" data-j`)}</div>
        <span style="flex:1"></span><button type="button" class="btn small subtle" data-delg="${gi}" aria-label="Remove group">${ic('trash')}</button></div>
      ${g.conds.map((c, ci) => (ci ? `<span class="conj">${g.join}</span>` : '') + condRow(c, gi, ci)).join('')}
      <div><button type="button" class="btn small subtle" data-addc="${gi}">${ic('plus')}${t('s.addCond')}</button></div>
    </div>`).join('');
}
$('#dslToggle').addEventListener('click', () => { S.dsl.open = !S.dsl.open; renderDsl(); });
$('#addGroup').addEventListener('click', () => { S.dsl.groups.push({ join: 'AND', conds: [{ f: 'sender', v: '' }] }); renderSearch(); });
$('#clearDsl').addEventListener('click', () => { S.dsl.groups = []; renderSearch(); });
$('#dsl').addEventListener('click', e => {
  const b = e.target.closest('button'); if (!b) return;
  const d = b.dataset;
  if (d.top) S.dsl.join = d.top;
  else if (d.j) S.dsl.groups[+d.gj].join = d.j;
  else if (d.addc) S.dsl.groups[+d.addc].conds.push({ f: 'sender', v: '' });
  else if (d.delg) S.dsl.groups.splice(+d.delg, 1);
  else if (d.del) { const [g, c] = d.del.split(':').map(Number); S.dsl.groups[g].conds.splice(c, 1); }
  else return;
  renderSearch();
});
$('#dsl').addEventListener('change', e => {
  const el = e.target; if (!el.dataset.k) return;
  const c = S.dsl.groups[+el.dataset.g].conds[+el.dataset.c];
  if (el.dataset.k === 'f') { c.f = el.value; c.v = c.f === 'att' ? 'yes' : ''; c.v2 = ''; } else c[el.dataset.k] = el.value;
  renderSearch();
});
$('#dsl').addEventListener('input', e => {
  const el = e.target; if (el.tagName !== 'INPUT' || el.dataset.k !== 'v' || el.type === 'date') return;
  S.dsl.groups[+el.dataset.g].conds[+el.dataset.c].v = el.value; renderResults();
});
$('#q').addEventListener('input', e => { S.q = e.target.value; renderResults(); });

function mailItem(m, sel, terms, compact) {
  return `<div class="mail-item ${compact ? 'compact' : ''}" role="option" tabindex="-1" aria-selected="${sel}" data-id="${m.id}">
    ${compact ? '' : avatar(m.from)}
    <span class="from">${hl(m.from, terms)}</span><span class="date">${m.date.slice(0, 10)}</span>
    <span class="subj">${m.atts.length ? ic('clip') : ''}<span style="overflow:hidden;text-overflow:ellipsis">${hl(m.subject, terms)}</span></span>
    <span class="snip">${hl(strip(m.body).slice(0, 140), terms)}</span></div>`;
}
function renderPreview(el, m, terms) {
  if (!m) { el.innerHTML = empty('search', t('s.pick'), t('s.pickP')); return; }
  const ext = n => n.split('.').pop().toUpperCase().slice(0, 4);
  const extColor = { PDF: '#a4262c', XLSX: '#0e700e', XLS: '#0e700e', DOCX: '#0f6cbd', DOC: '#0f6cbd', PPTX: '#8a3707' };
  el.innerHTML = `<div class="preview-tools">
      <span class="badge">${ic('folder')}${esc(m.folder)}</span><span class="spacer" style="flex:1"></span>
      <button type="button" class="btn small subtle">${ic('copy')}${t('m.copy')}</button></div>
    <div class="scroll"><article class="read">
      <h2 class="subject">${hl(m.subject, terms)}</h2>
      <div class="meta">${avatar(m.from, true)}
        <div><span class="who">${hl(m.from, terms)}</span> <span class="addr">&lt;${hl(m.email, terms)}&gt;</span></div>
        <div class="to">${t('m.to')}: ${hl(m.to, terms)}${m.cc ? `<br>${t('m.cc')}: ${hl(m.cc, terms)}` : ''}</div>
        <div class="when">${m.date}</div></div>
      <div class="sources">${t('m.foundIn')}: ${[m.pst, ...m.also].map(p => `<span class="badge brand">${ic('pst')}${esc(p)}</span>`).join('')}</div>
      ${m.atts.length ? `<div class="field"><span class="caption muted">${t('m.atts')}</span><div class="atts">${m.atts.map(a => `
        <div class="att"><span class="ext" style="background:${extColor[ext(a.n)] || '#616161'}">${ext(a.n)}</span><div><div class="nm">${esc(a.n)}</div><div class="caption muted">${a.s} · ${esc(a.m.split('/')[1].slice(0, 24))}</div></div>
        <button type="button" class="btn small subtle" disabled title="${t('m.dlLater')}" aria-label="${t('m.dlLater')}">${ic('down')}</button></div>`).join('')}</div></div>` : ''}
      <div class="mail-body">${terms.length ? m.body.replace(/>([^<]+)</g, (_, txt) => '>' + hl(txt, terms).replace(/&amp;/g, '&') + '<') : m.body}
        ${m.inline ? `<div class="inline-img">${ic('image')} ${t('m.inline')} · cid:image001.png → 38 KB</div>` : ''}</div>
    </article></div>`;
}
function renderResults() {
  const list = $('#resList'), terms = [S.q.trim(), ...S.dsl.groups.flatMap(g => g.conds.filter(c => c.f === 'body' || c.f === 'sender' || c.f === 'recipient').map(c => c.v))].filter(Boolean);
  const short = S.q.trim().length > 0 && S.q.trim().length < 3;
  $('#hint').innerHTML = short ? `<div class="msgbar warn">${ic('warn')}<span>${t('s.short')}</span></div>` : '';
  if (!mails().length) {
    $('#resCount').textContent = '';
    list.innerHTML = empty('import', t('s.emptyT'), t('s.emptyP'), `<button type="button" class="btn primary" data-goto="import">${t('s.goImport')}</button>`);
    $('#searchPreview').innerHTML = '';
    return;
  }
  const rs = results();
  $('#resCount').textContent = t('s.count', { n: fmt(rs.length) });
  if (!rs.length) {
    list.innerHTML = empty('search', t('s.zeroT'), t('s.zeroP'), `<button type="button" class="btn" id="clearAllSearch">${t('s.clearAll')}</button>`);
    renderPreview($('#searchPreview'), null, []);
    return;
  }
  if (!rs.some(m => m.id === S.selMail)) S.selMail = rs[0].id;
  let lastMonth = '';
  list.innerHTML = rs.map(m => {
    const mo = m.date.slice(0, 7); const head = mo !== lastMonth ? `<div class="list-group">${mo}</div>` : ''; lastMonth = mo;
    return head + mailItem(m, m.id === S.selMail, terms);
  }).join('');
  renderPreview($('#searchPreview'), rs.find(m => m.id === S.selMail), terms);
}
function renderSearch() { renderFacets(); renderDsl(); renderResults(); }
function listNav(listEl, onPick) {
  listEl.addEventListener('click', e => { const it = e.target.closest('[data-id]'); if (it) onPick(+it.dataset.id, it); });
  listEl.addEventListener('keydown', e => {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    const its = $$('[data-id]', listEl); if (!its.length) return;
    const i = its.findIndex(x => x.getAttribute('aria-selected') === 'true');
    const n = its[Math.max(0, Math.min(its.length - 1, i + (e.key === 'ArrowDown' ? 1 : -1)))];
    e.preventDefault(); onPick(+n.dataset.id, n);
    const again = $(`[data-id="${n.dataset.id}"]`, listEl); again && again.scrollIntoView({ block: 'nearest' });
  });
}
listNav($('#resList'), id => { S.selMail = id; renderResults(); $('#resList').focus(); });
document.addEventListener('click', e => {
  if (e.target.closest('#clearAllSearch')) { S.q = ''; $('#q').value = ''; S.dsl.groups = []; S.facetPst = S.facetFolder = null; S.quickAtt = S.quick90 = false; renderSearch(); }
  const g = e.target.closest('[data-goto]'); if (g) go(g.dataset.goto);
});

/* ---------------- 2. Advanced SQL ---------------- */
const KW = /^(SELECT|FROM|WHERE|AND|OR|NOT|IN|IS|NULL|AS|ON|JOIN|LEFT|INNER|GROUP|BY|ORDER|HAVING|LIMIT|OFFSET|DESC|ASC|DISTINCT|FILTER|INTERVAL|ILIKE|LIKE|TRUE|FALSE|WITH|CASE|WHEN|THEN|ELSE|END|UNION)$/i;
function highlightSql(src) {
  const re = /(--[^\n]*)|('(?:[^']|'')*'?)|(\b\d+(?:\.\d+)?\b)|([A-Za-z_][A-Za-z0-9_]*)(?=\s*\()|([A-Za-z_][A-Za-z0-9_]*)/g;
  let out = '', last = 0, m;
  while ((m = re.exec(src))) {
    out += esc(src.slice(last, m.index)); last = re.lastIndex;
    const [tok, com, str, num, fn, word] = m;
    if (com) out += `<span class="tk-com">${esc(com)}</span>`;
    else if (str) out += `<span class="tk-str">${esc(str)}</span>`;
    else if (num) out += `<span class="tk-num">${num}</span>`;
    else if (fn) out += KW.test(fn) ? `<span class="tk-kw">${fn}</span>` : `<span class="tk-fn">${fn}</span>`;
    else out += KW.test(word) ? `<span class="tk-kw">${word}</span>` : esc(tok);
  }
  return out + esc(src.slice(last)) + '\n';
}
function syncEditor() {
  const ta = $('#sqlText'), lines = ta.value.split('\n').length;
  $('#hl').innerHTML = highlightSql(ta.value);
  const cur = ta.value.slice(0, ta.selectionStart).split('\n').length;
  $('#gutter').innerHTML = Array.from({ length: Math.max(lines, 1) }, (_, i) => `<span class="${i + 1 === cur ? 'cur' : ''}">${i + 1}</span>`).join('\n');
  $('#activeLine').style.top = (8 + (cur - 1) * 20) + 'px';
}
$('#sqlText').addEventListener('input', syncEditor);
['keyup', 'click'].forEach(ev => $('#sqlText').addEventListener(ev, syncEditor));
$('#sqlText').addEventListener('keydown', e => {
  if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); runSql(); }
});
const cur = () => SAVED.find(s => s.id === S.savedId);
function renderSaved() {
  $('#savedList').innerHTML = SAVED.map(s => `<button type="button" role="option" class="saved-item" aria-selected="${s.id === S.savedId}" data-sid="${s.id}">
    <span class="t">${esc(S.lang === 'zh' ? s.title : s.titleEn)}</span>
    <span class="d">${ic(s.type === 'mail' ? 'search' : 'format')}${s.type === 'mail' ? t('q.typeMail') : t('q.typeTable')}</span></button>`).join('');
  $('#schema').innerHTML = Object.entries(SCHEMA).map(([tb, cols], i) => `<details ${i === 0 ? 'open' : ''}><summary>${tb}</summary><ul>${cols.map(c => { const [n, ...ty] = c.split(' '); return `<li>${n} <span>${ty.join(' ')}</span></li>`; }).join('')}</ul></details>`).join('');
}
$('#savedList').addEventListener('click', e => {
  const b = e.target.closest('[data-sid]'); if (!b) return;
  S.savedId = b.dataset.sid; S.selRow = 0; S.sqlMail = null; $('#sqlText').value = cur().sql; S.ran = true; renderSql(); runSql(true);
});
$('[aria-label="New query"]').addEventListener('click', () => { S.savedId = null; S.ran = false; $('#sqlText').value = 'SELECT m.id, m.subject\nFROM messages m\nWHERE '; renderSql(); $('#sqlText').focus(); });
$('#runSql').addEventListener('click', () => runSql());
function runSql(silent) {
  if (!S.savedId) S.savedId = /\bid\b/i.test($('#sqlText').value) ? 's1' : 's3';
  S.ran = true; renderSql(); if (!silent) toast(t('c.toastRun'));
}
function renderSql() {
  renderSaved();
  if (!$('#sqlText').value && cur()) $('#sqlText').value = cur().sql;
  syncEditor();
  const c3 = $('#sqlCol3'), c4 = $('#sqlCol4'), s = cur();
  if (!S.ran || !s) {
    $('#sqlStatus').innerHTML = `<span>${t('q.readonly')}</span>`;
    c3.innerHTML = `<div class="pane-head"><h3>${t('q.mailList')}</h3></div>` + empty('sql', t('q.noRunT'), t('q.noRunP'));
    c4.innerHTML = empty('search', t('s.pick'), t('s.pickP'));
    return;
  }
  const isEmpty = S.data === 'empty';
  const rows = isEmpty ? 0 : (s.type === 'mail' ? s.ids.length : s.rows.length);
  $('#sqlStatus').innerHTML = `<span class="ok">${ic('check')}${t('q.ok')}</span><span>${t('q.rows', { n: rows })}</span><span>${t('q.ms', { n: s.ms })}</span><span class="spacer" style="flex:1"></span><span>${s.type === 'mail' ? t('q.mailMode') : t('q.tableMode')}</span>`;
  if (s.type === 'mail') {
    const ms = isEmpty ? [] : s.ids.map(id => MAILS.find(m => m.id === id));
    if (!ms.some(m => m.id === S.sqlMail) && ms.length) S.sqlMail = ms[0].id;
    c3.innerHTML = `<div class="pane-head"><h3>${t('q.mailList')}</h3><span class="count">${rows}</span></div>` +
      (ms.length ? `<div class="scroll" id="sqlMails" role="listbox" tabindex="0" aria-label="${t('q.mailList')}">${ms.map(m => mailItem(m, m.id === S.sqlMail, [], true)).join('')}</div>` : empty('sql', t('q.noMailT'), t('s.emptyP')));
    renderPreview(c4, ms.find(m => m.id === S.sqlMail), []);
    const l = $('#sqlMails'); if (l) listNav(l, id => { S.sqlMail = id; renderSql(); $('#sqlMails').focus(); });
  } else {
    const rs = isEmpty ? [] : s.rows;
    c3.innerHTML = `<div class="pane-head"><h3>${t('q.table')}</h3><span class="count">${rows}</span></div>
      <div class="scroll"><table class="grid" role="grid"><thead><tr>${s.cols.map(c => `<th class="mono">${c}</th>`).join('')}</tr></thead>
      <tbody>${rs.map((r, i) => `<tr tabindex="0" data-row="${i}" aria-selected="${i === S.selRow}">${r.map(v => `<td class="${typeof v === 'number' ? 'num' : ''}">${esc(typeof v === 'number' ? fmt(v) : v)}</td>`).join('')}</tr>`).join('')}</tbody></table></div>`;
    const r = rs[S.selRow];
    c4.innerHTML = `<div class="pane-head"><h3>${t('q.rowDetail')}</h3><span class="count">${r ? '#' + (S.selRow + 1) : ''}</span></div>` +
      (r ? `<div class="scroll"><dl class="kv">${s.cols.map((c, i) => `<dt>${c}</dt><dd>${esc(typeof r[i] === 'number' ? fmt(r[i]) : r[i])}</dd>`).join('')}</dl></div>` : empty('format', t('q.rowPick'), ''));
    $$('#sqlCol3 tr[data-row]').forEach(tr => {
      const pick = () => { S.selRow = +tr.dataset.row; renderSql(); $(`#sqlCol3 tr[data-row="${S.selRow}"]`).focus(); };
      tr.addEventListener('click', pick); tr.addEventListener('keydown', e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); pick(); } });
    });
  }
}

/* ---------------- 3. Import ---------------- */
const PH = ['scan', 'parse', 'webp', 'index'];
function renderImport() {
  const ps = S.psts, total = ps.reduce((a, p) => a + p.msgs, 0), uniq = ps.reduce((a, p) => a + p.unique, 0);
  $('#importStats').innerHTML = [[fmt(uniq), t('i.s.total')], [fmt(total - uniq), t('i.s.dedup')], [ps.length ? '31.6 GB' : '0 GB', t('i.s.size')]].map(([b, s]) => `<span class="stat"><b>${b}</b><span>${s}</span></span>`).join('');
  $('#pstCount').textContent = ps.length || '';
  const stBadge = s => ({ ok: `<span class="badge ok">${ic('check')}${t('i.st.ok')}</span>`, warn: `<span class="badge warn">${ic('warn')}${t('i.st.warn')}</span>` }[s]);
  $('#pstTable').innerHTML = ps.length ? `<table class="grid"><thead><tr><th>${t('i.c.name')} · ${t('i.c.path')}</th><th class="num">${t('i.c.size')}</th><th class="num">${t('i.c.msgs')}</th><th class="num">${t('i.c.unique')}</th><th>${t('i.c.date')}</th><th>${t('i.c.status')}</th><th><span class="sr-only">${t('i.remove')}</span></th></tr></thead>
    <tbody>${ps.map(p => `<tr><td style="max-width:280px"><span class="row">${ic('pst')}<span style="min-width:0"><strong>${esc(p.name)}</strong><span class="mono muted" style="display:block;overflow:hidden;text-overflow:ellipsis" title="${esc(p.path)}">${esc(p.path)}</span></span></span></td>
      <td class="num">${p.size}</td><td class="num">${fmt(p.msgs)}</td><td class="num">${fmt(p.unique)}</td><td>${p.date}</td><td>${stBadge(p.status)}</td>
      <td><button type="button" class="btn small subtle danger" data-rm="${p.id}" aria-label="${t('i.remove')} ${esc(p.name)}" title="${t('i.remove')}">${ic('trash')}</button></td></tr>`).join('')}</tbody></table>
    <div class="dropzone">${ic('import', '')}<span>${t('i.drop')}</span></div>`
    : empty('pst', t('i.emptyT'), t('i.emptyP'), `<button type="button" class="btn primary" id="addPst2">${ic('plus')}${t('i.add')}</button>`);
  renderQueue();
}
function qActions(q) {
  const b = (act, label, icon, cls = '') => `<button type="button" class="btn small ${cls}" data-qa="${act}" data-qid="${q.id}" data-key="${q.id}-${act}">${ic(icon)}${label}</button>`;
  return {
    running: b('pause', t('i.pause'), 'pause') + b('cancel', t('i.cancel'), 'x', 'subtle'),
    paused: b('resume', t('i.resume'), 'play', 'primary') + b('cancel', t('i.cancel'), 'x', 'subtle'),
    failed: b('resume', t('i.retry'), 'play', 'primary') + b('remove', t('i.removeQ'), 'trash', 'subtle danger'),
    cancelled: b('resume', t('i.resume'), 'play') + b('remove', t('i.removeQ'), 'trash', 'subtle danger'),
    queued: b('remove', t('i.removeQ'), 'trash', 'subtle danger'),
    ok: b('remove', t('i.removeQ'), 'x', 'subtle'),
  }[q.status];
}
function renderQueue() {
  const focusKey = document.activeElement && document.activeElement.dataset.key;
  const Q = S.queue;
  $('#qCount').textContent = Q.length || '';
  const badge = { running: 'brand', paused: '', failed: 'err', queued: '', cancelled: '', ok: 'ok' };
  $('#queue').innerHTML = Q.length ? Q.map(q => {
    const pct = q.total ? Math.round(q.done / q.total * 100) : 0;
    return `<div class="q-item" role="option" tabindex="0" aria-selected="${q.id === S.selQ}" data-q="${q.id}" data-key="${q.id}">
      <div class="top">${ic('pst')}<span class="nm">${esc(q.name)}</span><span class="badge ${badge[q.status]}">${t('i.st.' + q.status)}</span></div>
      <div class="phases" aria-hidden="true">${PH.map((_, i) => `<i class="${i < q.phase || q.status === 'ok' ? 'done' : i === q.phase && q.status !== 'queued' ? 'now' : ''}"></i>`).join('')}</div>
      <div class="sub"><span>${q.status === 'queued' ? q.size : `${t('i.ph.' + PH[Math.min(q.phase, 3)])} · ${q.size}`}</span><span style="flex:1"></span><span data-pc="${q.id}">${q.total ? `${fmt(q.done)} / ${fmt(q.total)} · ${pct}%` : ''}</span></div>
      <div class="progress ${q.status === 'failed' ? 'err' : q.status === 'running' || q.status === 'ok' ? '' : 'paused'}" role="progressbar" aria-valuenow="${pct}" aria-valuemin="0" aria-valuemax="100" aria-label="${esc(q.name)}"><i data-pb="${q.id}" style="width:${pct}%"></i></div>
      <div class="sub">${q.status === 'running' ? `<span>${t('i.rate', { n: fmt(q.rate) })}</span><span>${t('i.eta', { n: q.eta })}</span>` : ''}${q.ckpt && q.status !== 'ok' ? `<span>${t('i.ckpt', { n: q.ckpt })}</span>` : ''}${q.log.length ? `<span class="${q.log.some(l => l.lv === 'err') ? 'lv-err' : ''}" style="color:${q.log.some(l => l.lv === 'err') ? 'var(--colorStatusDangerForeground1)' : 'var(--colorStatusWarningForeground1)'}">${ic(q.log.some(l => l.lv === 'err') ? 'err' : 'warn')} ${q.log.length}</span>` : ''}</div>
      <div class="acts">${qActions(q)}</div></div>`;
  }).join('') : empty('import', t('i.qEmpty'), t('i.qEmptyP'));
  const sel = Q.find(q => q.id === S.selQ);
  $('#logFor').textContent = sel ? t('i.logFor', { n: sel.name }) : '';
  $('#log').innerHTML = !sel ? empty('info', t('i.pickQ'), '') : sel.log.length ? sel.log.map(l => `<div class="log-entry"><span class="mono muted">${l.t}</span><span class="lv-${l.lv}">${ic(l.lv === 'err' ? 'err' : 'warn')}</span><span class="msg">${esc(l.msg)}</span><span class="loc">${esc(l.loc)}</span></div>`).join('') : empty('check', t('i.noLog'), '');
  if (focusKey) { const f = $(`#queue [data-key="${focusKey}"]`); f && f.focus(); }
}
$('#queue').addEventListener('click', e => {
  const a = e.target.closest('[data-qa]');
  if (a) {
    const q = S.queue.find(x => x.id === a.dataset.qid);
    if (a.dataset.qa === 'pause') q.status = 'paused';
    if (a.dataset.qa === 'cancel') q.status = 'cancelled';
    if (a.dataset.qa === 'resume') { q.status = 'running'; q.rate = 1650; q.eta = '3 min'; if (!q.total) q.total = 64000; }
    if (a.dataset.qa === 'remove') { S.queue = S.queue.filter(x => x !== q); if (S.selQ === q.id) S.selQ = S.queue[0] && S.queue[0].id; }
    renderQueue(); return;
  }
  const it = e.target.closest('[data-q]'); if (it) { S.selQ = it.dataset.q; renderQueue(); }
});
$('#queue').addEventListener('keydown', e => { const it = e.target.closest('[data-q]'); if (it && e.target === it && (e.key === 'Enter' || e.key === ' ')) { e.preventDefault(); S.selQ = it.dataset.q; renderQueue(); } });
$('#pauseAll').addEventListener('click', () => { S.queue.forEach(q => { if (q.status === 'running') q.status = 'paused'; }); renderQueue(); });
function addPst() { S.queue.push({ id: 'q' + Date.now(), name: 'Archive_2024_H1.pst', size: '7.3 GB', status: 'queued', phase: 0, done: 0, total: 0, rate: 0, eta: '', ckpt: 0, log: [] }); renderQueue(); }
$('#addPst').addEventListener('click', addPst);
document.addEventListener('click', e => { if (e.target.closest('#addPst2')) addPst(); });
setInterval(() => { // ponytail: fake progress for the running item; real app streams events from the import worker
  S.queue.filter(q => q.status === 'running').forEach(q => {
    q.done = Math.min(q.total, q.done + 2400);
    if (q.done >= q.total) { if (q.phase >= 3) q.status = 'ok'; else { q.phase++; q.done = 0; } renderQueue(); return; }
    const pct = Math.round(q.done / q.total * 100), pb = $(`[data-pb="${q.id}"]`), pc = $(`[data-pc="${q.id}"]`);
    if (pb) { pb.style.width = pct + '%'; pb.parentElement.setAttribute('aria-valuenow', pct); }
    if (pc) pc.textContent = `${fmt(q.done)} / ${fmt(q.total)} · ${pct}%`;
  });
}, 800);

const dlg = $('#confirmDlg');
$('#pstTable').addEventListener('click', e => {
  const b = e.target.closest('[data-rm]'); if (!b) return;
  const p = S.psts.find(x => x.id === +b.dataset.rm);
  $('#dlgTitle').textContent = t('i.rmT', { n: p.name });
  $('#dlgBody').innerHTML = `<p>${t('i.rmB', { u: fmt(p.unique), s: fmt(p.shared) })}</p>`;
  $('#dlgOk').textContent = t('i.rmOk');
  dlg.onclose = () => { if (dlg.returnValue === 'ok') { S.psts = S.psts.filter(x => x !== p); renderImport(); renderFacets(); toast(t('i.removed', { n: p.name })); } };
  dlg.returnValue = ''; dlg.showModal();
});
$('#dlgCancel').addEventListener('click', () => dlg.close('cancel'));
$('#dlgOk').addEventListener('click', () => dlg.close('ok'));

/* ---------------- 4. Settings ---------------- */
function renderSettings() {
  const e = S.data === 'empty';
  $('#usageLegend').innerHTML = [[t('st.u.db'), e ? '0 GB' : '25.4 GB', 'var(--colorBrandBackground)'], [t('st.u.img'), e ? '0 GB' : '6.2 GB', '#4f9ee8'], [t('st.u.log'), e ? '0 GB' : '1.1 GB', '#9fc9f2'], [t('st.u.free'), '312 GB', 'var(--colorNeutralBackground5)']]
    .map(([l, v, c]) => `<span style="--c:${c}">${l} <b>${v}</b></span>`).join('');
  $('#usageBar').style.opacity = e ? .3 : 1;
  const f = [[t('st.f.status'), `<span class="row"><span class="dot"></span>${t('st.f.running')}</span>`], [t('st.f.engine'), 'PostgreSQL (bundled)'], [t('st.f.conn'), '127.0.0.1 · local only'], [t('st.f.uptime'), '3 h 12 min'],
    [t('st.f.msgs'), e ? '0' : '1,048,213'], [t('st.f.occ'), e ? '0' : '1,097,545'], [t('st.f.size'), e ? '48 MB' : '31.6 GB'], [t('st.f.ext'), '<span class="badge ok">pg_trgm</span>']];
  $('#dbFacts').innerHTML = `<dl class="facts">${f.map(([k, v]) => `<dt>${k}</dt><dd>${v}</dd>`).join('')}</dl>`;
  $('#idxTable').innerHTML = `<thead><tr>${['name', 'purpose', 'size', 'bloat', 'last', 'state'].map(k => `<th class="${k === 'size' || k === 'bloat' ? 'num' : ''}">${t('st.i.' + k)}</th>`).join('')}</tr></thead>
    <tbody>${INDEXES.map(r => `<tr><td class="mono">${r[0]}</td><td>${r[1]}</td><td class="num">${e ? '8 KB' : r[2]}</td><td class="num">${e ? '0%' : r[3]}</td><td>${r[4]}</td><td>${r[5] === 'ok' || e ? `<span class="badge ok">${t('st.i.healthy')}</span>` : `<span class="badge warn">${t('st.i.needs')}</span>`}</td></tr>`).join('')}</tbody>`;
}
$('#webpQ').addEventListener('input', e => { $('#webpVal').textContent = e.target.value; });
$$('input[name=lang]').forEach(r => r.addEventListener('change', () => { S.lang = r.value; renderAll(); }));

/* ---------------- 5. Onboarding ---------------- */
function renderOnboarding() {
  const steps = ['ob.s1', 'ob.s2', 'ob.s3', 'ob.s4'];
  $('#obSteps').innerHTML = steps.map((k, i) => `<li class="${i < S.ob ? 'done' : i === S.ob ? 'cur' : ''}" ${i === S.ob ? 'aria-current="step"' : ''}><span class="n">${i < S.ob ? ic('check') : i + 1}</span><span style="align-self:center">${t(k)}</span></li>`).join('');
  const choice = (key, on, title, desc, extra = '') => `<button type="button" class="ob-choice" data-ob="${key}" aria-pressed="${on}"><strong>${title}</strong><span class="muted">${desc}</span>${extra}</button>`;
  let body = '';
  if (S.ob === 0) body = `<h3>${t('ob.h1')}</h3><p class="lead">${t('ob.p1')}</p><div class="ob-grid">${choice('zh', S.lang === 'zh', '繁體中文', 'Traditional Chinese (Taiwan)')}${choice('en', S.lang === 'en', 'English', '英文')}</div>`;
  if (S.ob === 1) body = `<h3>${t('ob.h2')}</h3><p class="lead">${t('ob.p2')}</p><div class="ob-grid">
      ${choice('default', S.obDir === 'default', t('ob.default'), t('ob.defaultD'), `<span class="mono muted">C:\\Users\\Wen\\AppData\\Local\\Outlooker\\data</span><span class="badge">${t('ob.free', { n: '88 GB' })}</span>`)}
      ${choice('other', S.obDir === 'other', t('ob.other'), t('ob.otherD'), `<span class="mono muted">D:\\OutlookerData</span><span class="badge ok">${t('ob.free', { n: '1.6 TB' })}</span>`)}</div>`;
  if (S.ob === 2) {
    const items = ['ob.c1', 'ob.c2', 'ob.c3', 'ob.c4'];
    body = `<h3>${t('ob.h3')}</h3><p class="lead">${t('ob.p3')}</p>
      <div class="progress" style="max-width:640px;height:6px" role="progressbar" aria-valuenow="${S.obDb * 25}" aria-valuemin="0" aria-valuemax="100"><i style="width:${S.obDb * 25}%"></i></div>
      <ul class="checklist">${items.map((k, i) => `<li class="${i < S.obDb ? 'ok' : 'wait'}">${ic(i < S.obDb ? 'check' : 'info', '')}${t(k)}</li>`).join('')}</ul>`;
    clearTimeout(renderOnboarding.tm);
    if (S.obDb < 4) renderOnboarding.tm = setTimeout(() => { if (S.ob === 2 && S.obDb < 4) { S.obDb++; renderOnboarding(); } }, 700);
  }
  if (S.ob === 3) body = `<h3>${t('ob.h4')}</h3><p class="lead">${t('ob.p4')}</p>
      <div class="dropzone" style="margin:0;max-width:1200px;padding:var(--spacingXXL)">${ic('import', '')}<button type="button" class="btn primary">${t('ob.pick')}</button><span>${t('ob.dropHint')}</span></div>
      <div class="field" style="max-width:1200px"><span class="lbl">${t('ob.found')}</span>
      ${PSTS.slice(0, 3).map((p, i) => `<label class="check" style="padding-left:0"><input type="checkbox" ${i < 2 ? 'checked' : ''}>${ic('pst')}<strong>${esc(p.name)}</strong><span class="mono muted">${esc(p.path)}</span><span class="badge">${p.size}</span></label>`).join('')}</div>`;
  const nextDisabled = S.ob === 2 && S.obDb < 4;
  $('#obBody').innerHTML = body + `<div class="ob-foot">
      ${S.ob ? `<button type="button" class="btn" data-obnav="back">${t('ob.back')}</button>` : ''}<span class="spacer"></span>
      ${S.ob === 3 ? `<button type="button" class="btn subtle" data-obnav="skip">${t('ob.skip')}</button><button type="button" class="btn primary" data-obnav="finish">${t('ob.finish')}</button>`
      : `<button type="button" class="btn primary" data-obnav="next" ${nextDisabled ? 'disabled' : ''}>${t('ob.next')}</button>`}</div>`;
}
$('#obBody').addEventListener('click', e => {
  const c = e.target.closest('[data-ob]');
  if (c) { const k = c.dataset.ob; if (k === 'zh' || k === 'en') { S.lang = k; renderAll(); } else { S.obDir = k; renderOnboarding(); } return; }
  const n = e.target.closest('[data-obnav]'); if (!n) return;
  const a = n.dataset.obnav;
  if (a === 'next') S.ob = Math.min(3, S.ob + 1);
  if (a === 'back') S.ob = Math.max(0, S.ob - 1);
  if (S.ob === 2 && a === 'next') S.obDb = 0;
  if (a === 'skip') { S.ob = 0; go('search'); }
  if (a === 'finish') { S.ob = 0; go('import'); }
  renderOnboarding();
});

$('#sqlText').value = SAVED[0].sql;
renderAll();
// Deep link for reviewers: index.html#sql, #import:empty, #settings:en, #search:empty:en
if (location.hash) {
  const [scr, ...flags] = location.hash.slice(1).split(':');
  if (flags.includes('en')) $('#langSeg [data-lang="en"]').click();
  if (flags.includes('empty')) $('#dataSeg [data-data="empty"]').click();
  if (flags.includes('table')) $('#savedList [data-sid="s3"]').click();
  if (flags.includes('short')) { $('#q').value = '陳'; $('#q').dispatchEvent(new Event('input')); }
  if (flags.includes('zero')) { $('#q').value = 'xyz-no-such-mail'; $('#q').dispatchEvent(new Event('input')); }
  go(scr || 'search');
}

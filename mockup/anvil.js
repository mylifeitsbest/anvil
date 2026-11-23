'use strict';
/*
 * Anvil — анимированный макет интерфейса.
 * Всё состояние кадра считается детерминированно из времени t (сцена → view → render),
 * поэтому один и тот же код и крутится вживую в браузере, и покадрово снимается в GIF.
 */
(() => {
  // ============================================================ утилиты
  const $ = (s, r = document) => r.querySelector(s);
  const clamp = (x, a = 0, b = 1) => (x < a ? a : x > b ? b : x);
  const lerp = (a, b, k) => a + (b - a) * k;
  const E = {
    out: k => 1 - Math.pow(1 - k, 3),
    inOut: k => (k < 0.5 ? 4 * k * k * k : 1 - Math.pow(-2 * k + 2, 3) / 2),
  };
  const FX = { anim: true, smooth: true };
  const prog = (t, a, d) => (FX.anim ? clamp((t - a) / d) : t >= a ? 1 : 0);
  const eo = (t, a, d) => E.out(prog(t, a, d));
  const eio = (t, a, d) => E.inOut(prog(t, a, d));
  const pulse = (t, a, din, hold, dout) => eo(t, a, din) * (1 - eo(t, a + din + hold, dout));
  const esc = s => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
  function rng(seed) {
    let s = Math.imul(seed + 1, 2654435761) >>> 0 || 1;
    return () => { s = (Math.imul(s, 1664525) + 1013904223) >>> 0; return s / 4294967296; };
  }
  function blinkO(dt) {
    if (dt < 0.5) return 1;
    if (!FX.anim) return Math.floor((dt - 0.5) / 0.53) % 2 ? 0 : 1;
    return clamp(0.5 + 0.95 * Math.cos((2 * Math.PI * (dt - 0.5)) / 1.06));
  }

  // ============================================================ цвета
  function pc(c) {
    if (Array.isArray(c)) return c;
    if (c[0] === '#') {
      let h = c.slice(1);
      if (h.length === 3) h = [...h].map(x => x + x).join('');
      const n = parseInt(h.slice(0, 6), 16);
      return [(n >> 16) & 255, (n >> 8) & 255, n & 255, h.length === 8 ? parseInt(h.slice(6), 16) / 255 : 1];
    }
    const m = c.match(/[\d.]+/g).map(Number);
    return [m[0], m[1], m[2], m.length > 3 ? m[3] : 1];
  }
  const mixc = (a, b, k) => {
    a = pc(a); b = pc(b);
    return [lerp(a[0], b[0], k), lerp(a[1], b[1], k), lerp(a[2], b[2], k), lerp(a[3], b[3], k)];
  };
  const rgba = c => { c = pc(c); return `rgba(${Math.round(c[0])},${Math.round(c[1])},${Math.round(c[2])},${Math.round(c[3] * 1000) / 1000})`; };
  const alpha = (c, a) => { c = pc(c); return [c[0], c[1], c[2], c[3] * a]; };
  const toHex = c => '#' + pc(c).slice(0, 3).map(v => Math.round(v).toString(16).padStart(2, '0')).join('');
  function relLum(c) {
    c = pc(c);
    const f = v => { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); };
    return 0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2]);
  }
  function onColor(c) { const L = relLum(c); return 1.05 / (L + 0.05) >= ((L + 0.05) / 0.05) * 0.75 ? '#ffffff' : '#141110'; }

  // ============================================================ темы
  const THEMES = {
    'anvil-dark': {
      title: 'Anvil Dark', dark: 1,
      card: '#060607', bg: '#0f0f12', bg2: '#0b0b0e', bg3: '#17171c', line: '#ffffff0f', line2: '#ffffff17',
      text: '#e7e7eb', text2: '#8e8e9a', text3: '#55555f', hover: '#ffffff0a', guide: '#ffffff0b', curline: '#ffffff06',
      shadow: '#000000b3', scrim: '#00000047', ok: '#4ad295',
      kw: '#f5a37a', fn: '#8db4ff', ty: '#7fd4c1', str: '#b6d98f', num: '#f1c77e', cm: '#5a5a66', mac: '#d2a6ff', attr: '#80808c', p: '#7a7a86', lt: '#d2a6ff', var: '#dedee4',
    },
    'anvil-light': {
      title: 'Anvil Light', dark: 0,
      card: '#e7e7e3', bg: '#fdfdfc', bg2: '#f5f5f3', bg3: '#ffffff', line: '#0000000f', line2: '#0000001a',
      text: '#1c1c20', text2: '#62626c', text3: '#a2a2ab', hover: '#0000000a', guide: '#00000010', curline: '#00000006',
      shadow: '#0000003d', scrim: '#00000014', ok: '#16a36a',
      kw: '#c8501c', fn: '#2459d6', ty: '#0b7d6e', str: '#507c10', num: '#b25d00', cm: '#a3a3ac', mac: '#8b3fd3', attr: '#8b8b94', p: '#7a7a84', lt: '#8b3fd3', var: '#2a2a30',
    },
    midnight: {
      title: 'Midnight', dark: 1,
      card: '#04060b', bg: '#0d1119', bg2: '#0a0d14', bg3: '#141a26', line: '#a9bcff12', line2: '#a9bcff1c',
      text: '#e1e7f5', text2: '#8390ab', text3: '#4a5571', hover: '#a9bcff0d', guide: '#a9bcff0f', curline: '#a9bcff07',
      shadow: '#000000c0', scrim: '#00000050', ok: '#5ad7a0',
      kw: '#c4a7ff', fn: '#82c8ff', ty: '#72e2d1', str: '#a8e2a0', num: '#f9b588', cm: '#4c5774', mac: '#f4a4c8', attr: '#6e7894', p: '#6e7894', lt: '#f4a4c8', var: '#d6ddef',
    },
    paper: {
      title: 'Paper', dark: 0,
      card: '#e4ddcf', bg: '#f8f4eb', bg2: '#f0eadd', bg3: '#fcfaf4', line: '#3a2c1a12', line2: '#3a2c1a1f',
      text: '#2b251f', text2: '#7a6e60', text3: '#b1a596', hover: '#3a2c1a0b', guide: '#3a2c1a12', curline: '#3a2c1a07',
      shadow: '#3a2c1a40', scrim: '#3a2c1a14', ok: '#4f8a3a',
      kw: '#a9471d', fn: '#2d5e8c', ty: '#2c7a63', str: '#69791d', num: '#9d5900', cm: '#b2a696', mac: '#8a4b9a', attr: '#9a8f80', p: '#8c8174', lt: '#8a4b9a', var: '#3a332b',
    },
    graphite: {
      title: 'Graphite', dark: 1,
      card: '#070707', bg: '#121212', bg2: '#0d0d0d', bg3: '#1b1b1b', line: '#ffffff10', line2: '#ffffff19',
      text: '#ededed', text2: '#8f8f8f', text3: '#565656', hover: '#ffffff0b', guide: '#ffffff0c', curline: '#ffffff06',
      shadow: '#000000c0', scrim: '#00000050', ok: '#9be29b',
      kw: '#ffffff', fn: '#cfcfcf', ty: '#b8b8b8', str: '#9b9b9b', num: '#d9d9d9', cm: '#585858', mac: '#e6e6e6', attr: '#707070', p: '#6f6f6f', lt: '#bdbdbd', var: '#d4d4d4',
    },
  };
  const THEME_ORDER = ['anvil-dark', 'anvil-light', 'midnight', 'paper', 'graphite'];
  const COLOR_KEYS = Object.keys(THEMES['anvil-dark']).filter(k => k !== 'title' && k !== 'dark');
  const SYN = ['kw', 'fn', 'ty', 'str', 'num', 'cm', 'mac', 'attr', 'p', 'lt', 'var'];
  const themeCache = {};
  function resolveTheme(id) {
    if (themeCache[id]) return themeCache[id];
    const T = THEMES[id] || THEMES['anvil-dark'];
    const o = { dark: T.dark };
    for (const k of COLOR_KEYS) o[k] = pc(T[k]);
    return (themeCache[id] = o);
  }
  function mixTheme(A, B, k) {
    if (k <= 0) return A;
    if (k >= 1) return B;
    const o = { dark: k < 0.5 ? A.dark : B.dark };
    for (const key of COLOR_KEYS) o[key] = mixc(A[key], B[key], k);
    return o;
  }

  // ============================================================ настройки
  const DEFAULTS = {
    theme: 'anvil-dark', accent: '#ff7a45', radius: 10, fontSize: 13, density: 'comfortable',
    sidebar: 'left', minimap: true, cursor: 'line', smooth: true, animations: true,
  };
  const ACCENTS = [['Ember', '#ff7a45'], ['Violet', '#8b7bff'], ['Cyan', '#2fc6f6'], ['Lime', '#9fd84a'], ['Rose', '#ff5c93'], ['Gold', '#f5c04a'], ['Mono', '#e6e6e6']];
  function settingsToml(S) {
    return [
      '# ~/.config/anvil/settings.toml',
      '# изменения применяются на лету',
      '',
      '[theme]',
      `name    = "${S.theme}"`,
      `accent  = "${S.accent}"`,
      `radius  = ${S.radius}`,
      '',
      '[editor]',
      'font      = "JetBrains Mono"',
      `font_size = ${S.fontSize}`,
      `cursor    = "${S.cursor}"`,
      `smooth    = ${S.smooth}`,
      `minimap   = ${S.minimap}`,
      '',
      '[ui]',
      `sidebar   = "${S.sidebar}"`,
      `density   = "${S.density}"`,
      '',
      '[animations]',
      `enabled   = ${S.animations}`,
      'speed     = 1.0',
    ].join('\n');
  }
  function lookFrom(S) {
    return {
      th: resolveTheme(S.theme), accent: pc(S.accent), radius: S.radius, fs: S.fontSize,
      density: S.density === 'compact' ? 0 : 1, side: S.sidebar === 'right' ? 1 : 0, minimap: S.minimap ? 1 : 0, cursor: S.cursor,
    };
  }
  function mixLook(A, B, k) {
    const near = (a, b, e) => Math.abs(a - b) < e;
    const num = (a, b, e) => (near(a, b, e) ? b : lerp(a, b, k));
    const col = (a, b) => (a.every((v, i) => near(v, b[i], i === 3 ? 0.004 : 0.6)) ? b : mixc(a, b, k));
    const th = { dark: B.th.dark };
    for (const key of COLOR_KEYS) th[key] = col(A.th[key], B.th[key]);
    return {
      th, accent: col(A.accent, B.accent), radius: num(A.radius, B.radius, 0.05), fs: num(A.fs, B.fs, 0.01),
      density: num(A.density, B.density, 0.01), side: num(A.side, B.side, 0.002), minimap: num(A.minimap, B.minimap, 0.01), cursor: B.cursor,
    };
  }

  // ============================================================ подсветка
  const RUST_KW = new Set('as async await break const continue crate dyn else enum extern false fn for if impl in let loop match mod move mut pub ref return Self static struct super trait true type unsafe use where while'.split(' '));
  const RUST_TY = new Set('usize u8 u16 u32 u64 u128 i8 i16 i32 i64 isize f32 f64 bool char str String Vec Option Result Box'.split(' '));
  const tokCache = new Map();
  function tokens(lang, s) {
    const key = lang + '\u0000' + s;
    let r = tokCache.get(key);
    if (r) return r;
    r = lang === 'rust' ? tokRust(s) : lang === 'toml' ? tokToml(s) : [{ t: s, c: 'var' }];
    if (tokCache.size > 6000) tokCache.clear();
    tokCache.set(key, r);
    return r;
  }
  function pusher(out) {
    return (t, c) => {
      const last = out[out.length - 1];
      if (last && last.c === c && (c === 'p' || c === 'ws')) last.t += t;
      else out.push({ t, c });
    };
  }
  function tokRust(s) {
    const out = [], push = pusher(out);
    let i = 0, m;
    while (i < s.length) {
      const r = s.slice(i);
      if ((m = r.match(/^\/\/.*/))) { push(m[0], 'cm'); i += m[0].length; continue; }
      if ((m = r.match(/^\s+/))) { push(m[0], 'ws'); i += m[0].length; continue; }
      if ((m = r.match(/^"(?:[^"\\]|\\.)*"?/))) { push(m[0], 'str'); i += m[0].length; continue; }
      if ((m = r.match(/^'[a-zA-Z_]\w*(?!')/))) { push(m[0], 'lt'); i += m[0].length; continue; }
      if ((m = r.match(/^#!?\[[^\]]*\]?/))) { push(m[0], 'attr'); i += m[0].length; continue; }
      if ((m = r.match(/^\d[\d_]*(?:\.\d+)?(?:[iu](?:8|16|32|64|128|size)|f32|f64)?/))) { push(m[0], 'num'); i += m[0].length; continue; }
      if ((m = r.match(/^[A-Za-z_]\w*/))) {
        const w = m[0];
        if (s[i + w.length] === '!') { push(w + '!', 'mac'); i += w.length + 1; continue; }
        let c = 'var';
        if (w === 'self') c = 'sf';
        else if (RUST_KW.has(w)) c = 'kw';
        else if (RUST_TY.has(w) || /^[A-Z]/.test(w)) c = 'ty';
        else if (/^\s*\(/.test(s.slice(i + w.length))) c = 'fn';
        push(w, c); i += w.length; continue;
      }
      push(s[i], 'p'); i++;
    }
    return out;
  }
  function tokTomlVal(s, out) {
    const push = pusher(out);
    let i = 0, m;
    while (i < s.length) {
      const r = s.slice(i);
      if ((m = r.match(/^\s+/))) { push(m[0], 'ws'); i += m[0].length; continue; }
      if (r[0] === '#') { push(r, 'cm'); break; }
      if ((m = r.match(/^"[^"]*"?/))) { push(m[0], 'str'); i += m[0].length; continue; }
      if ((m = r.match(/^(true|false)\b/))) { push(m[0], 'kw'); i += m[0].length; continue; }
      if ((m = r.match(/^-?\d[\d_]*(\.\d+)?/))) { push(m[0], 'num'); i += m[0].length; continue; }
      if ((m = r.match(/^[A-Za-z_][\w-]*(?=\s*=)/))) { push(m[0], 'fn'); i += m[0].length; continue; }
      push(r[0], 'p'); i++;
    }
    return out;
  }
  function tokToml(s) {
    const out = [];
    let m;
    if ((m = s.match(/^(\s*)#/))) { if (m[1]) out.push({ t: m[1], c: 'ws' }); out.push({ t: s.slice(m[1].length), c: 'cm' }); return out; }
    if ((m = s.match(/^(\s*)(\[[^\]]*\]?)(.*)$/))) {
      if (m[1]) out.push({ t: m[1], c: 'ws' });
      out.push({ t: m[2], c: 'ty' });
      return tokTomlVal(m[3], out);
    }
    if ((m = s.match(/^(\s*)([A-Za-z0-9_.-]+)(\s*)(=)(.*)$/))) {
      if (m[1]) out.push({ t: m[1], c: 'ws' });
      out.push({ t: m[2], c: 'fn' });
      if (m[3]) out.push({ t: m[3], c: 'ws' });
      out.push({ t: '=', c: 'p' });
      return tokTomlVal(m[5], out);
    }
    return tokTomlVal(s, out);
  }
  const span = (t, c) => (!t ? '' : c === 'ws' ? esc(t) : `<span class="t-${c}">${esc(t)}</span>`);
  function lineHTML(lang, s, inl) {
    const toks = tokens(lang, s);
    if (!inl || !inl.length) return toks.map(tk => span(tk.t, tk.c)).join('');
    let html = '', col = 0, ii = 0;
    const ins = inl.slice().sort((a, b) => a.c - b.c);
    for (const tk of toks) {
      let t = tk.t;
      while (ii < ins.length && ins[ii].c >= col && ins[ii].c < col + t.length) {
        const cut = ins[ii].c - col;
        html += span(t.slice(0, cut), tk.c) + `<span class="inlay">${esc(ins[ii].text)}</span>`;
        t = t.slice(cut); col += cut; ii++;
      }
      html += span(t, tk.c); col += t.length;
    }
    for (; ii < ins.length; ii++) html += `<span class="inlay">${esc(ins[ii].text)}</span>`;
    return html;
  }
  function fuzzy(q, s) {
    if (!q) return { score: 0, idx: [] };
    const ql = q.toLowerCase(), sl = s.toLowerCase();
    const idx = [];
    let j = 0, score = 0, prev = -2;
    for (let i = 0; i < sl.length && j < ql.length; i++) {
      if (sl[i] === ql[j]) {
        idx.push(i);
        score += (i === prev + 1 ? 3 : 1) + (i === 0 || /[\s:_.(]/.test(s[i - 1]) ? 2 : 0);
        prev = i; j++;
      }
    }
    return j === ql.length ? { score, idx } : null;
  }
  function hlIdx(s, idx) {
    const set = new Set(idx);
    let o = '';
    for (let i = 0; i < s.length; i++) o += set.has(i) ? `<span class="hl">${esc(s[i])}</span>` : esc(s[i]);
    return o;
  }

  // ============================================================ документ и правки (мини-CRDT для сцен)
  function mkDoc(name, lang, text, marks) {
    return { name, lang, lines: text.split('\n').map((s, i) => ({ s, m: (marks && marks[i]) || null })) };
  }
  const cloneDoc = d => ({ name: d.name, lang: d.lang, lines: d.lines.map(l => ({ s: l.s, m: l.m })) });
  const isMe = who => who.startsWith('me');
  const markOf = (who, prev) => (isMe(who) ? (prev === 'add' ? 'add' : 'mod') : who);
  function ins(doc, A, l, c, s, who) {
    const line = doc.lines[l];
    const before = line.s.slice(0, c), after = line.s.slice(c);
    const parts = s.split('\n');
    if (parts.length === 1) {
      line.s = before + s + after;
      line.m = markOf(who, line.m);
      for (const k in A) {
        const b = A[k];
        if (b.l === l && (b.c > c || (b.c === c && k === who))) b.c += s.length;
        if (k !== who && b.sel && b.sel.l === l && b.sel.c1 >= c) { b.sel.c1 += s.length; b.sel.c2 += s.length; }
      }
      return;
    }
    const n = parts.length - 1;
    line.s = before + parts[0];
    if (parts[0].length) line.m = markOf(who, line.m);
    const nl = parts.slice(1).map(p => ({ s: p, m: isMe(who) ? 'add' : who }));
    nl[n - 1].s += after;
    doc.lines.splice(l + 1, 0, ...nl);
    for (const k in A) {
      const b = A[k];
      if (b.l > l) b.l += n;
      else if (b.l === l && (b.c > c || (b.c === c && k === who))) { b.l += n; b.c = b.c - c + parts[n].length; }
      if (k !== who && b.sel && b.sel.l > l) b.sel.l += n;
    }
  }
  function del(doc, A, l, c, n, who) {
    const line = doc.lines[l];
    line.s = line.s.slice(0, c) + line.s.slice(c + n);
    line.m = markOf(who, line.m);
    for (const k in A) { const b = A[k]; if (b.l === l && b.c > c) b.c = Math.max(c, b.c - n); }
  }
  const PAIRS = { '(': ')', '[': ']', '{': '}' };
  function applyOp(doc, A, op) {
    const a = A[op.who];
    if (!a) return;
    switch (op.k) {
      case 'move': a.l = op.l; a.c = op.c; a.sel = null; break;
      case 'sel': a.sel = { l: op.l, c1: op.c1, c2: op.c2, t: op.t }; a.l = op.l; a.c = op.c2; break;
      case 'unsel': a.sel = null; break;
      case 'show': a.vis = true; a.vt = op.t; break;
      case 'hide': a.vis = false; a.vt = op.t; a.sel = null; break;
      case 'type': {
        if (a.sel) {
          const sl = a.sel;
          a.sel = null; a.l = sl.l; a.c = sl.c1;
          del(doc, A, sl.l, sl.c1, sl.c2 - sl.c1, op.who);
        }
        const s = op.s, next = doc.lines[a.l].s[a.c];
        if (s.length === 1 && ')]}'.includes(s) && next === s) { a.c += 1; break; } // перепечатка закрывающей скобки
        if (s.length === 1 && PAIRS[s]) { ins(doc, A, a.l, a.c, s + PAIRS[s], op.who); a.c -= 1; break; } // автоскобки
        if (s[0] === '\n' && next === '}') { // Enter между {} — раскрываем блок
          const ind = s.slice(1);
          ins(doc, A, a.l, a.c, s + '\n' + ind.slice(4), op.who);
          a.l -= 1; a.c = ind.length;
          break;
        }
        ins(doc, A, a.l, a.c, s, op.who);
        break;
      }
    }
  }
  function replay(doc0, actors0, ops, t) {
    const doc = cloneDoc(doc0);
    const A = {};
    for (const k in actors0) {
      const a = actors0[k];
      A[k] = Object.assign({ id: k, vis: true, vt: null }, a, { sel: a.sel ? { ...a.sel } : null, hist: [{ t: -9, l: a.l, c: a.c }] });
    }
    for (const op of ops) {
      if (op.t > t) break;
      applyOp(doc, A, op);
      for (const k in A) {
        const a = A[k], h = a.hist[a.hist.length - 1];
        if (h.l !== a.l || h.c !== a.c) a.hist.push({ t: op.t, l: a.l, c: a.c });
      }
    }
    return { doc, A };
  }
  function glide(a, t) {
    const h = a.hist, cur = h[h.length - 1], prev = h.length > 1 ? h[h.length - 2] : cur;
    const d = Math.abs(cur.l - prev.l) > 0.5 ? 0.17 : 0.085;
    const k = FX.smooth && FX.anim ? E.out(clamp((t - cur.t) / d)) : 1;
    return { l: lerp(prev.l, cur.l, k), c: lerp(prev.c, cur.c, k), since: cur.t };
  }
  function actorsView(A, t) {
    const out = [];
    for (const k in A) {
      const a = A[k];
      let o = 1;
      if (a.vt != null) o = a.vis ? eo(t, a.vt, 0.2) : 1 - eo(t, a.vt, 0.2);
      else if (!a.vis) o = 0;
      if (o <= 0.001) continue;
      const g = glide(a, t);
      out.push({ id: k, me: !!a.me, color: a.color, name: a.name, l: g.l, c: g.c, since: g.since, o, sel: a.sel, L: a.l, C: a.c });
    }
    return out;
  }
  function typeOps(who, text, t0, cps = 15, seed = 7) {
    const r = rng(seed), ops = [];
    let t = t0, i = 0;
    while (i < text.length) {
      let s = text[i];
      if (s === '\n') { let j = i + 1; while (text[j] === ' ') j++; s = text.slice(i, j); }
      ops.push({ t, who, k: 'type', s });
      i += s.length;
      t += (1 / cps) * (0.6 + 0.8 * r()) * (s === ' ' ? 1.25 : s[0] === '\n' ? 2.4 : 1);
    }
    return { ops, last: ops.length ? ops[ops.length - 1].t : t0 };
  }
  const byTime = ops => ops.sort((a, b) => a.t - b.t);

  // ============================================================ исходники для сцен
  const BUFFER_SRC = [
    'use ropey::Rope;',
    'use crate::history::{Edit, History};',
    '',
    '/// Текстовый буфер на основе rope: вставка и удаление за O(log n).',
    'pub struct Buffer {',
    '    text: Rope,',
    '    history: History,',
    '    version: u64,',
    '}',
    '',
    'impl Buffer {',
    '    pub fn new() -> Self {',
    '        Self { text: Rope::new(), history: History::default(), version: 0 }',
    '    }',
    '',
    '    /// Вставляет текст по смещению и пишет правку в дерево истории.',
    '    pub fn insert(&mut self, at: usize, text: &str) -> Result<(), BufferError> {',
    '        if at > self.text.len_chars() {',
    '            return Err(BufferError::OutOfBounds { at });',
    '        }',
    '        self.text.insert(at, text);',
    '        self.history.push(Edit::insert(at, text));',
    '        self.version += 1;',
    '        Ok(())',
    '    }',
    '',
    '    pub fn undo(&mut self) -> Option<Edit> {',
    '        let edit = self.history.undo()?;',
    '        edit.revert(&mut self.text);',
    '        Some(edit)',
    '    }',
    '}',
  ].join('\n');
  const BUFFER_MARKS = { 20: 'mod', 21: 'mod', 26: 'add', 27: 'add', 28: 'add', 29: 'add', 30: 'add' };
  const bufferDoc = () => mkDoc('buffer.rs', 'rust', BUFFER_SRC, BUFFER_MARKS);
  const BUF_TABS = () => [{ n: 'buffer.rs', on: true }, { n: 'history.rs' }, { n: 'Cargo.toml' }];
  const BUF_CRUMBS = ['crates', 'anvil-editor', 'src', 'buffer.rs', 'impl Buffer'];
  function inlaysFor(doc) {
    const m = {};
    if (doc.lang !== 'rust') return m;
    doc.lines.forEach((ln, i) => {
      const r = ln.s.match(/^(\s*let )(\w+)( = self\.history\.undo\(\)\?;)/);
      if (r) m[i] = [{ c: r[1].length + r[2].length, text: ': Edit' }];
    });
    return m;
  }
  const TERM_PROMPT = '<span class="p">~/anvil</span> <span class="b">main</span> <span class="a">❯</span> ';
  const TERM_OUT = [
    '   <span class="g">Compiling</span> anvil-editor v0.1.0 (crates/anvil-editor)',
    '   <span class="g">Compiling</span> anvil v0.1.0 (crates/anvil)',
    '    <span class="g">Finished</span> `release` profile [optimized] target(s) in 4.21s',
    '     <span class="g">Running</span> `target/release/anvil`',
    '<span class="a">▸</span> старт <span class="w">182 мс</span> · кадр <span class="w">8.3 мс</span> · память <span class="w">48 МБ</span>',
  ];
  const RUST_COMP = [
    { n: 'len_lines', d: 'fn(&self) -> usize', doc: 'Число строк в rope' },
    { n: 'len_chars', d: 'fn(&self) -> usize', doc: 'Число символов Unicode' },
    { n: 'len_bytes', d: 'fn(&self) -> usize', doc: 'Размер в байтах UTF-8' },
    { n: 'line', d: 'fn(&self, idx) -> RopeSlice', doc: 'Строка по индексу' },
    { n: 'lines', d: 'fn(&self) -> Lines', doc: 'Итератор по строкам' },
    { n: 'slice', d: 'fn(&self, range) -> RopeSlice', doc: 'Срез без копирования' },
    { n: 'chars', d: 'fn(&self) -> Chars', doc: 'Итератор по символам' },
  ];
  const COMMANDS = [
    ['Файл', 'Открыть папку…', 'Ctrl K O', 'files'],
    ['Вид', 'Разделить редактор', 'Ctrl \\', 'split'],
    ['Git', 'Закоммитить изменения', 'Ctrl Enter', 'branch'],
    ['Тема', 'Цветовая тема', 'Ctrl K T', 'palette'],
    ['Терминал', 'Новый терминал', 'Ctrl `', 'term'],
    ['Удалённо', 'Подключиться по SSH…', '', 'server'],
    ['Совместно', 'Пригласить в сессию', '', 'users'],
    ['Тема', 'Значки файлов', '', 'palette'],
    ['Редактор', 'Переключить миникарту', '', 'map'],
    ['Настройки', 'Открыть settings.toml', 'Ctrl ,', 'sliders'],
  ];
  const TREE = [
    { d: 0, dir: 1, open: 1, n: 'crates' },
    { d: 1, dir: 1, open: 1, n: 'anvil-editor' },
    { d: 2, dir: 1, open: 1, n: 'src' },
    { d: 3, n: 'buffer.rs', g: 'M' },
    { d: 3, n: 'cursor.rs' },
    { d: 3, n: 'history.rs', g: 'M' },
    { d: 3, n: 'lib.rs' },
    { d: 2, n: 'Cargo.toml' },
    { d: 1, dir: 1, n: 'anvil-ui' },
    { d: 1, dir: 1, n: 'anvil-lsp' },
    { d: 1, dir: 1, n: 'anvil-backend' },
    { d: 1, dir: 1, n: 'anvil-collab', g: 'A' },
    { d: 1, dir: 1, n: 'anvil-relay' },
    { d: 1, dir: 1, n: 'anvil-ext' },
    { d: 0, n: 'Cargo.toml' },
    { d: 0, n: 'deny.toml' },
    { d: 0, n: 'README.md' },
  ];
  const ficol = n => (n.endsWith('.rs') ? '#e8835a' : n.endsWith('.md') ? '#6ea8ff' : '#9a9aa6');

  // ============================================================ иконки
  const I = {
    search: '<circle cx="11" cy="11" r="6.5"/><path d="m20 20-4.2-4.2"/>',
    files: '<path d="M13.5 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8.5z"/><path d="M13.5 3v5.5H19"/>',
    branch: '<circle cx="6" cy="5.5" r="2"/><circle cx="6" cy="18.5" r="2"/><circle cx="18" cy="7.5" r="2"/><path d="M6 7.5v9"/><path d="M18 9.5c0 4.5-6 4-10.5 7"/>',
    blocks: '<rect x="3.5" y="3.5" width="7" height="7" rx="1.5"/><rect x="13.5" y="3.5" width="7" height="7" rx="1.5"/><rect x="3.5" y="13.5" width="7" height="7" rx="1.5"/><path d="M17 13.5v7M13.5 17h7"/>',
    dots: '<circle cx="5.5" cy="12" r="1.3" fill="currentColor" stroke="none"/><circle cx="12" cy="12" r="1.3" fill="currentColor" stroke="none"/><circle cx="18.5" cy="12" r="1.3" fill="currentColor" stroke="none"/>',
    chevR: '<path d="m9.5 6 6 6-6 6"/>',
    chevD: '<path d="m6 9.5 6 6 6-6"/>',
    x: '<path d="M6.5 6.5l11 11M17.5 6.5l-11 11"/>',
    min: '<path d="M5 12h14"/>',
    max: '<rect x="5.5" y="5.5" width="13" height="13" rx="1.5"/>',
    split: '<rect x="3.5" y="4.5" width="17" height="15" rx="2"/><path d="M12 4.5v15"/>',
    err: '<circle cx="12" cy="12" r="8.5"/><path d="M14.8 9.2l-5.6 5.6M9.2 9.2l5.6 5.6"/>',
    warn: '<path d="M12 4 3 19.5h18z"/><path d="M12 10v4.5M12 17v.01"/>',
    check: '<path d="m5 12.5 4.5 4.5L19 7.5"/>',
    users: '<circle cx="9" cy="8.5" r="3.5"/><path d="M3 19.5c.8-3.3 3.1-5 6-5s5.2 1.7 6 5"/><path d="M15.5 5a3.5 3.5 0 0 1 0 7M21 19.5c-.5-2.4-1.7-3.9-3.4-4.6"/>',
    sync: '<path d="M7 20V5M3.5 8.5 7 5l3.5 3.5M17 4v15M13.5 15.5 17 19l3.5-3.5"/>',
    lock: '<rect x="5" y="11" width="14" height="9.5" rx="2"/><path d="M8.5 11V7.5a3.5 3.5 0 0 1 7 0V11"/>',
    term: '<path d="m5 8 4 4-4 4M12 17h7"/>',
    server: '<rect x="3.5" y="4" width="17" height="7" rx="2"/><rect x="3.5" y="13" width="17" height="7" rx="2"/><path d="M7.5 7.5h.01M7.5 16.5h.01"/>',
    palette: '<circle cx="12" cy="12" r="8.5"/><path d="M12 3.5a8.5 8.5 0 0 0 0 17z" fill="currentColor"/>',
    map: '<rect x="4" y="3.5" width="16" height="17" rx="2"/><path d="M8 8h5M8 11h8M8 14h4M8 17h6"/>',
    sliders: '<path d="M4 7h9M17 7h3M4 17h3M11 17h9"/><circle cx="15" cy="7" r="2"/><circle cx="9" cy="17" r="2"/>',
  };
  const svg = (p, extra = '') => `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" ${extra}>${p}</svg>`;
  const LOGO = id => `<svg class="logo" viewBox="0 0 24 24"><defs><linearGradient id="${id}" x1="0" y1="0" x2="1" y2="1"><stop offset="0" style="stop-color:var(--accent2)"/><stop offset="1" style="stop-color:var(--accent)"/></linearGradient></defs><path fill="url(#${id})" d="M6 5H21V9H18C17 9 16.5 10 16.5 11V14H19.5C20.5 14 21 15 21 16V19H6V16C6 15 6.5 14 7.5 14H10.5V11C10.5 10 10 9 9 9H8C5.5 9 3.5 8 2 6.5C3.5 5.5 4.5 5 6 5Z"/></svg>`;

  // ============================================================ DOM
  const BW = 958, BH = 516, SBW = 216, PANEL_H = 170;
  const stage = $('#stage');
  const R = {};
  let measEl;
  function buildStage() {
    stage.innerHTML = `
<div class="glow"></div>
<div class="win">
  <div class="titlebar">
    <div class="tb-l">${LOGO('lg1')}<span class="proj">anvil</span><span class="slash">/</span><span class="branch">${svg(I.branch)}main</span><div class="live"></div></div>
    <div class="omni">${svg(I.search)}<span>Поиск файлов, символов и команд</span><span class="k"><kbd>Ctrl</kbd><kbd>P</kbd></span></div>
    <div class="tb-r"><div class="avatars"></div><div class="share">${svg(I.users)}Поделиться</div><div class="winctl"><span>${svg(I.min)}</span><span>${svg(I.max)}</span><span>${svg(I.x)}</span></div></div>
  </div>
  <div class="body">
    <div class="sidebar">
      <div class="sb-tabs"><span class="sb-tab on">${svg(I.files)}</span><span class="sb-tab">${svg(I.search)}</span><span class="sb-tab">${svg(I.branch)}<span class="badge">3</span></span><span class="sb-tab">${svg(I.blocks)}</span><span class="sb-tab" style="margin-left:auto">${svg(I.dots)}</span></div>
      <div class="sb-title"><span>Проводник</span><span>anvil</span></div>
      <div class="tree">${TREE.map(r => `<div class="row${r.dir ? ' dir' : ''}" style="padding-left:${10 + r.d * 12}px">${r.dir ? svg(r.open ? I.chevD : I.chevR, 'class="chev"') : `<span class="fi" style="background:${ficol(r.n)}"></span>`}<span>${r.n}</span>${r.g ? `<span class="git" style="color:${r.g === 'A' ? 'var(--ok)' : '#e5b45a'}">${r.g}</span>` : ''}</div>`).join('')}</div>
      <div class="sb-foot"><i></i><b>backend</b><span>· локально</span><span class="mem">12 МБ</span></div>
    </div>
    <div class="eg"><div class="pane"></div><div class="pane second"></div></div>
    <div class="panel"></div>
  </div>
  <div class="status"></div>
  <div class="scrim"></div>
  <div class="palette"></div>
  <div class="toasts"></div>
  <div class="keys"></div>
</div>
<div class="caption"><div class="cap-l">${LOGO('lg2')}<b>Anvil</b><span class="cap-d"></span><span class="cap-n"></span><span class="cap-t"></span></div><div class="cap-r"></div></div>
<span class="meas" style="position:absolute;left:-99999px;top:0;visibility:hidden;white-space:pre;font-family:var(--mono)">${'0'.repeat(200)}</span>`;
    R.win = $('.win', stage); R.title = $('.titlebar', stage); R.live = $('.live', stage); R.avatars = $('.avatars', stage);
    R.sb = $('.sidebar', stage); R.rows = Array.from(stage.querySelectorAll('.row'));
    R.eg = $('.eg', stage); R.panel = $('.panel', stage); R.status = $('.status', stage);
    R.scrim = $('.scrim', stage); R.pal = $('.palette', stage); R.toasts = $('.toasts', stage); R.keys = $('.keys', stage);
    R.capN = $('.cap-n', stage); R.capT = $('.cap-t', stage); R.capR = $('.cap-r', stage);
    measEl = $('.meas', stage);
    const panes = stage.querySelectorAll('.pane');
    R.P = [makePane(panes[0]), makePane(panes[1])];
  }
  function makePane(el) {
    el.innerHTML = `<div class="tabs"></div><div class="crumbs"></div><div class="view"><div class="gutter"><div class="scr gs"></div></div><div class="code"><div class="scr ob"></div><div class="scr ls"></div><div class="scr of"></div></div><canvas class="mm"></canvas><div class="cmp" style="display:none"></div></div>`;
    return {
      el, tabs: $('.tabs', el), crumbs: $('.crumbs', el), view: $('.view', el), gs: $('.gs', el), code: $('.code', el),
      ob: $('.ob', el), ls: $('.ls', el), of: $('.of', el), mm: $('.mm', el), cmp: $('.cmp', el), lineEls: [],
    };
  }

  // ============================================================ метрики шрифта
  const lhOf = fs => Math.round(fs * 1.62 * 100) / 100;
  const cwCache = new Map();
  function chw(fs) {
    const k = Math.round(fs * 100);
    let w = cwCache.get(k);
    if (w) return w;
    measEl.style.fontSize = k / 100 + 'px';
    w = measEl.offsetWidth / 200;
    if (cwCache.size > 400) cwCache.clear();
    cwCache.set(k, w);
    return w;
  }

  // ============================================================ применение темы
  const rootStyle = document.documentElement.style;
  const varCache = {};
  const setVar = (k, v) => { if (varCache[k] !== v) { rootStyle.setProperty(k, v); varCache[k] = v; } };
  function applyLook(L) {
    const T = L.th, A = L.accent;
    for (const k of COLOR_KEYS) if (!SYN.includes(k)) setVar('--' + k, rgba(T[k]));
    for (const k of SYN) setVar('--t-' + k, rgba(T[k]));
    setVar('--accent', rgba(A));
    setVar('--accent2', rgba(mixc(A, [255, 255, 255, 1], 0.35)));
    setVar('--accent-soft', rgba(alpha(A, T.dark ? 0.13 : 0.12)));
    setVar('--accent-sel', rgba(alpha(A, T.dark ? 0.26 : 0.22)));
    setVar('--accent-glow', rgba(alpha(A, 0.55)));
    setVar('--glow', rgba(alpha(A, T.dark ? 0.17 : 0.2)));
    setVar('--ok-soft', rgba(alpha(T.ok, 0.18)));
    setVar('--on-accent', onColor(A));
    setVar('--cap', rgba(T.text));
    setVar('--cap2', rgba(T.text2));
    setVar('--r', L.radius.toFixed(2) + 'px');
    setVar('--r-win', (L.radius * 1.2 + 2).toFixed(2) + 'px');
    setVar('--fs', L.fs.toFixed(2) + 'px');
    setVar('--lh', lhOf(L.fs) + 'px');
    setVar('--tabw', (chw(L.fs) * 4).toFixed(3) + 'px');
    setVar('--row', lerp(20, 24, L.density).toFixed(2) + 'px');
  }

  // ============================================================ рендер редактора
  function indentLevels(lines) {
    const n = lines.length, lv = new Array(n).fill(0);
    for (let i = 0; i < n; i++) {
      const s = lines[i].s;
      lv[i] = s.trim() ? Math.floor((s.length - s.trimStart().length) / 4) : -1;
    }
    for (let i = n - 1; i >= 0; i--) if (lv[i] < 0) lv[i] = i + 1 < n ? Math.max(0, lv[i + 1]) : 0;
    return lv;
  }
  const markColor = (m, colors) => (m === 'mod' ? 'var(--t-fn)' : m === 'add' ? 'var(--ok)' : (colors && colors[m]) || 'var(--text3)');
  function drawMinimap(cv, doc, scroll, visLines, L) {
    const ctx = cv.getContext('2d');
    ctx.setTransform(2, 0, 0, 2, 0, 0);
    ctx.clearRect(0, 0, 64, cv.height / 2);
    const T = L.th, step = 3.4, cwm = 1.25;
    ctx.fillStyle = rgba(alpha(T.text, T.dark ? 0.055 : 0.07));
    ctx.fillRect(0, 6 + scroll * step - 2, 64, visLines * step + 2);
    doc.lines.forEach((ln, i) => {
      const y = 6 + i * step;
      let x = 8;
      for (const tk of tokens(doc.lang, ln.s)) {
        const w = tk.t.length * cwm;
        if (tk.c !== 'ws') {
          ctx.fillStyle = rgba(alpha(T[tk.c === 'sf' ? 'kw' : tk.c] || T.var, 0.72));
          ctx.fillRect(x, y, Math.max(0, Math.min(w, 60 - x)), 2);
        }
        x += w;
        if (x > 60) break;
      }
    });
  }
  function renderPane(P, st, t, L, w, h) {
    P.el.style.width = Math.max(0, w) + 'px';
    const fs = L.fs, lh = lhOf(fs), cw = chw(fs);
    const mmOn = st.minimap === false ? 0 : L.minimap;
    const mmW = 64 * mmOn;
    // вкладки
    const tk = st.tabs.map(x => `${x.n}${x.on ? '*' : ''}${x.dirty ? '~' : ''}`).join('|');
    if (P.tk !== tk) {
      P.tk = tk;
      P.tabs.innerHTML = st.tabs.map(x => `<div class="tab${x.on ? ' on' : ''}">${x.on ? '<span class="ind"></span>' : ''}<span class="fi" style="background:${ficol(x.n)}"></span><span>${esc(x.n)}</span>${x.dirty ? '<span class="dot"></span>' : svg(I.x, 'class="x"')}</div>`).join('') + `<div class="tools">${svg(I.split)}${svg(I.dots)}</div>`;
      P.ind = $('.ind', P.tabs);
      P.tabEls = Array.from(P.tabs.querySelectorAll('.tab'));
    }
    const tabsK = st.tabsK == null ? 1 : st.tabsK;
    P.tabEls.forEach((e, i) => {
      const k = st.tabsK == null ? 1 : clamp(tabsK * 1.6 - i * 0.3);
      e.style.opacity = k; e.style.transform = k < 1 ? `translateY(${(1 - k) * 6}px)` : '';
    });
    if (P.ind) P.ind.style.transform = `scaleX(${st.indK == null ? 1 : st.indK})`;
    const ck = st.crumbs.join('/');
    if (P.ck !== ck) {
      P.ck = ck;
      P.crumbs.innerHTML = st.crumbs.map((c, i) => `<span class="${i === st.crumbs.length - 1 ? 'last' : ''}">${esc(c)}</span>`).join('<span class="sep">›</span>');
    }
    P.crumbs.style.opacity = st.tabsK == null ? 1 : tabsK;
    P.code.style.right = mmW + 'px';
    const viewH = h - 60;
    // строки
    const doc = st.doc;
    const sig = doc.lang + '|' + fs.toFixed(3) + '|' + doc.lines.map(l => l.s).join('\n');
    if (P.sig !== sig) {
      P.sig = sig;
      const ind = indentLevels(doc.lines), inl = inlaysFor(doc);
      let html = '';
      doc.lines.forEach((ln, i) => {
        html += `<div class="ln" style="top:${(i * lh).toFixed(2)}px">${ind[i] ? `<span class="ig" style="width:${(ind[i] * 4 * cw).toFixed(2)}px"></span>` : ''}${lineHTML(doc.lang, ln.s, inl[i])}</div>`;
      });
      P.ls.innerHTML = html;
      P.lineEls = Array.from(P.ls.children);
    }
    if (st.codeT != null) {
      P.lineEls.forEach((e, i) => { const k = eo(st.codeT, i * 0.022, 0.32); e.style.opacity = k; e.style.transform = `translateY(${(1 - k) * 6}px)`; });
      P.stag = true;
    } else if (P.stag) {
      P.lineEls.forEach(e => { e.style.opacity = ''; e.style.transform = ''; });
      P.stag = false;
    }
    // гуттер
    const curL = st.curLine == null ? -1 : st.curLine;
    const gk = sig + '|' + curL + '|' + doc.lines.map(l => l.m || '').join(',');
    if (P.gk !== gk) {
      P.gk = gk;
      let g = '';
      doc.lines.forEach((ln, i) => {
        g += `<div class="num${i === curL ? ' cur' : ''}" style="top:${(i * lh).toFixed(2)}px">${i + 1}</div>`;
        if (ln.m) g += `<div class="gmark" style="top:${(i * lh + 3).toFixed(2)}px;height:${(lh - 6).toFixed(2)}px;background:${markColor(ln.m, st.markColors)}"></div>`;
        if (doc.lang === 'rust' && /\{\s*$/.test(ln.s)) g += `<div class="fold" style="top:${(i * lh).toFixed(2)}px">${svg(I.chevD)}</div>`;
      });
      P.gs.innerHTML = g;
    }
    P.gs.style.opacity = st.gutterK == null ? 1 : st.gutterK;
    const tr = `translateY(${(-st.scroll * lh).toFixed(2)}px)`;
    P.ls.style.transform = tr; P.gs.style.transform = tr; P.ob.style.transform = tr; P.of.style.transform = tr;
    // выделения, курсоры
    let ob = '', of = '';
    const acts = st.actors || [];
    const meA = acts.find(a => a.id === 'me');
    if (st.active !== false && meA) ob += `<div class="curline" style="top:${(meA.l * lh).toFixed(2)}px;opacity:${meA.o.toFixed(3)}"></div>`;
    for (const a of acts) {
      if (!a.sel) continue;
      const s = a.sel, k = eo(t, s.t, 0.12);
      ob += `<div class="sel" style="left:${(12 + s.c1 * cw).toFixed(2)}px;top:${(s.l * lh).toFixed(2)}px;width:${((s.c2 - s.c1) * cw * k).toFixed(2)}px;background:${a.me ? 'var(--accent-sel)' : rgba(alpha(a.color, 0.28))};opacity:${a.o.toFixed(3)}"></div>`;
    }
    for (const a of acts) {
      if (a.me && st.active === false) continue;
      const x = 12 + a.c * cw, y = a.l * lh;
      if (a.me) {
        const o = a.o * blinkO(t - a.since);
        let geo;
        if (L.cursor === 'block') geo = `left:${x.toFixed(2)}px;top:${(y + lh * 0.12).toFixed(2)}px;width:${cw.toFixed(2)}px;height:${(lh * 0.76).toFixed(2)}px;opacity:${(o * 0.75).toFixed(3)}`;
        else if (L.cursor === 'underline') geo = `left:${x.toFixed(2)}px;top:${(y + lh * 0.84).toFixed(2)}px;width:${cw.toFixed(2)}px;height:2px;opacity:${o.toFixed(3)}`;
        else geo = `left:${(x - 1).toFixed(2)}px;top:${(y + lh * 0.12).toFixed(2)}px;width:2px;height:${(lh * 0.76).toFixed(2)}px;opacity:${o.toFixed(3)}`;
        of += `<div class="caret me" style="${geo}"></div>`;
      } else {
        of += `<div class="caret" style="left:${(x - 1).toFixed(2)}px;top:${(y + lh * 0.12).toFixed(2)}px;width:2px;height:${(lh * 0.76).toFixed(2)}px;opacity:${a.o.toFixed(3)};background:${a.color};box-shadow:0 0 10px ${rgba(alpha(a.color, 0.5))}"></div>`;
        of += `<div class="flag" style="left:${(x - 1).toFixed(2)}px;top:${(y + lh * 0.12).toFixed(2)}px;background:${a.color};opacity:${a.o.toFixed(3)}">${esc(a.name)}</div>`;
      }
    }
    if (P.obh !== ob) { P.ob.innerHTML = ob; P.obh = ob; }
    if (P.ofh !== of) { P.of.innerHTML = of; P.ofh = of; }
    // автодополнение
    const cm = st.comp;
    if (cm && cm.k > 0.001 && cm.list.length) {
      const key = cm.q + '|' + cm.list.map(x => x.it.n).join(',') + '|' + cm.sel;
      if (P.cmk !== key) {
        P.cmk = key;
        const cur = cm.list[cm.sel];
        P.cmp.innerHTML = `<div class="cmp-list">${cm.list.map((x, i) => `<div class="cmp-item${i === cm.sel ? ' on' : ''}"><span class="cmp-ic" style="color:var(--t-fn);background:color-mix(in srgb, var(--t-fn) 16%, transparent)">ƒ</span><span>${hlIdx(x.it.n, x.f.idx)}</span><span class="det">${esc(x.it.d)}</span></div>`).join('')}</div>${cur && cur.it.doc ? `<div class="cmp-doc"><span>${esc(cur.it.doc)}</span><span>ropey 1.6</span></div>` : ''}`;
      }
      P.cmp.style.display = 'block';
      P.cmp.style.left = (64 + cm.c0 * cw - 36).toFixed(2) + 'px';
      const popH = cm.list.length * 24 + 8 + (cm.list[cm.sel] && cm.list[cm.sel].it.doc ? 31 : 0);
      const below = (cm.l + 1 - st.scroll) * lh + 3;
      const flip = below + popH > viewH - 4;
      P.cmp.style.top = (flip ? (cm.l - st.scroll) * lh - popH - 3 : below).toFixed(2) + 'px';
      P.cmp.style.transformOrigin = flip ? '0 100%' : '0 0';
      P.cmp.style.opacity = cm.k;
      P.cmp.style.transform = `translateY(${((1 - cm.k) * -4).toFixed(2)}px) scale(${(0.97 + 0.03 * cm.k).toFixed(4)})`;
    } else P.cmp.style.display = 'none';
    // миникарта
    if (mmW > 0.5) {
      P.mm.style.display = 'block';
      P.mm.style.opacity = mmOn * (st.mmK == null ? 1 : st.mmK);
      const H = Math.max(1, Math.round(viewH));
      if (P.mmH !== H) { P.mmH = H; P.mm.width = 128; P.mm.height = H * 2; P.mm.style.width = '64px'; P.mm.style.height = H + 'px'; }
      const mk = sig + '|' + st.scroll.toFixed(3) + '|' + rgba(L.th.text) + rgba(L.th.kw) + rgba(L.th.fn) + rgba(L.th.str) + '|' + H;
      if (P.mmk !== mk) { P.mmk = mk; drawMinimap(P.mm, doc, st.scroll, viewH / lh, L); }
    } else P.mm.style.display = 'none';
  }

  // ============================================================ рендер кадра
  function statusHTML(v) {
    const st = v.status || {};
    const msg = v.msg && v.msg.k > 0.01 ? `<span class="msg" style="opacity:${v.msg.k.toFixed(3)}"><i></i>${esc(v.msg.text)}</span>` : '';
    return `<span class="it">${svg(I.branch)}main</span><span class="it">${svg(I.sync)}1↑ 0↓</span><span class="it">${svg(I.err)}0</span><span class="it">${svg(I.warn)}2</span>${msg}<span class="sp"></span>`
      + (st.collab ? `<span class="it">${svg(I.lock)}TLS</span><span class="it">${svg(I.users)}${st.collab} в сессии</span>` : '')
      + (st.pos ? `<span class="it">Стр ${st.pos[0]}, Кол ${st.pos[1]}</span>` : '')
      + `<span class="it">${st.lang || 'Rust'}</span><span class="it">UTF-8</span><span class="it">${svg(I.check)}${st.lsp || 'rust-analyzer'}</span><span class="it acc">● 120 fps</span>`;
  }
  function termHTML(term) {
    let s = `<div class="ph"><span class="on">Терминал</span><span>Проблемы</span><span>Вывод</span><span>Отладка</span><span class="sp">bash — anvil</span></div><div class="term">`;
    s += term.lines.join('\n');
    if (term.cursor) s += `<span class="tcur" style="opacity:${term.cursorO.toFixed(3)}"></span>`;
    return s + '</div>';
  }
  const cache = {};
  const setHTML = (key, el, html) => { if (cache[key] !== html) { el.innerHTML = html; cache[key] = html; } };

  function renderAll(v, t) {
    const L = v.look;
    applyLook(L);
    const wo = v.winK * (v.winO == null ? 1 : v.winO);
    R.win.style.opacity = wo.toFixed(4);
    R.win.style.transform = v.winK < 1 ? `translateY(${((1 - v.winK) * 14).toFixed(2)}px) scale(${(0.975 + 0.025 * v.winK).toFixed(4)})` : '';
    R.title.style.setProperty('--tk', v.titleK == null ? 1 : v.titleK);
    R.status.style.setProperty('--sk', v.statusK == null ? 1 : v.statusK);
    // раскладка
    const taken = SBW * v.sbK, side = L.side;
    const sbX = lerp(-SBW * (1 - v.sbK), BW - taken, side);
    R.sb.style.transform = `translateX(${sbX.toFixed(2)}px)`;
    R.sb.classList.toggle('right', side > 0.5);
    R.sb.classList.toggle('moving', side > 0.01 && side < 0.99);
    const egX = lerp(taken, 0, side), egW = BW - taken;
    const ph = PANEL_H * v.panelK, egH = BH - ph;
    R.eg.style.left = egX.toFixed(2) + 'px'; R.eg.style.width = egW.toFixed(2) + 'px'; R.eg.style.height = egH.toFixed(2) + 'px';
    const s = v.splitK;
    const p0W = s > 0.001 && v.panes[1] ? Math.round(egW * (1 - 0.5 * s)) : egW;
    R.P[0].el.style.left = '0px';
    renderPane(R.P[0], v.panes[0], t, L, p0W, egH);
    if (s > 0.001 && v.panes[1]) {
      R.P[1].el.style.display = 'block';
      R.P[1].el.style.left = p0W + 'px';
      renderPane(R.P[1], v.panes[1], t, L, egW - p0W, egH);
    } else R.P[1].el.style.display = 'none';
    // дерево файлов
    if (R.activeRow !== v.activeRow) { R.rows.forEach((r, i) => r.classList.toggle('active', i === v.activeRow)); R.activeRow = v.activeRow; }
    if (v.treeT != null) {
      R.rows.forEach((r, i) => { const k = eo(v.treeT, i * 0.03, 0.3); r.style.opacity = k; r.style.transform = `translateX(${((1 - k) * -8).toFixed(2)}px)`; });
      R.treeAnim = true;
    } else if (R.treeAnim) { R.rows.forEach(r => { r.style.opacity = ''; r.style.transform = ''; }); R.treeAnim = false; }
    // нижняя панель
    if (ph > 0.5 && v.term) {
      R.panel.style.display = 'block';
      R.panel.style.left = egX.toFixed(2) + 'px'; R.panel.style.width = egW.toFixed(2) + 'px';
      R.panel.style.top = egH.toFixed(2) + 'px'; R.panel.style.height = ph.toFixed(2) + 'px';
      setHTML('term', R.panel, termHTML(v.term));
    } else R.panel.style.display = 'none';
    // заголовок
    const avs = v.avatars.filter(a => a.k > 0.001);
    setHTML('av', R.avatars, avs.map(a => `<div class="av" style="background:${a.c};${a.me ? 'color:var(--on-accent);' : ''}transform:scale(${(0.6 + 0.4 * a.k).toFixed(3)});opacity:${a.k.toFixed(3)}">${a.i}</div>`).join(''));
    setHTML('live', R.live, v.live ? `<i></i>В эфире · ${v.live}` : '');
    setHTML('status', R.status, statusHTML(v));
    // палитра
    const p = v.palette;
    if (p && p.k > 0.001) {
      R.pal.style.display = 'block';
      R.pal.style.opacity = p.k.toFixed(4);
      R.pal.style.transform = `translateY(${((1 - p.k) * -10).toFixed(2)}px) scale(${(0.98 + 0.02 * p.k).toFixed(4)})`;
      R.scrim.style.opacity = p.k.toFixed(4);
      const input = p.mode === 'theme'
        ? `<div class="p-in">${svg(I.search)}<span class="chip">Тема</span><span class="ph2">Выберите цветовую тему — предпросмотр сразу</span></div>`
        : `<div class="p-in">${svg(I.search)}<span>${esc(p.q)}</span><span class="p-car" style="opacity:${p.caretO.toFixed(2)}"></span>${p.q ? '' : '<span class="ph2">Введите команду или имя файла…</span>'}</div>`;
      setHTML('pal', R.pal, input + `<div class="p-list" style="height:${p.listH.toFixed(2)}px"><div class="p-selbg" style="top:${(6 + p.selF * 34).toFixed(2)}px"></div><div style="opacity:${p.listK.toFixed(3)}">${p.items.map(x => x.html).join('')}</div></div><div class="p-foot"><span><kbd>↑</kbd><kbd>↓</kbd> выбрать</span><span><kbd>Enter</kbd> применить</span><span><kbd>Esc</kbd> закрыть</span></div>`);
    } else { R.pal.style.display = 'none'; R.scrim.style.opacity = 0; }
    // тосты, клавиши
    setHTML('toasts', R.toasts, (v.toasts || []).filter(x => x.k > 0.001).map(x => `<div class="toast" style="opacity:${x.k.toFixed(3)};transform:translateX(${((1 - x.k) * 24).toFixed(2)}px)"><div class="av" style="background:${x.c}">${x.i}</div><div>${esc(x.title)}<small>${esc(x.sub)}</small></div></div>`).join(''));
    if (v.keys && v.keys.k > 0.001) {
      const k = v.keys.k;
      R.keys.style.display = 'flex';
      R.keys.style.opacity = k.toFixed(3);
      R.keys.style.transform = `translateX(-50%) translateY(${((1 - k) * 8).toFixed(2)}px)`;
      setHTML('keys', R.keys, v.keys.keys.map(x => `<kbd>${esc(x)}</kbd>`).join(''));
    } else R.keys.style.display = 'none';
    // подпись
    const c = v.caption || {};
    if (cache.cap !== c.id) { cache.cap = c.id; R.capN.textContent = c.n || ''; R.capT.textContent = c.title || ''; R.capR.textContent = c.sub || ''; }
  }

  // ============================================================ сцены
  function baseView(S) {
    return {
      look: lookFrom(S), winK: 1, winO: 1, sbK: 1, treeT: null, panelK: 0, term: null, splitK: 0, panes: [],
      palette: null, keys: null, toasts: [], avatars: [{ i: 'Я', c: 'var(--accent)', k: 1, me: true }], live: 0,
      msg: null, status: { pos: [1, 1] }, statusK: 1, titleK: 1, activeRow: 3,
    };
  }
  const fio = (t, dur) => eo(t, 0, 0.3) * (1 - eo(t, dur - 0.4, 0.38));
  function keysAt(t, list) {
    let best = null;
    for (const [ta, keys, hold = 0.45] of list) {
      const k = pulse(t, ta, 0.1, hold, 0.2);
      if (k > 0.001 && (!best || k > best.k)) best = { keys, k };
    }
    return best;
  }
  function track(t, v0, steps) { // steps: [время, значение, длительность]
    let v = v0;
    for (const [ta, val, d] of steps) v = lerp(v, val, eio(t, ta, d));
    return v;
  }
  const posOf = a => (a ? [Math.round(a.L != null ? a.L : a.l) + 1, Math.round(a.C != null ? a.C : a.c) + 1] : [1, 1]);
  const fullTerm = t => ({ lines: [TERM_PROMPT + 'cargo run --release', ...TERM_OUT, TERM_PROMPT], cursor: true, cursorO: blinkO(t) });

  // 01 — запуск
  function sceneLaunch(S) {
    const doc = bufferDoc(), A0 = { me: { l: 12, c: 8, me: true } };
    const dur = 6.4, cmd = 'cargo run --release', tCmd = 2.45, cps = 17;
    const tOut = tCmd + cmd.length / cps + 0.3;
    return {
      dur,
      at(t) {
        const v = baseView(S);
        v.winK = eo(t, 0, 0.5); v.winO = 1 - eo(t, dur - 0.4, 0.38);
        v.titleK = eo(t, 0.12, 0.4); v.sbK = eo(t, 0.15, 0.55); v.treeT = t < 2 ? t - 0.32 : null; v.statusK = eo(t, 0.7, 0.4);
        const { doc: d, A } = replay(doc, A0, [], t);
        const acts = actorsView(A, t);
        acts.forEach(a => { a.o = eo(t, 1.0, 0.2); a.since = 1.0; });
        v.panes = [{
          doc: d, actors: acts, scroll: 0, tabs: BUF_TABS(), crumbs: BUF_CRUMBS, curLine: 12,
          tabsK: eo(t, 0.48, 0.3), indK: eo(t, 0.62, 0.35), codeT: t < 2.4 ? t - 0.62 : null, gutterK: eo(t, 0.62, 0.4), mmK: eo(t, 0.9, 0.4),
        }];
        v.panelK = eio(t, 1.85, 0.5);
        const n = Math.floor(clamp((t - tCmd) * cps, 0, cmd.length));
        const lines = [TERM_PROMPT + esc(cmd.slice(0, n))];
        TERM_OUT.forEach((ln, i) => { if (t >= tOut + i * 0.2) lines.push(ln); });
        const done = t >= tOut + TERM_OUT.length * 0.2 + 0.15;
        if (done) lines.push(TERM_PROMPT);
        v.term = { lines, cursor: n >= cmd.length ? done : true, cursorO: n < cmd.length ? 1 : blinkO(t - (tOut + 1.15)) };
        v.keys = keysAt(t, [[1.6, ['Ctrl', '`']]]);
        v.msg = { text: 'Готово · 182 мс', k: pulse(t, 1.05, 0.2, 2.4, 0.4) };
        v.status.pos = [13, 9];
        return v;
      },
    };
  }

  // 02 — редактирование, LSP, мультикурсоры
  function sceneEditing(S) {
    const doc0 = bufferDoc();
    const occ = [];
    doc0.lines.forEach((ln, i) => { const c = ln.s.indexOf('version'); if (c >= 0) occ.push({ l: i, c }); });
    const A0 = {
      me: { l: 13, c: 5, me: true },
      me1: { l: occ[1].l, c: occ[1].c, me: true, vis: false },
      me2: { l: occ[2].l, c: occ[2].c, me: true, vis: false },
    };
    const ops = [{ t: 0.6, who: 'me', k: 'move', l: 30, c: 5 }];
    const r1 = typeOps('me', '\n\n    pub fn line_count(&self) -> usize {\n        self.text.', 1.25, 17, 3);
    ops.push(...r1.ops);
    const tDot = r1.last;
    const r2 = typeOps('me', 'len_l', tDot + 0.4, 12, 5);
    ops.push(...r2.ops);
    const tTab = r2.last + 0.5;
    ops.push({ t: tTab, who: 'me', k: 'type', s: 'ines()' });
    const tMC = tTab + 0.95;
    ops.push({ t: tMC, who: 'me', k: 'move', l: occ[0].l, c: occ[0].c + 3 });
    const tD1 = tMC + 0.6, tD2 = tD1 + 0.45, tD3 = tD2 + 0.45;
    ops.push({ t: tD1, who: 'me', k: 'sel', l: occ[0].l, c1: occ[0].c, c2: occ[0].c + 7 });
    ops.push({ t: tD2, who: 'me1', k: 'show' }, { t: tD2, who: 'me1', k: 'sel', l: occ[1].l, c1: occ[1].c, c2: occ[1].c + 7 });
    ops.push({ t: tD3, who: 'me2', k: 'show' }, { t: tD3, who: 'me2', k: 'sel', l: occ[2].l, c1: occ[2].c, c2: occ[2].c + 7 });
    const r3 = typeOps('me', 'revision', tD3 + 0.6, 13, 9);
    for (const o of r3.ops) for (const w of ['me', 'me1', 'me2']) ops.push({ ...o, who: w });
    const tEsc = r3.last + 0.65;
    ops.push({ t: tEsc, who: 'me1', k: 'hide' }, { t: tEsc, who: 'me2', k: 'hide' });
    byTime(ops);
    const dur = tEsc + 1.4;
    return {
      dur,
      at(t) {
        const v = baseView(S);
        v.winO = fio(t, dur);
        const { doc, A } = replay(doc0, A0, ops, t);
        const scroll = track(t, 0, [[0.6, 16, 0.6], [tMC - 0.05, 0, 0.6], [tD3 - 0.1, 2, 0.45]]);
        let comp = null;
        if (t >= tDot && t < tTab + 0.08) {
          const me = A.me, line = doc.lines[me.l].s;
          const c0 = line.lastIndexOf('.', me.c - 1) + 1, q = line.slice(c0, me.c);
          let list = RUST_COMP.map(it => ({ it, f: fuzzy(q, it.n) })).filter(x => x.f);
          list.sort((a, b) => b.f.score - a.f.score);
          list = list.slice(0, 6);
          comp = { l: me.l, c0, q, list, sel: 0, k: eo(t, tDot, 0.14) * (t >= tTab ? 1 - clamp((t - tTab) / 0.08) : 1) };
        }
        v.panes = [{ doc, actors: actorsView(A, t), scroll, tabs: [{ n: 'buffer.rs', on: true, dirty: t >= 1.25 }, { n: 'history.rs' }, { n: 'Cargo.toml' }], crumbs: BUF_CRUMBS, curLine: A.me.l, comp }];
        v.keys = keysAt(t, [[0.5, ['Ctrl', 'End'], 0.35], [tTab - 0.06, ['Tab'], 0.35], [tD1 - 0.06, ['Ctrl', 'D'], 0.25], [tD2 - 0.06, ['Ctrl', 'D'], 0.25], [tD3 - 0.06, ['Ctrl', 'D'], 0.35], [tEsc - 0.06, ['Esc'], 0.3]]);
        v.msg = { text: 'rust-analyzer · 0 ошибок', k: pulse(t, tTab + 0.25, 0.2, 1.0, 0.3) };
        v.status.pos = posOf(A.me);
        return v;
      },
    };
  }

  // 03 — палитра команд и смена темы
  function cmdItems(q, sel) {
    let list = COMMANDS.map(c => ({ c, f: fuzzy(q, `${c[0]}: ${c[1]}`) })).filter(x => x.f);
    if (q) list.sort((a, b) => b.f.score - a.f.score);
    list = list.slice(0, 7);
    return list.map((x, i) => {
      const [cat, name, keys, ic] = x.c, full = `${cat}: ${name}`, set = new Set(x.f.idx);
      let a = '', b = '';
      for (let j = 0; j < full.length; j++) {
        const h = set.has(j) ? `<span class="hl">${esc(full[j])}</span>` : esc(full[j]);
        if (j < cat.length + 1) a += h; else b += h;
      }
      return { html: `<div class="p-item${i === sel ? ' on' : ''}">${svg(I[ic] || I.dots)}<span><span class="cat">${a}</span>${b}</span>${keys ? `<span class="k">${keys.split(' ').map(k => `<kbd>${esc(k)}</kbd>`).join('')}</span>` : ''}</div>` };
    });
  }
  function themeItems(sel, cur) {
    return THEME_ORDER.map((id, i) => {
      const T = THEMES[id];
      return { html: `<div class="p-item${i === sel ? ' on' : ''}"><span class="sw"><span style="background:${T.bg}"></span><span style="background:${T.bg3}"></span><span style="background:${T.kw}"></span><span style="background:${T.fn}"></span></span><span>${T.title}</span>${id === cur ? '<span class="cur">текущая</span>' : ''}</div>` };
    });
  }
  function scenePalette(S) {
    const dur = 7.6, doc0 = bufferDoc(), A0 = { me: { l: 16, c: 11, me: true } };
    const q = 'тема', qT = [1.02, 1.16, 1.31, 1.47];
    const tOpen = 0.62, tEnter = 2.0, downs = [2.6, 3.35, 4.1, 4.85], tPick = 5.65;
    const N = THEME_ORDER.length, si = Math.max(0, THEME_ORDER.indexOf(S.theme));
    const seq = downs.map((_, i) => THEME_ORDER[(si + i + 1) % N]);
    const listH = n => n * 34 + 12;
    const cmdCount = n => cmdItems(q.slice(0, n), 0).length;
    return {
      dur,
      at(t) {
        const v = baseView(S);
        v.winO = fio(t, dur);
        const { doc, A } = replay(doc0, A0, [], t);
        v.panes = [{ doc, actors: actorsView(A, t), scroll: 0, tabs: BUF_TABS(), crumbs: BUF_CRUMBS, curLine: 16 }];
        let th = resolveTheme(S.theme);
        downs.forEach((td, i) => { th = mixTheme(th, resolveTheme(seq[i]), eio(t, td, 0.45)); });
        v.look.th = th;
        const k = eo(t, tOpen, 0.22) * (1 - eo(t, tPick, 0.18));
        if (k > 0.001) {
          let p;
          if (t < tEnter) {
            const n = qT.filter(x => t >= x).length;
            const items = cmdItems(q.slice(0, n), 0);
            const lastT = n ? qT[n - 1] : tOpen;
            p = { mode: 'cmd', q: q.slice(0, n), items, selF: 0, listH: lerp(listH(cmdCount(Math.max(0, n - 1))), listH(items.length), eo(t, lastT, 0.16)), listK: 1, caretO: blinkO(t - lastT) };
          } else {
            const nd = downs.filter(x => t >= x).length;
            const cur = (si + nd) % N, prv = nd ? (si + nd - 1) % N : cur;
            const lastD = nd ? downs[nd - 1] : tEnter;
            const selF = prv <= cur ? lerp(prv, cur, eo(t, lastD, 0.16)) : cur;
            p = { mode: 'theme', items: themeItems(cur, S.theme), selF, listH: lerp(listH(cmdCount(q.length)), listH(N), eo(t, tEnter, 0.18)), listK: eo(t, tEnter, 0.16) };
          }
          p.k = k;
          v.palette = p;
        }
        v.keys = keysAt(t, [[0.42, ['Ctrl', 'Shift', 'P'], 0.5], [tEnter - 0.06, ['Enter'], 0.3], ...downs.map(d => [d - 0.06, ['↓'], 0.3]), [tPick - 0.06, ['Enter'], 0.3]]);
        v.msg = { text: `Тема: ${THEMES[seq[seq.length - 1]].title}`, k: pulse(t, tPick + 0.1, 0.2, 1.2, 0.3) };
        v.status.pos = [17, 12];
        return v;
      },
    };
  }

  // 04 — настройки в TOML, применяются на лету
  function sceneCustomize(S) {
    const doc0 = bufferDoc();
    const doc1 = mkDoc('settings.toml', 'toml', settingsToml(S));
    const find = key => {
      const i = doc1.lines.findIndex(l => new RegExp('^\\s*' + key + '\\s*=').test(l.s));
      const s = doc1.lines[i].s;
      let c1 = s.indexOf('=') + 1;
      while (s[c1] === ' ') c1++;
      let c2;
      if (s[c1] === '"') { c1++; c2 = s.indexOf('"', c1); } else { c2 = c1; while (c2 < s.length && /[\w.#]/.test(s[c2])) c2++; }
      return { l: i, c1, c2 };
    };
    const tgt = {
      accent: S.accent.toLowerCase() !== '#8b7bff' ? '#8b7bff' : '#2fc6f6',
      radius: S.radius > 5 ? 3 : 14,
      font_size: S.fontSize <= 15 ? S.fontSize + 2 : S.fontSize - 2,
      sidebar: S.sidebar === 'right' ? 'left' : 'right',
    };
    const ops = [], ch = {};
    let T = 1.25;
    for (const key of ['accent', 'radius', 'font_size', 'sidebar']) {
      const p = find(key);
      ops.push({ t: T, who: 'me', k: 'move', l: p.l, c: p.c2 });
      ops.push({ t: T + 0.32, who: 'me', k: 'sel', l: p.l, c1: p.c1, c2: p.c2 });
      const r = typeOps('me', String(tgt[key]), T + 0.62, 15, key.length);
      ops.push(...r.ops);
      ch[key] = r.last + 0.08;
      T = r.last + 0.62;
    }
    byTime(ops);
    const A0 = { me: { l: 4, c: 0, me: true, vis: false } };
    ops.unshift({ t: 0.9, who: 'me', k: 'show' });
    const dur = T + 1.8;
    const base = lookFrom(S);
    return {
      dur,
      at(t) {
        const v = baseView(S);
        v.winO = fio(t, dur);
        const { doc, A } = replay(doc1, A0, ops, t);
        v.splitK = eio(t, 0.55, 0.5);
        v.panes = [
          { doc: doc0, actors: [], scroll: 0, tabs: BUF_TABS(), crumbs: BUF_CRUMBS, active: false },
          { doc, actors: actorsView(A, t), scroll: 0, tabs: [{ n: 'settings.toml', on: true, dirty: t >= ch.accent - 0.5 }], crumbs: ['~', '.config', 'anvil', 'settings.toml'], curLine: A.me.l, minimap: false },
        ];
        const L = v.look;
        L.accent = mixc(base.accent, pc(tgt.accent), eo(t, ch.accent, 0.45));
        L.radius = lerp(base.radius, tgt.radius, eio(t, ch.radius, 0.4));
        L.fs = lerp(base.fs, tgt.font_size, eio(t, ch.font_size, 0.4));
        L.side = lerp(base.side, tgt.sidebar === 'right' ? 1 : 0, eio(t, ch.sidebar, 0.6));
        v.keys = keysAt(t, [[0.38, ['Ctrl', ','], 0.4]]);
        const mk = Math.max(...Object.values(ch).map(tc => pulse(t, tc, 0.15, 0.85, 0.3)));
        v.msg = { text: 'settings.toml применён на лету', k: mk };
        v.status.pos = posOf(A.me);
        v.status.lang = 'TOML'; v.status.lsp = 'taplo';
        return v;
      },
    };
  }

  // 05 — совместная работа
  function sceneCollab(S) {
    const doc0 = bufferDoc();
    const COL = { alice: '#ff5fa2', boris: '#3cc8ff' };
    const A0 = {
      me: { l: 22, c: 26, me: true },
      alice: { l: 12, c: 8, color: COL.alice, name: 'Алиса', vis: false },
      boris: { l: 5, c: 4, color: COL.boris, name: 'Борис', vis: false },
    };
    const ops = [
      { t: 0.7, who: 'alice', k: 'show' },
      { t: 1.25, who: 'boris', k: 'show' },
      { t: 1.5, who: 'alice', k: 'move', l: 10, c: 13 },
      { t: 1.6, who: 'boris', k: 'move', l: 7, c: 17 },
    ];
    ops.push(...typeOps('boris', '\n    dirty: bool,', 1.95, 12, 11).ops);
    ops.push(...typeOps('alice', '\n    /// Создаёт пустой буфер.', 2.2, 15, 12).ops);
    const rm = typeOps('me', '\n        self.dirty = true;', 3.0, 15, 13);
    ops.push(...rm.ops);
    ops.push({ t: 4.55, who: 'boris', k: 'move', l: 6, c: 11 }, { t: 4.85, who: 'boris', k: 'sel', l: 6, c1: 4, c2: 11 });
    ops.push({ t: 5.0, who: 'alice', k: 'move', l: 12, c: 26 });
    byTime(ops);
    const dur = 7.2;
    return {
      dur,
      at(t) {
        const v = baseView(S);
        v.winO = fio(t, dur);
        const { doc, A } = replay(doc0, A0, ops, t);
        const scroll = track(t, 2, [[2.35, 4, 0.6]]);
        v.panes = [{ doc, actors: actorsView(A, t), scroll, tabs: BUF_TABS(), crumbs: BUF_CRUMBS, curLine: A.me.l, markColors: COL }];
        const ka = eo(t, 0.6, 0.3), kb = eo(t, 1.15, 0.3);
        v.avatars = [{ i: 'Я', c: 'var(--accent)', k: 1, me: true }, { i: 'А', c: COL.alice, k: ka }, { i: 'Б', c: COL.boris, k: kb }];
        v.live = 1 + (t >= 0.6 ? 1 : 0) + (t >= 1.15 ? 1 : 0);
        v.toasts = [
          { i: 'А', c: COL.alice, title: 'Алиса подключилась', sub: 'по приглашению · редактирование', k: pulse(t, 0.5, 0.3, 2.0, 0.35) },
          { i: 'Б', c: COL.boris, title: 'Борис подключился', sub: 'по приглашению · редактирование', k: pulse(t, 1.05, 0.3, 1.9, 0.35) },
        ];
        v.status.collab = v.live;
        v.status.pos = posOf(A.me);
        v.msg = { text: 'CRDT синхронизировано · 12 мс', k: pulse(t, rm.last + 0.2, 0.2, 1.2, 0.3) };
        return v;
      },
    };
  }

  // 00 — свободный режим (для ручной настройки)
  function sceneIdle(S) {
    const doc0 = bufferDoc(), A0 = { me: { l: 20, c: 8, me: true } };
    return {
      dur: Infinity,
      at(t) {
        const v = baseView(S);
        const { doc, A } = replay(doc0, A0, [], t);
        v.panes = [{ doc, actors: actorsView(A, t), scroll: 0, tabs: BUF_TABS(), crumbs: BUF_CRUMBS, curLine: 20 }];
        v.panelK = 1;
        v.term = fullTerm(t);
        v.status.pos = [21, 9];
        return v;
      },
    };
  }

  const SCENES = [
    { id: 'launch', n: '01', short: 'Запуск', title: 'Мгновенный запуск', sub: 'GPU-рендер · старт < 300 мс · 120 FPS', make: sceneLaunch },
    { id: 'editing', n: '02', short: 'Редактор', title: 'Плавный курсор, LSP и мультикурсоры', sub: 'tree-sitter · rust-analyzer', make: sceneEditing },
    { id: 'palette', n: '03', short: 'Палитра', title: 'Командная палитра и живые темы', sub: 'Ctrl+Shift+P · нечёткий поиск', make: scenePalette },
    { id: 'customize', n: '04', short: 'Настройки', title: 'Настраивается всё', sub: 'settings.toml · применяется на лету', make: sceneCustomize },
    { id: 'collab', n: '05', short: 'Совместно', title: 'Совместная работа', sub: 'CRDT · TLS · доступ по приглашению', make: sceneCollab },
    { id: 'idle', n: '00', short: 'Свободно', title: 'Свободный режим', sub: 'настройте всё справа', make: sceneIdle },
  ];
  const AUTO = ['launch', 'editing', 'palette', 'customize', 'collab'];

  // ============================================================ запуск
  const params = new URLSearchParams(location.search);
  const CAPTURE = params.has('capture');
  const loadSettings = () => {
    if (CAPTURE) return { ...DEFAULTS };
    try { return { ...DEFAULTS, ...JSON.parse(localStorage.getItem('anvil.settings') || '{}') }; } catch (e) { return { ...DEFAULTS }; }
  };
  let S = loadSettings();
  let scene = SCENES.find(s => s.id === params.get('scene')) || SCENES[0];
  let built;
  let auto = !CAPTURE && !params.get('scene');
  function setScene(s) { scene = s; built = s.make(S); }
  function frame(t) {
    FX.anim = S.animations; FX.smooth = S.smooth;
    const v = built.at(t);
    v.caption = scene;
    return v;
  }

  buildStage();
  setScene(scene);

  if (CAPTURE) {
    document.body.classList.add('capture');
    window.anvil = {
      ready: false,
      scenes: SCENES.map(s => s.id),
      duration: () => built.dur,
      seek(t) { renderAll(frame(t), t); return true; },
    };
    document.fonts.ready.then(() => { cwCache.clear(); renderAll(frame(0), 0); window.anvil.ready = true; });
    return;
  }

  // ---------- интерактивный режим
  let start = performance.now(), playing = true, pauseT = 0, lastNow = start, curLook = null;
  const now = () => performance.now();
  const sceneTime = () => (playing ? (now() - start) / 1000 : pauseT);
  function restart() { start = now(); pauseT = 0; }
  function loop(ts) {
    const dt = Math.min(0.1, (ts - lastNow) / 1000);
    lastNow = ts;
    let t = playing ? (ts - start) / 1000 : pauseT;
    if (isFinite(built.dur) && t >= built.dur) {
      if (auto) { const i = AUTO.indexOf(scene.id); setScene(SCENES.find(s => s.id === AUTO[(i + 1) % AUTO.length])); syncControls(); }
      start = ts; t = 0;
    }
    const v = frame(t);
    curLook = !curLook || !FX.anim ? v.look : mixLook(curLook, v.look, 1 - Math.exp(-dt / 0.07));
    v.look = curLook;
    renderAll(v, t);
    requestAnimationFrame(loop);
  }
  function fit() {
    const vp = $('#viewport');
    const k = Math.max(0.3, Math.min((vp.clientWidth - 40) / 1040, (vp.clientHeight - 40) / 680, 1.6));
    stage.style.position = 'absolute'; stage.style.left = '50%'; stage.style.top = '50%';
    stage.style.transform = `translate(-50%, -50%) scale(${k})`;
  }
  window.addEventListener('resize', fit);

  function save() { try { localStorage.setItem('anvil.settings', JSON.stringify(S)); } catch (e) { /* приватный режим */ } }
  function update(patch) { S = { ...S, ...patch }; save(); built = scene.make(S); syncControls(); }
  function tomlHTML(text) { return text.split('\n').map(l => lineHTML('toml', l)).join('\n'); }
  function buildControls() {
    const c = $('#controls');
    c.innerHTML = `
<h1>${LOGO('lg3')}Anvil <span style="font-weight:400;color:#6c6c78;font-size:12px">макет интерфейса</span></h1>
<p class="lead">Живой макет. Каждый переключатель ниже — обычный ключ в <b>settings.toml</b>, меняется на лету, без перезапуска.</p>
<h2>Демо-сцены</h2>
<div class="scenes">${SCENES.map(s => `<button data-scene="${s.id}">${s.short}<small>${s.n === '00' ? 'ручная настройка' : s.n + ' · ' + s.title}</small></button>`).join('')}<button data-scene="auto">По очереди<small>все сцены подряд</small></button></div>
<div class="ctl-actions"><button class="ctl-btn" id="pp">Пауза</button><button class="ctl-btn" id="rs">Сначала</button></div>
<h2>Тема</h2><div class="seg" id="themes">${THEME_ORDER.map(id => `<button data-v="${id}">${THEMES[id].title}</button>`).join('')}</div>
<h2>Акцент</h2><div class="swatches" id="acc">${ACCENTS.map(([n, col]) => `<button title="${n}" data-v="${col}" style="background:${col}"></button>`).join('')}<input type="color" id="accPick" title="Свой цвет"></div>
<h2>Форма</h2>
<div class="ctl-row"><label>Скругление</label><input type="range" id="radius" min="0" max="18" step="1"><output id="radiusO"></output></div>
<div class="ctl-row"><label>Размер шрифта</label><input type="range" id="fs" min="11" max="17" step="0.5"><output id="fsO"></output></div>
<div class="ctl-row"><label>Плотность</label><div class="seg" id="density"><button data-v="compact">Компактно</button><button data-v="comfortable">Просторно</button></div></div>
<div class="ctl-row"><label>Боковая панель</label><div class="seg" id="sidebar"><button data-v="left">Слева</button><button data-v="right">Справа</button></div></div>
<div class="ctl-row"><label>Миникарта</label><button class="toggle" id="minimap"></button></div>
<h2>Курсор</h2>
<div class="ctl-row"><div class="seg" id="cursor"><button data-v="line">Линия</button><button data-v="block">Блок</button><button data-v="underline">Подчёркивание</button></div></div>
<div class="ctl-row"><label>Плавное движение</label><button class="toggle" id="smooth"></button></div>
<h2>Анимации</h2>
<div class="ctl-row"><label>Включены</label><button class="toggle" id="animations"></button></div>
<h2>settings.toml</h2>
<div class="tomlbox" id="toml"></div>
<div class="ctl-actions"><button class="ctl-btn" id="copy">Копировать</button><button class="ctl-btn" id="reset">Сбросить всё</button></div>`;
    c.addEventListener('click', e => {
      const b = e.target.closest('button');
      if (!b) return;
      if (b.dataset.scene) {
        if (b.dataset.scene === 'auto') { auto = true; setScene(SCENES[0]); }
        else { auto = false; setScene(SCENES.find(s => s.id === b.dataset.scene)); }
        playing = true; restart(); syncControls(); return;
      }
      const grp = b.parentElement && b.parentElement.id;
      if (grp === 'themes') update({ theme: b.dataset.v });
      else if (grp === 'acc') update({ accent: b.dataset.v });
      else if (grp === 'density') update({ density: b.dataset.v });
      else if (grp === 'sidebar') update({ sidebar: b.dataset.v });
      else if (grp === 'cursor') update({ cursor: b.dataset.v });
      else if (['minimap', 'smooth', 'animations'].includes(b.id)) update({ [b.id]: !S[b.id] });
      else if (b.id === 'pp') { if (playing) { pauseT = sceneTime(); playing = false; } else { playing = true; start = now() - pauseT * 1000; } syncControls(); }
      else if (b.id === 'rs') { playing = true; restart(); syncControls(); }
      else if (b.id === 'copy') { navigator.clipboard && navigator.clipboard.writeText(settingsToml(S)); b.textContent = 'Скопировано'; setTimeout(() => { b.textContent = 'Копировать'; }, 1200); }
      else if (b.id === 'reset') { S = { ...DEFAULTS }; save(); built = scene.make(S); syncControls(); }
    });
    $('#radius').addEventListener('input', e => update({ radius: +e.target.value }));
    $('#fs').addEventListener('input', e => update({ fontSize: +e.target.value }));
    $('#accPick').addEventListener('input', e => update({ accent: e.target.value }));
    window.addEventListener('keydown', e => {
      if (e.target.tagName === 'INPUT') return;
      if (e.code === 'Space') { e.preventDefault(); $('#pp').click(); }
      const n = parseInt(e.key, 10);
      if (n >= 1 && n <= 5) { auto = false; setScene(SCENES[n - 1]); restart(); syncControls(); }
      if (e.key === '0') { auto = false; setScene(SCENES[5]); restart(); syncControls(); }
    });
  }
  function syncControls() {
    const c = $('#controls');
    c.querySelectorAll('[data-scene]').forEach(b => b.classList.toggle('on', auto ? b.dataset.scene === 'auto' : b.dataset.scene === scene.id));
    const seg = (id, val) => c.querySelectorAll(`#${id} button`).forEach(b => b.classList.toggle('on', b.dataset.v === val));
    seg('themes', S.theme); seg('density', S.density); seg('sidebar', S.sidebar); seg('cursor', S.cursor);
    c.querySelectorAll('#acc button').forEach(b => b.classList.toggle('on', b.dataset.v.toLowerCase() === S.accent.toLowerCase()));
    $('#accPick').value = toHex(S.accent);
    $('#radius').value = S.radius; $('#radiusO').textContent = S.radius + 'px';
    $('#fs').value = S.fontSize; $('#fsO').textContent = S.fontSize + 'px';
    ['minimap', 'smooth', 'animations'].forEach(id => $('#' + id).classList.toggle('on', !!S[id]));
    $('#pp').textContent = playing ? 'Пауза' : 'Играть';
    $('#toml').innerHTML = tomlHTML(settingsToml(S));
    rootStyle.setProperty('--ui-accent', S.accent);
  }
  buildControls();
  syncControls();
  fit();
  document.fonts.ready.then(() => { cwCache.clear(); requestAnimationFrame(loop); });
})();

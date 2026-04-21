//! Design tokens + shared HTML shell.
//!
//! Palette mirrors the desktop GUI (`aiem-gui/src/theme.rs`): monochrome
//! minimal with a white-on-black dark mode default and an optional light mode.
//! Tailwind is used only for layout utilities; colors come from CSS variables
//! so light/dark switching is a single class toggle on <html>.

use maud::{html, Markup, PreEscaped, DOCTYPE};

pub const TABS: &[(&str, &str)] = &[
    ("/skills",   "Skills"),
    ("/mcp",      "MCP"),
    ("/secrets",  "Secrets"),
    ("/profiles", "Profiles"),
    ("/projects", "Projects"),
    ("/discover", "Discover"),
    ("/store",    "Store"),
    ("/ides",     "IDEs"),
    ("/settings", "Settings"),
];

pub fn page(title: &str, active: &str, body: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" class="dark" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "aiem · " (title) }
                script src="https://cdn.tailwindcss.com" {}
                script src="https://unpkg.com/htmx.org@1.9.12" {}
                script src="https://unpkg.com/htmx.org@1.9.12/dist/ext/sse.js" {}
                style { (PreEscaped(CSS)) }
            }
            body class="h-screen overflow-hidden"
                 hx-ext="sse" sse-connect="/events" {
                div class="flex h-screen" {
                    aside class="w-56 shrink-0 border-r flex flex-col h-screen sticky top-0" style="border-color:var(--stroke);background:var(--surface);" {
                        div class="px-5 py-4 border-b" style="border-color:var(--stroke);" {
                            div class="text-base font-semibold tracking-tight" { "aiem" }
                            div class="text-xs" style="color:var(--muted);" { "AI Extension Manager" }
                        }
                        nav class="p-2 space-y-0.5 flex-1" {
                            @for (path, label) in TABS {
                                a href=(path) class=(nav_class(active == *path)) { (label) }
                            }
                        }
                        div class="p-3 border-t" style="border-color:var(--stroke);" {
                            button id="theme-toggle" type="button" class="btn-secondary w-full text-xs" { "Toggle theme" }
                        }
                    }
                    main class="flex-1 min-w-0 flex flex-col h-screen" {
                        header class="px-6 py-3 border-b flex items-center justify-between" style="border-color:var(--stroke);background:var(--surface);" {
                            h1 class="text-sm font-semibold" { (title) }
                            div class="flex items-center gap-3" {
                                div id="global-task-indicator" class="text-xs" style="color:var(--muted);" {}
                                div id="connection-dot" class="w-1.5 h-1.5 rounded-full" style="background:var(--muted);" title="SSE" {}
                            }
                        }
                        div id="toasts" class="fixed top-4 right-4 z-50 space-y-2 w-80 pointer-events-none" {}
                        section class="flex-1 p-6 overflow-auto" { (body) }
                    }
                }
                script { (PreEscaped(JS)) }
            }
        }
    }
}

fn nav_class(active: bool) -> &'static str {
    if active { "nav-link nav-link-active" } else { "nav-link" }
}

// ─── Reusable primitives ────────────────────────────────────────────────

pub fn page_header(title: &str, subtitle: &str, actions: Markup) -> Markup {
    html! {
        div class="flex items-start justify-between mb-5 gap-4" {
            div class="min-w-0" {
                h2 class="text-lg font-semibold" { (title) }
                p class="text-xs mt-0.5" style="color:var(--muted);" { (subtitle) }
            }
            div class="flex items-center gap-2 flex-wrap justify-end" { (actions) }
        }
    }
}

pub fn card(inner: Markup) -> Markup {
    html! { div class="aiem-card" { (inner) } }
}

pub fn empty_state(title: &str, sub: &str) -> Markup {
    html! {
        div class="py-16 text-center" {
            div class="text-3xl mb-2" style="color:var(--muted);" { "—" }
            div class="text-sm font-semibold" { (title) }
            div class="text-xs mt-1" style="color:var(--muted);" { (sub) }
        }
    }
}

pub fn btn_primary(label: &str) -> Markup {
    html! { button type="submit" class="btn-primary" { (label) } }
}
pub fn btn_secondary(label: &str) -> Markup {
    html! { button type="submit" class="btn-secondary" { (label) } }
}
pub fn btn_danger(label: &str) -> Markup {
    html! { button type="submit" class="btn-danger" { (label) } }
}

pub enum TagKind { Neutral, Success, Danger }

pub fn tag(label: &str, kind: TagKind) -> Markup {
    let cls = match kind {
        TagKind::Neutral => "tag tag-neutral",
        TagKind::Success => "tag tag-success",
        TagKind::Danger  => "tag tag-danger",
    };
    html! { span class=(cls) { (label) } }
}

// ─── Styles ─────────────────────────────────────────────────────────────

const CSS: &str = r#"
:root {
  --bg:          #F7F7F7;
  --surface:     #FFFFFF;
  --surface-hi:  #F0F0F0;
  --surface-hov: #EAEAEA;
  --stroke:      #DFDFDF;
  --text:        #1A1A1A;
  --muted:       #888888;
  --accent:      #1A1A1A;
  --accent-fg:   #FFFFFF;
  --danger:      #D04444;
  --success:     #1E7A3A;
  --tag-bg:      #EEEEEE;
  --tag-fg:      #555555;
}
html.dark {
  --bg:          #0E0E0E;
  --surface:     #181818;
  --surface-hi:  #1E1E1E;
  --surface-hov: #242424;
  --stroke:      #2E2E2E;
  --text:        #E8E8E8;
  --muted:       #8A8A8A;
  --accent:      #FFFFFF;
  --accent-fg:   #000000;
  --danger:      #CC5555;
  --success:     #4ABB6A;
  --tag-bg:      #242424;
  --tag-fg:      #AEAEAE;
}

html, body { background: var(--bg); color: var(--text); }
body { font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif; font-size: 14px; -webkit-font-smoothing: antialiased; }
* { box-sizing: border-box; }
input, select, textarea { color-scheme: dark light; }
a { color: inherit; text-decoration: none; }
button { font: inherit; }
:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }

.nav-link {
  display: block; padding: 7px 12px; border-radius: 6px; font-size: 13px;
  color: var(--muted);
  transition: background .15s, color .15s;
}
.nav-link:hover { background: var(--surface-hi); color: var(--text); }
.nav-link-active { background: var(--surface-hi); color: var(--text); font-weight: 500; }

.aiem-card {
  border: 1px solid var(--stroke);
  background: var(--surface);
  border-radius: 10px;
  padding: 16px;
  margin-bottom: 14px;
  box-shadow: 0 1px 2px rgba(0,0,0,.05);
}

.field, select.field, textarea.field {
  width: 100%;
  background: var(--surface-hi);
  border: 1px solid var(--stroke);
  color: var(--text);
  border-radius: 6px;
  padding: 6px 10px;
  font-size: 13px;
  line-height: 1.4;
  transition: border-color .15s, background .15s;
}
.field:focus, textarea.field:focus, select.field:focus { outline: none; border-color: var(--text); }
.field::placeholder { color: var(--muted); }
textarea.field { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 12.5px; }

.label { display: block; font-size: 12px; color: var(--muted); margin-bottom: 4px; font-weight: 500; }

.btn-primary, .btn-secondary, .btn-danger, .btn-ghost {
  display: inline-flex; align-items: center; justify-content: center; gap: 6px;
  min-height: 30px; padding: 4px 12px;
  font-size: 12.5px; font-weight: 500;
  border-radius: 6px; border: 1px solid transparent;
  cursor: pointer;
  transition: background .15s, border-color .15s, transform .05s, color .15s;
  white-space: nowrap;
}
.btn-primary:active, .btn-secondary:active, .btn-danger:active, .btn-ghost:active { transform: translateY(1px); }
.btn-primary  { background: var(--accent); color: var(--accent-fg); }
.btn-primary:hover  { filter: brightness(0.9); }
.btn-secondary { background: var(--surface-hi); color: var(--text); border-color: var(--stroke); }
.btn-secondary:hover { background: var(--surface-hov); }
.btn-danger   { background: transparent; color: var(--danger); border-color: var(--stroke); }
.btn-danger:hover   { background: rgba(204,85,85,.08); border-color: var(--danger); }
.btn-ghost    { background: transparent; color: var(--muted); border-color: transparent; min-height: 26px; padding: 2px 8px; font-size: 12px; }
.btn-ghost:hover { color: var(--text); background: var(--surface-hi); }

.tag {
  display: inline-flex; align-items: center;
  padding: 2px 8px; font-size: 11px; font-weight: 500;
  border-radius: 999px; line-height: 1.4;
  margin-right: 4px; margin-bottom: 2px;
  background: var(--tag-bg); color: var(--tag-fg);
}
.tag-success { background: rgba(74,187,106,.12); color: var(--success); }
.tag-danger  { background: rgba(204,85,85,.12); color: var(--danger); }

.group-box { border: 1px solid var(--stroke); background: var(--surface); border-radius: 10px; margin-bottom: 14px; overflow: hidden; }
.group-box > summary { list-style: none; cursor: pointer; padding: 10px 14px; background: var(--surface); font-weight: 600; font-size: 13px; display: flex; align-items: center; gap: 8px; }
.group-box > summary::-webkit-details-marker { display: none; }
.group-box > summary:hover { background: var(--surface-hi); }
.group-box > summary .chev { transition: transform .15s; display: inline-block; color: var(--muted); }
.group-box[open] > summary .chev { transform: rotate(90deg); }
.group-box[open] > summary { border-bottom: 1px solid var(--stroke); }
.group-actions { padding: 10px 14px; background: color-mix(in srgb, var(--surface-hi) 70%, var(--surface)); border-bottom: 1px solid var(--stroke); display: flex; flex-wrap: wrap; gap: 6px; align-items: center; }
.group-body { padding: 10px 14px; }

table.aiem { width: 100%; border-collapse: collapse; }
table.aiem th { text-align: left; font-size: 11px; font-weight: 500; text-transform: uppercase; letter-spacing: .05em; color: var(--muted); padding: 8px 10px; border-bottom: 1px solid var(--stroke); }
table.aiem td { padding: 10px; border-bottom: 1px solid var(--stroke); font-size: 13px; vertical-align: top; }
table.aiem tr:last-child td { border-bottom: 0; }
table.aiem tr:hover td { background: var(--surface-hi); }

.toast { background: var(--surface); border: 1px solid var(--stroke); color: var(--text); padding: 10px 12px; border-radius: 8px; font-size: 13px; box-shadow: 0 4px 16px rgba(0,0,0,.25); pointer-events: auto; opacity: 0; transform: translateY(-4px); transition: opacity .2s, transform .2s; }
.toast.show { opacity: 1; transform: translateY(0); }
.toast.toast-success { border-left: 3px solid var(--success); }
.toast.toast-error   { border-left: 3px solid var(--danger); }
.toast.toast-info    { border-left: 3px solid var(--muted); }
.toast.toast-warn    { border-left: 3px solid #D0A040; }

#connection-dot.live { background: var(--success) !important; box-shadow: 0 0 6px var(--success); }
#connection-dot.dead { background: var(--danger) !important; }
.htmx-indicator { opacity: 0; transition: opacity .2s; }
.htmx-request .htmx-indicator { opacity: 1; }

.meta { font-size: 12px; color: var(--muted); }
.mono { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 12.5px; }
.row-gap { display: flex; flex-wrap: wrap; gap: 6px; align-items: center; }
"#;

const JS: &str = r#"
(function(){
  const KEY='aiem.theme';
  function apply(mode){
    document.documentElement.classList.toggle('dark', mode==='dark');
    try { localStorage.setItem(KEY, mode); } catch(e){}
  }
  try {
    const stored = localStorage.getItem(KEY);
    if (stored) apply(stored);
  } catch(e){}
  document.addEventListener('click', (e)=>{
    if (e.target && e.target.id === 'theme-toggle') {
      const cur = document.documentElement.classList.contains('dark') ? 'dark' : 'light';
      apply(cur === 'dark' ? 'light' : 'dark');
    }
  });
})();

(function(){
  function toast(level, msg) {
    const root = document.getElementById('toasts');
    if (!root) return;
    const el = document.createElement('div');
    el.className = 'toast toast-' + (level || 'info');
    el.textContent = msg;
    root.appendChild(el);
    requestAnimationFrame(()=> el.classList.add('show'));
    setTimeout(()=>{ el.classList.remove('show'); setTimeout(()=>el.remove(), 250); }, 4000);
  }
  window.aiemToast = toast;

  function dot(){ return document.getElementById('connection-dot'); }
  function setTaskLine(s){
    const t = document.getElementById('global-task-indicator');
    if (t) t.textContent = s || '';
  }

  document.body.addEventListener('htmx:sseOpen',  ()=> { const d=dot(); if (d){d.classList.remove('dead'); d.classList.add('live'); } });
  document.body.addEventListener('htmx:sseError', ()=> { const d=dot(); if (d){d.classList.remove('live'); d.classList.add('dead'); } });

  document.body.addEventListener('htmx:sseMessage', function(ev){
    try {
      const data = JSON.parse(ev.detail.data);
      if (data.kind === 'toast') toast(data.level || 'info', data.msg);
      else if (data.kind === 'task_started')  { toast('info', '▶ ' + data.label); setTaskLine('… ' + data.label); }
      else if (data.kind === 'task_progress') setTaskLine(data.note);
      else if (data.kind === 'task_finished') { toast(data.ok ? 'success' : 'error', data.msg); setTaskLine(''); }
      else if (data.kind === 'invalidate') {
        document.querySelectorAll(`[data-resource="${data.resource}"]`).forEach(el => htmx.trigger(el, 'refresh'));
      }
    } catch(e){}
  });

  document.body.addEventListener('htmx:responseError', function(ev){
    toast('error', 'HTTP ' + ev.detail.xhr.status + ': ' + (ev.detail.xhr.responseText || ev.detail.xhr.statusText));
  });
})();
"#;

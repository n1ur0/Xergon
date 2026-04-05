import { NextRequest, NextResponse } from "next/server";
import { mergeConfig, generateCSP } from "@/lib/embed/config";

/**
 * GET /embed/chat?model=...&title=...&color=...&position=...&welcome=...&pk=...
 *
 * Serves the widget as a self-contained HTML page designed to be loaded in an iframe.
 * The HTML renders a full React app via Next.js client-side hydration or a standalone
 * approach. Since this runs inside Next.js, we render a minimal HTML shell that loads
 * the widget component.
 */

export async function GET(req: NextRequest) {
  const { searchParams } = new URL(req.url);

  const config = mergeConfig({
    model: searchParams.get("model") || undefined,
    title: searchParams.get("title") || undefined,
    color: searchParams.get("color") || undefined,
    position: searchParams.get("position") || undefined,
    welcome: searchParams.get("welcome") || undefined,
    pk: searchParams.get("pk") || undefined,
  });

  const html = buildWidgetHTML(config);

  return new NextResponse(html, {
    status: 200,
    headers: {
      "Content-Type": "text/html; charset=utf-8",
      "Content-Security-Policy": generateCSP("*"),
      "X-Content-Type-Options": "nosniff",
      "X-Frame-Options": "ALLOWALL",
      "Access-Control-Allow-Origin": "*",
      "Access-Control-Allow-Methods": "GET, OPTIONS",
      "Access-Control-Allow-Headers": "Content-Type",
      "Cache-Control": "public, max-age=3600",
    },
  });
}

export async function OPTIONS() {
  return new NextResponse(null, {
    status: 204,
    headers: {
      "Access-Control-Allow-Origin": "*",
      "Access-Control-Allow-Methods": "GET, OPTIONS",
      "Access-Control-Allow-Headers": "Content-Type",
      "Access-Control-Max-Age": "86400",
    },
  });
}

function buildWidgetHTML(config: {
  model: string;
  title: string;
  primaryColor: string;
  position: string;
  welcomeMessage: string;
  publicKey: string;
}): string {
  // Escape values for inline JS safety
  const esc = (s: string) =>
    s.replace(/\\/g, "\\\\").replace(/'/g, "\\'").replace(/</g, "\\x3c").replace(/>/g, "\\x3e");

  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width,initial-scale=1"/>
<title>${esc(config.title)}</title>
<style>
  *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
  html, body { height: 100%; overflow: hidden; }
  body {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
    background: transparent;
  }
  #root { height: 100%; }
  /* highlight.js dark theme inline */
  pre code.hljs { display: block; overflow-x: auto; padding: 1em; }
  code.hljs { padding: 3px 5px; }
  .hljs { color: #cdd6f4; background: #1e1e2e; }
  .hljs-keyword { color: #cba6f7; }
  .hljs-string { color: #a6e3a1; }
  .hljs-number { color: #fab387; }
  .hljs-comment { color: #6c7086; font-style: italic; }
  .hljs-built_in { color: #f9e2af; }
  .hljs-function { color: #89b4fa; }
  .hljs-title { color: #89b4fa; }
  .hljs-params { color: #f2cdcd; }
  .hljs-attr { color: #89dceb; }
  .hljs-tag { color: #89b4fa; }
  .hljs-name { color: #cba6f7; }
  .hljs-selector-class { color: #f5c2e7; }
  .hljs-selector-id { color: #fab387; }
  .hljs-type { color: #f9e2af; }
  .hljs-meta { color: #f5c2e7; }
  .hljs-literal { color: #fab387; }
  .hljs-variable { color: #cdd6f4; }
  .hljs-symbol { color: #f5c2e7; }
  .hljs-deletion { color: #f38ba8; background: rgba(243,139,168,0.1); }
  .hljs-addition { color: #a6e3a1; background: rgba(166,227,161,0.1); }
</style>
</head>
<body>
<div id="root"></div>
<script>
(function() {
  'use strict';

  // ── Config from query params ──
  var CONFIG = {
    model: '${esc(config.model)}',
    title: '${esc(config.title)}',
    primaryColor: '${esc(config.primaryColor)}',
    position: '${esc(config.position)}',
    welcomeMessage: '${esc(config.welcomeMessage)}',
    publicKey: '${esc(config.publicKey)}',
    apiBase: (window.__XERGON_API_BASE || '/v1')
  };

  // ── State ──
  var state = {
    messages: [],
    isGenerating: false,
    error: null,
    input: '',
    inputFocused: false,
    model: CONFIG.model,
    abortController: null,
    showWelcome: true
  };

  var root = document.getElementById('root');

  // ── Helpers ──
  function escHTML(s) {
    var d = document.createElement('div');
    d.textContent = s;
    return d.innerHTML;
  }

  function genId() {
    return 'xg-' + Math.random().toString(36).slice(2) + Date.now().toString(36);
  }

  // ── SSE Parser ──
  function parseSSEChunk(buffer) {
    var accumulated = '';
    var lines = buffer.split('\\n');
    var remaining = lines.pop() || '';
    for (var i = 0; i < lines.length; i++) {
      var trimmed = lines[i].trim();
      if (!trimmed || trimmed === 'data: [DONE]') continue;
      if (!trimmed.indexOf('data: ') === 0) continue;
      try {
        var json = JSON.parse(trimmed.slice(6));
        var content = json.choices && json.choices[0] && json.choices[0].delta && json.choices[0].delta.content;
        if (content) accumulated += content;
      } catch(e) {}
    }
    return { content: accumulated, remaining: remaining };
  }

  // ── Streaming fetch ──
  function streamChat(userContent) {
    if (!userContent.trim() || !state.model || state.isGenerating) return;

    state.error = null;
    state.isGenerating = true;

    var userMsg = { id: genId(), role: 'user', content: userContent.trim(), timestamp: Date.now() };
    var asstMsg = { id: genId(), role: 'assistant', content: '', model: state.model, timestamp: Date.now() };

    state.messages.push(userMsg);
    state.messages.push(asstMsg);
    state.showWelcome = false;
    render();

    var abort = new AbortController();
    state.abortController = abort;

    var history = state.messages.slice(0, -1).map(function(m) {
      return { role: m.role, content: m.content };
    });

    var headers = { 'Content-Type': 'application/json', 'Accept': 'text/event-stream' };
    if (CONFIG.publicKey) headers['X-Wallet-PK'] = CONFIG.publicKey;

    fetch(CONFIG.apiBase + '/chat/completions', {
      method: 'POST',
      headers: headers,
      body: JSON.stringify({ model: state.model, messages: history, stream: true }),
      signal: abort.signal
    }).then(function(res) {
      if (!res.ok) throw new Error('Model returned status ' + res.status);
      var reader = res.body.getReader();
      var decoder = new TextDecoder();
      var accumulated = '';
      var sseBuffer = '';

      function read() {
        return reader.read().then(function(result) {
          if (result.done) {
            if (!accumulated) {
              var last = state.messages[state.messages.length - 1];
              if (last && last.role === 'assistant') last.content = '(No response content received)';
            }
            state.isGenerating = false;
            state.abortController = null;
            render();
            return;
          }
          sseBuffer += decoder.decode(result.value, { stream: true });
          var parsed = parseSSEChunk(sseBuffer);
          sseBuffer = parsed.remaining;
          if (parsed.content) {
            accumulated += parsed.content;
            var last = state.messages[state.messages.length - 1];
            if (last && last.role === 'assistant') last.content = accumulated;
            render();
          }
          return read();
        });
      }
      return read();
    }).catch(function(err) {
      var last = state.messages[state.messages.length - 1];
      if (last && last.role === 'assistant') {
        if (err.name === 'AbortError') {
          last.content = last.content || '(Generation stopped)';
        } else {
          last.content = 'Error: ' + (err.message || 'Failed to get response');
        }
        last.isError = true;
      }
      state.error = err.message || 'Unknown error';
      state.isGenerating = false;
      state.abortController = null;
      render();
    });
  }

  // ── Simple markdown to HTML (no external deps needed) ──
  function simpleMD(text) {
    if (!text) return '';
    var html = escHTML(text);
    // Code blocks
    html = html.replace(/\x60\x60\x60(\\w*)\\n([\\s\\S]*?)\x60\x60\x60/g, function(_, lang, code) {
      return '<pre style="margin:8px 0;border-radius:8px;background:#1e1e2e;padding:12px;overflow-x:auto;font-size:13px;color:#cdd6f4"><code class="hljs">' + code.trim() + '</code></pre>';
    });
    // Inline code
    html = html.replace(/\x60([^\x60]+)\x60/g, '<code style="background:#f3f4f6;padding:1px 5px;border-radius:4px;font-size:13px;font-family:monospace;color:' + escHTML(CONFIG.primaryColor) + '">$1</code>');
    // Bold
    html = html.replace(/\\*\\*([^*]+)\\*\\*/g, '<strong>$1</strong>');
    // Italic
    html = html.replace(/\\*([^*]+)\\*/g, '<em>$1</em>');
    // Line breaks
    html = html.replace(/\\n/g, '<br/>');
    return html;
  }

  // ── Render ──
  function render() {
    var c = CONFIG.primaryColor;
    var msgs = state.messages.map(function(m) {
      if (m.role === 'user') {
        return '<div style="align-self:flex-end;max-width:80%;background:' + c + ';color:#fff;padding:10px 14px;border-radius:16px 16px 4px 16px;font-size:14px;line-height:1.5;word-break:break-word;white-space:pre-wrap">' + escHTML(m.content) + '</div>';
      }
      var bg = m.isError ? '#fef2f2' : '#fff';
      var border = m.isError ? '#fecaca' : '#e5e7eb';
      var color = m.isError ? '#991b1b' : '#1f2937';
      var content = m.content ? simpleMD(m.content) : (state.isGenerating ? '<span style="display:flex;gap:4px;align-items:center;padding:4px 0"><span style="width:6px;height:6px;border-radius:50%;background:' + c + ';opacity:0.5;animation:xg-bounce 1.4s infinite 0ms"></span><span style="width:6px;height:6px;border-radius:50%;background:' + c + ';opacity:0.5;animation:xg-bounce 1.4s infinite 150ms"></span><span style="width:6px;height:6px;border-radius:50%;background:' + c + ';opacity:0.5;animation:xg-bounce 1.4s infinite 300ms"></span></span>' : '');
      var streaming = (state.isGenerating && m.content) ? '<div style="display:flex;align-items:center;gap:6px;padding:2px 4px"><span style="width:6px;height:6px;border-radius:50%;background:' + c + ';animation:xg-pulse 1.5s infinite"></span><span style="font-size:12px;color:#9ca3af">Streaming...</span></div>' : '';
      return '<div style="align-self:flex-start;max-width:85%;background:' + bg + ';color:' + color + ';padding:10px 14px;border-radius:16px 16px 16px 4px;font-size:14px;line-height:1.6;word-break:break-word;border:1px solid ' + border + '">' + content + '</div>' + streaming;
    }).join('');

    var welcome = state.showWelcome ? '<div style="align-self:center;max-width:90%;text-align:center;color:#6b7280;font-size:13px;padding:8px 0">' + escHTML(CONFIG.welcomeMessage) + '</div>' : '';

    var sendDisabled = !state.input.trim() || !state.model || state.isGenerating;
    var btnStyle = 'width:40px;height:40px;border-radius:50%;border:none;cursor:pointer;display:flex;align-items:center;justify-content:center;flex-shrink:0;transition:opacity 0.2s;';
    var btnBg = state.isGenerating ? '#ef4444' : c;
    var btnOpacity = sendDisabled && !state.isGenerating ? 'opacity:0.4;cursor:not-allowed;' : '';
    var btnContent = state.isGenerating
      ? '<svg width="16" height="16" viewBox="0 0 24 24" fill="white"><rect x="6" y="6" width="12" height="12" rx="2"/></svg>'
      : '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12h14"/><path d="m12 5 7 7-7 7"/></svg>';

    var inputBorder = state.inputFocused ? c : '#d1d5db';
    var inputShadow = state.inputFocused ? '0 0 0 2px ' + c + '33' : 'none';

    root.innerHTML = '<div style="height:100%;display:flex;flex-direction:column">' +
      '<div style="display:flex;align-items:center;justify-content:space-between;padding:14px 16px;background:' + c + ';color:#fff;flex-shrink:0">' +
        '<h3 style="font-size:15px;font-weight:600;margin:0">' + escHTML(CONFIG.title) + '</h3>' +
        '<button id="xg-close" style="background:rgba(255,255,255,0.2);border:none;border-radius:50%;width:28px;height:28px;display:flex;align-items:center;justify-content:center;cursor:pointer;color:#fff;font-size:18px;line-height:1">&#x2715;</button>' +
      '</div>' +
      '<div id="xg-messages" style="flex:1;overflow-y:auto;padding:16px;display:flex;flex-direction:column;gap:12px;background:#f9fafb">' +
        welcome + msgs +
        '<div id="xg-end"></div>' +
      '</div>' +
      '<div style="padding:12px;border-top:1px solid #e5e7eb;background:#fff;flex-shrink:0">' +
        '<div style="display:flex;gap:8px;align-items:flex-end">' +
          '<textarea id="xg-input" placeholder="Type a message..." rows="1" disabled="' + state.isGenerating + '" style="flex:1;border:1px solid ' + inputBorder + ';border-radius:12px;padding:10px 14px;font-size:14px;font-family:inherit;resize:none;outline:none;min-height:40px;max-height:120px;line-height:1.5;box-shadow:' + inputShadow + '">' + escHTML(state.input) + '</textarea>' +
          '<button id="xg-send" style="' + btnStyle + 'background:' + btnBg + ';' + btnOpacity + '">' + btnContent + '</button>' +
        '</div>' +
      '</div>' +
      '<div style="padding:8px 12px;border-top:1px solid #f3f4f6;text-align:center;flex-shrink:0">' +
        '<a href="https://xergon.network" target="_blank" rel="noopener noreferrer" style="font-size:11px;color:#9ca3af;text-decoration:none">Powered by Xergon</a>' +
      '</div>' +
    '</div>';

    // Post resize to parent
    if (window.parent !== window) {
      window.parent.postMessage({ type: 'xergon-widget-resize', height: 520, open: true }, '*');
    }

    // Auto-scroll
    var end = document.getElementById('xg-end');
    if (end) end.scrollIntoView({ behavior: 'smooth' });

    // Bind events
    var input = document.getElementById('xg-input');
    if (input) {
      // Restore cursor position
      var len = state.input.length;
      input.setSelectionRange(len, len);
      input.style.height = 'auto';
      input.style.height = Math.min(input.scrollHeight, 120) + 'px';

      input.addEventListener('input', function(e) {
        state.input = e.target.value;
      });
      input.addEventListener('focus', function() {
        state.inputFocused = true;
      });
      input.addEventListener('blur', function() {
        state.inputFocused = false;
      });
      input.addEventListener('keydown', function(e) {
        if (e.key === 'Enter' && !e.shiftKey) {
          e.preventDefault();
          doSend();
        }
      });
    }

    var sendBtn = document.getElementById('xg-send');
    if (sendBtn) {
      sendBtn.addEventListener('click', function() {
        if (state.isGenerating) {
          state.abortController && state.abortController.abort();
        } else {
          doSend();
        }
      });
    }

    var closeBtn = document.getElementById('xg-close');
    if (closeBtn) {
      closeBtn.addEventListener('click', function() {
        if (window.parent !== window) {
          window.parent.postMessage({ type: 'xergon-widget-close' }, '*');
        }
      });
    }
  }

  function doSend() {
    if (!state.input.trim() || state.isGenerating || !state.model) return;
    var msg = state.input;
    state.input = '';
    streamChat(msg);
  }

  // Listen for config updates from parent
  window.addEventListener('message', function(e) {
    if (e.data && e.data.type === 'xergon-widget-config') {
      if (e.data.model) { state.model = e.data.model; CONFIG.model = e.data.model; }
      if (e.data.publicKey) CONFIG.publicKey = e.data.publicKey;
      render();
    }
  });

  // Initial render
  render();
})();
</script>
<style>
@keyframes xg-bounce {
  0%, 80%, 100% { transform: translateY(0); opacity: 0.4; }
  40% { transform: translateY(-6px); opacity: 1; }
}
@keyframes xg-pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.4; }
}
</style>
</body>
</html>`;
}

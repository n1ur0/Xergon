import { NextRequest, NextResponse } from "next/server";

/**
 * GET /embed/script
 *
 * Returns a small JavaScript snippet that third-party sites include via:
 *   <script src="https://marketplace.xergon.network/embed/script" async></script>
 *   <script>
 *     window.XergonChat = { model: 'llama-3.3-70b', color: '#6366f1', ... };
 *   </script>
 *
 * The script creates a floating chat bubble + iframe pointing to /embed/chat.
 */

export async function GET(req: NextRequest) {
  const origin = new URL(req.url).origin;

  const script = buildEmbedScript(origin);

  return new NextResponse(script, {
    status: 200,
    headers: {
      "Content-Type": "application/javascript; charset=utf-8",
      "Cache-Control": "public, max-age=3600",
      "Access-Control-Allow-Origin": "*",
      "Access-Control-Allow-Methods": "GET, OPTIONS",
      "Access-Control-Allow-Headers": "Content-Type",
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

function buildEmbedScript(origin: string): string {
  return `(function(){
  'use strict';

  var XW = window.XergonChat || {};
  var cfg = {
    model: XW.model || '',
    title: XW.title || 'Xergon Chat',
    color: XW.color || '#6366f1',
    position: XW.position || 'bottom-right',
    welcome: XW.welcome || 'Hello! How can I help you today?',
    pk: XW.publicKey || XW.pk || '',
    apiBase: XW.apiBase || ''
  };

  var EMBED_BASE = '${origin}/embed/chat';

  // ── Build iframe URL ──
  function buildURL() {
    var p = [];
    if (cfg.model) p.push('model=' + encodeURIComponent(cfg.model));
    if (cfg.title) p.push('title=' + encodeURIComponent(cfg.title));
    if (cfg.color) p.push('color=' + encodeURIComponent(cfg.color));
    if (cfg.position) p.push('position=' + encodeURIComponent(cfg.position));
    if (cfg.welcome) p.push('welcome=' + encodeURIComponent(cfg.welcome));
    if (cfg.pk) p.push('pk=' + encodeURIComponent(cfg.pk));
    var sep = EMBED_BASE.indexOf('?') === -1 ? '?' : '&';
    return EMBED_BASE + sep + p.join('&');
  }

  // ── Create container ──
  var container = document.createElement('div');
  container.id = 'xergon-widget-container';
  container.style.cssText = 'position:fixed;bottom:0;z-index:2147483647;font-family:inherit;';
  if (cfg.position === 'bottom-left') {
    container.style.left = '0';
  } else {
    container.style.right = '0';
  }
  document.body.appendChild(container);

  // ── Create bubble button ──
  var bubble = document.createElement('button');
  bubble.setAttribute('aria-label', 'Open Xergon Chat');
  bubble.style.cssText = 'width:60px;height:60px;border-radius:50%;background:' + cfg.color + ';border:none;cursor:pointer;display:flex;align-items:center;justify-content:center;box-shadow:0 4px 12px rgba(0,0,0,0.15);margin:24px;transition:transform 0.2s,box-shadow 0.2s;';
  bubble.innerHTML = '<svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>';
  bubble.addEventListener('mouseenter', function() {
    bubble.style.transform = 'scale(1.05)';
    bubble.style.boxShadow = '0 6px 20px rgba(0,0,0,0.2)';
  });
  bubble.addEventListener('mouseleave', function() {
    bubble.style.transform = 'scale(1)';
    bubble.style.boxShadow = '0 4px 12px rgba(0,0,0,0.15)';
  });

  // ── Create iframe (hidden initially) ──
  var iframe = document.createElement('iframe');
  iframe.setAttribute('title', cfg.title);
  iframe.setAttribute('allow', 'clipboard-write');
  iframe.style.cssText = 'border:none;border-radius:16px;box-shadow:0 8px 32px rgba(0,0,0,0.12),0 2px 8px rgba(0,0,0,0.08);overflow:hidden;transition:opacity 0.25s,transform 0.25s;opacity:0;transform:translateY(10px) scale(0.95);pointer-events:none;width:400px;max-width:calc(100vw - 32px);height:520px;max-height:calc(100vh - 100px);margin:0 24px 24px 24px;';
  if (cfg.position === 'bottom-left') {
    iframe.style.marginLeft = '24px';
    iframe.style.marginRight = '0';
  }

  var isOpen = false;

  function toggleChat() {
    isOpen = !isOpen;
    if (isOpen) {
      iframe.src = buildURL();
      iframe.style.opacity = '1';
      iframe.style.transform = 'translateY(0) scale(1)';
      iframe.style.pointerEvents = 'auto';
      bubble.style.display = 'none';
    } else {
      iframe.style.opacity = '0';
      iframe.style.transform = 'translateY(10px) scale(0.95)';
      iframe.style.pointerEvents = 'none';
      bubble.style.display = 'flex';
      iframe.src = 'about:blank';
    }
  }

  bubble.addEventListener('click', toggleChat);

  // ── Listen for postMessage from iframe ──
  window.addEventListener('message', function(e) {
    if (!e.data || typeof e.data.type !== 'string') return;

    if (e.data.type === 'xergon-widget-resize' && e.data.height) {
      iframe.style.height = e.data.height + 'px';
    }

    if (e.data.type === 'xergon-widget-close') {
      if (isOpen) toggleChat();
    }
  });

  // ── Assemble ──
  container.appendChild(bubble);
  container.appendChild(iframe);

  // ── Expose API ──
  window.XergonChat = {
    open: function() { if (!isOpen) toggleChat(); },
    close: function() { if (isOpen) toggleChat(); },
    toggle: toggleChat,
    setConfig: function(opts) {
      if (opts.model !== undefined) cfg.model = opts.model;
      if (opts.title !== undefined) cfg.title = opts.title;
      if (opts.color !== undefined) cfg.color = opts.color;
      if (opts.position !== undefined) cfg.position = opts.position;
      if (opts.welcome !== undefined) cfg.welcome = opts.welcome;
      if (opts.publicKey !== undefined) cfg.pk = opts.publicKey;
      if (opts.pk !== undefined) cfg.pk = opts.pk;
      if (opts.apiBase !== undefined) cfg.apiBase = opts.apiBase;
      // Update bubble color
      bubble.style.background = cfg.color;
      // If open, reload with new config
      if (isOpen) {
        iframe.src = buildURL();
      }
    }
  };
})();`;
}

import { NextRequest, NextResponse } from 'next/server';
import { promises as fs } from 'fs';
import path from 'path';

/**
 * Serve the OpenAPI spec as JSON at /api/docs
 *
 * GET /api/docs        -> JSON spec
 * GET /api/docs?format=html -> Rendered HTML documentation
 */
export async function GET(request: NextRequest) {
  try {
    const specPath = path.join(process.cwd(), 'docs', 'openapi.yaml');
    const yamlContent = await fs.readFile(specPath, 'utf-8');

    const format = request.nextUrl.searchParams.get('format');

    if (format === 'html') {
      // Convert YAML to JSON for embedding in the HTML page
      const openapiJson = yamlToJson(yamlContent);
      const html = buildHtmlDocs(openapiJson);
      return new NextResponse(html, {
        headers: { 'Content-Type': 'text/html; charset=utf-8' },
      });
    }

    // Return as JSON
    const openapiJson = yamlToJson(yamlContent);
    return NextResponse.json(openapiJson, {
      headers: {
        'Content-Type': 'application/json',
        'Cache-Control': 'public, max-age=300',
      },
    });
  } catch (err) {
    return NextResponse.json(
      { error: 'OpenAPI spec not found' },
      { status: 404 },
    );
  }
}

// ── Simple YAML to JSON converter ────────────────────────────────────────
// Handles the subset of YAML used in our OpenAPI spec.

function yamlToJson(yaml: string): unknown {
  const lines = yaml.split('\n');
  const result = parseValue(lines, 0, 0).value;
  return result;
}

interface ParseResult {
  value: unknown;
  nextLine: number;
}

function parseValue(
  lines: string[],
  startLine: number,
  indent: number,
): ParseResult {
  if (startLine >= lines.length) return { value: null, nextLine: startLine };

  const line = lines[startLine];

  // Empty line -- skip
  if (line.trim() === '' || line.trim().startsWith('#')) {
    return parseValue(lines, startLine + 1, indent);
  }

  // Check if this is a list item
  if (line.startsWith('  '.repeat(indent / 2) + '- ') || (indent === 0 && line.startsWith('- '))) {
    return parseList(lines, startLine, indent);
  }

  // Check if this is a key-value mapping
  const kvMatch = line.match(/^(\s*)([^:]+?):\s*(.*)$/);
  if (kvMatch) {
    return parseObject(lines, startLine, indent);
  }

  // Plain scalar value
  return { value: parseScalar(line.trim()), nextLine: startLine + 1 };
}

function getIndent(line: string): number {
  const match = line.match(/^(\s*)/);
  return match ? match[1].length : 0;
}

function parseObject(
  lines: string[],
  startLine: number,
  baseIndent: number,
): ParseResult {
  const obj: Record<string, unknown> = {};
  let i = startLine;

  while (i < lines.length) {
    const line = lines[i];

    // Skip empty and comment lines
    if (line.trim() === '' || line.trim().startsWith('#')) {
      i++;
      continue;
    }

    const currentIndent = getIndent(line);

    // If we've dedented past the base, we're done with this object
    if (currentIndent < baseIndent) break;

    // Must be a key: value line at the current indent
    const kvMatch = line.match(/^(\s*)([^:#]+?):\s*(.*)$/);
    if (!kvMatch) {
      i++;
      continue;
    }

    const key = kvMatch[2].trim();
    const valueStr = kvMatch[3].trim();
    const keyIndent = currentIndent;

    if (valueStr === '' || valueStr === '|') {
      // Value is on subsequent lines (nested object or list)
      const nextNonEmpty = findNextNonEmpty(lines, i + 1);
      if (nextNonEmpty < lines.length) {
        const nextIndent = getIndent(lines[nextNonEmpty]);
        if (nextIndent > keyIndent) {
          if (lines[nextNonEmpty].trimStart().startsWith('- ')) {
            const parsed = parseList(lines, nextNonEmpty, nextIndent);
            obj[key] = parsed.value;
            i = parsed.nextLine;
            continue;
          } else {
            const parsed = parseObject(lines, nextNonEmpty, nextIndent);
            obj[key] = parsed.value;
            i = parsed.nextLine;
            continue;
          }
        }
      }
      obj[key] = null;
      i++;
    } else {
      obj[key] = parseScalar(valueStr);
      i++;
    }
  }

  return { value: obj, nextLine: i };
}

function parseList(
  lines: string[],
  startLine: number,
  baseIndent: number,
): ParseResult {
  const list: unknown[] = [];
  let i = startLine;

  while (i < lines.length) {
    const line = lines[i];

    if (line.trim() === '' || line.trim().startsWith('#')) {
      i++;
      continue;
    }

    const currentIndent = getIndent(line);

    if (currentIndent < baseIndent) break;

    if (!line.trimStart().startsWith('- ')) {
      i++;
      continue;
    }

    const itemContent = line.trimStart().substring(2).trim();

    if (itemContent === '') {
      // Nested content follows
      const nextNonEmpty = findNextNonEmpty(lines, i + 1);
      if (nextNonEmpty < lines.length && getIndent(lines[nextNonEmpty]) > baseIndent) {
        const nextIndent = getIndent(lines[nextNonEmpty]);
        const parsed = parseObject(lines, nextNonEmpty, nextIndent);
        list.push(parsed.value);
        i = parsed.nextLine;
      } else {
        list.push(null);
        i++;
      }
    } else if (itemContent.includes(': ') || itemContent.endsWith(':')) {
      // Inline object start -- treat rest as nested object
      // Reconstruct as if it were indented
      const fakeLines = [...lines];
      const innerIndent = baseIndent + 2;
      fakeLines[i] = ' '.repeat(innerIndent) + itemContent;
      const parsed = parseObject(fakeLines, i, innerIndent);
      list.push(parsed.value);
      i = parsed.nextLine;
    } else {
      list.push(parseScalar(itemContent));
      i++;
    }
  }

  return { value: list, nextLine: i };
}

function findNextNonEmpty(lines: string[], start: number): number {
  for (let i = start; i < lines.length; i++) {
    if (lines[i].trim() !== '' && !lines[i].trim().startsWith('#')) return i;
  }
  return lines.length;
}

function parseScalar(value: string): unknown {
  // Boolean
  if (value === 'true') return true;
  if (value === 'false') return false;
  // Null
  if (value === 'null' || value === '~') return null;
  // Number
  if (/^-?\d+$/.test(value)) return parseInt(value, 10);
  if (/^-?\d+\.\d+$/.test(value)) return parseFloat(value);
  // Quoted string
  if ((value.startsWith('"') && value.endsWith('"')) || (value.startsWith("'") && value.endsWith("'"))) {
    return value.slice(1, -1);
  }
  // Plain string
  return value;
}

// ── HTML Documentation Builder ──────────────────────────────────────────

function buildHtmlDocs(spec: any): string {
  const info = spec.info || {};
  const servers = spec.servers || [];
  const tags = spec.tags || [];
  const paths = spec.paths || {};
  const schemas = spec.components?.schemas || {};

  const endpointsHtml = Object.entries(paths)
    .map(([path, methods]: [string, any]) => {
      return Object.entries(methods)
        .filter(([, _op]: [string, any]) => typeof _op === 'object' && _op !== null)
        .map(([method, op]: [string, any]) => {
          const methodColor = getMethodColor(method.toUpperCase());
          const tag = (op.tags || [''])[0];
          const params = (op.parameters || []).map((p: any) =>
            `<tr>
              <td class="param-name">${p.name}</td>
              <td><code>${p.in}</code></td>
              <td>${p.required ? '<span class="badge-required">required</span>' : ''} ${p.description || ''}</td>
            </tr>`
          ).join('');

          const reqBody = op.requestBody?.content?.['application/json']?.schema;
          const reqBodyHtml = reqBody
            ? `<div class="schema-block"><strong>Request Body:</strong><pre>${JSON.stringify(resolveSchema(reqBody, schemas), null, 2)}</pre></div>`
            : '';

          const responsesHtml = Object.entries(op.responses || {})
            .map(([code, resp]: [string, any]) => {
              const schema = resp.content?.['application/json']?.schema;
              const schemaHtml = schema
                ? `<pre>${JSON.stringify(resolveSchema(schema, schemas), null, 2)}</pre>`
                : '';
              return `<div class="response">
                <span class="response-code ${parseInt(code) < 400 ? 'code-ok' : 'code-err'}">${code}</span>
                ${resp.description ? `<span class="response-desc">${resp.description}</span>` : ''}
                ${schemaHtml}
              </div>`;
            }).join('');

          return `<div class="endpoint" id="${method}-${path.replace(/[{}\/]/g, '-')}">
            <div class="endpoint-header">
              <span class="method-badge ${methodColor}">${method.toUpperCase()}</span>
              <code class="endpoint-path">${path}</code>
              <span class="endpoint-tag">${tag}</span>
            </div>
            <p class="endpoint-summary">${op.summary || ''}</p>
            ${op.description ? `<p class="endpoint-desc">${op.description.replace(/\n/g, '<br>')}</p>` : ''}
            ${params ? `<table class="params-table"><thead><tr><th>Name</th><th>In</th><th>Description</th></tr></thead><tbody>${params}</tbody></table>` : ''}
            ${reqBodyHtml}
            <div class="responses"><strong>Responses:</strong>${responsesHtml}</div>
          </div>`;
        }).join('');
    })
    .join('\n');

  const schemasHtml = Object.entries(schemas)
    .map(([name, schema]: [string, any]) =>
      `<div class="schema-block">
        <h3 id="schema-${name}">${name}</h3>
        <pre>${JSON.stringify(resolveSchema(schema, schemas), null, 2)}</pre>
      </div>`
    ).join('\n');

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>${info.title || 'Xergon API'} - Documentation</title>
  <style>
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #0a0a0a; color: #e5e5e5; line-height: 1.6; }
    .container { max-width: 960px; margin: 0 auto; padding: 2rem; }
    h1 { font-size: 2rem; color: #fff; margin-bottom: 0.25rem; }
    h2 { font-size: 1.4rem; color: #fff; margin: 2rem 0 1rem; border-bottom: 1px solid #262626; padding-bottom: 0.5rem; }
    h3 { font-size: 1.1rem; color: #ccc; margin: 1rem 0 0.5rem; }
    .version { color: #888; margin-bottom: 1.5rem; }
    .description { color: #aaa; margin-bottom: 2rem; max-width: 700px; }
    .server { color: #888; margin-bottom: 0.5rem; }
    .server code { background: #1a1a1a; padding: 0.15rem 0.5rem; border-radius: 4px; color: #4ade80; }

    .endpoint { background: #111; border: 1px solid #222; border-radius: 8px; padding: 1.25rem; margin-bottom: 1rem; }
    .endpoint-header { display: flex; align-items: center; gap: 0.75rem; margin-bottom: 0.5rem; flex-wrap: wrap; }
    .method-badge { padding: 0.2rem 0.6rem; border-radius: 4px; font-size: 0.8rem; font-weight: 700; font-family: monospace; }
    .method-get { background: #164e23; color: #4ade80; }
    .method-post { background: #3b1408; color: #fb923c; }
    .method-put { background: #1e2a4a; color: #60a5fa; }
    .method-delete { background: #4a1a1a; color: #f87171; }
    .method-patch { background: #3b2f08; color: #fbbf24; }
    .endpoint-path { font-family: monospace; font-size: 0.95rem; color: #e5e5e5; }
    .endpoint-tag { background: #262626; padding: 0.15rem 0.5rem; border-radius: 4px; font-size: 0.75rem; color: #888; }
    .endpoint-summary { color: #e5e5e5; font-weight: 600; margin-bottom: 0.25rem; }
    .endpoint-desc { color: #888; font-size: 0.9rem; margin-bottom: 0.75rem; }

    .params-table { width: 100%; border-collapse: collapse; margin: 0.75rem 0; font-size: 0.85rem; }
    .params-table th { text-align: left; color: #888; padding: 0.4rem; border-bottom: 1px solid #222; }
    .params-table td { padding: 0.4rem; border-bottom: 1px solid #1a1a1a; }
    .param-name { font-family: monospace; color: #e5e5e5; }
    .badge-required { background: #7f1d1d; color: #fca5a5; padding: 0.1rem 0.4rem; border-radius: 3px; font-size: 0.7rem; }

    .schema-block { background: #0d0d0d; border: 1px solid #1a1a1a; border-radius: 6px; padding: 1rem; margin: 0.75rem 0; }
    .schema-block pre { font-family: 'SF Mono', 'Fira Code', monospace; font-size: 0.8rem; color: #a5b4fc; overflow-x: auto; white-space: pre-wrap; }

    .responses { margin-top: 0.75rem; }
    .responses strong { display: block; color: #888; margin-bottom: 0.5rem; font-size: 0.85rem; }
    .response { display: flex; align-items: flex-start; gap: 0.5rem; margin-bottom: 0.5rem; flex-wrap: wrap; }
    .response pre { width: 100%; }
    .response-code { font-family: monospace; font-weight: 700; padding: 0.15rem 0.5rem; border-radius: 4px; font-size: 0.8rem; min-width: 3rem; text-align: center; }
    .code-ok { background: #164e23; color: #4ade80; }
    .code-err { background: #7f1d1d; color: #fca5a5; }
    .response-desc { color: #888; font-size: 0.85rem; }

    .nav { position: sticky; top: 0; background: #0a0a0a; border-bottom: 1px solid #222; padding: 0.75rem 2rem; z-index: 10; }
    .nav a { color: #888; text-decoration: none; margin-right: 1.5rem; font-size: 0.85rem; }
    .nav a:hover { color: #fff; }

    .footer { color: #444; font-size: 0.8rem; margin-top: 3rem; padding-top: 1rem; border-top: 1px solid #1a1a1a; }
  </style>
</head>
<body>
  <nav class="nav">
    <a href="#endpoints">Endpoints</a>
    <a href="#schemas">Schemas</a>
    <a href="/api/docs">JSON Spec</a>
    <a href="https://relay.xergon.gg" target="_blank">Relay</a>
  </nav>
  <div class="container">
    <h1>${info.title || 'Xergon Relay API'}</h1>
    <p class="version">v${info.version || '1.0.0'} | ${info.license?.name || 'MIT'}</p>
    <p class="description">${(info.description || '').replace(/\n/g, '<br>')}</p>
    ${servers.map((s: any) => `<p class="server">Server: <code>${s.url}</code> ${s.description ? `-- ${s.description}` : ''}</p>`).join('')}

    <h2 id="endpoints">Endpoints</h2>
    ${endpointsHtml}

    <h2 id="schemas">Schemas</h2>
    ${schemasHtml}

    <div class="footer">
      <p>Generated from OpenAPI spec v${info.version || '1.0.0'}. ${new Date().toISOString().split('T')[0]}</p>
    </div>
  </div>
</body>
</html>`;
}

function getMethodColor(method: string): string {
  const colors: Record<string, string> = {
    GET: 'method-get',
    POST: 'method-post',
    PUT: 'method-put',
    DELETE: 'method-delete',
    PATCH: 'method-patch',
  };
  return colors[method] || '';
}

function resolveSchema(schema: any, schemas: Record<string, any>): any {
  if (!schema) return schema;
  if (schema.$ref && typeof schema.$ref === 'string') {
    const name = schema.$ref.replace('#/components/schemas/', '');
    return { $ref: name, ...(schemas[name] ? resolveSchema(schemas[name], schemas) : {}) };
  }
  if (Array.isArray(schema.items)) {
    return schema.items.map((item: any) => resolveSchema(item, schemas));
  }
  if (schema.items) {
    return { ...schema, items: resolveSchema(schema.items, schemas) };
  }
  if (schema.properties) {
    const resolved: Record<string, any> = {};
    for (const [k, v] of Object.entries(schema.properties)) {
      resolved[k] = resolveSchema(v as any, schemas);
    }
    return { ...schema, properties: resolved };
  }
  if (schema.additionalProperties) {
    return { ...schema, additionalProperties: resolveSchema(schema.additionalProperties, schemas) };
  }
  return schema;
}

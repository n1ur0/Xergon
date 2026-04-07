/**
 * Xergon SDK -- Prompt Template System.
 *
 * Provides built-in and custom prompt templates with {{variable}}
 * placeholders, persistent storage in ~/.xergon/templates.json,
 * and a rendering engine.
 *
 * @example
 * ```ts
 * import { renderTemplate, listTemplates, addTemplate } from '@xergon/sdk';
 *
 * const rendered = renderTemplate('explain', {
 *   concept: 'Recursion',
 *   level: 'beginner',
 * });
 * console.log(rendered);
 * ```
 */

import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

// ── Types ───────────────────────────────────────────────────────────

export interface PromptTemplate {
  name: string;
  description: string;
  template: string; // with {{variable}} placeholders
  variables: string[];
  category: 'system' | 'creative' | 'code' | 'analysis' | 'custom';
}

export interface RenderedPrompt {
  system: string;
  user: string;
  variables: Record<string, string>;
}

interface TemplatesData {
  customTemplates: Record<string, PromptTemplate>;
}

// ── Storage helpers ────────────────────────────────────────────────

const TEMPLATES_DIR = () => path.join(os.homedir(), '.xergon');
const TEMPLATES_FILE = () => path.join(TEMPLATES_DIR(), 'templates.json');

function ensureDir(): void {
  const dir = TEMPLATES_DIR();
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
}

function loadCustomTemplates(): Record<string, PromptTemplate> {
  try {
    const data = fs.readFileSync(TEMPLATES_FILE(), 'utf-8');
    const parsed: TemplatesData = JSON.parse(data);
    return parsed.customTemplates ?? {};
  } catch {
    return {};
  }
}

function saveCustomTemplates(templates: Record<string, PromptTemplate>): void {
  ensureDir();
  const data: TemplatesData = { customTemplates: templates };
  fs.writeFileSync(TEMPLATES_FILE(), JSON.stringify(data, null, 2) + '\n');
}

// ── Built-in Templates ─────────────────────────────────────────────

const builtinTemplates: PromptTemplate[] = [
  {
    name: 'code-review',
    description: 'Review code for bugs, style, and best practices',
    category: 'code',
    variables: ['code', 'language', 'focus'],
    template: `You are an expert code reviewer. Review the following {{language}} code{{#focus}} with a focus on {{focus}}{{/focus}}.

Identify:
- Potential bugs or errors
- Performance issues
- Security vulnerabilities
- Code style and best practice violations
- Suggestions for improvement

Provide your review in a structured format with severity levels (critical, warning, info).

\`\`\`{{language}}
{{code}}
\`\`\``,
  },
  {
    name: 'explain',
    description: 'Explain a concept at a specified complexity level',
    category: 'system',
    variables: ['concept', 'level'],
    template: `Explain the concept of "{{concept}}" at a {{level}} level.

${'  '}If the level is "beginner", use simple analogies and avoid jargon.
${'  '}If the level is "intermediate", assume basic knowledge and go deeper.
${'  '}If the level is "expert", provide rigorous technical detail.

Structure your explanation with:
1. A brief summary (1-2 sentences)
2. The core explanation
3. A practical example
4. Related concepts or further reading`,
  },
  {
    name: 'summarize',
    description: 'Summarize text to a target word count',
    category: 'analysis',
    variables: ['text', 'words'],
    template: `Summarize the following text in approximately {{words}} words.

Preserve the key points, main arguments, and critical details. Omit minor
details and filler content. Maintain a neutral, objective tone.

Text to summarize:
---
{{text}}
---`,
  },
  {
    name: 'translate',
    description: 'Translate text between languages',
    category: 'system',
    variables: ['text', 'source_language', 'target_language'],
    template: `Translate the following text from {{source_language}} to {{target_language}}.

Rules:
- Preserve the original meaning and tone
- Use natural, idiomatic language in the target
- Keep technical terms accurate
- Maintain formatting (paragraphs, lists, etc.)

Text:
---
{{text}}
---`,
  },
  {
    name: 'debug',
    description: 'Debug an error with context and suggestions',
    category: 'code',
    variables: ['error', 'context', 'language'],
    template: `Debug the following error in {{language}}.

Error message:
{{error}}

Context:
{{context}}

Please provide:
1. Root cause analysis
2. Step-by-step explanation of what went wrong
3. The fix (with code diff if applicable)
4. How to prevent this error in the future`,
  },
  {
    name: 'refactor',
    description: 'Refactor code with specific constraints',
    category: 'code',
    variables: ['code', 'language', 'constraints'],
    template: `Refactor the following {{language}} code.

Constraints:
{{constraints}}

Original code:
\`\`\`{{language}}
{{code}}
\`\`\`

Provide:
1. The refactored code
2. A summary of changes made
3. Any trade-offs or considerations
4. Before/after comparison of key differences`,
  },
  {
    name: 'test',
    description: 'Generate unit tests for code',
    category: 'code',
    variables: ['code', 'language', 'framework'],
    template: `Generate comprehensive unit tests for the following {{language}} code
using {{framework}}.

Code to test:
\`\`\`{{language}}
{{code}}
\`\`\`

Generate tests that cover:
- Happy path / normal operation
- Edge cases and boundary conditions
- Error handling
- Input validation (if applicable)

For each test, include a brief comment explaining what it verifies.`,
  },
  {
    name: 'doc',
    description: 'Generate documentation for code',
    category: 'code',
    variables: ['code', 'language', 'style'],
    template: `Generate documentation for the following {{language}} code
in {{style}} style.

Code:
\`\`\`{{language}}
{{code}}
\`\`\`

Include:
1. Module/file-level description
2. Function/method documentation (params, returns, examples)
3. Type annotations documentation (if applicable)
4. Usage examples
5. Any important notes or caveats`,
  },
];

// ── Public API ─────────────────────────────────────────────────────

/**
 * List all templates (built-in + custom).
 */
export function listTemplates(): PromptTemplate[] {
  const custom = loadCustomTemplates();
  return [...builtinTemplates, ...Object.values(custom)];
}

/**
 * Get a single template by name (built-in first, then custom).
 */
export function getTemplate(name: string): PromptTemplate | undefined {
  const builtin = builtinTemplates.find(t => t.name === name);
  if (builtin) return builtin;
  const custom = loadCustomTemplates();
  return custom[name];
}

/**
 * Render a template by substituting {{variable}} placeholders.
 *
 * Variables found in the template but not provided are left as-is.
 * The output is returned as a RenderedPrompt with a sensible system
 * message derived from the template category.
 */
export function renderTemplate(
  name: string,
  variables: Record<string, string>,
): RenderedPrompt {
  const template = getTemplate(name);
  if (!template) {
    throw new Error(`Template not found: ${name}. Available: ${listTemplates().map(t => t.name).join(', ')}`);
  }

  let rendered = template.template;

  // Replace {{variable}} with values
  for (const [key, value] of Object.entries(variables)) {
    const pattern = new RegExp(`\\{\\{${escapeRegex(key)}\\}\\}`, 'g');
    rendered = rendered.replace(pattern, value);
  }

  // Build a system prompt based on category
  const systemMessages: Record<string, string> = {
    system: 'You are a helpful, accurate assistant.',
    creative: 'You are a creative, imaginative assistant.',
    code: 'You are an expert software engineer and code assistant.',
    analysis: 'You are a precise, analytical assistant.',
    custom: 'You are a helpful assistant.',
  };

  return {
    system: systemMessages[template.category] ?? systemMessages.system,
    user: rendered,
    variables,
  };
}

/**
 * Render a template to a plain string (no system/user split).
 */
export function renderTemplateRaw(
  name: string,
  variables: Record<string, string>,
): string {
  return renderTemplate(name, variables).user;
}

/**
 * Add a custom template. Persists to ~/.xergon/templates.json.
 */
export function addTemplate(template: PromptTemplate): void {
  if (!template.name || template.name.trim() === '') {
    throw new Error('Template name is required.');
  }

  // Check for name collision with built-ins
  if (builtinTemplates.some(t => t.name === template.name)) {
    throw new Error(`Cannot overwrite built-in template: ${template.name}`);
  }

  // Validate that variables in template match declared variables
  const declaredVars = new Set(template.variables);
  const foundVars = extractVariables(template.template);
  for (const v of foundVars) {
    if (!declaredVars.has(v)) {
      throw new Error(
        `Template references {{${v}}} but it is not declared in variables. ` +
        `Add "${v}" to the variables array.`,
      );
    }
  }

  const custom = loadCustomTemplates();
  custom[template.name] = {
    ...template,
    category: template.category === 'custom' ? 'custom' : template.category,
  };
  saveCustomTemplates(custom);
}

/**
 * Remove a custom template. Cannot remove built-in templates.
 */
export function removeTemplate(name: string): boolean {
  if (builtinTemplates.some(t => t.name === name)) {
    throw new Error(`Cannot remove built-in template: ${name}`);
  }

  const custom = loadCustomTemplates();
  if (!(name in custom)) {
    return false;
  }

  delete custom[name];
  saveCustomTemplates(custom);
  return true;
}

// ── Helpers ────────────────────────────────────────────────────────

function escapeRegex(str: string): string {
  return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function extractVariables(template: string): string[] {
  const matches = template.match(/\{\{(\w+)\}\}/g);
  if (!matches) return [];
  return [...new Set(matches.map(m => m.slice(2, -2)))];
}

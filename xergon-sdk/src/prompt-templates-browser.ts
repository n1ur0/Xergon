/**
 * Browser-safe prompt templates (in-memory only, no file persistence)
 */

import type { PromptTemplate, RenderedPrompt } from './prompt-templates';

// In-memory template store for browser
const _browserTemplates: Map<string, PromptTemplate> = new Map();
_browserTemplates.set('system', { name: 'system', description: 'System prompt', template: 'You are a helpful AI assistant.', variables: [], category: 'system' });
_browserTemplates.set('code-review', { name: 'code-review', description: 'Code review template', template: 'Please review this code for bugs and improvements:\n{{code}}', variables: ['code'], category: 'code' });

export function listTemplates(): PromptTemplate[] {
  return Array.from(_browserTemplates.values());
}

export function getTemplate(name: string): PromptTemplate | undefined {
  return _browserTemplates.get(name);
}

export function renderTemplate(template: string, context: Record<string, unknown>): string {
  return template.replace(/\{\{(\w+)\}\}/g, (_, key) => {
    const value = context[key];
    return value !== undefined ? String(value) : `{{${key}}}`;
  });
}

export function renderTemplateRaw(template: string, context: Record<string, unknown>): string {
  return renderTemplate(template, context);
}

export function addTemplate(template: PromptTemplate): void {
  _browserTemplates.set(template.name, template);
}

export function removeTemplate(name: string): void {
  _browserTemplates.delete(name);
}

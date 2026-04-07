/**
 * Tests for the CLI output formatter.
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { OutputFormatter } from '../../src/cli/mod';

describe('OutputFormatter', () => {
  let formatter: OutputFormatter;
  let originalNoColor: string | undefined;

  beforeEach(() => {
    originalNoColor = process.env.NO_COLOR;
    delete process.env.NO_COLOR;
  });

  afterEach(() => {
    if (originalNoColor !== undefined) {
      process.env.NO_COLOR = originalNoColor;
    } else {
      delete process.env.NO_COLOR;
    }
  });

  describe('constructor', () => {
    it('defaults to text format with color', () => {
      const f = new OutputFormatter();
      // Format and color are internal; test via behavior
      expect(f).toBeDefined();
    });
  });

  describe('formatText', () => {
    it('formats string data', () => {
      const f = new OutputFormatter('text', false);
      const result = f.formatText('hello world');
      expect(result).toContain('hello world');
    });

    it('formats object data with title', () => {
      const f = new OutputFormatter('text', false);
      const result = f.formatText({ name: 'test', count: 42 }, 'Title');
      expect(result).toContain('Title');
      expect(result).toContain('Name:');
      expect(result).toContain('test');
      expect(result).toContain('Count:');
      expect(result).toContain('42');
    });

    it('formats null values', () => {
      const f = new OutputFormatter('text', false);
      const result = f.formatText({ value: null });
      expect(result).toContain('null');
    });

    it('formats boolean values', () => {
      const f = new OutputFormatter('text', false);
      const result = f.formatText({ active: true, disabled: false });
      expect(result).toContain('true');
      expect(result).toContain('false');
    });

    it('formats array data', () => {
      const f = new OutputFormatter('text', false);
      const result = f.formatText(['item1', 'item2']);
      expect(result).toContain('item1');
      expect(result).toContain('item2');
    });

    it('formats nested objects', () => {
      const f = new OutputFormatter('text', false);
      const result = f.formatText({ nested: { key: 'value' } });
      expect(result).toContain('Nested:');
    });
  });

  describe('formatJSON', () => {
    it('formats data as pretty-printed JSON', () => {
      const f = new OutputFormatter('json', false);
      const result = f.formatJSON({ key: 'value', count: 42 });
      const parsed = JSON.parse(result);
      expect(parsed).toEqual({ key: 'value', count: 42 });
    });

    it('formats arrays as JSON', () => {
      const f = new OutputFormatter('json', false);
      const result = f.formatJSON([1, 2, 3]);
      expect(JSON.parse(result)).toEqual([1, 2, 3]);
    });
  });

  describe('formatTable', () => {
    it('formats array of objects as a table', () => {
      const f = new OutputFormatter('text', false);
      const data = [
        { Name: 'Alice', Age: '30' },
        { Name: 'Bob', Age: '25' },
      ];
      const result = f.formatTable(data, 'Users');
      expect(result).toContain('Name');
      expect(result).toContain('Age');
      expect(result).toContain('Alice');
      expect(result).toContain('Bob');
      expect(result).toContain('2 item(s)');
    });

    it('handles empty data', () => {
      const f = new OutputFormatter('text', false);
      const result = f.formatTable([], 'Empty');
      expect(result).toContain('No data available');
    });

    it('includes title when provided', () => {
      const f = new OutputFormatter('text', false);
      const data = [{ A: '1' }];
      const result = f.formatTable(data, 'My Title');
      expect(result).toContain('My Title');
    });

    it('aligns columns properly', () => {
      const f = new OutputFormatter('text', false);
      const data = [
        { Short: 'a', VeryLongColumn: 'x' },
        { Short: 'bbb', VeryLongColumn: 'yyy' },
      ];
      const result = f.formatTable(data);
      // Column headers and data should be aligned
      const lines = result.split('\n');
      expect(lines.length).toBeGreaterThan(3);
    });
  });

  describe('formatOutput', () => {
    it('uses text format by default', () => {
      const f = new OutputFormatter('text', false);
      const result = f.formatOutput({ key: 'value' });
      expect(result).toContain('Key:');
      expect(result).toContain('value');
    });

    it('uses JSON format when set', () => {
      const f = new OutputFormatter('json', false);
      const result = f.formatOutput({ key: 'value' });
      expect(result).toBe('{\n  "key": "value"\n}');
    });

    it('uses table format for arrays', () => {
      const f = new OutputFormatter('table', false);
      const data = [{ A: '1', B: '2' }];
      const result = f.formatOutput(data);
      expect(result).toContain('A');
      expect(result).toContain('B');
    });

    it('falls back to text for non-array data in table mode', () => {
      const f = new OutputFormatter('table', false);
      const result = f.formatOutput('hello');
      expect(result).toContain('hello');
    });
  });

  describe('colorize', () => {
    it('returns plain text when color is disabled', () => {
      const f = new OutputFormatter('text', false);
      const result = f.colorize('hello', 'green');
      expect(result).toBe('hello');
      expect(result).not.toContain('\x1b');
    });

    it('returns plain text when NO_COLOR is set', () => {
      process.env.NO_COLOR = '1';
      const f = new OutputFormatter('text', true);
      const result = f.colorize('hello', 'green');
      expect(result).toBe('hello');
    });

    it('includes ANSI codes when color is enabled', () => {
      const f = new OutputFormatter('text', true);
      const result = f.colorize('hello', 'green');
      expect(result).toContain('\x1b[32m');
      expect(result).toContain('hello');
      expect(result).toContain('\x1b[0m');
    });

    it('supports bold style', () => {
      const f = new OutputFormatter('text', true);
      const result = f.colorize('bold', 'bold');
      expect(result).toContain('\x1b[1m');
    });

    it('supports dim style', () => {
      const f = new OutputFormatter('text', true);
      const result = f.colorize('dim', 'dim');
      expect(result).toContain('\x1b[2m');
    });

    it('supports red style', () => {
      const f = new OutputFormatter('text', true);
      const result = f.colorize('error', 'red');
      expect(result).toContain('\x1b[31m');
    });
  });

  describe('setFormat', () => {
    it('changes the output format', () => {
      const f = new OutputFormatter('text', false);
      f.setFormat('json');
      const result = f.formatOutput({ key: 'value' });
      expect(result).toBe('{\n  "key": "value"\n}');
    });
  });

  describe('setColor', () => {
    it('disables color', () => {
      const f = new OutputFormatter('text', true);
      f.setColor(false);
      const result = f.colorize('text', 'green');
      expect(result).toBe('text');
    });
  });
});

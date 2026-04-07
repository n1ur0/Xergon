/**
 * Tests for the CLI argument parser.
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { ArgumentParser, CLIError, type Command, type ParsedArgs } from '../../src/cli/mod';

describe('ArgumentParser', () => {
  let parser: ArgumentParser;

  const testCommand: Command = {
    name: 'test',
    description: 'A test command',
    aliases: ['t', 'testing'],
    options: [
      {
        name: 'model',
        short: '-m',
        long: '--model',
        description: 'Model name',
        required: false,
        default: 'default-model',
        type: 'string',
      },
      {
        name: 'count',
        short: '-n',
        long: '--count',
        description: 'Number of items',
        required: false,
        type: 'number',
      },
      {
        name: 'verbose',
        short: '-v',
        long: '--verbose',
        description: 'Verbose output',
        required: false,
        type: 'boolean',
      },
      {
        name: 'required',
        short: '-r',
        long: '--required',
        description: 'A required option',
        required: true,
        type: 'string',
      },
      {
        name: 'output',
        short: '-o',
        long: '--output',
        description: 'Output format',
        required: false,
        type: 'string',
      },
    ],
    action: async () => {},
  };

  beforeEach(() => {
    parser = new ArgumentParser('xergon', '0.1.0');
    parser.registerCommand(testCommand);
  });

  describe('parse', () => {
    it('parses positional arguments', () => {
      const result = parser.parse(['node', 'xergon', 'test', '-r', 'val', 'hello', 'world']);
      expect(result.command).toBe('test');
      expect(result.positional).toEqual(['hello', 'world']);
    });

    it('parses short options', () => {
      const result = parser.parse(['node', 'xergon', 'test', '-m', 'gpt-4', '-r', 'val']);
      expect(result.options.model).toBe('gpt-4');
      expect(result.options.required).toBe('val');
    });

    it('parses long options', () => {
      const result = parser.parse(['node', 'xergon', 'test', '--model', 'gpt-4', '--required', 'val']);
      expect(result.options.model).toBe('gpt-4');
      expect(result.options.required).toBe('val');
    });

    it('parses boolean flags', () => {
      const result = parser.parse(['node', 'xergon', 'test', '--verbose', '-r', 'val']);
      expect(result.options.verbose).toBe(true);
    });

    it('parses equal syntax for long options', () => {
      const result = parser.parse(['node', 'xergon', 'test', '--model=gpt-4', '-r', 'val']);
      expect(result.options.model).toBe('gpt-4');
    });

    it('parses attached value for short options', () => {
      const result = parser.parse(['node', 'xergon', 'test', '-mgpt-4', '-r', 'val']);
      expect(result.options.model).toBe('gpt-4');
    });

    it('parses number options', () => {
      const result = parser.parse(['node', 'xergon', 'test', '-n', '42', '-r', 'val']);
      expect(result.options.count).toBe(42);
    });

    it('applies default values', () => {
      const result = parser.parse(['node', 'xergon', 'test', '-r', 'val']);
      expect(result.options.model).toBe('default-model');
    });

    it('throws on missing required option', () => {
      expect(() => parser.parse(['node', 'xergon', 'test']))
        .toThrow(CLIError);
      expect(() => parser.parse(['node', 'xergon', 'test']))
        .toThrow('Missing required option: --required');
    });

    it('throws on unknown long option', () => {
      expect(() => parser.parse(['node', 'xergon', 'test', '--unknown', 'val', '-r', 'val']))
        .toThrow(CLIError);
      expect(() => parser.parse(['node', 'xergon', 'test', '--unknown', 'val', '-r', 'val']))
        .toThrow('Unknown option: --unknown');
    });

    it('throws on unknown short option', () => {
      expect(() => parser.parse(['node', 'xergon', 'test', '-x', 'val', '-r', 'val']))
        .toThrow(CLIError);
    });

    it('throws on missing value for option', () => {
      expect(() => parser.parse(['node', 'xergon', 'test', '-m', '-r', 'val']))
        .toThrow(CLIError);
    });

    it('handles --version flag', () => {
      const result = parser.parse(['node', 'xergon', '--version']);
      expect(result.command).toBe('version');
    });

    it('handles -v flag as version', () => {
      const result = parser.parse(['node', 'xergon', '-v']);
      expect(result.command).toBe('version');
    });

    it('handles --help flag', () => {
      const result = parser.parse(['node', 'xergon', '--help']);
      expect(result.command).toBe('help');
    });

    it('handles no args as help', () => {
      const result = parser.parse(['node', 'xergon']);
      expect(result.command).toBe('help');
    });

    it('handles unknown command', () => {
      const result = parser.parse(['node', 'xergon', 'foobar', '--arg']);
      expect(result.command).toBe('unknown');
      expect(result.positional).toEqual(['foobar', '--arg']);
    });

    it('parses command alias', () => {
      const result = parser.parse(['node', 'xergon', 't', '-r', 'val']);
      expect(result.command).toBe('t');
      expect(result.options.required).toBe('val');
    });
  });

  describe('registerCommand', () => {
    it('registers a command by name', () => {
      const cmd = parser.getCommand('test');
      expect(cmd).toBeDefined();
      expect(cmd!.name).toBe('test');
    });

    it('registers command aliases', () => {
      expect(parser.getCommand('t')).toBeDefined();
      expect(parser.getCommand('testing')).toBeDefined();
    });

    it('getAllCommands returns unique commands', () => {
      const all = parser.getAllCommands();
      expect(all).toHaveLength(1);
      expect(all[0].name).toBe('test');
    });
  });

  describe('generateHelp', () => {
    it('generates program help with no command', () => {
      const help = parser.generateHelp();
      expect(help).toContain('xergon');
      expect(help).toContain('test');
      expect(help).toContain('A test command');
      expect(help).toContain('Aliases: t, testing');
    });

    it('generates command-specific help', () => {
      const help = parser.generateHelp('test');
      expect(help).toContain('COMMAND: test');
      expect(help).toContain('--model');
      expect(help).toContain('--required');
      expect(help).toContain('(required)');
      expect(help).toContain('(default: default-model)');
    });

    it('returns error for unknown command help', () => {
      const help = parser.generateHelp('nonexistent');
      expect(help).toContain('Unknown command: nonexistent');
    });
  });
});

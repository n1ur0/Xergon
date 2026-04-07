/**
 * Tests for CancellationToken and CancellationManager.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { CancellationToken, CancellationManager } from '../src/cancellation';

describe('CancellationToken', () => {
  it('creates a token with a unique id', () => {
    const token = new CancellationToken();
    expect(token.id).toMatch(/^token-\d+$/);
    expect(token.isCancelled).toBe(false);
  });

  it('creates a token with a custom id', () => {
    const token = new CancellationToken('my-token');
    expect(token.id).toBe('my-token');
  });

  it('has an AbortSignal', () => {
    const token = new CancellationToken();
    expect(token.signal).toBeInstanceOf(AbortSignal);
    expect(token.signal.aborted).toBe(false);
  });

  describe('cancel()', () => {
    it('cancels the token', () => {
      const token = new CancellationToken();
      token.cancel('test reason');
      expect(token.isCancelled).toBe(true);
      expect(token.reason).toBe('test reason');
      expect(token.signal.aborted).toBe(true);
    });

    it('is idempotent (cancelling again is a no-op)', () => {
      const token = new CancellationToken();
      token.cancel('first');
      token.cancel('second');
      expect(token.reason).toBe('first');
    });

    it('triggers abort event on signal', () => {
      const token = new CancellationToken();
      const listener = vi.fn();
      token.signal.addEventListener('abort', listener);
      token.cancel('reason');
      expect(listener).toHaveBeenCalledTimes(1);
    });
  });

  describe('throwIfCancelled()', () => {
    it('does not throw if not cancelled', () => {
      const token = new CancellationToken();
      expect(() => token.throwIfCancelled()).not.toThrow();
    });

    it('throws AbortError if cancelled', () => {
      const token = new CancellationToken();
      token.cancel('oops');
      expect(() => token.throwIfCancelled()).toThrow(DOMException);
      try {
        token.throwIfCancelled();
      } catch (e) {
        expect((e as DOMException).name).toBe('AbortError');
      }
    });
  });

  describe('linkTo()', () => {
    it('child cancels when parent cancels', () => {
      const parent = new CancellationToken('parent');
      const child = new CancellationToken('child');
      child.linkTo(parent);
      expect(child.isCancelled).toBe(false);
      parent.cancel('parent cancelled');
      expect(child.isCancelled).toBe(true);
      expect(child.reason).toContain('parent');
    });

    it('immediately cancels child if parent is already cancelled', () => {
      const parent = new CancellationToken('parent');
      parent.cancel('already cancelled');
      const child = new CancellationToken('child');
      child.linkTo(parent);
      expect(child.isCancelled).toBe(true);
    });

    it('child does NOT cancel parent', () => {
      const parent = new CancellationToken('parent');
      const child = new CancellationToken('child');
      child.linkTo(parent);
      child.cancel('child cancelled');
      expect(parent.isCancelled).toBe(false);
      expect(child.isCancelled).toBe(true);
    });

    it('supports multi-level chaining', () => {
      const grandparent = new CancellationToken('gp');
      const parent = new CancellationToken('p');
      const child = new CancellationToken('c');
      parent.linkTo(grandparent);
      child.linkTo(parent);
      grandparent.cancel('top');
      expect(parent.isCancelled).toBe(true);
      expect(child.isCancelled).toBe(true);
    });
  });

  describe('withTimeout()', () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it('auto-cancels after timeout', () => {
      const token = new CancellationToken();
      const timeoutToken = token.withTimeout(5000);
      expect(timeoutToken.isCancelled).toBe(false);
      vi.advanceTimersByTime(5000);
      expect(timeoutToken.isCancelled).toBe(true);
      expect(timeoutToken.reason).toContain('5000ms');
    });

    it('does NOT cancel the original token', () => {
      const token = new CancellationToken();
      const timeoutToken = token.withTimeout(1000);
      vi.advanceTimersByTime(2000);
      expect(token.isCancelled).toBe(false);
      expect(timeoutToken.isCancelled).toBe(true);
    });

    it('timeout token cancels if parent cancels before timeout', () => {
      const token = new CancellationToken();
      const timeoutToken = token.withTimeout(10000);
      token.cancel('parent cancelled');
      expect(timeoutToken.isCancelled).toBe(true);
    });
  });
});

describe('CancellationManager', () => {
  it('creates tokens', () => {
    const manager = new CancellationManager();
    const token = manager.createToken('t1');
    expect(token.id).toBe('t1');
    expect(token.isCancelled).toBe(false);
    expect(manager.activeCount()).toBe(1);
  });

  it('auto-removes cancelled tokens from active count', () => {
    const manager = new CancellationManager();
    const token = manager.createToken();
    expect(manager.activeCount()).toBe(1);
    token.cancel();
    expect(manager.activeCount()).toBe(0);
  });

  it('cancelAll() cancels all active tokens', () => {
    const manager = new CancellationManager();
    const t1 = manager.createToken('a');
    const t2 = manager.createToken('b');
    const t3 = manager.createToken('c');
    expect(manager.activeCount()).toBe(3);
    manager.cancelAll('shutdown');
    expect(t1.isCancelled).toBe(true);
    expect(t2.isCancelled).toBe(true);
    expect(t3.isCancelled).toBe(true);
    expect(t1.reason).toBe('shutdown');
    expect(manager.activeCount()).toBe(0);
  });

  it('getToken() returns active token', () => {
    const manager = new CancellationManager();
    const token = manager.createToken('findme');
    expect(manager.getToken('findme')).toBe(token);
    expect(manager.getToken('nonexistent')).toBeUndefined();
  });

  it('getToken() returns undefined for cancelled token', () => {
    const manager = new CancellationManager();
    const token = manager.createToken('gone');
    token.cancel();
    expect(manager.getToken('gone')).toBeUndefined();
  });

  it('dispose() cancels all and clears', () => {
    const manager = new CancellationManager();
    manager.createToken('a');
    manager.createToken('b');
    manager.dispose();
    expect(manager.activeCount()).toBe(0);
  });
});

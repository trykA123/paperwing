import './test-support/svelte-loader.js';
import { expect, test } from 'bun:test';

const { Counter } = await import('./test-support/counter.svelte.ts');

test('Server rune loader compiles project modules beyond state and compare', () => {
    const first = new Counter(), second = new Counter();
    expect(first.snapshot()).toEqual({ value: 2, doubled: 4 });
    first.increment();
    expect(first.snapshot()).toEqual({ value: 3, doubled: 6 });
    expect(second.snapshot()).toEqual({ value: 2, doubled: 4 });
});

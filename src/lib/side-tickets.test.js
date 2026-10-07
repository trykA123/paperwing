import { expect, test } from 'bun:test';
import { withIpc } from './test-support/ipc-fixture';
import { MIXED_EOL_REASON, reopenSide } from './side-tickets';

const target = { sessionId: 's', generation: 1, fileId: 'f' };
const bytes = text => Array.from(new TextEncoder().encode(text));

async function reopen(text) {
    const calls = [];
    let result;
    await withIpc((command, args) => {
        calls.push([command, args.ticket ?? args.side]);
        if (command === 'file_edit_open') return { ticket: 'new', bytes: bytes(text), exists: true };
        return true;
    }, async () => { result = await reopenSide(target, 'left', { ticket: 'old', bytes: [], exists: true }); });
    return { result, calls };
}

test('an undone save that reopens with one line-ending style keeps its ticket', async () => {
    const { result, calls } = await reopen('one\r\ntwo\r\n');
    expect(result.ticket?.ticket).toBe('new');
    expect(result.reason).toBeNull();
    expect(calls.map(call => call[0])).toEqual(['file_edit_close', 'file_edit_open']);
});

test('an undone save that reopens with mixed line endings closes its ticket and stays read-only', async () => {
    const { result, calls } = await reopen('one\r\ntwo\n');
    expect(result.ticket).toBeNull();
    expect(result.reason).toBe(MIXED_EOL_REASON);
    expect(result.content.format.editable).toBe(false);
    expect(calls).toEqual([['file_edit_close', 'old'], ['file_edit_open', 'left'], ['file_edit_close', 'new']]);
});

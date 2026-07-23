import { describe, expect, it } from 'vitest';
import { createEmptyQueueState } from '../../domain/queueTypes';
import { createPrintSummary, queueReducer } from './queueReducer';

describe('queueReducer', () => {
  it('appends unique files and keeps existing items', () => {
    const first = queueReducer(createEmptyQueueState(), {
      type: 'append_files',
      paths: ['C:\\\\docs\\\\a.pdf', 'C:\\\\docs\\\\b.docx'],
    });
    const second = queueReducer(first, {
      type: 'append_files',
      paths: ['C:\\\\docs\\\\a.pdf', 'C:\\\\docs\\\\c.txt'],
    });
    expect(second.items).toHaveLength(3);
    expect(second.items.map((item) => item.fileName)).toEqual(['a.pdf', 'b.docx', 'c.txt']);
  });

  it('resets on clear and supports failed retry', () => {
    let state = queueReducer(createEmptyQueueState(), {
      type: 'append_files',
      paths: ['C:\\\\docs\\\\a.pdf'],
    });
    state = queueReducer(state, {
      type: 'finish_print',
      summary: createPrintSummary([
        {
          queueItemId: state.items[0].id,
          path: state.items[0].path,
          fileName: state.items[0].fileName,
          status: 'failed',
          message: 'demo',
        },
      ]),
    });
    expect(state.items[0].status).toBe('failed');
    state = queueReducer(state, { type: 'retry_failed' });
    expect(state.items[0].status).toBe('ready');
    state = queueReducer(state, { type: 'clear_queue' });
    expect(state.items).toHaveLength(0);
  });
});

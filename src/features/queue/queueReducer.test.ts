import { describe, expect, it } from 'vitest';
import { createEmptyQueueState } from '../../domain/queueTypes';
import {
  calculateBatchMetrics,
  createDefaultGlobalSettings,
  effectiveSidesMode,
  resolveDuplexAvailability,
  supportsDuplexForKind,
} from '../../domain/printSettings';
import { createPrintSummary, detectDocumentKind, queueReducer } from './queueReducer';

describe('detectDocumentKind', () => {
  it('recognizes common image formats as image', () => {
    const imagePaths = [
      'photo.webp',
      'scan.jfif',
      'camera.heic',
      'shot.avif',
      'logo.ico',
      'clip.emf',
      'raw.dib',
      'pic.jpe',
    ];
    for (const path of imagePaths) {
      expect(detectDocumentKind(path)).toBe('image');
    }
  });
});

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

  it('supports batch update and batch remove', () => {
    let state = queueReducer(createEmptyQueueState(), {
      type: 'append_files',
      paths: ['C:\\\\docs\\\\a.pdf', 'C:\\\\docs\\\\b.pdf', 'C:\\\\docs\\\\c.pdf'],
    });
    const idA = state.items[0].id;
    const idB = state.items[1].id;
    const idC = state.items[2].id;

    state = queueReducer(state, {
      type: 'batch_update_override',
      ids: [idA, idB],
      overridePatch: { colorMode: 'color', sidesMode: 'duplex' },
    });
    expect(state.items[0].override.colorMode).toBe('color');
    expect(state.items[0].override.sidesMode).toBe('duplex');
    expect(state.items[1].override.colorMode).toBe('color');
    expect(state.items[2].override.colorMode).toBeUndefined();

    state = queueReducer(state, {
      type: 'batch_remove',
      ids: [idA, idC],
    });
    expect(state.items).toHaveLength(1);
    expect(state.items[0].id).toBe(idB);
  });

  it('supports reordering items with move_item', () => {
    let state = queueReducer(createEmptyQueueState(), {
      type: 'append_files',
      paths: ['C:\\\\docs\\\\1.pdf', 'C:\\\\docs\\\\2.pdf', 'C:\\\\docs\\\\3.pdf'],
    });
    const id2 = state.items[1].id;

    state = queueReducer(state, {
      type: 'move_item',
      id: id2,
      direction: 'up',
    });
    expect(state.items.map((item) => item.fileName)).toEqual(['2.pdf', '1.pdf', '3.pdf']);

    state = queueReducer(state, {
      type: 'move_item',
      id: id2,
      direction: 'down',
    });
    expect(state.items.map((item) => item.fileName)).toEqual(['1.pdf', '2.pdf', '3.pdf']);
  });

  it('updates pageCount with set_item_page_count', () => {
    let state = queueReducer(createEmptyQueueState(), {
      type: 'append_files',
      paths: ['C:\\\\docs\\\\report.pdf'],
    });
    expect(state.items[0].pageCount).toBeNull();

    state = queueReducer(state, {
      type: 'set_item_page_count',
      id: state.items[0].id,
      pageCount: 18,
    });
    expect(state.items[0].pageCount).toBe(18);
  });
});


describe('calculateBatchMetrics', () => {
  it('correctly calculates total pages, sheets, and paper savings with duplex', () => {
    const globalSettings = createDefaultGlobalSettings('HP');
    globalSettings.sidesMode = 'duplex';

    const items = [
      { pageCount: 4, override: {} },
      { pageCount: 12, override: { pageRange: { mode: 'custom' as const, expression: '1,3,5-8' } } }, // 6 pages
      { pageCount: 1, override: { sidesMode: 'simplex' as const } }, // 1 page, simplex = 1 sheet
    ];

    const metrics = calculateBatchMetrics(items, globalSettings);
    // Item 1: 4 pages duplex -> 2 sheets
    // Item 2: 6 pages duplex -> 3 sheets
    // Item 3: 1 page simplex -> 1 sheet
    // Total pages = 4 + 6 + 1 = 11
    // Total sheets = 2 + 3 + 1 = 6
    // Saved sheets = 11 - 6 = 5
    expect(metrics.totalFiles).toBe(3);
    expect(metrics.totalPages).toBe(11);
    expect(metrics.totalSheets).toBe(6);
    expect(metrics.savedSheets).toBe(5);
    expect(metrics.savingPercentage).toBe(45);
  });
});

describe('duplex capability by file kind', () => {
  it('treats text files as unable to enforce duplex', () => {
    expect(supportsDuplexForKind('text')).toBe(false);
    expect(supportsDuplexForKind('unknown')).toBe(false);
    expect(supportsDuplexForKind('pdf')).toBe(true);
    expect(supportsDuplexForKind('word')).toBe(true);
  });

  it('reports why duplex is unavailable', () => {
    expect(
      resolveDuplexAvailability({ printerDuplexEnabled: false, kind: 'pdf' }).enabled,
    ).toBe(false);
    const text = resolveDuplexAvailability({ printerDuplexEnabled: true, kind: 'text' });
    expect(text.enabled).toBe(false);
    expect(text.reason).toContain('文本文件');
  });

  it('forces simplex for text so the UI never claims duplex', () => {
    const globalSettings = createDefaultGlobalSettings('HP');
    globalSettings.sidesMode = 'duplex';

    // Item 1: 8 pages but text -> duplex cannot be enforced -> 8 sheets
    // Item 2: 8 pages pdf -> duplex honored -> 4 sheets
    const metrics = calculateBatchMetrics(
      [
        { pageCount: 8, override: {}, kind: 'text' },
        { pageCount: 8, override: {}, kind: 'pdf' },
      ],
      globalSettings,
      true,
    );

    expect(metrics.totalPages).toBe(16);
    expect(metrics.totalSheets).toBe(12);
    expect(metrics.savedSheets).toBe(4);
  });

  it('does not halve sheets when the printer cannot duplex', () => {
    const globalSettings = createDefaultGlobalSettings('HP');
    globalSettings.sidesMode = 'duplex';

    const metrics = calculateBatchMetrics(
      [{ pageCount: 10, override: {}, kind: 'pdf' }],
      globalSettings,
      false,
    );

    expect(metrics.totalSheets).toBe(10);
    expect(metrics.savedSheets).toBe(0);
  });

  it('effectiveSidesMode degrades to simplex when duplex is unavailable', () => {
    const unavailable = resolveDuplexAvailability({
      printerDuplexEnabled: true,
      kind: 'text',
    });
    expect(effectiveSidesMode('duplex', unavailable)).toBe('simplex');

    const available = resolveDuplexAvailability({
      printerDuplexEnabled: true,
      kind: 'pdf',
    });
    expect(effectiveSidesMode('duplex', available)).toBe('duplex');
  });
});


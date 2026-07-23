import type {
  PrintJobResultItem,
  PrintJobSummary,
  QueueItem,
  QueueState,
  SupportedDocumentKind,
} from '../../domain/queueTypes';
import { createEmptyQueueState } from '../../domain/queueTypes';
import type { FileSettingsOverride } from '../../domain/printSettings';

export type QueueAction =
  | { type: 'append_files'; paths: string[] }
  | { type: 'remove_item'; id: string }
  | { type: 'clear_queue' }
  | { type: 'update_override'; id: string; override: FileSettingsOverride }
  | { type: 'set_item_status'; id: string; status: QueueItem['status']; errorMessage?: string }
  | { type: 'begin_print' }
  | { type: 'finish_print'; summary: PrintJobSummary }
  | { type: 'retry_failed' }
  | { type: 'move_item'; id: string; direction: 'up' | 'down' };

const SUPPORTED_EXTENSIONS: Record<string, SupportedDocumentKind> = {
  pdf: 'pdf',
  // 位图 / 常见照片
  png: 'image',
  jpg: 'image',
  jpeg: 'image',
  jpe: 'image',
  jfif: 'image',
  bmp: 'image',
  dib: 'image',
  tif: 'image',
  tiff: 'image',
  gif: 'image',
  webp: 'image',
  ico: 'image',
  // 现代/移动端格式（依赖系统编解码与关联程序）
  heic: 'image',
  heif: 'image',
  avif: 'image',
  // Windows 图元文件
  emf: 'image',
  wmf: 'image',
  txt: 'text',
  log: 'text',
  md: 'text',
  doc: 'word',
  docx: 'word',
  xls: 'excel',
  xlsx: 'excel',
  ppt: 'powerpoint',
  pptx: 'powerpoint',
};

export function detectDocumentKind(filePath: string): SupportedDocumentKind {
  const extension = filePath.split('.').pop()?.toLowerCase() ?? '';
  return SUPPORTED_EXTENSIONS[extension] ?? 'unknown';
}

export function extractFileName(filePath: string): string {
  const normalizedPath = filePath.replace(/\//g, '\\');
  const segments = normalizedPath.split('\\');
  return segments[segments.length - 1] || filePath;
}

function createQueueItem(filePath: string): QueueItem {
  const kind = detectDocumentKind(filePath);
  return {
    id: `${filePath}::${Date.now()}::${Math.random().toString(36).slice(2, 8)}`,
    path: filePath,
    fileName: extractFileName(filePath),
    kind,
    pageCount: null,
    status: kind === 'unknown' ? 'failed' : 'ready',
    override: {},
    errorMessage: kind === 'unknown' ? '不支持的文件类型' : undefined,
    addedAt: Date.now(),
  };
}

function normalizePathKey(filePath: string): string {
  return filePath.replace(/\//g, '\\').toLowerCase();
}

export function queueReducer(state: QueueState, action: QueueAction): QueueState {
  switch (action.type) {
    case 'append_files': {
      const existingPathKeys = new Set(state.items.map((item) => normalizePathKey(item.path)));
      const nextItems = [...state.items];

      for (const filePath of action.paths) {
        const pathKey = normalizePathKey(filePath);
        if (existingPathKeys.has(pathKey)) {
          continue;
        }
        existingPathKeys.add(pathKey);
        nextItems.push(createQueueItem(filePath));
      }

      return {
        ...state,
        items: nextItems,
        lastSummary: null,
      };
    }

    case 'remove_item':
      return {
        ...state,
        items: state.items.filter((item) => item.id !== action.id),
      };

    case 'clear_queue':
      return createEmptyQueueState();

    case 'update_override':
      return {
        ...state,
        items: state.items.map((item) =>
          item.id === action.id
            ? {
                ...item,
                override: action.override,
              }
            : item,
        ),
      };

    case 'set_item_status':
      return {
        ...state,
        items: state.items.map((item) =>
          item.id === action.id
            ? {
                ...item,
                status: action.status,
                errorMessage: action.errorMessage,
              }
            : item,
        ),
      };

    case 'begin_print':
      return {
        ...state,
        isPrinting: true,
        lastSummary: null,
        items: state.items.map((item) =>
          item.status === 'failed' || item.status === 'succeeded' || item.status === 'skipped'
            ? item
            : { ...item, status: 'pending', errorMessage: undefined },
        ),
      };

    case 'finish_print': {
      const resultById = new Map(
        action.summary.results.map((resultItem) => [resultItem.queueItemId, resultItem]),
      );

      return {
        ...state,
        isPrinting: false,
        lastSummary: action.summary,
        items: state.items.map((item) => {
          const resultItem = resultById.get(item.id);
          if (!resultItem) {
            return item;
          }
          return {
            ...item,
            status: resultItem.status,
            errorMessage: resultItem.message,
          };
        }),
      };
    }

    case 'retry_failed':
      return {
        ...state,
        lastSummary: null,
        items: state.items.map((item) =>
          item.status === 'failed'
            ? {
                ...item,
                status: 'ready',
                errorMessage: undefined,
              }
            : item,
        ),
      };

    case 'move_item': {
      const currentIndex = state.items.findIndex((item) => item.id === action.id);
      if (currentIndex < 0) {
        return state;
      }

      const targetIndex =
        action.direction === 'up' ? currentIndex - 1 : currentIndex + 1;
      if (targetIndex < 0 || targetIndex >= state.items.length) {
        return state;
      }

      const nextItems = [...state.items];
      const [movedItem] = nextItems.splice(currentIndex, 1);
      nextItems.splice(targetIndex, 0, movedItem);
      return {
        ...state,
        items: nextItems,
      };
    }

    default:
      return state;
  }
}

export function createPrintSummary(results: PrintJobResultItem[]): PrintJobSummary {
  return {
    succeeded: results.filter((resultItem) => resultItem.status === 'succeeded').length,
    failed: results.filter((resultItem) => resultItem.status === 'failed').length,
    skipped: results.filter((resultItem) => resultItem.status === 'skipped').length,
    results,
  };
}

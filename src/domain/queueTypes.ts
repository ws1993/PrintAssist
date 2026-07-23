import type { FileSettingsOverride } from './printSettings';

export type QueueItemStatus =
  | 'pending'
  | 'analyzing'
  | 'ready'
  | 'printing'
  | 'succeeded'
  | 'failed'
  | 'skipped';

export type SupportedDocumentKind =
  | 'pdf'
  | 'image'
  | 'text'
  | 'word'
  | 'excel'
  | 'powerpoint'
  | 'unknown';

export interface QueueItem {
  id: string;
  path: string;
  fileName: string;
  kind: SupportedDocumentKind;
  pageCount: number | null;
  status: QueueItemStatus;
  override: FileSettingsOverride;
  errorMessage?: string;
  addedAt: number;
}

export interface PrintJobResultItem {
  queueItemId: string;
  path: string;
  fileName: string;
  status: 'succeeded' | 'failed' | 'skipped';
  message?: string;
}

export interface PrintJobSummary {
  succeeded: number;
  failed: number;
  skipped: number;
  results: PrintJobResultItem[];
}

export interface QueueState {
  items: QueueItem[];
  isPrinting: boolean;
  lastSummary: PrintJobSummary | null;
}

export function createEmptyQueueState(): QueueState {
  return {
    items: [],
    isPrinting: false,
    lastSummary: null,
  };
}

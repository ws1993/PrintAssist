import type { ColorMode, FlipMode, SidesMode } from '../../domain/printSettings';

export interface ResolvedPrintSettingsPayload {
  printerName: string;
  colorMode: ColorMode;
  sidesMode: SidesMode;
  flipMode: FlipMode;
  copies: number;
  pageRangeMode: 'all' | 'custom';
  pageRangeExpression: string;
}

export interface PrintQueueItemPayload {
  queueItemId: string;
  path: string;
  fileName: string;
  settings: ResolvedPrintSettingsPayload;
  allowAssociationFallback: boolean;
}

export interface PrintBatchRequest {
  items: PrintQueueItemPayload[];
}

export interface PrintBatchResultItem {
  queueItemId: string;
  path: string;
  fileName: string;
  status: 'succeeded' | 'failed' | 'skipped';
  message?: string;
}

export interface PrintBatchResult {
  succeeded: number;
  failed: number;
  skipped: number;
  results: PrintBatchResultItem[];
}

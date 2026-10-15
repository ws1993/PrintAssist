import type { SystemPrinter } from '../shared/contracts/printer';
import { parsePageRangeExpression, type PageRangeInput } from './pageRange';


export type ColorMode = 'color' | 'monochrome';
export type SidesMode = 'simplex' | 'duplex';
export type FlipMode = 'longEdge' | 'shortEdge';

export interface PrintSettings {
  printerName: string;
  colorMode: ColorMode;
  sidesMode: SidesMode;
  flipMode: FlipMode;
  copies: number;
  pageRange: PageRangeInput;
}

export interface FileSettingsOverride {
  colorMode?: ColorMode;
  sidesMode?: SidesMode;
  flipMode?: FlipMode;
  copies?: number;
  pageRange?: PageRangeInput;
}

export function createDefaultGlobalSettings(printerName = ''): PrintSettings {
  return {
    printerName,
    colorMode: 'monochrome',
    sidesMode: 'duplex',
    flipMode: 'longEdge',
    copies: 1,
    pageRange: {
      mode: 'all',
      expression: '',
    },
  };
}

export function mergePrintSettings(
  globalSettings: PrintSettings,
  fileOverride: FileSettingsOverride = {},
): PrintSettings {
  return {
    printerName: globalSettings.printerName,
    colorMode: fileOverride.colorMode ?? globalSettings.colorMode,
    sidesMode: fileOverride.sidesMode ?? globalSettings.sidesMode,
    flipMode: fileOverride.flipMode ?? globalSettings.flipMode,
    copies: fileOverride.copies ?? globalSettings.copies,
    pageRange: fileOverride.pageRange ?? globalSettings.pageRange,
  };
}

export function hasFileOverride(fileOverride: FileSettingsOverride): boolean {
  return Object.keys(fileOverride).length > 0;
}

/**
 * 文件类型是否能让双面真正下发到打印机。
 *
 * 文本文件走系统关联程序（`printto`）打印，DEVMODE 由关联程序持有，应用无法强制双面；
 * 未识别类型本就无法打印，同样按不支持处理。未提供 kind 时按可双面处理，
 * 与既有行为保持一致。
 */
export function supportsDuplexForKind(kind?: string): boolean {
  return kind !== 'text' && kind !== 'unknown';
}

export interface DuplexAvailability {
  enabled: boolean;
  reason?: string;
}

/**
 * 双面可用性 = 打印机能力 且 文件类型可下发。
 * 用于让界面按实际情况展示，而不是显示一个不会生效的“双面”。
 */
export function resolveDuplexAvailability(options: {
  printerDuplexEnabled: boolean;
  kind?: string;
}): DuplexAvailability {
  if (!options.printerDuplexEnabled) {
    return { enabled: false, reason: '当前打印机不支持双面' };
  }
  if (!supportsDuplexForKind(options.kind)) {
    return {
      enabled: false,
      reason: '文本文件由系统关联程序打印，无法强制双面，将按单面输出',
    };
  }
  return { enabled: true };
}

/**
 * 实际会送达打印机的单双面模式：能力不可用时一律按单面计。
 */
export function effectiveSidesMode(
  sidesMode: SidesMode,
  duplex: DuplexAvailability,
): SidesMode {
  return duplex.enabled ? sidesMode : 'simplex';
}

export interface SettingAvailability {
  colorEnabled: boolean;
  duplexEnabled: boolean;
  flipEnabled: boolean;
  printEnabled: boolean;
  reasons: string[];
}

export function evaluateSettingAvailability(
  printer: SystemPrinter | undefined,
): SettingAvailability {
  if (!printer) {
    return {
      colorEnabled: false,
      duplexEnabled: false,
      flipEnabled: false,
      printEnabled: false,
      reasons: ['尚未选择打印机'],
    };
  }

  const reasons: string[] = [];
  const colorEnabled = printer.color.support === 'supported';
  const duplexEnabled = printer.duplex.support === 'supported';
  const flipEnabled = duplexEnabled;
  const printEnabled = printer.state !== 'offline' && printer.state !== 'error';

  if (printer.state === 'offline') {
    reasons.push('打印机离线');
  } else if (printer.state === 'error') {
    reasons.push('打印机处于错误状态');
  }

  if (printer.color.support === 'unsupported') {
    reasons.push('当前打印机不支持彩色');
  } else if (printer.color.support === 'unknown') {
    reasons.push(printer.color.detail ?? '彩色能力未知');
  }

  if (printer.duplex.support === 'unsupported') {
    reasons.push('当前打印机不支持双面');
  } else if (printer.duplex.support === 'unknown') {
    reasons.push(printer.duplex.detail ?? '双面能力未知');
  }

  if (printer.error) {
    reasons.push(printer.error);
  }

  return {
    colorEnabled,
    duplexEnabled,
    flipEnabled,
    printEnabled,
    reasons,
  };
}

export function sanitizeSettingsForPrinter(
  settings: PrintSettings,
  printer: SystemPrinter | undefined,
): PrintSettings {
  const availability = evaluateSettingAvailability(printer);
  const nextSettings = { ...settings };

  if (!availability.colorEnabled && nextSettings.colorMode === 'color') {
    nextSettings.colorMode = 'monochrome';
  }

  if (!availability.duplexEnabled && nextSettings.sidesMode === 'duplex') {
    nextSettings.sidesMode = 'simplex';
  }

  return nextSettings;
}

export interface BatchMetrics {
  totalFiles: number;
  totalPages: number;
  totalSheets: number;
  savedSheets: number;
  savingPercentage: number;
  colorFilesCount: number;
  monochromeFilesCount: number;
}

/**
 * Calculates overall batch pages, physical sheet consumption, and paper savings.
 */
export function calculateBatchMetrics(
  items: Array<{
    pageCount: number | null;
    override?: FileSettingsOverride;
    kind?: string;
  }>,
  globalSettings: PrintSettings,
  printerDuplexEnabled = true,
): BatchMetrics {
  let totalPages = 0;
  let totalSheets = 0;
  let colorFilesCount = 0;
  let monochromeFilesCount = 0;

  for (const item of items) {
    const merged = mergePrintSettings(globalSettings, item.override);
    if (merged.colorMode === 'color') {
      colorFilesCount += 1;
    } else {
      monochromeFilesCount += 1;
    }

    let itemPages = item.pageCount && item.pageCount > 0 ? item.pageCount : 1;
    if (merged.pageRange.mode === 'custom' && merged.pageRange.expression.trim()) {
      const parsed = parsePageRangeExpression(
        merged.pageRange.expression,
        item.pageCount ?? undefined,
      );
      if (parsed.ok) {
        itemPages = parsed.pages.length;
      }
    }

    const copies = merged.copies > 0 ? merged.copies : 1;
    const pagesForThisItem = itemPages * copies;
    const duplex = resolveDuplexAvailability({
      printerDuplexEnabled,
      kind: item.kind,
    });
    const sheetsPerCopy =
      effectiveSidesMode(merged.sidesMode, duplex) === 'duplex'
        ? Math.ceil(itemPages / 2)
        : itemPages;
    const sheetsForThisItem = sheetsPerCopy * copies;

    totalPages += pagesForThisItem;
    totalSheets += sheetsForThisItem;
  }

  const savedSheets = Math.max(0, totalPages - totalSheets);
  const savingPercentage =
    totalPages > 0 ? Math.round((savedSheets / totalPages) * 100) : 0;

  return {
    totalFiles: items.length,
    totalPages,
    totalSheets,
    savedSheets,
    savingPercentage,
    colorFilesCount,
    monochromeFilesCount,
  };
}


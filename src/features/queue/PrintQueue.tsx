import {
  BookOpen,
  ChevronDown,
  Copy,
  FileCode2,
  FileSpreadsheet,
  FileText,
  Files,
  Image as ImageIcon,
  Inbox,
  Leaf,
  Loader2,
  Palette,
  Play,
  SlidersHorizontal,
  Trash,
  Trash2,
} from 'lucide-react';

import { useEffect, useMemo, useRef, useState } from 'react';
import type { QueueItem } from '../../domain/queueTypes';
import { describePageRange } from '../../domain/pageRange';
import { message } from 'antd';
import {
  calculateBatchMetrics,
  effectiveSidesMode,
  hasFileOverride,
  mergePrintSettings,
  resolveDuplexAvailability,
  supportsDuplexForKind,
  type FileSettingsOverride,
  type PrintSettings,
} from '../../domain/printSettings';

interface PrintQueueProps {
  items: QueueItem[];
  globalSettings: PrintSettings;
  isPrinting: boolean;
  colorEnabled?: boolean;
  duplexEnabled?: boolean;
  onRemove: (id: string) => void;
  onBatchRemove: (ids: string[]) => void;
  onOpenSettings: (id: string) => void;
  onUpdateOverride: (id: string, override: FileSettingsOverride) => void;
  onBatchUpdateOverride: (ids: string[], patch: Partial<FileSettingsOverride>) => void;
  onClearQueue: () => void;
  onStartPrint: () => void;
  printDisabled?: boolean;
  onPickFiles?: () => void;
}


function getKindInfo(kind: QueueItem['kind']) {
  switch (kind) {
    case 'pdf':
      return { className: 'file-type-pdf', text: 'PDF' };
    case 'excel':
      return { className: 'file-type-excel', text: 'XLSX' };
    case 'word':
      return { className: 'file-type-word', text: 'DOC' };
    case 'powerpoint':
      return { className: 'file-type-ppt', text: 'PPT' };
    case 'image':
      return { className: 'file-type-img', text: 'IMG' };
    case 'text':
      return { className: 'file-type-txt', text: 'TXT' };
    default:
      return { className: 'file-type-txt', text: 'FILE' };
  }
}

function renderStatusBadge(status: QueueItem['status'], errorMessage?: string) {
  if (status === 'printing') {
    return (
      <span className="status-badge status-printing">
        <Loader2 size={12} className="spin" /> 正在打印...
      </span>
    );
  }
  if (status === 'succeeded') {
    return (
      <span className="status-badge status-success">
        ✓ 打印成功
      </span>
    );
  }
  if (status === 'failed') {
    return (
      <div className="status-cell">
        <span className="status-badge status-failed">
          ✕ 打印失败
        </span>
        {errorMessage && <span className="error-msg" title={errorMessage}>{errorMessage}</span>}
      </div>
    );
  }
  if (status === 'analyzing') {
    return (
      <span className="status-badge status-analyzing">
        <Loader2 size={12} className="spin" /> 页面分析中
      </span>
    );
  }
  return (
    <span className="status-badge status-ready">
      待打印
    </span>
  );
}

export function PrintQueue({
  items,
  globalSettings,
  isPrinting,
  colorEnabled = true,
  duplexEnabled = true,
  onRemove,
  onBatchRemove,
  onOpenSettings,
  onUpdateOverride,
  onBatchUpdateOverride,
  onClearQueue,
  onStartPrint,
  printDisabled = false,
  onPickFiles,
}: PrintQueueProps) {

  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [batchMenuOpen, setBatchMenuOpen] = useState(false);
  const [batchCopies, setBatchCopies] = useState(1);
  const batchMenuContainerRef = useRef<HTMLDivElement | null>(null);

  // Clean up selected IDs that don't exist anymore
  const validSelectedIds = useMemo(() => {
    const valid = new Set<string>();
    for (const item of items) {
      if (selectedIds.has(item.id)) {
        valid.add(item.id);
      }
    }
    return valid;
  }, [selectedIds, items]);

  const metrics = useMemo(
    () => calculateBatchMetrics(items, globalSettings, duplexEnabled),
    [items, globalSettings, duplexEnabled],
  );

  const isAllSelected = items.length > 0 && validSelectedIds.size === items.length;
  const hasSelection = validSelectedIds.size > 0;

  // Determine target IDs for batch operations: if items are checked, use them; otherwise apply to whole queue
  const targetIds = useMemo(() => {
    if (validSelectedIds.size > 0) {
      return Array.from(validSelectedIds);
    }
    return items.map((item) => item.id);
  }, [validSelectedIds, items]);

  const targetLabel = validSelectedIds.size > 0 ? `已选 ${validSelectedIds.size} 项` : `全部 ${items.length} 项`;

  // 批量双面只对“真正能下发双面”的文件生效，避免把无效承诺写进任务
  const duplexTargetIds = useMemo(
    () =>
      targetIds.filter((id) => {
        const item = items.find((candidate) => candidate.id === id);
        return supportsDuplexForKind(item?.kind);
      }),
    [targetIds, items],
  );

  // Close floating batch menu on outside click or escape
  useEffect(() => {
    if (!batchMenuOpen) return;

    const handlePointerDownOutside = (event: MouseEvent) => {
      if (
        batchMenuContainerRef.current &&
        !batchMenuContainerRef.current.contains(event.target as Node)
      ) {
        setBatchMenuOpen(false);
      }
    };

    const handleEscapeKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setBatchMenuOpen(false);
      }
    };

    document.addEventListener('mousedown', handlePointerDownOutside);
    document.addEventListener('keydown', handleEscapeKey);
    return () => {
      document.removeEventListener('mousedown', handlePointerDownOutside);
      document.removeEventListener('keydown', handleEscapeKey);
    };
  }, [batchMenuOpen]);

  const handleToggleSelectAll = (checked: boolean) => {
    if (checked) {
      setSelectedIds(new Set(items.map((item) => item.id)));
    } else {
      setSelectedIds(new Set());
    }
  };

  const handleToggleItem = (id: string, checked: boolean) => {
    const next = new Set(validSelectedIds);
    if (checked) {
      next.add(id);
    } else {
      next.delete(id);
    }
    setSelectedIds(next);
  };

  const handleQuickToggleColor = (item: QueueItem) => {
    if (isPrinting) return;
    const resolved = mergePrintSettings(globalSettings, item.override);
    const nextColor = resolved.colorMode === 'color' ? 'monochrome' : 'color';
    onUpdateOverride(item.id, {
      ...item.override,
      colorMode: nextColor,
    });
  };

  const handleQuickToggleSides = (item: QueueItem) => {
    if (isPrinting) return;
    const duplex = resolveDuplexAvailability({
      printerDuplexEnabled: duplexEnabled,
      kind: item.kind,
    });
    if (!duplex.enabled) {
      message.info(duplex.reason ?? '当前无法使用双面打印');
      return;
    }
    const resolved = mergePrintSettings(globalSettings, item.override);
    const nextSides = resolved.sidesMode === 'duplex' ? 'simplex' : 'duplex';
    onUpdateOverride(item.id, {
      ...item.override,
      sidesMode: nextSides,
      flipMode: resolved.flipMode || 'longEdge',
    });
  };

  return (
    <div className="main-workspace-inner">
      {/* 顶部工具条 */}
      <div className="action-toolbar">
        <div className="toolbar-left">
          <div className="batch-selection-box">
            <input
              type="checkbox"
              className="checkbox"
              checked={isAllSelected}
              disabled={isPrinting || items.length === 0}
              onChange={(e) => handleToggleSelectAll(e.target.checked)}
            />
            <span className="selection-count-text">已选 {validSelectedIds.size} 项</span>
          </div>

          <div className="batch-actions-group" ref={batchMenuContainerRef}>
            <button
              type="button"
              className={`btn-batch-trigger ${batchMenuOpen ? 'active' : ''}`}
              disabled={items.length === 0 || isPrinting}
              onClick={() => setBatchMenuOpen((prev) => !prev)}
            >
              <SlidersHorizontal size={13} />
              <span>
                批量设置 {validSelectedIds.size > 0 ? `(${validSelectedIds.size})` : ''}
              </span>
              <ChevronDown size={13} className={`caret-down ${batchMenuOpen ? 'open' : ''}`} />
            </button>

            {/* 悬浮弹窗面板 */}
            {batchMenuOpen && (
              <div className="batch-settings-popover">
                <div className="batch-pop-header">
                  <span>批量设置参数 ({targetLabel})</span>
                  {validSelectedIds.size === 0 && (
                    <span className="batch-pop-hint">未勾选时默认作用于全部文件</span>
                  )}
                </div>

                {/* 色彩设置 */}
                <div className="batch-pop-section">
                  <div className="batch-pop-label">色彩模式</div>
                  <div className="batch-pop-btns">
                    <button
                      type="button"
                      className="btn-batch-pop"
                      onClick={() => {
                        onBatchUpdateOverride(targetIds, { colorMode: 'monochrome' });
                      }}
                    >
                      ⚫ 设为黑白
                    </button>
                    <button
                      type="button"
                      className="btn-batch-pop"
                      disabled={!colorEnabled}
                      onClick={() => {
                        onBatchUpdateOverride(targetIds, { colorMode: 'color' });
                      }}
                    >
                      🎨 设为彩色
                    </button>
                  </div>
                </div>

                {/* 单双面设置 */}
                <div className="batch-pop-section">
                  <div className="batch-pop-label">单双面与装订</div>
                  <div className="batch-pop-btns">
                    <button
                      type="button"
                      className="btn-batch-pop"
                      onClick={() => {
                        onBatchUpdateOverride(targetIds, { sidesMode: 'simplex' });
                      }}
                    >
                      📃 单面
                    </button>
                    <button
                      type="button"
                      className="btn-batch-pop"
                      disabled={!duplexEnabled || duplexTargetIds.length === 0}
                      onClick={() => {
                        onBatchUpdateOverride(duplexTargetIds, {
                          sidesMode: 'duplex',
                          flipMode: 'longEdge',
                        });
                      }}
                    >
                      📄 双面长边
                    </button>
                    <button
                      type="button"
                      className="btn-batch-pop"
                      disabled={!duplexEnabled || duplexTargetIds.length === 0}
                      onClick={() => {
                        onBatchUpdateOverride(duplexTargetIds, {
                          sidesMode: 'duplex',
                          flipMode: 'shortEdge',
                        });
                      }}
                    >
                      📑 双面短边
                    </button>
                  </div>
                </div>

                {/* 批量份数设置 */}
                <div className="batch-pop-section">
                  <div className="batch-pop-label">打印份数</div>
                  <div className="batch-copies-row">
                    <div className="stepper-input" style={{ width: '100px' }}>
                      <button
                        type="button"
                        className="stepper-btn"
                        onClick={() => setBatchCopies((c) => Math.max(1, c - 1))}
                      >
                        -
                      </button>
                      <div className="stepper-val">{batchCopies} 份</div>
                      <button
                        type="button"
                        className="stepper-btn"
                        onClick={() => setBatchCopies((c) => Math.min(99, c + 1))}
                      >
                        +
                      </button>
                    </div>
                    <button
                      type="button"
                      className="btn-batch-pop active-btn"
                      style={{ flex: 1 }}
                      onClick={() => {
                        onBatchUpdateOverride(targetIds, { copies: batchCopies });
                      }}
                    >
                      应用份数
                    </button>
                  </div>
                </div>

                <div className="batch-pop-divider" />

                <button
                  type="button"
                  className="btn-batch-pop btn-batch-pop-danger"
                  onClick={() => {
                    onBatchRemove(targetIds);
                    setSelectedIds(new Set());
                    setBatchMenuOpen(false);
                  }}
                >
                  <Trash2 size={13} /> 批量移除 ({targetLabel})
                </button>
              </div>
            )}

            <button
              type="button"
              className="btn-batch btn-batch-danger"
              disabled={items.length === 0 || isPrinting}
              onClick={() => {
                onBatchRemove(targetIds);
                setSelectedIds(new Set());
              }}
              title="批量删除"
            >
              <Trash2 size={13} />
            </button>
          </div>
        </div>

        {/* 耗材与纸张智能计算条 */}
        <div className="paper-calculator-bar">
          <div className="eco-stat">
            <Files size={14} />
            <span>{metrics.totalFiles} 个文件</span>
          </div>
          <span className="eco-stat-divider">•</span>
          <div className="eco-stat">
            <BookOpen size={14} />
            <span>{metrics.totalPages} 页内容</span>
          </div>
          {metrics.savedSheets > 0 && (
            <>
              <span className="eco-stat-divider">•</span>
              <div className="eco-stat eco-stat-highlight">
                <Leaf size={14} />
                <span>
                  🌱 双面节约 {metrics.savedSheets} 页 ({metrics.savingPercentage}%)
                </span>
              </div>
            </>
          )}
        </div>

        <div className="toolbar-right">
          <button
            type="button"
            className="btn-ghost-danger"
            disabled={isPrinting || items.length === 0}
            onClick={onClearQueue}
          >
            <Trash size={14} /> 清空
          </button>
          <button
            type="button"
            className="btn-primary-print"
            disabled={isPrinting || items.length === 0 || printDisabled}
            onClick={onStartPrint}
          >
            {isPrinting ? (
              <>
                <Loader2 size={16} className="spin" />
                <span>打印中...</span>
              </>
            ) : (
              <>
                <Play size={16} fill="currentColor" />
                <span>开始打印</span>
              </>
            )}
          </button>
        </div>
      </div>

      {/* 队列内容区域 */}
      <div className="queue-content-wrap">
        {items.length === 0 ? (
          <div className="queue-empty-card">
            <div className="empty-icon-wrap">
              <Inbox size={44} />
            </div>
            <div className="empty-title">队列为空</div>
            <div className="empty-subtitle">
              请在左侧拖放文件、选择文件或通过 Windows 资源管理器右键“发送到”追加
            </div>
            {onPickFiles && (
              <button
                type="button"
                className="btn-secondary"
                style={{ marginTop: '12px', padding: '0 12px' }}
                onClick={onPickFiles}
              >
                选择文件追加
              </button>
            )}
          </div>
        ) : (
          <div className="queue-table-card">
            <div className="table-header-row">
              <span>选</span>
              <span>文件名称与路径</span>
              <span>页数</span>
              <span>打印参数 (快捷微调)</span>
              <span>当前状态</span>
              <span>操作</span>
            </div>

            <div className="queue-list">
              {items.map((item) => {
                const isSelected = validSelectedIds.has(item.id);
                const resolved = mergePrintSettings(globalSettings, item.override);
                const isColorOverridden = item.override.colorMode !== undefined;
                const isSidesOverridden = item.override.sidesMode !== undefined;
                const isRangeOverridden = item.override.pageRange !== undefined;
                const isCopiesOverridden = item.override.copies !== undefined;
                const kindInfo = getKindInfo(item.kind);
                const itemDuplex = resolveDuplexAvailability({
                  printerDuplexEnabled: duplexEnabled,
                  kind: item.kind,
                });
                const effectiveSides = effectiveSidesMode(resolved.sidesMode, itemDuplex);

                return (
                  <div
                    key={item.id}
                    className={`queue-row ${isSelected ? 'selected' : ''} ${
                      item.status === 'printing' ? 'printing' : ''
                    } ${item.status === 'failed' ? 'failed' : ''}`}
                  >
                    <div className="row-check-cell">
                      <input
                        type="checkbox"
                        className="checkbox"
                        checked={isSelected}
                        disabled={isPrinting}
                        onChange={(e) => handleToggleItem(item.id, e.target.checked)}
                      />
                    </div>


                    <div className="file-info-cell">
                      <div className={`file-type-icon ${kindInfo.className}`}>
                        {kindInfo.text}
                      </div>
                      <div className="file-meta-col">
                        <span className="file-name-text" title={item.fileName}>
                          {item.fileName}
                        </span>
                        <span className="file-path-text" title={item.path}>
                          {item.path}
                        </span>
                      </div>
                    </div>

                    <div>
                      <span className="page-count-badge">
                        <FileText size={12} style={{ opacity: 0.6 }} />
                        {item.pageCount !== null ? `${item.pageCount} 页` : '—'}
                      </span>
                    </div>

                    <div className="inline-settings-cell">
                      <div className="quick-toggle-bar">
                        <span
                          className={`quick-chip ${isColorOverridden ? 'active-override' : ''}`}
                          onClick={() => handleQuickToggleColor(item)}
                          title="点击快速切换色彩模式"
                        >
                          {resolved.colorMode === 'color' ? '🎨 彩色' : '⚫ 黑白'}
                        </span>
                        <span
                          className={`quick-chip ${isSidesOverridden ? 'active-override' : ''} ${
                            itemDuplex.enabled ? '' : 'locked'
                          }`}
                          onClick={() => handleQuickToggleSides(item)}
                          title={
                            itemDuplex.enabled
                              ? '点击快速切换单双面'
                              : `${itemDuplex.reason}；已按实际能力显示为单面`
                          }
                        >
                          {effectiveSides === 'duplex'
                            ? `📄 双面(${resolved.flipMode === 'shortEdge' ? '短边' : '长边'})`
                            : itemDuplex.enabled
                              ? '📃 单面'
                              : '📃 单面（类型限制）'}
                        </span>
                        {isCopiesOverridden && (
                          <span
                            className="quick-chip active-override"
                            onClick={() => onOpenSettings(item.id)}
                            title="份数覆盖"
                          >
                            {resolved.copies} 份
                          </span>
                        )}
                        <span
                          className={`quick-chip ${isRangeOverridden ? 'active-override' : ''}`}
                          onClick={() => onOpenSettings(item.id)}
                          title="点击在抽屉中配置自定义页码"
                        >
                          {describePageRange(resolved.pageRange)}
                        </span>
                      </div>
                      <span className="scope-note">
                        {isColorOverridden || isSidesOverridden || isRangeOverridden || isCopiesOverridden
                          ? '⚡ 含有独立覆盖'
                          : '· 继承公共设置'}
                      </span>
                    </div>

                    <div className="status-cell">
                      {renderStatusBadge(item.status, item.errorMessage)}
                    </div>

                    <div className="actions-cell">
                      <button
                        type="button"
                        className="btn-row-action"
                        title="单文件高级设置"
                        disabled={isPrinting}
                        onClick={() => onOpenSettings(item.id)}
                      >
                        <SlidersHorizontal size={14} />
                      </button>
                      <button
                        type="button"
                        className="btn-row-action danger"
                        title="移除此文件"
                        disabled={isPrinting}
                        onClick={() => onRemove(item.id)}
                      >
                        <Trash2 size={14} />
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

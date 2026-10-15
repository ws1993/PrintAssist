import {
  Button,
  ConfigProvider,
  Modal,
  message,
} from 'antd';
import {
  FilePlus2,
  FolderPlus,
  Keyboard,
  Printer,
  RefreshCw,
  Settings,
  Sparkles,
  UploadCloud,
} from 'lucide-react';
import { useCallback, useEffect, useMemo, useReducer, useState } from 'react';
import type { DragEvent } from 'react';
import {
  checkForAppUpdate,
  expandFilePaths,
  isTauriRuntime,
  listSystemPrinters,
  pickFiles,
  pickFolderFiles,
  probeDocumentInfo,
  runPrintBatch,
  subscribeIncomingFiles,
  subscribeNativeDragDrop,
} from './api/nativeBridge';

import {
  createDefaultGlobalSettings,
  effectiveSidesMode,
  evaluateSettingAvailability,
  mergePrintSettings,
  resolveDuplexAvailability,
  sanitizeSettingsForPrinter,
  type PrintSettings,
} from './domain/printSettings';
import { createEmptyQueueState } from './domain/queueTypes';
import { parsePageRangeExpression } from './domain/pageRange';
import { PrintQueue } from './features/queue/PrintQueue';
import { createPrintSummary, queueReducer } from './features/queue/queueReducer';
import { PrintSummary } from './features/results/PrintSummary';
import { FileSettingsDrawer } from './features/settings/FileSettingsDrawer';
import { GlobalSettingsPanel } from './features/settings/GlobalSettingsPanel';
import { ProxySettingsPanel } from './features/settings/ProxySettingsPanel';
import { UpdateModal, type UpdateInfo } from './features/update/UpdateModal';
import {
  createDefaultProxySettings,
  getProxyConfig,
  type ProxySettings,
} from './domain/proxySettings';
import type { SystemPrinter } from './shared/contracts/printer';
import type { PrintQueueItemPayload } from './shared/contracts/printJob';

export function App() {
  const [queueState, dispatch] = useReducer(queueReducer, undefined, createEmptyQueueState);
  const [printers, setPrinters] = useState<SystemPrinter[]>([]);
  const [loadingPrinters, setLoadingPrinters] = useState(true);
  const [globalSettings, setGlobalSettings] = useState<PrintSettings>(
    createDefaultGlobalSettings(),
  );
  const [settingsItemId, setSettingsItemId] = useState<string | null>(null);
  const [proxyModalOpen, setProxyModalOpen] = useState(false);
  const [isDragOver, setIsDragOver] = useState(false);
  const [allowAssociationFallback, setAllowAssociationFallback] = useState(false);
  const [updateModalOpen, setUpdateModalOpen] = useState(false);
  const [pendingUpdateInfo, setPendingUpdateInfo] = useState<UpdateInfo | null>(null);
  const [proxySettings, setProxySettings] = useState<ProxySettings>(() => {
    try {
      const saved = localStorage.getItem('proxySettings');
      if (saved) {
        return JSON.parse(saved) as ProxySettings;
      }
    } catch {
      // ignore
    }
    return createDefaultProxySettings();
  });

  useEffect(() => {
    localStorage.setItem('proxySettings', JSON.stringify(proxySettings));
  }, [proxySettings]);

  const selectedPrinter = useMemo(
    () => printers.find((printer) => printer.name === globalSettings.printerName),
    [printers, globalSettings.printerName],
  );
  const availability = evaluateSettingAvailability(selectedPrinter);
  const settingsItem = queueState.items.find((item) => item.id === settingsItemId) ?? null;

  const refreshPrinters = useCallback(async () => {
    setLoadingPrinters(true);
    try {
      const nextPrinters = await listSystemPrinters();
      setPrinters(nextPrinters);
      setGlobalSettings((currentSettings) => {
        const preferredName =
          currentSettings.printerName ||
          nextPrinters.find((printer) => printer.isDefault)?.name ||
          nextPrinters[0]?.name ||
          '';
        const preferredPrinter = nextPrinters.find((printer) => printer.name === preferredName);
        return sanitizeSettingsForPrinter(
          { ...currentSettings, printerName: preferredName },
          preferredPrinter,
        );
      });
    } catch (error) {
      message.error(error instanceof Error ? error.message : '读取系统打印机失败');
    } finally {
      setLoadingPrinters(false);
    }
  }, []);

  useEffect(() => {
    void refreshPrinters();
  }, [refreshPrinters]);

  useEffect(() => {
    return subscribeIncomingFiles((paths) => {
      if (paths.length > 0) {
        dispatch({ type: 'append_files', paths });
        message.success(`已追加 ${paths.length} 个文件`);
      }
    });
  }, []);

  useEffect(() => {
    return subscribeNativeDragDrop({
      onHoverChange: setIsDragOver,
      onDrop: (paths) => {
        void (async () => {
          try {
            const expanded = await expandFilePaths(paths);
            if (expanded.length === 0) {
              if (paths.length > 0) {
                message.warning('未找到可打印的文件');
              }
              return;
            }
            dispatch({ type: 'append_files', paths: expanded });
            message.success(`已追加 ${expanded.length} 个文件`);
          } catch (error) {
            message.error(error instanceof Error ? error.message : '处理拖放文件失败');
          }
        })();
      },
    });
  }, []);

  useEffect(() => {
    const unprobed = queueState.items.filter(
      (item) => item.pageCount === null && item.kind !== 'unknown',
    );
    if (unprobed.length === 0) return;

    for (const item of unprobed) {
      void (async () => {
        try {
          const { pageCount } = await probeDocumentInfo(item.path);
          if (pageCount !== null && pageCount > 0) {
            dispatch({ type: 'set_item_page_count', id: item.id, pageCount });
          }
        } catch {
          // ignore
        }
      })();
    }
  }, [queueState.items]);

  const appendPaths = (paths: string[]) => {
    if (paths.length === 0) return;
    dispatch({ type: 'append_files', paths });
    message.success(`已追加 ${paths.length} 个文件`);
  };


  const handlePickFiles = async () => {
    try {
      appendPaths(await pickFiles());
    } catch (error) {
      message.error(error instanceof Error ? error.message : '选择文件失败');
    }
  };

  const handlePickFolder = async () => {
    try {
      appendPaths(await pickFolderFiles());
    } catch (error) {
      message.error(error instanceof Error ? error.message : '选择文件夹失败');
    }
  };

  const handleDrop = (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    setIsDragOver(false);
    if (isTauriRuntime()) return;
    const paths = Array.from(event.dataTransfer.files)
      .map((file) => (file as File & { path?: string }).path)
      .filter((path): path is string => Boolean(path));
    if (paths.length === 0) {
      message.warning('浏览器预览无法获取本地路径，请在桌面应用中拖放，或使用选择按钮');
      return;
    }
    appendPaths(paths);
  };

  const buildBatchPayload = (onlyFailed = false): PrintQueueItemPayload[] | null => {
    if (!globalSettings.printerName) {
      message.warning('请先选择打印机');
      return null;
    }
    if (!availability.printEnabled) {
      message.error(availability.reasons.join('；') || '当前打印机不可用');
      return null;
    }

    const sourceItems = queueState.items.filter((item) => {
      if (onlyFailed) {
        return item.status === 'failed' || item.status === 'ready';
      }
      return item.status !== 'succeeded' && item.kind !== 'unknown';
    });
    if (sourceItems.length === 0) {
      const allSucceeded =
        queueState.items.length > 0 &&
        queueState.items.every(
          (item) => item.status === 'succeeded' || item.kind === 'unknown',
        ) &&
        queueState.items.some((item) => item.status === 'succeeded');
      if (allSucceeded) {
        message.info('当前批次文件均已打印成功，可清空后继续添加新文件');
      } else {
        message.warning('没有可打印的文件');
      }
      return null;
    }

    const payloads: PrintQueueItemPayload[] = [];
    for (const item of sourceItems) {
      const resolved = mergePrintSettings(globalSettings, item.override);
      // 只下发真实可达成的能力：打印机不支持双面，或文件类型（文本）无法强制双面时，按单面提交。
      const duplexAvailability = resolveDuplexAvailability({
        printerDuplexEnabled: availability.duplexEnabled,
        kind: item.kind,
      });
      const effectiveSides = effectiveSidesMode(resolved.sidesMode, duplexAvailability);
      if (resolved.pageRange.mode === 'custom') {
        const parseResult = parsePageRangeExpression(
          resolved.pageRange.expression,
          item.pageCount ?? undefined,
        );
        if (!parseResult.ok) {
          message.error(`${item.fileName}：${parseResult.message}`);
          return null;
        }
      }
      payloads.push({
        queueItemId: item.id,
        path: item.path,
        fileName: item.fileName,
        allowAssociationFallback,
        settings: {
          printerName: resolved.printerName,
          colorMode: resolved.colorMode,
          sidesMode: effectiveSides,
          flipMode: resolved.flipMode,
          copies: resolved.copies,
          pageRangeMode: resolved.pageRange.mode,
          pageRangeExpression: resolved.pageRange.expression,
        },
      });
    }
    return payloads;
  };

  const executePrint = async (onlyFailed = false) => {
    const payloads = buildBatchPayload(onlyFailed);
    if (!payloads) return;

    const hasOffice = payloads.some((item) =>
      /\.(doc|docx|xls|xlsx|ppt|pptx)$/i.test(item.path),
    );
    if (hasOffice && !allowAssociationFallback) {
      const confirmed = await new Promise<boolean>((resolve) => {
        Modal.confirm({
          title: 'Office 文档打印说明',
          content:
            'Office 文档优先通过本机已安装的 Word/Excel/PowerPoint 转换后打印。若仅有关联程序且无法证明完整参数支持，将提示能力受限。是否继续？',
          okText: '继续打印',
          cancelText: '取消',
          onOk: () => resolve(true),
          onCancel: () => resolve(false),
        });
      });
      if (!confirmed) return;
      setAllowAssociationFallback(true);
    }

    dispatch({ type: 'begin_print' });
    try {
      const batchResult = await runPrintBatch({
        items: payloads.map((item) => ({ ...item, allowAssociationFallback: true })),
      });
      dispatch({ type: 'finish_print', summary: createPrintSummary(batchResult.results) });
      if (batchResult.failed > 0) {
        message.warning(`完成：成功 ${batchResult.succeeded}，失败 ${batchResult.failed}`);
      } else {
        message.success(`全部完成：成功 ${batchResult.succeeded}`);
        Modal.confirm({
          title: '打印全部成功',
          content: `本批 ${batchResult.succeeded} 个文件均已打印成功。是否清空当前批次列表？清空后可继续添加新文件。`,
          okText: '清空列表',
          cancelText: '暂时保留',
          onOk: () => {
            dispatch({ type: 'clear_queue' });
            message.success('已清空当前批次');
          },
        });
      }
    } catch (error) {
      dispatch({
        type: 'finish_print',
        summary: createPrintSummary(
          payloads.map((item) => ({
            queueItemId: item.queueItemId,
            path: item.path,
            fileName: item.fileName,
            status: 'failed' as const,
            message: error instanceof Error ? error.message : '打印执行失败',
          })),
        ),
      });
      message.error(error instanceof Error ? error.message : '打印执行失败');
    }
  };

  const promptInstallUpdate = (updateInfo: UpdateInfo) => {
    setPendingUpdateInfo(updateInfo);
    setUpdateModalOpen(true);
  };

  const runUpdateCheck = async (options?: { silent?: boolean }) => {
    const silent = options?.silent ?? false;
    try {
      const proxyConfig = getProxyConfig(proxySettings);
      const updateInfo = await checkForAppUpdate({
        useSystemProxy: proxyConfig.useSystemProxy,
        customProxyUrl: proxyConfig.customProxyUrl,
        username: proxyConfig.username,
        password: proxyConfig.password,
      });
      if (!updateInfo.available) {
        if (!silent) message.success('当前已是最新版本');
        return;
      }
      promptInstallUpdate(updateInfo);
    } catch (error) {
      if (!silent) message.error(error instanceof Error ? error.message : '检查更新失败');
    }
  };

  const handleCheckUpdate = async () => {
    await runUpdateCheck({ silent: false });
  };

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let cancelled = false;
    const startupCheckTimer = window.setTimeout(() => {
      if (!cancelled) void runUpdateCheck({ silent: true });
    }, 800);
    return () => {
      cancelled = true;
      window.clearTimeout(startupCheckTimer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleShortcutsHelp = () => {
    Modal.info({
      title: '常用操作指南',
      content: (
        <div style={{ lineHeight: 1.8, fontSize: '13px', paddingTop: '8px' }}>
          <p>• <strong>文件追加</strong>：支持拖拽文件/文件夹，或在 Windows 资源管理器中右键选中文件“发送到 打印助手”追加到当前批次。</p>
          <p>• <strong>快捷微调</strong>：在列表行内直接点击色彩或单双面芯片即可直接切换，无需每次打开抽屉。</p>
          <p>• <strong>批量控制</strong>：勾选文件后，顶部工具条点亮批量双面、批量单面、批量黑白及批量删除。</p>
          <p>• <strong>装订顺序</strong>：点击行首上移/下移箭头可调整文档打印先后顺序。</p>
        </div>
      ),
      okText: '知道了',
    });
  };

  return (
    <ConfigProvider
      theme={{
        token: {
          colorPrimary: '#1e5eff',
          colorText: '#0f172a',
          colorBorder: '#e2e8f0',
          colorBorderSecondary: '#f1f5f9',
          colorBgContainer: '#ffffff',
          colorBgLayout: '#f4f6fa',
          borderRadius: 10,
          borderRadiusLG: 14,
          borderRadiusSM: 6,
          borderRadiusXS: 4,
          controlHeight: 34,
          fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI Variable Display", "Segoe UI", "Microsoft YaHei UI", sans-serif',
          boxShadow: '0 1px 3px rgba(15, 23, 42, 0.04), 0 1px 2px rgba(15, 23, 42, 0.02)',
        },
      }}
    >
      <div className="app-shell-root">
        {/* App Header (100% Prototype Replicated) */}
        <header className="app-header">
          <div className="brand-section">
            <div className="app-logo">
              <Printer size={18} />
            </div>
            <div className="brand-titles">
              <div className="brand-name">
                打印助手 <span className="version-tag">v{__APP_VERSION__}</span>
              </div>
              <div className="brand-meta">Windows 批量文件打印控制中心</div>
            </div>
          </div>

          <div className="header-quick-stats">
            <div className="status-pulse" />
            <span>{queueState.items.length} 个文件已就绪</span>
            <span style={{ opacity: 0.4 }}>|</span>
            <span>
              {selectedPrinter
                ? `${selectedPrinter.name} (${selectedPrinter.state === 'ready' ? '在线' : selectedPrinter.state === 'offline' ? '离线' : '就绪'})`
                : loadingPrinters
                  ? '读取设备中...'
                  : '未选择打印机'}
            </span>
          </div>

          <div className="header-actions">
            <button type="button" className="btn-ghost-dark" onClick={handleShortcutsHelp}>
              <Keyboard size={14} /> 操作指南
            </button>
            <button
              type="button"
              className="btn-ghost-dark"
              onClick={() => void refreshPrinters()}
            >
              <RefreshCw size={14} /> 刷新设备
            </button>
            <button
              type="button"
              className="btn-ghost-dark"
              onClick={() => setProxyModalOpen(true)}
            >
              <Settings size={14} /> 代理设置
            </button>
            <button
              type="button"
              className="btn-ghost-dark"
              onClick={() => void handleCheckUpdate()}
            >
              <Sparkles size={14} /> 检查更新
            </button>
          </div>
        </header>

        {/* App Body Grid (100% Prototype Replicated) */}
        <div className="app-body">
          {/* Left Control Rail */}
          <aside className="control-rail">
            {/* 01 / 文件入口 */}
            <div className="rail-section">
              <div className="section-header">
                <span className="section-title">
                  <FilePlus2 size={14} /> 01 / 文件入口
                </span>
                <span className="section-badge">支持追加</span>
              </div>

              <div
                className={`drop-card ${isDragOver ? 'drag-over' : ''}`}
                onDragOver={(event) => {
                  event.preventDefault();
                  setIsDragOver(true);
                }}
                onDragLeave={() => setIsDragOver(false)}
                onDrop={handleDrop}
                onClick={() => void handlePickFiles()}
              >
                <div className="drop-icon-wrap">
                  <UploadCloud size={20} />
                </div>
                <div className="drop-main-text">拖放文件或文件夹到这里</div>
                <div className="drop-sub-text">右键“发送到”与追加模式已启用</div>
                <div className="format-chips">
                  <span className="fmt-chip fmt-pdf">PDF</span>
                  <span className="fmt-chip fmt-xls">Excel</span>
                  <span className="fmt-chip fmt-doc">Word</span>
                  <span className="fmt-chip fmt-ppt">PPT</span>
                  <span className="fmt-chip fmt-img">JPG/PNG</span>
                </div>
              </div>

              <div className="entry-btn-grid">
                <button
                  type="button"
                  className="btn-secondary"
                  disabled={queueState.isPrinting}
                  onClick={() => void handlePickFiles()}
                >
                  <FilePlus2 size={14} /> 选择文件
                </button>
                <button
                  type="button"
                  className="btn-secondary"
                  disabled={queueState.isPrinting}
                  onClick={() => void handlePickFolder()}
                >
                  <FolderPlus size={14} /> 选择文件夹
                </button>
              </div>
            </div>

            {/* 02 打印机 & 03 公共设置 */}
            <GlobalSettingsPanel
              printers={printers}
              settings={globalSettings}
              loadingPrinters={loadingPrinters}
              onChange={(nextSettings) => {
                const printer = printers.find((item) => item.name === nextSettings.printerName);
                setGlobalSettings(sanitizeSettingsForPrinter(nextSettings, printer));
              }}
            />
          </aside>

          {/* Right Main Workspace */}
          <main className="main-workspace">
            <PrintSummary
              summary={queueState.lastSummary}
              onRetryFailed={() => {
                dispatch({ type: 'retry_failed' });
                void executePrint(true);
              }}
            />

            <PrintQueue
              items={queueState.items}
              globalSettings={globalSettings}
              isPrinting={queueState.isPrinting}
              colorEnabled={availability.colorEnabled}
              duplexEnabled={availability.duplexEnabled}
              printDisabled={!availability.printEnabled}
              onRemove={(id) => dispatch({ type: 'remove_item', id })}
              onBatchRemove={(ids) => {
                dispatch({ type: 'batch_remove', ids });
                message.success(`已批量移除 ${ids.length} 个文件`);
              }}
              onOpenSettings={(id) => setSettingsItemId(id)}
              onUpdateOverride={(id, override) =>
                dispatch({ type: 'update_override', id, override })
              }
              onBatchUpdateOverride={(ids, overridePatch) => {
                dispatch({ type: 'batch_update_override', ids, overridePatch });
                message.success(`已批量更新 ${ids.length} 个文件的打印设置`);
              }}
              onClearQueue={() => dispatch({ type: 'clear_queue' })}
              onStartPrint={() => void executePrint(false)}
              onPickFiles={() => void handlePickFiles()}
            />

          </main>
        </div>
      </div>

      <FileSettingsDrawer
        open={Boolean(settingsItem)}
        item={settingsItem}
        globalSettings={globalSettings}
        colorEnabled={availability.colorEnabled}
        duplexEnabled={availability.duplexEnabled}
        onClose={() => setSettingsItemId(null)}
        onSave={(override) => {
          if (!settingsItem) return;
          dispatch({ type: 'update_override', id: settingsItem.id, override });
          message.success('已保存单文件设置');
        }}
      />

      <Modal
        title="代理设置"
        open={proxyModalOpen}
        onCancel={() => setProxyModalOpen(false)}
        footer={
          <Button type="primary" onClick={() => setProxyModalOpen(false)}>
            确定
          </Button>
        }
        width={480}
        destroyOnClose
      >

        <ProxySettingsPanel settings={proxySettings} onChange={setProxySettings} />
      </Modal>

      <UpdateModal
        open={updateModalOpen}
        updateInfo={pendingUpdateInfo}
        proxyConfig={getProxyConfig(proxySettings)}
        onClose={() => {
          setUpdateModalOpen(false);
          setPendingUpdateInfo(null);
        }}
      />
    </ConfigProvider>
  );
}

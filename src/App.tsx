import {
  Button,
  ConfigProvider,
  Layout,
  Modal,
  Space,
  Typography,
  message,
} from 'antd';
import { FilePlus2, FolderPlus, Printer, RefreshCw } from 'lucide-react';
import { useCallback, useEffect, useMemo, useReducer, useState } from 'react';
import type { DragEvent } from 'react';
import {
  checkForAppUpdate,
  installAppUpdate,
  listSystemPrinters,
  pickFiles,
  pickFolderFiles,
  runPrintBatch,
  subscribeIncomingFiles,
} from './api/nativeBridge';
import {
  createDefaultGlobalSettings,
  evaluateSettingAvailability,
  mergePrintSettings,
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
import type { SystemPrinter } from './shared/contracts/printer';
import type { PrintQueueItemPayload } from './shared/contracts/printJob';

const { Header, Content, Sider } = Layout;

export function App() {
  const [queueState, dispatch] = useReducer(queueReducer, undefined, createEmptyQueueState);
  const [printers, setPrinters] = useState<SystemPrinter[]>([]);
  const [loadingPrinters, setLoadingPrinters] = useState(true);
  const [globalSettings, setGlobalSettings] = useState<PrintSettings>(
    createDefaultGlobalSettings(),
  );
  const [settingsItemId, setSettingsItemId] = useState<string | null>(null);
  const [isDragOver, setIsDragOver] = useState(false);
  const [allowAssociationFallback, setAllowAssociationFallback] = useState(false);

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

  const appendPaths = (paths: string[]) => {
    if (paths.length === 0) {
      return;
    }
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
      message.warning('没有可打印的文件');
      return null;
    }

    const payloads: PrintQueueItemPayload[] = [];
    for (const item of sourceItems) {
      const resolved = mergePrintSettings(globalSettings, item.override);
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
          sidesMode: resolved.sidesMode,
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
    if (!payloads) {
      return;
    }

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
      if (!confirmed) {
        return;
      }
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

  const handleCheckUpdate = async () => {
    try {
      const updateInfo = await checkForAppUpdate();
      if (!updateInfo.available) {
        message.success('当前已是最新版本');
        return;
      }
      Modal.confirm({
        title: `发现新版本 ${updateInfo.version ?? ''}`.trim(),
        content: updateInfo.body || '是否下载并安装更新？安装前会退出应用。',
        okText: '下载并安装',
        cancelText: '稍后',
        onOk: async () => {
          await installAppUpdate();
        },
      });
    } catch (error) {
      message.error(error instanceof Error ? error.message : '检查更新失败');
    }
  };

  return (
    <ConfigProvider
      theme={{
        token: {
          colorPrimary: '#1557d0',
          colorText: '#172033',
          borderRadius: 3,
          fontFamily: '"Segoe UI Variable", "Microsoft YaHei UI", sans-serif',
        },
      }}
    >
      <Layout className="app-shell">
        <Header className="app-header">
          <div className="brand-group">
            <div className="brand-mark">
              <Printer size={18} />
            </div>
            <div className="brand-copy">
              <Typography.Title level={4}>打印助手</Typography.Title>
              <Typography.Text>
                当前批次 · {queueState.items.length} 个文件
                {queueState.isPrinting ? ' · 打印中' : ''}
              </Typography.Text>
            </div>
          </div>
          <Space className="header-actions" size={10}>
            <Button ghost icon={<RefreshCw size={14} />} onClick={() => void refreshPrinters()}>
              刷新打印机
            </Button>
            <Button ghost onClick={() => void handleCheckUpdate()}>
              检查更新
            </Button>
          </Space>
        </Header>
        <Layout>
          <Sider width={342} theme="light" className="control-rail">
            <Typography.Text className="section-index">01 / 文件入口</Typography.Text>
            <Typography.Title level={5}>追加打印文件</Typography.Title>
            <div
              className={`drop-zone ${isDragOver ? 'dragging' : ''}`}
              onDragOver={(event) => {
                event.preventDefault();
                setIsDragOver(true);
              }}
              onDragLeave={() => setIsDragOver(false)}
              onDrop={handleDrop}
            >
              <FilePlus2 size={24} />
              <strong>拖放文件到这里</strong>
              <span>右键菜单、发送到和页面选择都会追加到当前批次</span>
            </div>
            <div className="entry-actions">
              <Button
                icon={<FilePlus2 size={15} />}
                disabled={queueState.isPrinting}
                onClick={() => void handlePickFiles()}
              >
                选择文件
              </Button>
              <Button
                icon={<FolderPlus size={15} />}
                disabled={queueState.isPrinting}
                onClick={() => void handlePickFolder()}
              >
                选择文件夹
              </Button>
            </div>
            <GlobalSettingsPanel
              printers={printers}
              settings={globalSettings}
              loadingPrinters={loadingPrinters}
              onChange={(nextSettings) => {
                const printer = printers.find((item) => item.name === nextSettings.printerName);
                setGlobalSettings(sanitizeSettingsForPrinter(nextSettings, printer));
              }}
            />
          </Sider>
          <Content className="queue-panel">
            <div className="queue-heading">
              <div>
                <Typography.Text className="section-index">当前批次</Typography.Text>
                <Typography.Title level={3}>待打印文件</Typography.Title>
              </div>
              <Space>
                <Button
                  disabled={queueState.isPrinting || queueState.items.length === 0}
                  onClick={() => dispatch({ type: 'clear_queue' })}
                >
                  清空
                </Button>
                <Button
                  type="primary"
                  icon={<Printer size={16} />}
                  loading={queueState.isPrinting}
                  disabled={!availability.printEnabled || queueState.items.length === 0}
                  onClick={() => void executePrint(false)}
                >
                  开始打印
                </Button>
              </Space>
            </div>
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
              onRemove={(id) => dispatch({ type: 'remove_item', id })}
              onOpenSettings={(id) => setSettingsItemId(id)}
            />
          </Content>
        </Layout>
      </Layout>
      <FileSettingsDrawer
        open={Boolean(settingsItem)}
        item={settingsItem}
        globalSettings={globalSettings}
        colorEnabled={availability.colorEnabled}
        duplexEnabled={availability.duplexEnabled}
        onClose={() => setSettingsItemId(null)}
        onSave={(override) => {
          if (!settingsItem) {
            return;
          }
          dispatch({ type: 'update_override', id: settingsItem.id, override });
          message.success('已保存单文件设置');
        }}
      />
    </ConfigProvider>
  );
}

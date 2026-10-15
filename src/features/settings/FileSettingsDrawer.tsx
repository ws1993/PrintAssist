import {
  Button,
  Drawer,
  Input,
  InputNumber,
  Radio,
  Segmented,
  Space,
  Switch,
  Tag,
  Tooltip,
  Typography,
  message,
} from 'antd';
import {
  BookOpen,
  Check,
  CheckCheck,
  FileText,
  HelpCircle,
  RotateCw,
  Sliders,
} from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import type { QueueItem } from '../../domain/queueTypes';
import type {
  ColorMode,
  FileSettingsOverride,
  FlipMode,
  PrintSettings,
  SidesMode,
} from '../../domain/printSettings';
import { mergePrintSettings, resolveDuplexAvailability } from '../../domain/printSettings';
import { parsePageRangeExpression } from '../../domain/pageRange';

interface FileSettingsDrawerProps {
  open: boolean;
  item: QueueItem | null;
  globalSettings: PrintSettings;
  colorEnabled: boolean;
  duplexEnabled: boolean;
  onClose: () => void;
  onSave: (override: FileSettingsOverride) => void;
}

export function formatPageNumbersToRanges(arr: number[]): string {
  if (arr.length === 0) return '';
  const sorted = Array.from(new Set(arr)).sort((a, b) => a - b);
  const ranges: string[] = [];
  let start = sorted[0];
  let end = sorted[0];
  for (let i = 1; i < sorted.length; i++) {
    if (sorted[i] === end + 1) {
      end = sorted[i];
    } else {
      ranges.push(start === end ? `${start}` : `${start}-${end}`);
      start = end = sorted[i];
    }
  }
  ranges.push(start === end ? `${start}` : `${start}-${end}`);
  return ranges.join(', ');
}

export function FileSettingsDrawer({
  open,
  item,
  globalSettings,
  colorEnabled,
  duplexEnabled,
  onClose,
  onSave,
}: FileSettingsDrawerProps) {
  const [useCustomColor, setUseCustomColor] = useState(false);
  const [useCustomSides, setUseCustomSides] = useState(false);
  const [useCustomCopies, setUseCustomCopies] = useState(false);
  const [useCustomPages, setUseCustomPages] = useState(false);
  const [colorMode, setColorMode] = useState<ColorMode>('monochrome');
  const [sidesMode, setSidesMode] = useState<SidesMode>('simplex');
  const [flipMode, setFlipMode] = useState<FlipMode>('longEdge');
  const [copies, setCopies] = useState(1);
  const [pageExpression, setPageExpression] = useState('');
  const [rangeQuickMode, setRangeQuickMode] = useState<'all' | 'odd' | 'even' | 'custom'>('all');

  const isPageCountKnown = Boolean(item?.pageCount && item.pageCount > 0);
  const actualPages = isPageCountKnown ? item!.pageCount! : 1;

  // 双面可用性 = 打印机能力 且 文件类型能真正下发双面（文本文件走关联程序，不能）
  const duplexAvailability = resolveDuplexAvailability({
    printerDuplexEnabled: duplexEnabled,
    kind: item?.kind,
  });

  useEffect(() => {
    if (!item) return;

    const merged = mergePrintSettings(globalSettings, item.override);
    setUseCustomColor(item.override.colorMode !== undefined);
    setUseCustomSides(
      item.override.sidesMode !== undefined || item.override.flipMode !== undefined,
    );
    setUseCustomCopies(item.override.copies !== undefined);
    setUseCustomPages(item.override.pageRange !== undefined);
    setColorMode(merged.colorMode);
    setSidesMode(duplexAvailability.enabled ? merged.sidesMode : 'simplex');
    setFlipMode(merged.flipMode);
    setCopies(merged.copies);

    const hasCustomRange = item.override.pageRange?.mode === 'custom';
    if (hasCustomRange) {
      setPageExpression(item.override.pageRange?.expression || '');
      setRangeQuickMode('custom');
    } else {
      // Default to full range representation based on actual page count
      if (item.pageCount && item.pageCount > 1) {
        setPageExpression(`1-${item.pageCount}`);
      } else {
        setPageExpression('1');
      }
      setRangeQuickMode('all');
    }
  }, [item, globalSettings, duplexAvailability.enabled]);

  const selectedPagesList = useMemo<number[]>(() => {
    if (!isPageCountKnown) {
      if (!useCustomPages || !pageExpression.trim()) return [];
      const parsed = parsePageRangeExpression(pageExpression);
      return parsed.ok ? parsed.pages : [];
    }

    if (!useCustomPages) {
      return Array.from({ length: actualPages }, (_, i) => i + 1);
    }
    const parsed = parsePageRangeExpression(pageExpression, actualPages);
    return parsed.ok ? parsed.pages : [];
  }, [useCustomPages, pageExpression, isPageCountKnown, actualPages]);

  const handleTogglePageBlock = (pageNumber: number) => {
    setUseCustomPages(true);
    setRangeQuickMode('custom');
    const isSelected = selectedPagesList.includes(pageNumber);
    let nextPages: number[];
    if (isSelected) {
      nextPages = selectedPagesList.filter((p) => p !== pageNumber);
    } else {
      nextPages = [...selectedPagesList, pageNumber];
    }
    setPageExpression(formatPageNumbersToRanges(nextPages));
  };

  const handleQuickModeChange = (val: 'all' | 'odd' | 'even' | 'custom') => {
    setRangeQuickMode(val);
    if (val === 'all') {
      setUseCustomPages(false);
      setPageExpression(actualPages > 1 ? `1-${actualPages}` : '1');
    } else if (val === 'odd') {
      setUseCustomPages(true);
      const odds = Array.from({ length: actualPages }, (_, i) => i + 1).filter((p) => p % 2 === 1);
      setPageExpression(formatPageNumbersToRanges(odds));
    } else if (val === 'even') {
      setUseCustomPages(true);
      const evens = Array.from({ length: actualPages }, (_, i) => i + 1).filter((p) => p % 2 === 0);
      setPageExpression(formatPageNumbersToRanges(evens));
    } else if (val === 'custom') {
      setUseCustomPages(true);
    }
  };

  const handleSave = () => {
    if (!item) return;

    const nextOverride: FileSettingsOverride = {};

    if (useCustomColor) {
      nextOverride.colorMode = colorMode;
    }
    if (useCustomSides) {
      nextOverride.sidesMode = sidesMode;
      nextOverride.flipMode = flipMode;
    }
    if (useCustomCopies) {
      nextOverride.copies = copies;
    }
    if (useCustomPages) {
      const parseResult = parsePageRangeExpression(pageExpression, item.pageCount ?? undefined);
      if (!parseResult.ok) {
        message.error(parseResult.message);
        return;
      }
      nextOverride.pageRange = {
        mode: 'custom',
        expression: pageExpression.trim(),
      };
    }

    onSave(nextOverride);
    onClose();
  };

  return (
    <Drawer
      title="单文件高级设置"
      open={open}
      onClose={onClose}
      width={460}
      destroyOnClose
      className="file-settings-drawer"
      extra={
        <Space>
          <Button onClick={onClose}>取消</Button>
          <Button type="primary" icon={<Check size={14} />} onClick={handleSave}>
            保存设置
          </Button>
        </Space>
      }
    >
      {item && (
        <div className="drawer-file-meta-card">
          <div className="drawer-file-header">
            <FileText size={18} className="drawer-file-icon" />
            <div className="drawer-file-info">
              <Tooltip title={item.fileName} placement="bottomLeft">
                <Typography.Text strong className="drawer-file-name">
                  {item.fileName}
                </Typography.Text>
              </Tooltip>
              <div className="drawer-file-path" title={item.path}>
                {item.path}
              </div>
            </div>
          </div>
          <div className="drawer-meta-badges">
            <Tag color="blue">{item.kind.toUpperCase()}</Tag>
            <Tag color="cyan">
              {item.pageCount ? `共 ${item.pageCount} 页` : '页数自动探测/未指定'}
            </Tag>
          </div>
        </div>
      )}

      <Typography.Paragraph type="secondary" className="drawer-help-text">
        开启开关可单独覆盖公共参数，未开启项将自动继承公共默认值。
      </Typography.Paragraph>

      {/* 页码范围可视化网格选择 */}
      <div className="drawer-section-box">
        <div className="drawer-field-head">
          <div className="field-title-group">
            <BookOpen size={15} />
            <Typography.Text strong>页码范围选择</Typography.Text>
          </div>
          <Switch checked={useCustomPages} onChange={setUseCustomPages} />
        </div>

        <div className="page-mode-row">
          <Segmented
            size="small"
            value={useCustomPages ? rangeQuickMode : 'all'}
            onChange={(val) => handleQuickModeChange(val as 'all' | 'odd' | 'even' | 'custom')}
            options={[
              { label: '全部页', value: 'all' },
              { label: '仅奇数页', value: 'odd' },
              { label: '仅偶数页', value: 'even' },
              { label: '自定义', value: 'custom' },
            ]}
          />
        </div>

        <div className="page-range-input-wrap">
          <Input
            disabled={!useCustomPages}
            value={pageExpression}
            placeholder={
              isPageCountKnown
                ? `例如 1, 3, 5-${actualPages}`
                : '例如 1, 3, 5-8'
            }
            onChange={(e) => {
              setPageExpression(e.target.value);
              setRangeQuickMode('custom');
            }}
          />
          {isPageCountKnown && (
            <div className="range-summary-text">
              已选 {selectedPagesList.length} / {actualPages} 页
            </div>
          )}
        </div>

        {/* 可视化页面缩略方块网格 */}
        {isPageCountKnown ? (
          <div className="visual-page-grid-container">
            <div className="grid-label">
              实际页数 ({actualPages} 页) · 点击方块直观勾选/取消：
            </div>
            <div className="visual-page-grid">
              {Array.from({ length: Math.min(actualPages, 60) }, (_, i) => i + 1).map((p) => {
                const isSelected = selectedPagesList.includes(p);
                return (
                  <div
                    key={p}
                    className={`page-grid-block ${isSelected ? 'selected' : ''}`}
                    onClick={() => handleTogglePageBlock(p)}
                    title={`点击切换第 ${p} 页`}
                  >
                    {isSelected && <CheckCheck size={11} className="check-icon" />}
                    <span className="page-num">{p}</span>
                  </div>
                );
              })}
            </div>
            {actualPages > 60 && (
              <div style={{ fontSize: '10px', color: '#94a3b8', marginTop: '4px' }}>
                页面较多，前 60 页已展示在网格中，超出部分可通过上方输入框指定。
              </div>
            )}
          </div>
        ) : (
          <div style={{ fontSize: '11px', color: '#64748b', background: '#f8fafc', padding: '8px 10px', borderRadius: '6px' }}>
            提示：当前文件类型未直接探测到物理总页数，您可在上方输入框直接填写需打印的页码区间（如 <code>1-5</code> 或 <code>1,3,5</code>）。
          </div>
        )}
      </div>

      {/* 色彩模式 */}
      <div className="drawer-section-box">
        <div className="drawer-field-head">
          <div className="field-title-group">
            <Typography.Text strong>色彩模式</Typography.Text>
          </div>
          <Switch checked={useCustomColor} onChange={setUseCustomColor} />
        </div>
        <Radio.Group
          disabled={!useCustomColor}
          value={colorMode}
          onChange={(e) => setColorMode(e.target.value)}
        >
          <Radio value="monochrome">黑白</Radio>
          <Radio value="color" disabled={!colorEnabled}>
            彩色 {!colorEnabled && '(打印机不支持)'}
          </Radio>
        </Radio.Group>
      </div>

      {/* 单双面与装订 */}
      <div className="drawer-section-box">
        <div className="drawer-field-head">
          <div className="field-title-group">
            <Typography.Text strong>单双面与装订</Typography.Text>
          </div>
          <Switch checked={useCustomSides} onChange={setUseCustomSides} />
        </div>
        <Radio.Group
          disabled={!useCustomSides}
          value={sidesMode}
          onChange={(e) => setSidesMode(e.target.value)}
        >
          <Radio value="simplex">单面打印</Radio>
          <Radio value="duplex" disabled={!duplexAvailability.enabled}>
            双面打印{' '}
            {!duplexAvailability.enabled && `（${duplexAvailability.reason ?? '不可用'}）`}
          </Radio>
        </Radio.Group>
        {!duplexAvailability.enabled && (
          <div style={{ marginTop: 6 }}>
            <Typography.Text type="secondary" style={{ fontSize: 12 }}>
              该文件将按单面打印，界面与纸张统计均按单面计算。
            </Typography.Text>
          </div>
        )}

        {sidesMode === 'duplex' && (
          <div style={{ marginTop: 8, paddingLeft: 12 }}>
            <Typography.Text type="secondary" style={{ fontSize: 12, marginRight: 8 }}>
              装订翻转：
            </Typography.Text>
            <Radio.Group
              size="small"
              disabled={!useCustomSides}
              value={flipMode}
              onChange={(e) => setFlipMode(e.target.value)}
            >
              <Radio value="longEdge">长边翻转</Radio>
              <Radio value="shortEdge">短边翻转</Radio>
            </Radio.Group>
          </div>
        )}
      </div>

      {/* 打印份数 */}
      <div className="drawer-section-box">
        <div className="drawer-field-head">
          <div className="field-title-group">
            <Typography.Text strong>打印份数</Typography.Text>
          </div>
          <Switch checked={useCustomCopies} onChange={setUseCustomCopies} />
        </div>
        <InputNumber
          min={1}
          max={99}
          value={copies}
          disabled={!useCustomCopies}
          onChange={(val) => setCopies(val || 1)}
          addonAfter="份"
        />
      </div>
    </Drawer>
  );
}

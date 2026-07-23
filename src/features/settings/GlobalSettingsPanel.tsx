import { Alert, InputNumber, Segmented, Select, Typography } from 'antd';
import type { SystemPrinter } from '../../shared/contracts/printer';
import type { ColorMode, FlipMode, PrintSettings, SidesMode } from '../../domain/printSettings';
import { evaluateSettingAvailability } from '../../domain/printSettings';

interface GlobalSettingsPanelProps {
  printers: SystemPrinter[];
  settings: PrintSettings;
  loadingPrinters: boolean;
  onChange: (nextSettings: PrintSettings) => void;
}

function describePrinterState(printer: SystemPrinter): string {
  if (printer.state === 'ready') return '在线';
  if (printer.state === 'offline') return '离线';
  if (printer.state === 'error') return '错误';
  return '未知';
}

function describeCapabilitySupport(support: string): string {
  if (support === 'supported') return '支持';
  if (support === 'unsupported') return '不支持';
  return '未知';
}

export function GlobalSettingsPanel({
  printers,
  settings,
  loadingPrinters,
  onChange,
}: GlobalSettingsPanelProps) {
  const selectedPrinter = printers.find((printer) => printer.name === settings.printerName);
  const availability = evaluateSettingAvailability(selectedPrinter);
  const showFlipOptions = settings.sidesMode === 'duplex' && availability.duplexEnabled;
  const showColorHint = Boolean(selectedPrinter) && !availability.colorEnabled;
  const showDuplexHint = Boolean(selectedPrinter) && !availability.duplexEnabled;
  const criticalReasons = availability.reasons.filter(
    (reason) =>
      reason.includes('离线') ||
      reason.includes('错误') ||
      reason.includes('尚未选择'),
  );

  return (
    <div className="settings-block">
      <div className="settings-block-head">
        <Typography.Text className="section-index">02 / 公共设置</Typography.Text>
        <Typography.Title level={5}>默认打印参数</Typography.Title>
      </div>

      <div className="setting-field">
        <label className="field-label" htmlFor="printer-select">
          打印机
        </label>
        <Select
          id="printer-select"
          className="full-width"
          size="middle"
          loading={loadingPrinters}
          value={settings.printerName || undefined}
          placeholder={loadingPrinters ? '正在读取系统打印机…' : '选择系统打印机'}
          options={printers.map((printer) => ({
            value: printer.name,
            label: `${printer.name}${printer.isDefault ? '（默认）' : ''}`,
          }))}
          onChange={(printerName) => onChange({ ...settings, printerName })}
        />
      </div>

      {selectedPrinter && (
        <div className={`printer-status-inline ${selectedPrinter.state}`}>
          <span className="status-pill">{describePrinterState(selectedPrinter)}</span>
          <span className="status-meta">
            彩色{describeCapabilitySupport(selectedPrinter.color.support)}
            <span className="status-dot-sep" aria-hidden>
              ·
            </span>
            双面{describeCapabilitySupport(selectedPrinter.duplex.support)}
            {selectedPrinter.portName ? (
              <>
                <span className="status-dot-sep" aria-hidden>
                  ·
                </span>
                {selectedPrinter.portName}
              </>
            ) : null}
          </span>
        </div>
      )}

      <div className="settings-controls">
        <div className="setting-row">
          <span className="setting-row-label" id="color-mode-label">
            颜色
          </span>
          <Segmented
            className="setting-segmented"
            size="small"
            block
            aria-labelledby="color-mode-label"
            value={settings.colorMode}
            options={[
              { label: '黑白', value: 'monochrome' },
              {
                label: '彩色',
                value: 'color',
                disabled: !availability.colorEnabled,
              },
            ]}
            onChange={(value) =>
              onChange({ ...settings, colorMode: value as ColorMode })
            }
          />
        </div>
        {showColorHint && (
          <Typography.Text type="secondary" className="field-hint field-hint-inline">
            彩色不可用：{selectedPrinter?.color.detail ?? '打印机不支持或能力未知'}
          </Typography.Text>
        )}

        <div className="setting-row">
          <span className="setting-row-label" id="sides-mode-label">
            单双面
          </span>
          <Segmented
            className="setting-segmented"
            size="small"
            block
            aria-labelledby="sides-mode-label"
            value={settings.sidesMode}
            options={[
              { label: '单面', value: 'simplex' },
              {
                label: '双面',
                value: 'duplex',
                disabled: !availability.duplexEnabled,
              },
            ]}
            onChange={(value) => {
              const sidesMode = value as SidesMode;
              onChange({
                ...settings,
                sidesMode,
              });
            }}
          />
        </div>

        {showFlipOptions && (
          <div className="setting-row setting-row-nested">
            <span className="setting-row-label" id="flip-mode-label">
              翻转
            </span>
            <Segmented
              className="setting-segmented"
              size="small"
              block
              aria-labelledby="flip-mode-label"
              value={settings.flipMode}
              options={[
                { label: '长边', value: 'longEdge' },
                { label: '短边', value: 'shortEdge' },
              ]}
              onChange={(value) =>
                onChange({ ...settings, flipMode: value as FlipMode })
              }
            />
          </div>
        )}
        {showDuplexHint && (
          <Typography.Text type="secondary" className="field-hint field-hint-inline">
            双面不可用：{selectedPrinter?.duplex.detail ?? '打印机不支持或能力未知'}
          </Typography.Text>
        )}

        <div className="setting-row setting-row-copies">
          <label className="setting-row-label" htmlFor="copies-input">
            份数
          </label>
          <div className="copies-control">
            <InputNumber
              id="copies-input"
              size="small"
              min={1}
              max={99}
              value={settings.copies}
              onChange={(value) =>
                onChange({
                  ...settings,
                  copies: typeof value === 'number' && value > 0 ? value : 1,
                })
              }
            />
            <Typography.Text type="secondary">份</Typography.Text>
          </div>
        </div>
      </div>

      {criticalReasons.length > 0 && (
        <Alert
          className="settings-alert"
          type="warning"
          showIcon
          banner
          message={criticalReasons.join('；')}
        />
      )}
    </div>
  );
}

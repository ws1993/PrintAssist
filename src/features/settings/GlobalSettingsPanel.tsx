import { Alert, InputNumber, Radio, Select, Space, Typography } from 'antd';
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
  return '状态未知';
}

export function GlobalSettingsPanel({
  printers,
  settings,
  loadingPrinters,
  onChange,
}: GlobalSettingsPanelProps) {
  const selectedPrinter = printers.find((printer) => printer.name === settings.printerName);
  const availability = evaluateSettingAvailability(selectedPrinter);

  return (
    <div className="settings-block">
      <Typography.Text className="section-index">02 / 公共设置</Typography.Text>
      <Typography.Title level={5}>默认打印参数</Typography.Title>

      <label className="field-label" htmlFor="printer-select">打印机</label>
      <Select
        id="printer-select"
        className="full-width"
        loading={loadingPrinters}
        value={settings.printerName || undefined}
        placeholder={loadingPrinters ? '正在读取系统打印机…' : '选择系统打印机'}
        options={printers.map((printer) => ({
          value: printer.name,
          label: `${printer.name}${printer.isDefault ? '（默认）' : ''}`,
        }))}
        onChange={(printerName) => onChange({ ...settings, printerName })}
      />

      {selectedPrinter && (
        <div className={`printer-status-card ${selectedPrinter.state}`}>
          <strong>{describePrinterState(selectedPrinter)}</strong>
          <span>
            彩色：{selectedPrinter.color.support} · 双面：{selectedPrinter.duplex.support}
            {selectedPrinter.portName ? ` · 端口 ${selectedPrinter.portName}` : ''}
          </span>
        </div>
      )}

      <label className="field-label">颜色</label>
      <Radio.Group
        value={settings.colorMode}
        disabled={!availability.colorEnabled}
        onChange={(event) =>
          onChange({ ...settings, colorMode: event.target.value as ColorMode })
        }
      >
        <Radio.Button value="monochrome">黑白</Radio.Button>
        <Radio.Button value="color">彩色</Radio.Button>
      </Radio.Group>
      {!availability.colorEnabled && (
        <Typography.Text type="secondary" className="field-hint">
          彩色不可用：{selectedPrinter?.color.detail ?? '打印机不支持或能力未知'}
        </Typography.Text>
      )}

      <label className="field-label">单双面</label>
      <Radio.Group
        value={settings.sidesMode}
        onChange={(event) => {
          const sidesMode = event.target.value as SidesMode;
          onChange({
            ...settings,
            sidesMode,
          });
        }}
      >
        <Radio.Button value="simplex">单面</Radio.Button>
        <Radio.Button value="duplex" disabled={!availability.duplexEnabled}>
          双面
        </Radio.Button>
      </Radio.Group>

      <label className="field-label">翻转方式</label>
      <Radio.Group
        value={settings.flipMode}
        disabled={settings.sidesMode !== 'duplex' || !availability.flipEnabled}
        onChange={(event) =>
          onChange({ ...settings, flipMode: event.target.value as FlipMode })
        }
      >
        <Radio.Button value="longEdge">长边翻转</Radio.Button>
        <Radio.Button value="shortEdge">短边翻转</Radio.Button>
      </Radio.Group>
      {settings.sidesMode === 'duplex' && !availability.duplexEnabled && (
        <Typography.Text type="secondary" className="field-hint">
          双面不可用：{selectedPrinter?.duplex.detail ?? '打印机不支持或能力未知'}
        </Typography.Text>
      )}

      <label className="field-label" htmlFor="copies-input">打印份数</label>
      <Space>
        <InputNumber
          id="copies-input"
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
      </Space>

      {availability.reasons.length > 0 && (
        <Alert
          className="settings-alert"
          type="warning"
          showIcon
          message="能力与状态提示"
          description={availability.reasons.join('；')}
        />
      )}
    </div>
  );
}

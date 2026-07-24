import { Input, Typography } from 'antd';
import { Globe, Monitor, Slash, User, Lock } from 'lucide-react';
import type { ProxyMode, ProxySettings } from '../../domain/proxySettings';
import { isValidProxyUrl } from '../../domain/proxySettings';

interface ProxySettingsPanelProps {
  settings: ProxySettings;
  onChange: (settings: ProxySettings) => void;
}

const proxyModes: Array<{
  value: ProxyMode;
  label: string;
  description: string;
  icon: React.ReactNode;
}> = [
  {
    value: 'system',
    label: '使用系统代理',
    description: '自动使用操作系统配置的代理',
    icon: <Monitor size={18} />,
  },
  {
    value: 'none',
    label: '不使用代理',
    description: '直接连接，不经过任何代理服务器',
    icon: <Slash size={18} />,
  },
  {
    value: 'custom',
    label: '自定义代理',
    description: '手动配置代理服务器地址',
    icon: <Globe size={18} />,
  },
];

export function ProxySettingsPanel({ settings, onChange }: ProxySettingsPanelProps) {
  const isCustom = settings.mode === 'custom';
  const showUrlError =
    isCustom &&
    settings.customUrl !== '' &&
    settings.customUrl !== undefined &&
    !isValidProxyUrl(settings.customUrl);

  return (
    <div className="proxy-settings-panel">
      <div className="proxy-settings-hint">
        <Typography.Text type="secondary">
          代理仅用于检查更新，不影响打印机的获取和打印功能。
        </Typography.Text>
      </div>

      <div className="proxy-mode-cards">
        {proxyModes.map((mode) => {
          const isActive = settings.mode === mode.value;
          return (
            <button
              key={mode.value}
              type="button"
              className={`proxy-mode-card ${isActive ? 'active' : ''}`}
              onClick={() => onChange({ ...settings, mode: mode.value })}
            >
              <div className="proxy-mode-icon">{mode.icon}</div>
              <div className="proxy-mode-content">
                <div className="proxy-mode-label">{mode.label}</div>
                <div className="proxy-mode-desc">{mode.description}</div>
              </div>
              <div className={`proxy-mode-indicator ${isActive ? 'active' : ''}`} />
            </button>
          );
        })}
      </div>

      {isCustom && (
        <div className="proxy-custom-fields">
          <div className="proxy-field">
            <label className="proxy-field-label" htmlFor="proxy-url">
              代理地址
            </label>
            <Input
              id="proxy-url"
              placeholder="http://127.0.0.1:7890"
              value={settings.customUrl ?? ''}
              onChange={(event) => onChange({ ...settings, customUrl: event.target.value })}
              status={showUrlError ? 'error' : undefined}
              className={showUrlError ? 'proxy-input-error' : ''}
            />
            {showUrlError && (
              <span className="proxy-field-error">
                请输入有效的代理地址（支持 http/https/socks4/socks5）
              </span>
            )}
          </div>

          <div className="proxy-auth-row">
            <div className="proxy-field proxy-field-half">
              <label className="proxy-field-label" htmlFor="proxy-username">
                <User size={12} />
                <span>用户名</span>
              </label>
              <Input
                id="proxy-username"
                placeholder="可选"
                value={settings.customUsername ?? ''}
                onChange={(event) => onChange({ ...settings, customUsername: event.target.value })}
              />
            </div>

            <div className="proxy-field proxy-field-half">
              <label className="proxy-field-label" htmlFor="proxy-password">
                <Lock size={12} />
                <span>密码</span>
              </label>
              <Input.Password
                id="proxy-password"
                placeholder="可选"
                value={settings.customPassword ?? ''}
                onChange={(event) => onChange({ ...settings, customPassword: event.target.value })}
              />
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
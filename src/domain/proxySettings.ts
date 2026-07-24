export type ProxyMode = 'none' | 'system' | 'custom';

export interface ProxySettings {
  mode: ProxyMode;
  customUrl?: string;
  customUsername?: string;
  customPassword?: string;
}

export function createDefaultProxySettings(): ProxySettings {
  return {
    mode: 'system',
    customUrl: '',
    customUsername: '',
    customPassword: '',
  };
}

export function isValidProxyUrl(url: string): boolean {
  if (!url) return false;
  try {
    const parsed = new URL(url);
    return ['http:', 'https:', 'socks4:', 'socks5:'].includes(parsed.protocol);
  } catch {
    return false;
  }
}

export function getProxyUrl(settings: ProxySettings): string | undefined {
  if (settings.mode === 'none') {
    return undefined;
  }
  if (settings.mode === 'system') {
    // 系统代理将在Rust端通过环境变量检测
    return undefined;
  }
  if (settings.mode === 'custom' && settings.customUrl) {
    return settings.customUrl;
  }
  return undefined;
}

export function getProxyConfig(settings: ProxySettings): {
  useSystemProxy: boolean;
  customProxyUrl?: string;
  username?: string;
  password?: string;
} {
  return {
    useSystemProxy: settings.mode === 'system',
    customProxyUrl: settings.mode === 'custom' ? settings.customUrl : undefined,
    username: settings.mode === 'custom' ? settings.customUsername : undefined,
    password: settings.mode === 'custom' ? settings.customPassword : undefined,
  };
}
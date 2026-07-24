import type { SystemPrinter } from '../shared/contracts/printer';
import type { PrintBatchRequest, PrintBatchResult } from '../shared/contracts/printJob';

interface TauriWindow {
  __TAURI_INTERNALS__?: unknown;
}

function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && Boolean((window as TauriWindow).__TAURI_INTERNALS__);
}

async function invokeCommand<TResponse>(
  commandName: string,
  args?: Record<string, unknown>,
): Promise<TResponse> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<TResponse>(commandName, args);
}

export async function listSystemPrinters(): Promise<SystemPrinter[]> {
  if (!isTauriRuntime()) {
    return [];
  }
  return invokeCommand<SystemPrinter[]>('list_system_printers');
}

export async function pickFiles(): Promise<string[]> {
  if (!isTauriRuntime()) {
    return [];
  }
  return invokeCommand<string[]>('pick_files');
}

export async function pickFolderFiles(): Promise<string[]> {
  if (!isTauriRuntime()) {
    return [];
  }
  return invokeCommand<string[]>('pick_folder_files');
}

export async function runPrintBatch(request: PrintBatchRequest): Promise<PrintBatchResult> {
  if (!isTauriRuntime()) {
    throw new Error('当前不在桌面运行时中，无法执行真实打印');
  }
  return invokeCommand<PrintBatchResult>('run_print_batch', { request });
}

export interface ProxyConfigPayload {
  useSystemProxy: boolean;
  customProxyUrl?: string;
  username?: string;
  password?: string;
}

export interface UpdateCheckResult {
  available: boolean;
  version?: string;
  body?: string;
  downloadUrl?: string;
  downloadSize?: number;
}

export async function checkForAppUpdate(
  proxy?: ProxyConfigPayload,
): Promise<UpdateCheckResult> {
  if (!isTauriRuntime()) {
    return { available: false };
  }
  return invokeCommand<UpdateCheckResult>('check_for_app_update', { proxy: proxy ?? null });
}

export interface UpdateDownloadProgress {
  percent: number;
  downloaded: number;
  total: number;
}

export async function downloadAndInstallUpdate(
  downloadUrl: string,
  proxy?: ProxyConfigPayload,
): Promise<string> {
  if (!isTauriRuntime()) {
    throw new Error('当前不在桌面运行时中，无法下载更新');
  }
  return invokeCommand<string>('download_and_install_update', {
    downloadUrl,
    proxy: proxy ?? null,
  });
}

export async function openReleasePage(): Promise<void> {
  if (!isTauriRuntime()) {
    window.open('https://github.com/ws1993/PrintAssist/releases/latest', '_blank');
    return;
  }
  await invokeCommand('open_release_page');
}

export function subscribeUpdateDownloadProgress(
  onProgress: (progress: UpdateDownloadProgress) => void,
): () => void {
  if (!isTauriRuntime()) {
    return () => undefined;
  }

  let disposed = false;
  let unlisten: (() => void) | undefined;

  void import('@tauri-apps/api/event').then(({ listen }) => {
    if (disposed) {
      return;
    }
    void listen<UpdateDownloadProgress>('update-download-progress', (event) => {
      onProgress(event.payload);
    }).then((stop) => {
      if (disposed) {
        stop();
        return;
      }
      unlisten = stop;
    });
  });

  return () => {
    disposed = true;
    unlisten?.();
  };
}

export function subscribeIncomingFiles(
  onFiles: (paths: string[]) => void,
): () => void {
  if (!isTauriRuntime()) {
    return () => undefined;
  }

  let disposed = false;
  let unlisten: (() => void) | undefined;

  void import('@tauri-apps/api/event').then(({ listen }) => {
    if (disposed) {
      return;
    }
    void listen<string[]>('files-added', (event) => {
      onFiles(event.payload ?? []);
    }).then((stop) => {
      if (disposed) {
        stop();
        return;
      }
      unlisten = stop;
    });
  });

  return () => {
    disposed = true;
    unlisten?.();
  };
}

/**
 * Tauri 2 桌面拖放：通过原生 onDragDropEvent 获取本地路径。
 * HTML5 DataTransfer.File.path 在 WebView2 中不可用，不能作为桌面路径来源。
 */
export function subscribeNativeDragDrop(handlers: {
  onHoverChange?: (active: boolean) => void;
  onDrop?: (paths: string[]) => void;
}): () => void {
  if (!isTauriRuntime()) {
    return () => undefined;
  }

  let disposed = false;
  let unlisten: (() => void) | undefined;

  void import('@tauri-apps/api/webview').then(({ getCurrentWebview }) => {
    if (disposed) {
      return;
    }
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        switch (event.payload.type) {
          case 'enter':
          case 'over':
            handlers.onHoverChange?.(true);
            break;
          case 'leave':
            handlers.onHoverChange?.(false);
            break;
          case 'drop':
            handlers.onHoverChange?.(false);
            handlers.onDrop?.(event.payload.paths ?? []);
            break;
          default:
            break;
        }
      })
      .then((stop) => {
        if (disposed) {
          stop();
          return;
        }
        unlisten = stop;
      });
  });

  return () => {
    disposed = true;
    unlisten?.();
  };
}

/** 将拖入的文件/文件夹路径展开为可打印文件列表（与右键/文件夹选择一致）。 */
export async function expandFilePaths(paths: string[]): Promise<string[]> {
  if (!isTauriRuntime() || paths.length === 0) {
    return paths;
  }
  return invokeCommand<string[]>('expand_file_paths', { paths });
}

export { isTauriRuntime };

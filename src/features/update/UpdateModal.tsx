import { Button, Modal, Progress, Typography } from 'antd';
import {
  AlertCircle,
  ArrowDownToLine,
  Download,
  ExternalLink,
  Loader2,
  X,
} from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import {
  downloadAndInstallUpdate,
  openReleasePage,
  subscribeUpdateDownloadProgress,
  type ProxyConfigPayload,
  type UpdateDownloadProgress,
} from '../../api/nativeBridge';

const { Text } = Typography;

export interface UpdateInfo {
  available: boolean;
  version?: string;
  body?: string;
  downloadUrl?: string;
  downloadSize?: number;
}

type DownloadState = 'idle' | 'downloading' | 'error';

interface UpdateModalProps {
  open: boolean;
  updateInfo: UpdateInfo | null;
  proxyConfig: ProxyConfigPayload;
  onClose: () => void;
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function UpdateModal({ open, updateInfo, proxyConfig, onClose }: UpdateModalProps) {
  const [downloadState, setDownloadState] = useState<DownloadState>('idle');
  const [progress, setProgress] = useState<UpdateDownloadProgress>({ percent: 0, downloaded: 0, total: 0 });
  const [errorMessage, setErrorMessage] = useState('');
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  useEffect(() => {
    if (!open) {
      setDownloadState('idle');
      setProgress({ percent: 0, downloaded: 0, total: 0 });
      setErrorMessage('');
    }
  }, [open]);

  const handleDownload = useCallback(async () => {
    if (!updateInfo?.downloadUrl) {
      setErrorMessage('未找到下载链接');
      setDownloadState('error');
      return;
    }

    setDownloadState('downloading');
    setProgress({ percent: 0, downloaded: 0, total: 0 });
    setErrorMessage('');

    const unsubscribe = subscribeUpdateDownloadProgress((p) => {
      if (mountedRef.current) {
        setProgress(p);
      }
    });

    try {
      await downloadAndInstallUpdate(updateInfo.downloadUrl, proxyConfig);
    } catch (error) {
      if (mountedRef.current) {
        setDownloadState('error');
        setErrorMessage(error instanceof Error ? error.message : '下载更新失败');
      }
    } finally {
      unsubscribe();
    }
  }, [updateInfo, proxyConfig]);

  const handleOpenBrowser = async () => {
    await openReleasePage();
  };

  const handleClose = () => {
    if (downloadState !== 'downloading') {
      onClose();
    }
  };

  const changelogLines = updateInfo?.body
    ? updateInfo.body
        .split('\n')
        .map((line) => line.trim())
        .filter((line) => line.length > 0)
    : [];

  const isDownloading = downloadState === 'downloading';
  const hasError = downloadState === 'error';
  const hasDownloadUrl = Boolean(updateInfo?.downloadUrl);

  return (
    <Modal
      open={open}
      onCancel={handleClose}
      closable={!isDownloading}
      closeIcon={<X size={16} />}
      footer={null}
      width={440}
      centered
      maskClosable={!isDownloading}
      keyboard={!isDownloading}
      className="update-modal"
      destroyOnClose
    >
      <div className="update-modal-content">
        <div className="update-modal-header">
          <div className={`update-modal-icon ${hasError ? 'error' : ''}`}>
            {hasError ? <AlertCircle size={22} /> : <Download size={22} />}
          </div>
          <div className="update-modal-title-group">
            <Text strong className="update-modal-title">
              {hasError ? '更新失败' : '发现新版本'}
            </Text>
            {updateInfo?.version && (
              <span className="update-modal-version-badge">v{updateInfo.version}</span>
            )}
          </div>
        </div>

        {hasError && (
          <div className="update-modal-error">
            <Text className="update-modal-error-message">{errorMessage}</Text>
            <Text className="update-modal-error-hint">
              您可以尝试从浏览器手动下载安装
            </Text>
          </div>
        )}

        {isDownloading && (
          <div className="update-modal-progress">
            <Progress
              percent={progress.percent}
              status="active"
              strokeColor="#1557d0"
              trailColor="#e8eef5"
              showInfo={false}
            />
            <div className="update-modal-progress-info">
              <Text className="update-modal-progress-text">
                正在下载更新...
              </Text>
              <Text className="update-modal-progress-size">
                {progress.total > 0
                  ? `${formatFileSize(progress.downloaded)} / ${formatFileSize(progress.total)}`
                  : formatFileSize(progress.downloaded)}
              </Text>
            </div>
          </div>
        )}

        {!isDownloading && !hasError && changelogLines.length > 0 && (
          <div className="update-modal-changelog">
            <Text className="update-modal-changelog-label">更新内容</Text>
            <div className="update-modal-changelog-body">
              {changelogLines.map((line, index) => (
                <div key={index} className="update-modal-changelog-line">
                  {line}
                </div>
              ))}
            </div>
          </div>
        )}

        <div className="update-modal-actions">
          <Button
            size="large"
            disabled={isDownloading}
            onClick={handleClose}
            className="update-modal-btn-secondary"
          >
            {hasError ? '取消' : '稍后再说'}
          </Button>
          {hasError ? (
            <Button
              type="primary"
              size="large"
              onClick={() => void handleOpenBrowser()}
              icon={<ExternalLink size={15} />}
              className="update-modal-btn-primary"
            >
              从浏览器下载
            </Button>
          ) : isDownloading ? (
            <Button
              type="primary"
              size="large"
              loading
              className="update-modal-btn-primary"
            >
              下载中...
            </Button>
          ) : hasDownloadUrl ? (
            <Button
              type="primary"
              size="large"
              onClick={() => void handleDownload()}
              icon={<ArrowDownToLine size={15} />}
              className="update-modal-btn-primary"
            >
              下载并安装
            </Button>
          ) : (
            <Button
              type="primary"
              size="large"
              onClick={() => void handleOpenBrowser()}
              icon={<ExternalLink size={15} />}
              className="update-modal-btn-primary"
            >
              前往下载
            </Button>
          )}
        </div>

        {!hasError && (
          <Text className="update-modal-hint">
            {isDownloading
              ? '下载完成后将自动退出并安装'
              : '安装前将自动退出应用，安装完成后重新打开'}
          </Text>
        )}
      </div>
    </Modal>
  );
}

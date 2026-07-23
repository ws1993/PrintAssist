import { Alert, Button, List, Space, Typography } from 'antd';
import type { PrintJobSummary } from '../../domain/queueTypes';

interface PrintSummaryProps {
  summary: PrintJobSummary | null;
  onRetryFailed: () => void;
}

export function PrintSummary({ summary, onRetryFailed }: PrintSummaryProps) {
  if (!summary) {
    return null;
  }

  const failedItems = summary.results.filter((resultItem) => resultItem.status === 'failed');

  return (
    <div className="summary-panel">
      <Alert
        type={summary.failed > 0 ? 'warning' : 'success'}
        showIcon
        message={`打印完成：成功 ${summary.succeeded}，失败 ${summary.failed}，跳过 ${summary.skipped}`}
        description={
          summary.failed > 0
            ? '单项失败不会阻断整批任务。可仅重试失败项。'
            : '全部成功。确认后可清空列表，避免再次点击「开始打印」时误以为仍有待打文件。'
        }
        action={
          summary.failed > 0 ? (
            <Button size="small" type="primary" onClick={onRetryFailed}>
              仅重试失败项
            </Button>
          ) : undefined
        }
      />

      {failedItems.length > 0 && (
        <List
          size="small"
          header={<Typography.Text strong>失败明细</Typography.Text>}
          dataSource={failedItems}
          renderItem={(item) => (
            <List.Item>
              <Space direction="vertical" size={0}>
                <Typography.Text>{item.fileName}</Typography.Text>
                <Typography.Text type="secondary">{item.message ?? '未知错误'}</Typography.Text>
              </Space>
            </List.Item>
          )}
        />
      )}
    </div>
  );
}

import { Button, Empty, Space, Table, Tag, Typography } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { Settings2, Trash2 } from 'lucide-react';
import type { QueueItem } from '../../domain/queueTypes';
import { describePageRange } from '../../domain/pageRange';
import { hasFileOverride, mergePrintSettings, type PrintSettings } from '../../domain/printSettings';

interface PrintQueueProps {
  items: QueueItem[];
  globalSettings: PrintSettings;
  isPrinting: boolean;
  onRemove: (id: string) => void;
  onOpenSettings: (id: string) => void;
}

function statusTag(status: QueueItem['status']) {
  switch (status) {
    case 'ready':
    case 'pending':
      return <Tag color="blue">待打印</Tag>;
    case 'printing':
      return <Tag color="processing">打印中</Tag>;
    case 'succeeded':
      return <Tag color="success">成功</Tag>;
    case 'failed':
      return <Tag color="error">失败</Tag>;
    case 'skipped':
      return <Tag>跳过</Tag>;
    case 'analyzing':
      return <Tag color="gold">分析中</Tag>;
    default:
      return <Tag>{status}</Tag>;
  }
}

function kindLabel(kind: QueueItem['kind']): string {
  switch (kind) {
    case 'pdf':
      return 'PDF';
    case 'image':
      return '图片';
    case 'text':
      return '文本';
    case 'word':
      return 'Word';
    case 'excel':
      return 'Excel';
    case 'powerpoint':
      return 'PowerPoint';
    default:
      return '未知';
  }
}

export function PrintQueue({
  items,
  globalSettings,
  isPrinting,
  onRemove,
  onOpenSettings,
}: PrintQueueProps) {
  const columns: ColumnsType<QueueItem> = [
    {
      title: '文件',
      dataIndex: 'fileName',
      key: 'fileName',
      render: (fileName: string, record) => (
        <div>
          <Typography.Text strong>{fileName}</Typography.Text>
          <div className="muted-path">{record.path}</div>
        </div>
      ),
    },
    {
      title: '类型',
      dataIndex: 'kind',
      key: 'kind',
      width: 110,
      render: (kind: QueueItem['kind']) => kindLabel(kind),
    },
    {
      title: '页数',
      dataIndex: 'pageCount',
      key: 'pageCount',
      width: 80,
      render: (pageCount: number | null) => pageCount ?? '—',
    },
    {
      title: '设置',
      key: 'settings',
      width: 220,
      render: (_, record) => {
        const resolved = mergePrintSettings(globalSettings, record.override);
        return (
          <div>
            <div>
              {resolved.colorMode === 'color' ? '彩色' : '黑白'} ·{' '}
              {resolved.sidesMode === 'duplex'
                ? `双面/${resolved.flipMode === 'longEdge' ? '长边' : '短边'}`
                : '单面'}{' '}
              · {resolved.copies} 份
            </div>
            <Typography.Text type="secondary">
              {describePageRange(resolved.pageRange)}
              {hasFileOverride(record.override) ? ' · 已覆盖' : ' · 继承公共'}
            </Typography.Text>
          </div>
        );
      },
    },
    {
      title: '状态',
      dataIndex: 'status',
      key: 'status',
      width: 110,
      render: (status: QueueItem['status'], record) => (
        <div>
          {statusTag(status)}
          {record.errorMessage && (
            <div className="error-text">{record.errorMessage}</div>
          )}
        </div>
      ),
    },
    {
      title: '操作',
      key: 'actions',
      width: 150,
      render: (_, record) => (
        <Space>
          <Button
            size="small"
            icon={<Settings2 size={14} />}
            disabled={isPrinting}
            onClick={() => onOpenSettings(record.id)}
          >
            设置
          </Button>
          <Button
            size="small"
            danger
            icon={<Trash2 size={14} />}
            disabled={isPrinting}
            onClick={() => onRemove(record.id)}
          >
            移除
          </Button>
        </Space>
      ),
    },
  ];

  if (items.length === 0) {
    return <Empty description="尚未添加文件。选择文件、文件夹、拖放或从资源管理器传入。" />;
  }

  return (
    <Table
      rowKey="id"
      size="middle"
      pagination={false}
      columns={columns}
      dataSource={items}
    />
  );
}

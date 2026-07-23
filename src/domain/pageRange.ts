export type PageRangeMode = 'all' | 'custom';

export interface PageRangeInput {
  mode: PageRangeMode;
  expression: string;
}

export interface PageRangeParseSuccess {
  ok: true;
  pages: number[];
}

export interface PageRangeParseFailure {
  ok: false;
  message: string;
}

export type PageRangeParseResult = PageRangeParseSuccess | PageRangeParseFailure;

/**
 * Parses expressions such as "1,3,5-8".
 * Pages are 1-based, unique, sorted ascending.
 */
export function parsePageRangeExpression(
  expression: string,
  totalPages?: number,
): PageRangeParseResult {
  const normalizedExpression = expression.replace(/\s+/g, '');
  if (!normalizedExpression) {
    return { ok: false, message: '页码表达式不能为空' };
  }

  if (!/^\d+(?:-\d+)?(?:,\d+(?:-\d+)?)*$/.test(normalizedExpression)) {
    return {
      ok: false,
      message: '页码格式无效，请使用类似 1,3,5-8 的表达式',
    };
  }

  const pageSet = new Set<number>();
  const segments = normalizedExpression.split(',');

  for (const segment of segments) {
    if (segment.includes('-')) {
      const [startText, endText] = segment.split('-');
      const rangeStart = Number(startText);
      const rangeEnd = Number(endText);
      if (!Number.isInteger(rangeStart) || !Number.isInteger(rangeEnd)) {
        return { ok: false, message: `页码段无效：${segment}` };
      }
      if (rangeStart < 1 || rangeEnd < 1) {
        return { ok: false, message: '页码必须从 1 开始' };
      }
      if (rangeStart > rangeEnd) {
        return { ok: false, message: `页码范围起止颠倒：${segment}` };
      }
      for (let pageNumber = rangeStart; pageNumber <= rangeEnd; pageNumber += 1) {
        if (totalPages !== undefined && pageNumber > totalPages) {
          return {
            ok: false,
            message: `页码 ${pageNumber} 超出文档总页数 ${totalPages}`,
          };
        }
        pageSet.add(pageNumber);
      }
      continue;
    }

    const pageNumber = Number(segment);
    if (!Number.isInteger(pageNumber) || pageNumber < 1) {
      return { ok: false, message: `页码无效：${segment}` };
    }
    if (totalPages !== undefined && pageNumber > totalPages) {
      return {
        ok: false,
        message: `页码 ${pageNumber} 超出文档总页数 ${totalPages}`,
      };
    }
    pageSet.add(pageNumber);
  }

  const pages = Array.from(pageSet).sort((left, right) => left - right);
  if (pages.length === 0) {
    return { ok: false, message: '未解析到有效页码' };
  }

  return { ok: true, pages };
}

export function describePageRange(pageRange: PageRangeInput): string {
  if (pageRange.mode === 'all') {
    return '全部页';
  }
  return pageRange.expression.trim() || '未指定';
}

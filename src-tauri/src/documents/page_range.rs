/// Parses expressions such as "1,3,5-8".
/// Pages are 1-based, unique, sorted ascending.
pub fn parse_page_range_expression(
    expression: &str,
    total_pages: Option<u32>,
) -> Result<Vec<u32>, String> {
    let normalized: String = expression
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect();
    if normalized.is_empty() {
        return Err("页码表达式不能为空".to_string());
    }

    if !is_valid_expression_shape(&normalized) {
        return Err("页码格式无效，请使用类似 1,3,5-8 的表达式".to_string());
    }

    let mut page_set = std::collections::BTreeSet::new();

    for segment in normalized.split(',') {
        if segment.contains('-') {
            let mut parts = segment.splitn(2, '-');
            let start_text = parts.next().unwrap_or_default();
            let end_text = parts.next().unwrap_or_default();
            let range_start = parse_page_number(start_text, segment)?;
            let range_end = parse_page_number(end_text, segment)?;
            if range_start > range_end {
                return Err(format!("页码范围起止颠倒：{segment}"));
            }
            for page_number in range_start..=range_end {
                validate_against_total(page_number, total_pages)?;
                page_set.insert(page_number);
            }
            continue;
        }

        let page_number = parse_page_number(segment, segment)?;
        validate_against_total(page_number, total_pages)?;
        page_set.insert(page_number);
    }

    if page_set.is_empty() {
        return Err("未解析到有效页码".to_string());
    }

    Ok(page_set.into_iter().collect())
}

fn is_valid_expression_shape(expression: &str) -> bool {
    // Mirrors frontend: /^\d+(?:-\d+)?(?:,\d+(?:-\d+)?)*$/
    let bytes = expression.as_bytes();
    if bytes.is_empty() {
        return false;
    }

    let mut index = 0;
    loop {
        if index >= bytes.len() || !bytes[index].is_ascii_digit() {
            return false;
        }
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index < bytes.len() && bytes[index] == b'-' {
            index += 1;
            if index >= bytes.len() || !bytes[index].is_ascii_digit() {
                return false;
            }
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
        }
        if index == bytes.len() {
            return true;
        }
        if bytes[index] != b',' {
            return false;
        }
        index += 1;
    }
}

fn parse_page_number(text: &str, segment: &str) -> Result<u32, String> {
    let page_number = text
        .parse::<u32>()
        .map_err(|_| format!("页码段无效：{segment}"))?;
    if page_number < 1 {
        return Err("页码必须从 1 开始".to_string());
    }
    Ok(page_number)
}

fn validate_against_total(page_number: u32, total_pages: Option<u32>) -> Result<(), String> {
    if let Some(total) = total_pages {
        if page_number > total {
            return Err(format!("页码 {page_number} 超出文档总页数 {total}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mixed_commas_and_ranges() {
        let pages = parse_page_range_expression("1,3,5-8", None).expect("valid expression");
        assert_eq!(pages, vec![1, 3, 5, 6, 7, 8]);
    }

    #[test]
    fn rejects_inverted_ranges() {
        assert!(parse_page_range_expression("8-5", None).is_err());
    }

    #[test]
    fn rejects_out_of_bounds_pages() {
        assert!(parse_page_range_expression("1,12", Some(10)).is_err());
    }

    #[test]
    fn rejects_empty_expression() {
        assert!(parse_page_range_expression("   ", None).is_err());
    }
}

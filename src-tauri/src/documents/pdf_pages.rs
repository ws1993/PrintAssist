use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use lopdf::{xref::XrefType, Document};

use super::page_range::parse_page_range_expression;

static OUTPUT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Builds a temporary PDF containing only the requested 1-based pages.
/// Caller owns cleanup of the returned path. Prefer deferred cleanup:
/// Shell `printto` opens the file asynchronously after ShellExecute returns.
pub fn extract_pages_to_temp_pdf(
    source_pdf: &Path,
    page_range_expression: &str,
) -> Result<PathBuf, String> {
    if !source_pdf.exists() {
        return Err(format!("文件不存在：{}", source_pdf.display()));
    }

    let mut document =
        Document::load(source_pdf).map_err(|error| format!("读取 PDF 失败：{error}"))?;

    let page_map = document.get_pages();
    let total_pages = page_map.len() as u32;
    if total_pages == 0 {
        return Err("PDF 不含任何页面".to_string());
    }

    let selected_pages = parse_page_range_expression(page_range_expression, Some(total_pages))?;
    let selected_set: BTreeSet<u32> = selected_pages.into_iter().collect();

    let pages_to_delete: Vec<u32> = (1..=total_pages)
        .filter(|page_number| !selected_set.contains(page_number))
        .collect();

    if pages_to_delete.len() as u32 == total_pages {
        return Err("页码范围未匹配到任何页面".to_string());
    }

    if !pages_to_delete.is_empty() {
        // Delete highest page numbers first so remaining 1-based indices stay stable.
        let mut descending = pages_to_delete;
        descending.sort_unstable_by(|left, right| right.cmp(left));
        document.delete_pages(&descending);
        let _ = document.prune_objects();
        // Gaps after prune confuse some writers; renumber keeps the rewrite compact.
        document.renumber_objects();
    }

    // Full rewrite must be self-contained. Source PDFs often carry incremental-update
    // trailer keys (Prev/XRefStm). Keeping them makes startxref point at a valid offset
    // in *this* file whose content is not an xref — lopdf then reports "Invalid file trailer".
    prepare_document_for_full_rewrite(&mut document);

    let sequence = OUTPUT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let output_path = staging_dir()?.join(format!(
        "printassist-pages-{}-{}-{}.pdf",
        std::process::id(),
        sequence,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0)
    ));

    // Explicitly drop the save handle so the print association can open the file.
    {
        let file = document
            .save(&output_path)
            .map_err(|error| format!("写入页码筛选 PDF 失败：{error}"))?;
        drop(file);
    }

    // Validate the written PDF is reloadable before handing it to Shell printto.
    let reloaded =
        Document::load(&output_path).map_err(|error| format!("页码筛选 PDF 校验失败：{error}"))?;
    let kept = reloaded.get_pages().len() as u32;
    if kept == 0 {
        let _ = fs::remove_file(&output_path);
        return Err("页码筛选后 PDF 不含任何页面".to_string());
    }
    if kept != selected_set.len() as u32 {
        let _ = fs::remove_file(&output_path);
        return Err(format!(
            "页码筛选结果页数不符：期望 {}，实际 {kept}",
            selected_set.len()
        ));
    }

    Ok(output_path)
}

fn staging_dir() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir()
        .join("PrintAssist")
        .join("print-staging");
    fs::create_dir_all(&dir).map_err(|error| format!("创建临时打印目录失败：{error}"))?;
    Ok(dir)
}

/// Strip incremental/xref-stream leftovers and force a classic xref table on save.
fn prepare_document_for_full_rewrite(document: &mut Document) {
    document.trailer.remove(b"Prev");
    document.trailer.remove(b"XRefStm");
    // Keys that only belong on a cross-reference *stream* dictionary.
    if document
        .trailer
        .get(b"Type")
        .ok()
        .and_then(|value| value.as_name().ok())
        .map(|name| name == b"XRef")
        .unwrap_or(false)
    {
        document.trailer.remove(b"Type");
    }
    document.trailer.remove(b"W");
    document.trailer.remove(b"Index");
    document.trailer.remove(b"Length");
    document.trailer.remove(b"Filter");
    document.trailer.remove(b"DecodeParms");
    document.reference_table.cross_reference_type = XrefType::CrossReferenceTable;
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Object, Stream, xref::XrefType};
    use std::fs;

    fn write_minimal_pdf(path: &Path, page_count: u32) {
        let mut document = Document::with_version("1.5");
        let pages_id = document.new_object_id();
        let font_id = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let resources_id = document.add_object(dictionary! {
            "Font" => dictionary! {
                "F1" => font_id,
            },
        });

        let mut kids = Vec::new();
        for page_index in 1..=page_count {
            let content = format!("BT /F1 24 Tf 72 720 Td (Page {page_index}) Tj ET");
            let content_id = document.add_object(Stream::new(dictionary! {}, content.into_bytes()));
            let page_id = document.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                "Contents" => content_id,
                "Resources" => resources_id,
            });
            kids.push(page_id.into());
        }

        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Count" => kids.len() as i64,
                "Kids" => kids,
            }),
        );

        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        document.save(path).expect("save test pdf");
    }

    #[test]
    fn extracts_selected_pages() {
        let source = std::env::temp_dir().join(format!(
            "printassist-test-source-{}.pdf",
            std::process::id()
        ));
        write_minimal_pdf(&source, 5);

        let extracted = extract_pages_to_temp_pdf(&source, "1,3,5").expect("extract pages");
        let document = Document::load(&extracted).expect("load extracted");
        assert_eq!(document.get_pages().len(), 3);

        let _ = fs::remove_file(source);
        let _ = fs::remove_file(extracted);
    }

    #[test]
    fn rejects_out_of_range_expression() {
        let source =
            std::env::temp_dir().join(format!("printassist-test-oor-{}.pdf", std::process::id()));
        write_minimal_pdf(&source, 2);
        let result = extract_pages_to_temp_pdf(&source, "1,9");
        assert!(result.is_err());
        let _ = fs::remove_file(source);
    }

    #[test]
    fn stale_prev_without_rewrite_prep_fails_reload() {
        let source = std::env::temp_dir().join(format!(
            "printassist-test-stale-prev-src-{}.pdf",
            std::process::id()
        ));
        let output = std::env::temp_dir().join(format!(
            "printassist-test-stale-prev-out-{}.pdf",
            std::process::id()
        ));
        write_minimal_pdf(&source, 3);

        let mut document = Document::load(&source).expect("load source");
        document.delete_pages(&[2]);
        let mid_offset = (fs::metadata(&source).expect("meta").len() / 2).max(1) as i64;
        document.trailer.set("Prev", Object::Integer(mid_offset));
        document.save(&output).expect("save without prep");

        let reload_error = Document::load(&output).expect_err("stale Prev must break reload");
        let message = reload_error.to_string();
        assert!(
            message.contains("trailer") || message.contains("Prev") || message.contains("xref"),
            "unexpected error: {message}"
        );

        let _ = fs::remove_file(source);
        let _ = fs::remove_file(output);
    }

    #[test]
    fn prepare_full_rewrite_allows_reload_with_stale_prev() {
        let source = std::env::temp_dir().join(format!(
            "printassist-test-prep-src-{}.pdf",
            std::process::id()
        ));
        let output = std::env::temp_dir().join(format!(
            "printassist-test-prep-out-{}.pdf",
            std::process::id()
        ));
        write_minimal_pdf(&source, 3);

        let mut document = Document::load(&source).expect("load source");
        document.delete_pages(&[2]);
        let mid_offset = (fs::metadata(&source).expect("meta").len() / 2).max(1) as i64;
        document.trailer.set("Prev", Object::Integer(mid_offset));
        document.trailer.set("XRefStm", Object::Integer(mid_offset));
        document.reference_table.cross_reference_type = XrefType::CrossReferenceStream;
        prepare_document_for_full_rewrite(&mut document);
        document.save(&output).expect("save with prep");

        let reloaded = Document::load(&output).expect("reload after prep");
        assert_eq!(reloaded.get_pages().len(), 2);

        let _ = fs::remove_file(source);
        let _ = fs::remove_file(output);
    }

    /// Real-world PDFs often use cross-reference streams and may carry
    /// incremental-update trailer keys (Prev/XRefStm). Rewrite after
    /// delete_pages must still produce a reloadable file.
    #[test]
    fn extracts_pages_from_cross_reference_stream_pdf() {
        let source = std::env::temp_dir().join(format!(
            "printassist-test-xref-stream-{}.pdf",
            std::process::id()
        ));
        {
            let mut document = Document::with_version("1.5");
            document.reference_table.cross_reference_type = XrefType::CrossReferenceStream;
            let pages_id = document.new_object_id();
            let mut kids = Vec::new();
            for page_index in 1..=4 {
                let content = format!("BT /F1 12 Tf 72 720 Td (P{page_index}) Tj ET");
                let content_id =
                    document.add_object(Stream::new(dictionary! {}, content.into_bytes()));
                let page_id = document.add_object(dictionary! {
                    "Type" => "Page",
                    "Parent" => pages_id,
                    "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                    "Contents" => content_id,
                });
                kids.push(page_id.into());
            }
            document.objects.insert(
                pages_id,
                Object::Dictionary(dictionary! {
                    "Type" => "Pages",
                    "Count" => kids.len() as i64,
                    "Kids" => kids,
                }),
            );
            let catalog_id = document.add_object(dictionary! {
                "Type" => "Catalog",
                "Pages" => pages_id,
            });
            document.trailer.set("Root", catalog_id);
            document.save(&source).expect("save xref-stream source");
        }

        let extracted = extract_pages_to_temp_pdf(&source, "1-2").expect("extract pages");
        let document = Document::load(&extracted).expect("load extracted");
        assert_eq!(document.get_pages().len(), 2);

        let _ = fs::remove_file(source);
        let _ = fs::remove_file(extracted);
    }
}

pub mod office;

#[cfg(windows)]
pub mod print_shell;

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    Pdf,
    Image,
    Text,
    Word,
    Excel,
    PowerPoint,
    Unknown,
}

pub fn detect_document_kind(path: &Path) -> DocumentKind {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "pdf" => DocumentKind::Pdf,
        "png" | "jpg" | "jpeg" | "bmp" | "tif" | "tiff" | "gif" => DocumentKind::Image,
        "txt" | "log" | "md" => DocumentKind::Text,
        "doc" | "docx" => DocumentKind::Word,
        "xls" | "xlsx" => DocumentKind::Excel,
        "ppt" | "pptx" => DocumentKind::PowerPoint,
        _ => DocumentKind::Unknown,
    }
}

pub mod office;
pub mod page_range;
pub mod pdf_pages;

#[cfg(windows)]
pub mod office_com;
#[cfg(windows)]
pub mod image_print;
#[cfg(windows)]
pub mod office_print;
#[cfg(windows)]
pub mod pdf_print;
#[cfg(windows)]
pub mod pdf_raster_d2d;
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
        "png" | "jpg" | "jpeg" | "jpe" | "jfif" | "bmp" | "dib" | "tif" | "tiff" | "gif"
        | "webp" | "ico" | "heic" | "heif" | "avif" | "emf" | "wmf" => DocumentKind::Image,
        "txt" | "log" | "md" => DocumentKind::Text,
        "doc" | "docx" => DocumentKind::Word,
        "xls" | "xlsx" => DocumentKind::Excel,
        "ppt" | "pptx" => DocumentKind::PowerPoint,
        _ => DocumentKind::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn detects_common_image_formats() {
        for extension in [
            "webp", "jfif", "jpe", "dib", "ico", "heic", "heif", "avif", "emf", "wmf", "png", "jpg",
        ] {
            let path_string = format!("C:\\tmp\\sample.{extension}");
            let path = Path::new(&path_string);
            assert_eq!(detect_document_kind(path), DocumentKind::Image);
        }
    }
}

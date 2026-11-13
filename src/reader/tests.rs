//! Reader-owned integration contracts. Window-only routing/focus workflows live
//! under app/document_window/tests; these suites also inspect private reader state.
use super::rendering::BITMAP_BUDGET;
use super::*;
use crate::app::keymap::install as bind_reader_keys;
use crate::reading_history::{LocalMark, RecentStore};
use crate::settings_ui::ShowSettings;
use gpui_kit::{AnyWindowHandle, TestAppContext, point, size, test::TestWindowExt};
use std::collections::BTreeMap;

mod authoring;
mod authoring_geometry;
mod authorship;
mod current_save;
mod find;
mod links;
mod loading;
mod marks;
mod navigation;
mod page_annotations;
mod positions;
mod reading_mode;
mod rendering;
mod selection;
mod sessions;
mod vim;
use rendering::cached_pages;

fn recent_test_window(
    cx: &mut TestAppContext,
    path: Option<PathBuf>,
) -> (AnyWindowHandle, Entity<Preview>) {
    cx.update(|cx| {
        let (handle, document) =
            gpui_kit::open_window(gpui::WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| crate::app::document_window::DocumentWindow::new(path, window, cx))
            })
            .unwrap();
        (handle, document.read(cx).reader().clone())
    })
}

fn mark(position: ReadingPosition) -> LocalMark {
    LocalMark {
        position,
        label: None,
    }
}

fn three_page_pdf(sizes: [(u32, u32); 3]) -> Vec<u8> {
    three_page_pdf_with_boxes(sizes, ["", "", ""])
}

fn three_page_pdf_with_boxes(sizes: [(u32, u32); 3], boxes: [&str; 3]) -> Vec<u8> {
    pdf_with_boxes(&sizes, &boxes)
}

fn pdf_with_boxes(sizes: &[(u32, u32)], boxes: &[&str]) -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            (3..3 + sizes.len())
                .map(|id| format!("{id} 0 R"))
                .collect::<Vec<_>>()
                .join(" "),
            sizes.len()
        ),
    ];
    for (size, box_options) in sizes.iter().zip(boxes) {
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] {} >>",
            size.0, size.1, box_options
        ));
    }
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

fn image_only_pdf() -> Vec<u8> {
    let width = 595;
    let height = 842;
    let pixels: Vec<u8> = (0..height)
        .flat_map(|y| (0..width).map(move |x| if (x / 13 + y / 13) % 2 == 0 { 224 } else { 64 }))
        .collect();
    let content = b"q\n595 0 0 842 0 0 cm\n/Im1 Do\nQ\n";
    let mut image = format!(
        "<< /Type /XObject /Subtype /Image /Width {width} /Height {height} \
         /ColorSpace /DeviceGray /BitsPerComponent 8 /Length {} >>\nstream\n",
        pixels.len()
    )
    .into_bytes();
    image.extend_from_slice(&pixels);
    image.extend_from_slice(b"\nendstream");
    let mut page_content = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
    page_content.extend_from_slice(content);
    page_content.extend_from_slice(b"endstream");
    let objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /XObject << /Im1 4 0 R >> >> /Contents 5 0 R >>".to_vec(),
        image,
        page_content,
    ];
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        pdf.extend_from_slice(object);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

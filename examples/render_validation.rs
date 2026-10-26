//! Render one PDF page at fixed scales for side-by-side inspection.
//! Run with `cargo +stable run --no-default-features --example render_validation -- PDF OUTPUT_DIR PAGE`.
use std::{env, error::Error, fs, path::PathBuf, time::Instant};

use kver::pdf;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let (Some(path), Some(output_dir), Some(page_number), None) =
        (args.next(), args.next(), args.next(), args.next())
    else {
        eprintln!("Usage: render_validation PDF OUTPUT_DIR PAGE_NUMBER (1-based)");
        std::process::exit(2);
    };
    let path = PathBuf::from(path);
    let output_dir = PathBuf::from(output_dir);
    let page_number: usize = page_number.to_string_lossy().parse()?;
    let page_index = page_number
        .checked_sub(1)
        .ok_or("Page number must be at least 1")?;
    fs::create_dir_all(&output_dir)?;

    println!("backend\tpage\tscale\twidth\theight\telapsed_ms\tpng");
    for scale in [1.0_f32, 2.0, 3.0] {
        // Includes file reading, PDF parsing, rasterization, and PNG encoding.
        // Each run is cold; disk writes are excluded from the timing.
        let start = Instant::now();
        let rendered = pdf::render_page(&path, page_index, scale)?;
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        let png_path = output_dir.join(format!("hayro-page-{page_number}-{scale:.0}x.png"));
        fs::write(&png_path, rendered.png)?;
        println!(
            "hayro\t{page_number}\t{scale:.0}\t{}\t{}\t{elapsed_ms:.1}\t{}",
            rendered.width,
            rendered.height,
            png_path.display()
        );
    }
    Ok(())
}

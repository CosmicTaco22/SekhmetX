// Hide the console window on Windows release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod db;
mod dicom_index;
mod frame;
mod geometry;
mod gpu_view;
mod i18n;
mod imaging;
mod loader;
mod navigation;
mod roi;
mod ui;
mod updater;
mod volume;

use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;

use eframe::egui;

const USAGE: &str = "\
SekhmetX

  sekhmetx [FOLDER_OR_FILES...]               open the viewer (optionally with a folder)
  sekhmetx --list FOLDER                      print the studies and series found
  sekhmetx --check FOLDER                     try to decode every image and report failures
  sekhmetx --render FOLDER OUT.png [SERIES] [IMAGE] [WIDTH LEVEL]
                                              save one image as PNG (no window)
";

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("--help") | Some("-h") => {
            print!("{USAGE}");
            return Ok(());
        }
        Some("--list") => {
            cli_list(&args[1..]);
            return Ok(());
        }
        Some("--check") => {
            cli_check(&args[1..]);
            return Ok(());
        }
        Some("--render") => {
            if let Err(e) = cli_render(&args[1..]) {
                eprintln!("error: {e:#}");
                std::process::exit(1);
            }
            return Ok(());
        }
        _ => {}
    }

    let initial: Vec<PathBuf> = args.iter().map(PathBuf::from).collect();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("SekhmetX")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([800.0, 500.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "SekhmetX",
        options,
        Box::new(move |cc| Ok(Box::new(app::ViewerApp::new(cc, initial)))),
    )
}

fn scan(folder: &str) -> dicom_index::Index {
    let (d, t) = (AtomicUsize::new(0), AtomicUsize::new(0));
    dicom_index::scan(&[PathBuf::from(folder)], &d, &t)
}

fn cli_list(args: &[String]) {
    let Some(folder) = args.first() else {
        eprint!("{USAGE}");
        return;
    };
    let index = scan(folder);
    println!(
        "{} files, {} skipped, {} studies, {} images",
        index.files_seen,
        index.files_skipped,
        index.studies.len(),
        index.image_count()
    );
    for (si, st) in index.studies.iter().enumerate() {
        println!("[{si}] {} | {} | {} | {}", st.patient_name, st.date, st.description, st.uid);
        for (ri, se) in st.series.iter().enumerate() {
            println!(
                "    [{si}.{ri}] {} #{} {:?} {} images {}x{}",
                se.modality,
                se.number,
                se.description,
                se.slices.len(),
                se.columns,
                se.rows
            );
        }
    }
}

fn cli_render(args: &[String]) -> anyhow::Result<()> {
    let (Some(folder), Some(out)) = (args.first(), args.get(1)) else {
        anyhow::bail!("usage: --render FOLDER OUT.png [SERIES] [IMAGE] [WIDTH LEVEL]");
    };
    let series_n: usize = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(0);
    let image_n: Option<usize> = args.get(3).map(|s| s.parse()).transpose()?;
    let window = match (args.get(4), args.get(5)) {
        (Some(w), Some(l)) => Some(imaging::Window::new(l.parse::<f32>()?, w.parse::<f32>()?)),
        _ => None,
    };

    let index = scan(folder);
    let series: Vec<&dicom_index::Series> = index.studies.iter().flat_map(|s| &s.series).collect();
    let se = series
        .get(series_n)
        .ok_or_else(|| anyhow::anyhow!("series {series_n} not found ({} series)", series.len()))?;
    let i = image_n.unwrap_or(se.slices.len() / 2).min(se.slices.len() - 1);
    let s = &se.slices[i];
    let f = frame::load_frame(&s.path, s.frame)?;
    let w = match window {
        Some(w) => f.requested(w).unwrap_or(f.full_window),
        None => f.default_window(),
    };
    let rgba = f.render(w, false);
    image::save_buffer(out, &rgba, f.width, f.height, image::ColorType::Rgba8)?;
    println!(
        "{} image {}/{} ({}x{}) WL {:.0} WW {:.0} -> {out}",
        se.description,
        i + 1,
        se.slices.len(),
        f.width,
        f.height,
        w.level,
        w.width
    );
    Ok(())
}

fn cli_check(args: &[String]) {
    let Some(folder) = args.first() else {
        eprint!("{USAGE}");
        return;
    };
    let index = scan(folder);
    let (mut ok, mut bad) = (0, 0);
    for st in &index.studies {
        for se in &st.series {
            for s in &se.slices {
                let r = std::panic::catch_unwind(|| frame::load_frame(&s.path, s.frame))
                    .unwrap_or_else(|_| Err(anyhow::anyhow!("decoder crashed")));
                match r {
                    Ok(_) => ok += 1,
                    Err(e) => {
                        bad += 1;
                        println!("FAIL {} [frame {}]: {e:#}", s.path.display(), s.frame);
                    }
                }
            }
        }
    }
    println!("{ok} images decoded, {bad} failed, {} non-image files skipped", index.files_skipped);
}

// Hide the console window on Windows release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod crash_log;
mod db;
mod dicom_index;
mod export;
mod frame;
mod geometry;
mod gpu_view;
mod i18n;
mod imaging;
mod loader;
mod navigation;
mod legacy_cleanup;
mod roi;
mod sculpt;
mod settings;
mod ui;
mod updater;
mod volume;

use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;

use eframe::egui;

pub const BUILD_VERSION: &str = env!("SEKHMETX_INTERNAL_VERSION");

const USAGE: &str = "\
SekhmetX

  sekhmetx [FOLDER_OR_FILES...]               open the viewer (optionally with a folder)
  sekhmetx --list FOLDER                      print the studies and series found
  sekhmetx --check FOLDER                     try to decode every image and report failures
  sekhmetx --render FOLDER OUT.png [SERIES] [IMAGE] [WIDTH LEVEL]
                                              save one image as PNG (no window)
  sekhmetx --roi FOLDER_OR_FILE [SERIES IMAGE] (ellipse|rect|length) X1 Y1 X2 Y2
                                              compute Horos-compatible ROI stats/length
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
        Some("--roi") => {
            if let Err(e) = cli_roi(&args[1..]) {
                eprintln!("error: {e:#}");
                std::process::exit(1);
            }
            return Ok(());
        }
        _ => {}
    }

    let prev_crash = crash_log::previous_run_crashed();
    crash_log::init();
    let initial: Vec<PathBuf> = args.iter().map(PathBuf::from).collect();
    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("SekhmetX")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([800.0, 500.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    #[cfg(target_os = "windows")]
    if std::env::var_os("WGPU_BACKEND").is_none()
        && windows_has_vulkan_adapter()
        && let eframe::egui_wgpu::WgpuSetup::CreateNew(ref mut create_new) =
            options.wgpu_options.wgpu_setup
    {
        // Prefer Vulkan on Windows for lower staging-memory overhead on large 3D MPR
        // volumes, but only when a Vulkan driver is present: many Intel GPUs, virtual
        // machines and Windows-on-ARM laptops have none, and forcing Vulkan there would
        // stop the app from starting. Without one, wgpu's default (Direct3D 12) is used.
        create_new.instance_descriptor.backends = eframe::egui_wgpu::wgpu::Backends::VULKAN;
    }
    let res = eframe::run_native(
        "SekhmetX",
        options,
        Box::new(move |cc| {
            let mut app = app::ViewerApp::new(cc, initial);
            app.previous_crash_pending = prev_crash;
            Ok(Box::new(app))
        }),
    );
    if let Err(ref e) = res {
        crash_log::log_line("ERROR", &format!("eframe::run_native failed: {e}"));
    }
    res
}

/// True when Windows has at least one Vulkan graphics adapter.
/// (Also built in tests, so this Windows-only path compiles and runs in CI on every system.)
#[cfg(any(target_os = "windows", test))]
fn windows_has_vulkan_adapter() -> bool {
    use eframe::egui_wgpu::wgpu;
    use std::future::Future;
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = wgpu::Backends::VULKAN;
    let instance = wgpu::Instance::new(desc);
    let mut fut = std::pin::pin!(instance.enumerate_adapters(wgpu::Backends::VULKAN));
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    // Native adapter enumeration completes immediately; if it ever did not,
    // treat Vulkan as unavailable and keep the safe default.
    match fut.as_mut().poll(&mut cx) {
        std::task::Poll::Ready(adapters) => {
            let found = !adapters.is_empty();
            crash_log::log_line(
                "INFO",
                if found { "Vulkan adapter found; using Vulkan" } else { "No Vulkan adapter; using Direct3D 12" },
            );
            found
        }
        std::task::Poll::Pending => false,
    }
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

fn evaluate_cli_roi(
    f: &frame::Frame,
    shape: &str,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
) -> anyhow::Result<String> {
    use roi::{RoiBoundingBox, RoiKind, RoiPlanarGeometry, RoiPoint, RoiSize};
    let a = RoiPoint::new(x1, y1);
    let b = RoiPoint::new(x2, y2);
    match shape.to_ascii_lowercase().as_str() {
        "ellipse" | "oval" => {
            let rect = RoiBoundingBox::rect(a, b, true);
            let outline = RoiPlanarGeometry::outline_of_oval(rect, 64);
            let eval = roi::evaluate_region(
                f,
                RoiKind::Oval,
                &outline,
                Some(RoiSize::new(rect.size.width, rect.size.height)),
            );
            let st = eval
                .stats
                .unwrap_or_else(|| roi::RoiPixelStatistics::compute(&[], false, false));
            let area_str = if let (Some(cm2), Some(mm2)) = (eval.area_cm2_or_px2, eval.area_mm2) {
                format!("area_cm2={cm2:.4} area_mm2={mm2:.2}")
            } else {
                format!("area_px2={:.2}", eval.area_cm2_or_px2.unwrap_or(0.0))
            };
            Ok(format!(
                "shape=ellipse {area_str} mean={:.2} sd={:.2} min={:.2} max={:.2} count={}",
                st.mean, st.deviation, st.minimum, st.maximum, st.count
            ))
        }
        "rect" | "rectangle" => {
            let rect = RoiBoundingBox::rect(a, b, false);
            let outline = RoiPlanarGeometry::outline_of_rectangle(rect);
            let eval = roi::evaluate_region(f, RoiKind::Rectangle, &outline, None);
            let st = eval
                .stats
                .unwrap_or_else(|| roi::RoiPixelStatistics::compute(&[], false, false));
            let area_str = if let (Some(cm2), Some(mm2)) = (eval.area_cm2_or_px2, eval.area_mm2) {
                format!("area_cm2={cm2:.4} area_mm2={mm2:.2}")
            } else {
                format!("area_px2={:.2}", eval.area_cm2_or_px2.unwrap_or(0.0))
            };
            Ok(format!(
                "shape=rect {area_str} mean={:.2} sd={:.2} min={:.2} max={:.2} count={}",
                st.mean, st.deviation, st.minimum, st.maximum, st.count
            ))
        }
        "length" | "line" => {
            let eval = roi::evaluate_length(f, a, b);
            if let Some(mm) = eval.length_mm {
                Ok(format!(
                    "shape=length length_mm={mm:.3} length_px={:.2}",
                    eval.length_px.unwrap_or(0.0)
                ))
            } else {
                Ok(format!(
                    "shape=length length_px={:.2} (uncalibrated)",
                    eval.length_px.unwrap_or(0.0)
                ))
            }
        }
        other => anyhow::bail!("unknown ROI shape '{other}'; expected ellipse, rect, or length"),
    }
}

fn cli_roi(args: &[String]) -> anyhow::Result<()> {
    let Some(target) = args.first() else {
        anyhow::bail!("usage: --roi FOLDER_OR_FILE [SERIES IMAGE] (ellipse|rect|length) X1 Y1 X2 Y2");
    };
    let (series_n, image_n, shape, coords) = if args.len() >= 6
        && matches!(
            args[1].to_ascii_lowercase().as_str(),
            "ellipse" | "oval" | "rect" | "rectangle" | "length" | "line"
        ) {
        (0usize, None, args[1].as_str(), &args[2..6])
    } else if args.len() >= 8 {
        let s: usize = args[1].parse()?;
        let i: usize = args[2].parse()?;
        (s, Some(i), args[3].as_str(), &args[4..8])
    } else {
        anyhow::bail!("usage: --roi FOLDER_OR_FILE [SERIES IMAGE] (ellipse|rect|length) X1 Y1 X2 Y2");
    };
    let x1: f64 = coords[0].parse()?;
    let y1: f64 = coords[1].parse()?;
    let x2: f64 = coords[2].parse()?;
    let y2: f64 = coords[3].parse()?;

    let index = scan(target);
    let series: Vec<&dicom_index::Series> = index.studies.iter().flat_map(|s| &s.series).collect();
    let se = series
        .get(series_n)
        .ok_or_else(|| anyhow::anyhow!("series {series_n} not found ({} series)", series.len()))?;
    let i = image_n.unwrap_or(se.slices.len() / 2).min(se.slices.len() - 1);
    let s = &se.slices[i];
    let f = frame::load_frame(&s.path, s.frame)?;
    let report = evaluate_cli_roi(&f, shape, x1, y1, x2, y2)?;
    println!("{report}");
    Ok(())
}

#[cfg(test)]
mod cli_tests {
    use super::*;

    #[test]
    fn cli_roi_computes_rect_ellipse_and_length() {
        let mut pixels = vec![100.0f32; 64 * 64];
        for y in 10..=20 {
            for x in 10..=20 {
                pixels[y * 64 + x] = 200.0;
            }
        }
        let f = frame::Frame {
            width: 64,
            height: 64,
            pixels: frame::Pixels::Gray(pixels),
            stored_window: None,
            full_window: imaging::Window::new(150.0, 200.0),
            bits_window: None,
            inverts: false,
            voi_lut_applied: false,
            slope: 1.0,
            spacing: Some((0.5, 0.5)),
            pixel_ratio: 1.0,
            origin: None,
            orientation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            is_3d_plane: true,
            frame_of_reference_uid: None,
            slice_thickness: None,
            slice_location: None,
            value_unit: "HU",
            suv_factor: None,
        };
        let rect_out = evaluate_cli_roi(&f, "rect", 10.0, 10.0, 20.0, 20.0).unwrap();
        assert!(rect_out.contains("shape=rect"), "{rect_out}");
        assert!(rect_out.contains("mean=200.00"), "{rect_out}");

        let ell_out = evaluate_cli_roi(&f, "ellipse", 15.0, 15.0, 5.0, 5.0).unwrap();
        assert!(ell_out.contains("shape=ellipse"), "{ell_out}");

        let len_out = evaluate_cli_roi(&f, "length", 0.0, 0.0, 10.0, 0.0).unwrap();
        assert!(len_out.contains("length_mm=5.000"), "{len_out}");
    }
}

#[cfg(test)]
mod vulkan_probe_tests {
    #[test]
    fn vulkan_probe_answers_without_panicking() {
        // The answer depends on the machine; it must simply return.
        let _ = super::windows_has_vulkan_adapter();
    }
}

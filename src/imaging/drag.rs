//! Dragging the window with the mouse.
//!
//! Follows IsiX `ViewerWindowLevel/WindowLevelDrag.swift`. The window moves by
//! width ÷ 80 per screen point (÷ 100 in the fused and MPR views), measured
//! from where the drag started. Moving up raises the level (darker), moving
//! right widens the window (less contrast).

use super::Window;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Fused and OrthogonalMpr arrive with fusion and MPR
pub enum Variant {
    /// The 2D viewer's image: step width / 80, at least 0.001 × rescale slope.
    Image,
    /// A fused image dragged with the blending tool: width / 100.
    Fused,
    /// The orthogonal MPR views: width / 100, at least 0.001.
    OrthogonalMpr,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drag {
    pub variant: Variant,
    pub level: f32,
    pub width: f32,
    /// Bottom and top of the starting range.
    pub minimum: f32,
    pub maximum: f32,
}

impl Drag {
    pub fn new(variant: Variant, start: Window) -> Self {
        Self {
            variant,
            level: start.level,
            width: start.width,
            minimum: start.level - start.width / 2.0,
            maximum: start.level + start.width / 2.0,
        }
    }

    /// How far the window moves for one screen point.
    pub fn step(&self, slope: f32) -> f32 {
        let (divisor, floor) = match self.variant {
            Variant::Image => (80.0, 0.001 * slope as f64),
            Variant::Fused => (100.0, 0.001 * slope as f64),
            Variant::OrthogonalMpr => (100.0, 0.001),
        };
        let step = (self.width as f64 / divisor) as f32;
        if (step as f64) < floor { floor as f32 } else { step }
    }

    /// The window after the mouse moved by (`dx`, `dy`) screen points from the
    /// start, with **y pointing up** (macOS convention; negate a screen y).
    /// `pet_mode` is Horos's PETWindowingMode preference for PET-like images
    /// and `None` for every other image.
    pub fn window(&self, dx: f64, dy: f64, pet_mode: Option<i32>, pet_minimum: i32, slope: f32) -> Window {
        let step = self.step(slope) as f64;
        let Some(mode) = pet_mode else {
            return Window::new((self.level as f64 + dy * step) as f32, (self.width as f64 + dx * step) as f32);
        };
        let floor_width = |w: f32| if (w as f64) < 0.1 { 0.1f64 as f32 } else { w };
        match mode {
            0 => Window::new(
                (self.level as f64 + dy * step) as f32,
                floor_width((self.width as f64 + dx * step) as f32),
            ),
            1 => {
                let top = (self.maximum as f64 + dy * step) as f32;
                let mut level = (top - self.minimum) / 2.0 + pet_minimum as f32;
                let width = floor_width(top - self.minimum);
                if level - width / 2.0 < 0.0 {
                    level = width / 2.0;
                }
                Window::new(level, width)
            }
            2 => {
                let top = (self.maximum as f64 + dy * step) as f32;
                let bottom = ((self.minimum as f64 + dx * step) as f32).max(0.0);
                let mut level = bottom + (top - bottom) / 2.0;
                let width = floor_width(top - bottom);
                if level - width / 2.0 < 0.0 {
                    level = width / 2.0;
                }
                Window::new(level, width)
            }
            _ => match self.variant {
                Variant::Image | Variant::Fused => Window::new(0.0, 0.0),
                Variant::OrthogonalMpr => Window::new(5.0, 5.0),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Objective-C drag IsiX's test compares against (WindowLevelTests.swift,
    /// `formerViewDrag` / `formerOrthogonalDrag`), written out again.
    #[allow(clippy::too_many_arguments)]
    fn former(variant: Variant, wl: f32, ww: f32, dx: f64, dy: f64, slope: f32, pet: bool, mode: i32, pet_min: i32) -> (f32, f32) {
        let (min, max) = (wl - ww / 2.0, wl + ww / 2.0);
        let mut adapter = match variant {
            Variant::Image => (ww as f64 / 80.0) as f32,
            _ => (ww as f64 / 100.0) as f32,
        };
        let floor = if variant == Variant::OrthogonalMpr { 0.001 } else { 0.001 * slope as f64 };
        if (adapter as f64) < floor {
            adapter = floor as f32;
        }
        let a = adapter as f64;
        if pet {
            let (mut e_wl, mut e_ww) = if variant == Variant::OrthogonalMpr { (5.0f32, 5.0f32) } else { (0.0, 0.0) };
            match mode {
                0 => {
                    e_wl = (wl as f64 + dy * a) as f32;
                    e_ww = (ww as f64 + dx * a) as f32;
                    if (e_ww as f64) < 0.1 { e_ww = 0.1f64 as f32; }
                }
                1 => {
                    let end = (max as f64 + dy * a) as f32;
                    e_wl = (end - min) / 2.0 + pet_min as f32;
                    e_ww = end - min;
                    if (e_ww as f64) < 0.1 { e_ww = 0.1f64 as f32; }
                    if e_wl - e_ww / 2.0 < 0.0 { e_wl = e_ww / 2.0; }
                }
                2 => {
                    let end = (max as f64 + dy * a) as f32;
                    let mut start = (min as f64 + dx * a) as f32;
                    if start < 0.0 { start = 0.0; }
                    e_wl = start + (end - start) / 2.0;
                    e_ww = end - start;
                    if (e_ww as f64) < 0.1 { e_ww = 0.1f64 as f32; }
                    if e_wl - e_ww / 2.0 < 0.0 { e_wl = e_ww / 2.0; }
                }
                _ => {}
            }
            return (e_wl, e_ww);
        }
        ((wl as f64 + dy * a) as f32, (ww as f64 + dx * a) as f32)
    }

    // IsiX WindowLevelDragTests.everyVariantModeAndMoveGivesWhatTheViewGave
    #[test]
    fn every_variant_mode_and_move_gives_what_the_view_gave() {
        let windows = [(40.0, 400.0), (-500.0, 1400.0), (3.5, 6.0), (0.0, 0.0), (2_000.0, 0.02), (12_000.0, 30_000.0), (-1.0, 1.0)];
        let moves = [(0.0, 0.0), (13.25, -7.5), (-250.0, 180.5), (0.5, 1_000.0), (-3.0, -3.0), (77.125, -0.25)];
        let slopes = [1.0, 0.0, 2.5, 0.01, 40.0];
        let modes = [0, 1, 2, 3, -1];
        let minimums = [0, 7, -20];
        let mut compared = 0;
        for variant in [Variant::Image, Variant::Fused, Variant::OrthogonalMpr] {
            for &(wl, ww) in &windows {
                let drag = Drag::new(variant, Window::new(wl, ww));
                assert_eq!((drag.minimum, drag.maximum), (wl - ww / 2.0, wl + ww / 2.0));
                for &(dx, dy) in &moves {
                    for &slope in &slopes {
                        for pet in [false, true] {
                            for &mode in &modes {
                                for &pet_min in &minimums {
                                    let want = former(variant, wl, ww, dx, dy, slope, pet, mode, pet_min);
                                    let got = drag.window(dx, dy, pet.then_some(mode), pet_min, slope);
                                    assert_eq!(
                                        (got.level.to_bits(), got.width.to_bits()),
                                        (want.0.to_bits(), want.1.to_bits()),
                                        "{variant:?} wl {wl} ww {ww} move ({dx},{dy}) slope {slope} pet {pet} mode {mode}"
                                    );
                                    compared += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(compared, 3 * 7 * 6 * 5 * 2 * 5 * 3);
    }

    // IsiX WindowLevelDragTests.theStepsOfTheThreeViews
    #[test]
    fn the_steps_of_the_three_views() {
        assert_eq!(Drag::new(Variant::Image, Window::new(40.0, 400.0)).step(1.0), 5.0);
        assert_eq!(Drag::new(Variant::Fused, Window::new(40.0, 400.0)).step(1.0), 4.0);
        assert_eq!(Drag::new(Variant::OrthogonalMpr, Window::new(40.0, 400.0)).step(1.0), 4.0);
        assert_eq!(Drag::new(Variant::Image, Window::new(1.0, 0.01)).step(20.0), 0.02f32);
        assert_eq!(Drag::new(Variant::OrthogonalMpr, Window::new(1.0, 0.01)).step(20.0), 0.001f32);
        assert_eq!(Drag::new(Variant::Image, Window::new(40.0, 400.0)).window(1.0, 1.0, Some(9), 0, 1.0), Window::new(0.0, 0.0));
        assert_eq!(Drag::new(Variant::OrthogonalMpr, Window::new(40.0, 400.0)).window(1.0, 1.0, Some(9), 0, 1.0), Window::new(5.0, 5.0));
    }
}

//! 3D sculpting ("scissors") for volume rendering: a coarse keep/cut map over the
//! volume, edited by drawing a loop on the 3D view and cutting everything inside
//! (or outside) it through the whole depth. The renderer skips cut cells.
//!
//! IsiX does this in its VTK-based 3D window (scissors on `VRView`), which cannot be
//! translated; this is a rebuild for the wgpu ray-caster in `src/gpu_view.rs`.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::volume::Volume3D;

/// The 3D view's camera, in the ray-caster's box space (the volume spans
/// `-half_ext..half_ext`, scaled so the longest side is 1).
#[derive(Debug, Clone, Copy)]
pub struct VrCamera {
    pub right: [f32; 3],
    pub up: [f32; 3],
    pub fwd: [f32; 3],
    pub half_ext: [f32; 3],
}

fn norm3(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-5);
    [v[0] / l, v[1] / l, v[2] / l]
}

fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The orbit camera of the 3D view: yaw 0, pitch 0 looks from the front (anterior)
/// with the head up and the patient's left on the right; then the view's in-plane
/// rotation and flips. Shared by the renderer and the scissors so a loop drawn on
/// screen cuts exactly what it covers.
pub fn vr_camera(vol: &Volume3D, yaw: f32, pitch: f32, rotation_deg: f32, x_flipped: bool, y_flipped: bool) -> VrCamera {
    let phys_x = (vol.width as f32 * vol.spacing[0] as f32).max(1.0);
    let phys_y = (vol.height as f32 * vol.spacing[1] as f32).max(1.0);
    let phys_z = (vol.depth as f32 * vol.spacing[2] as f32).max(1.0);
    let max_dim = phys_x.max(phys_y).max(phys_z);
    let half_ext = [0.5 * phys_x / max_dim, 0.5 * phys_y / max_dim, 0.5 * phys_z / max_dim];
    // Patient left, superior and anterior expressed along the volume's axes.
    let v_left = norm3([vol.row_dir[0] as f32, vol.col_dir[0] as f32, vol.slice_dir[0] as f32]);
    let v_sup = norm3([vol.row_dir[2] as f32, vol.col_dir[2] as f32, vol.slice_dir[2] as f32]);
    let v_ant = norm3(cross3(v_left, v_sup));
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    let cam_dir: [f32; 3] =
        std::array::from_fn(|k| v_ant[k] * cy * cp + v_left[k] * sy * cp + v_sup[k] * sp);
    let fwd = norm3([-cam_dir[0], -cam_dir[1], -cam_dir[2]]);
    let right0 = norm3(cross3(fwd, v_sup));
    let up0 = norm3(cross3(right0, fwd));
    let (sr, cr) = rotation_deg.to_radians().sin_cos();
    let r_rot: [f32; 3] = std::array::from_fn(|k| right0[k] * cr + up0[k] * sr);
    let u_rot: [f32; 3] = std::array::from_fn(|k| -right0[k] * sr + up0[k] * cr);
    let right = if x_flipped { r_rot.map(|v| -v) } else { r_rot };
    let up = if y_flipped { u_rot.map(|v| -v) } else { u_rot };
    VrCamera { right, up, fwd, half_ext }
}

impl VrCamera {
    /// Where box-space point `p` lands in the view, 0..1 across and down (the
    /// ray-caster's mapping inverted: `ro = right·sx·0.65 + up·sy·0.65`).
    pub fn project(&self, p: [f32; 3], zoom: f32, aspect: f32) -> [f32; 2] {
        let sx = dot3(p, self.right) / 0.65;
        let sy = dot3(p, self.up) / 0.65;
        let zoom = zoom.max(0.1);
        [(sx * zoom / aspect.max(1e-3) + 1.0) * 0.5, (1.0 - sy * zoom) * 0.5]
    }
}

/// Which side of the drawn loop is removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutSide {
    Inside,
    Outside,
}

static NEXT_VERSION: AtomicU64 = AtomicU64::new(1);

/// Keep (255) or cut (0) for each cell of a grid laid over the volume, at most 128
/// cells per side. `version` changes with every edit so the GPU copy is refreshed.
#[derive(Debug, Clone)]
pub struct SculptMask {
    pub dims: [usize; 3],
    pub data: Vec<u8>,
    pub version: u64,
    /// The volume it was made for (`Volume3D::gpu_id`).
    pub volume_id: u64,
}

impl SculptMask {
    pub fn new(vol: &Volume3D) -> Self {
        let dims = [vol.width.clamp(1, 128), vol.height.clamp(1, 128), vol.depth.clamp(1, 128)];
        Self {
            dims,
            data: vec![255; dims[0] * dims[1] * dims[2]],
            version: NEXT_VERSION.fetch_add(1, Ordering::Relaxed),
            volume_id: vol.gpu_id,
        }
    }

    /// Cells removed so far.
    pub fn cut_count(&self) -> usize {
        self.data.iter().filter(|&&v| v == 0).count()
    }

    /// Cuts every cell whose centre falls inside (or outside) `lasso`, a closed loop
    /// in view coordinates (0..1 across and down), as seen through `cam` at `zoom`
    /// in a view `aspect` (width / height) wide. Returns how many cells were cut.
    pub fn cut(&mut self, cam: &VrCamera, zoom: f32, aspect: f32, lasso: &[[f32; 2]], side: CutSide) -> usize {
        if lasso.len() < 3 {
            return 0;
        }
        // Rasterise the loop once (even-odd rule) so each cell is one lookup.
        const GRID: usize = 512;
        let mut inside = vec![false; GRID * GRID];
        for gy in 0..GRID {
            let y = (gy as f32 + 0.5) / GRID as f32;
            let mut xs: Vec<f32> = Vec::new();
            for i in 0..lasso.len() {
                let a = lasso[i];
                let b = lasso[(i + 1) % lasso.len()];
                if (a[1] <= y) != (b[1] <= y) {
                    xs.push(a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]));
                }
            }
            xs.sort_by(|p, q| p.total_cmp(q));
            for pair in xs.chunks(2) {
                if let [x0, x1] = pair {
                    let g0 = ((x0 * GRID as f32).ceil().max(0.0) as usize).min(GRID);
                    let g1 = ((x1 * GRID as f32).floor().max(0.0) as usize).min(GRID);
                    for gx in g0..g1 {
                        inside[gy * GRID + gx] = true;
                    }
                }
            }
        }
        let [mw, mh, md] = self.dims;
        let ext = cam.half_ext;
        let mut cut = 0usize;
        for z in 0..md {
            for y in 0..mh {
                for x in 0..mw {
                    let i = (z * mh + y) * mw + x;
                    if self.data[i] == 0 {
                        continue;
                    }
                    let uvw = [(x as f32 + 0.5) / mw as f32, (y as f32 + 0.5) / mh as f32, (z as f32 + 0.5) / md as f32];
                    let p: [f32; 3] = std::array::from_fn(|k| (uvw[k] * 2.0 - 1.0) * ext[k]);
                    let [vx, vy] = cam.project(p, zoom, aspect);
                    let hit = (0.0..1.0).contains(&vx)
                        && (0.0..1.0).contains(&vy)
                        && inside[(vy * GRID as f32) as usize * GRID + (vx * GRID as f32) as usize];
                    let remove = match side {
                        CutSide::Inside => hit,
                        CutSide::Outside => !hit,
                    };
                    if remove {
                        self.data[i] = 0;
                        cut += 1;
                    }
                }
            }
        }
        if cut > 0 {
            self.version = NEXT_VERSION.fetch_add(1, Ordering::Relaxed);
        }
        cut
    }
}

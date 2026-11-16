use super::*;

impl ViewerApp {
    pub(crate) fn disable_viewport_mpr(&mut self, vp_idx: usize) {
        if let (Some(vol), Some(plane)) = (
            self.viewport_volume(vp_idx).cloned(),
            self.viewports[vp_idx].mpr_plane.clone(),
        )
            && vol.depth > 0 {
                let vox = vol.lps_to_voxel(plane.slice_center_lps());
                let mapped = (vox[2].round() as isize).clamp(0, vol.depth as isize - 1) as usize;
                self.viewports[vp_idx].slice = mapped;
            }
        self.viewports[vp_idx].mpr_enabled = false;
        self.viewports[vp_idx].mpr_plane = None;
        self.viewports[vp_idx].mpr_cached = None;
    }

    pub(crate) fn link_group_color(group: u8) -> Color32 {
        match group {
            1 => Color32::from_rgb(56, 189, 248),  // Blue / Cyan
            2 => Color32::from_rgb(251, 191, 36),  // Gold / Amber
            3 => Color32::from_rgb(52, 211, 153),  // Emerald
            4 => Color32::from_rgb(244, 114, 182), // Pink / Magenta
            _ => Color32::from_gray(140),
        }
    }

    pub(crate) fn any_link_enabled(&self) -> bool {
        self.link_zoom || self.link_wl || self.link_cut
    }

    pub(crate) fn ensure_default_link_pair(&mut self) {
        if self.tiling.cell_count() == 2
            && self.any_link_enabled()
            && self.viewports.iter().take(2).all(|v| v.link_group == 0)
        {
            self.viewports[0].link_group = 1;
            self.viewports[1].link_group = 1;
        }
    }

    pub(crate) fn are_viewports_linked(&self, a: usize, b: usize) -> bool {
        if a == b {
            return false;
        }
        if self.ortho_mpr_active {
            return a < 3 && b < 3;
        }
        if self.tiling.cell_count() <= 1 {
            return false;
        }
        let ga = self.viewports.get(a).map_or(0, |v| v.link_group);
        let gb = self.viewports.get(b).map_or(0, |v| v.link_group);
        if ga != 0 && ga == gb {
            true
        } else if self.tiling.cell_count() == 2 && ga == 0 && gb == 0 {
            self.any_link_enabled()
        } else {
            false
        }
    }

    fn viewport_fov_mm(&self, vp_idx: usize) -> Option<f32> {
        if self.viewports.get(vp_idx)?.mpr_enabled
            && let Some(vol) = self.viewport_volume(vp_idx)
        {
            let fx = vol.width as f32 * vol.spacing[0] as f32;
            let fy = vol.height as f32 * vol.spacing[1] as f32;
            return Some(fx.max(fy).max(1.0));
        }
        let f = self.viewport_frame(vp_idx)?;
        let (rs, cs) = f.spacing?;
        let w = f.width as f32 * cs as f32;
        let h = f.height as f32 * rs as f32;
        Some(w.max(h).max(1.0))
    }

    fn viewport_min_side(&self, vp_idx: usize) -> f32 {
        self.viewport_rects
            .get(vp_idx)
            .filter(|r| r.is_positive())
            .map_or(1.0, |r| r.width().min(r.height()).max(1.0))
    }

    pub(crate) fn apply_same_size_zoom_from(&mut self, leader_idx: usize) {
        let Some(leader_fov) = self.viewport_fov_mm(leader_idx) else {
            return;
        };
        let leader_zoom = self.viewports[leader_idx].zoom;
        let leader_side = self.viewport_min_side(leader_idx);
        let count = self.tiling.cell_count();
        for i in 0..count {
            if self.are_viewports_linked(leader_idx, i)
                && self.viewports[i].selected.is_some()
                && let Some(other_fov) = self.viewport_fov_mm(i)
            {
                let other_side = self.viewport_min_side(i);
                let z = leader_zoom * (other_fov / leader_fov.max(1e-3)) * (leader_side / other_side);
                self.viewports[i].zoom = z.clamp(0.05, 50.0);
            }
        }
    }

    pub(super) fn sync_zoom_from(&mut self, leader_idx: usize, factor: f32) {
        if !self.ortho_mpr_active && !self.link_zoom {
            return;
        }
        if self.link_zoom_same_size && !self.ortho_mpr_active && self.viewport_fov_mm(leader_idx).is_some() {
            self.apply_same_size_zoom_from(leader_idx);
            return;
        }
        let count = self.tiling.cell_count();
        for i in 0..count {
            if self.are_viewports_linked(leader_idx, i) && self.viewports[i].selected.is_some() {
                let vp = &mut self.viewports[i];
                vp.zoom = (vp.zoom * factor).clamp(0.05, 50.0);
            }
        }
    }

    pub(super) fn sync_pan_from(&mut self, leader_idx: usize, delta: Vec2) {
        if !self.ortho_mpr_active && !self.link_zoom {
            return;
        }
        let count = self.tiling.cell_count();
        for i in 0..count {
            if self.are_viewports_linked(leader_idx, i) && self.viewports[i].selected.is_some() {
                self.viewports[i].pan += delta;
            }
        }
    }

    pub(super) fn sync_window_delta(&mut self, leader_idx: usize, d_level: f32, d_width: f32) {
        if !self.ortho_mpr_active && !self.link_wl {
            return;
        }
        let count = self.tiling.cell_count();
        for i in 0..count {
            if !self.are_viewports_linked(leader_idx, i) || self.viewports[i].selected.is_none() {
                continue;
            }
            let base = if let Some(w) = self.viewports[i].window {
                Some(w)
            } else {
                self.viewport_frame(i).map(|f| self.effective_window(i, f))
            };
            if let Some(w) = base {
                self.viewports[i].window = Some(Window {
                    level: w.level + d_level,
                    width: (w.width + d_width).max(1.0),
                });
            }
        }
    }

    pub(crate) fn viewport_volume(&self, vp_idx: usize) -> Option<&Arc<Volume3D>> {
        let sel = self.viewports.get(vp_idx)?.selected?;
        self.volumes.get(&sel)
    }

    pub(crate) fn viewport_sibling_volumes(&self, vp_idx: usize) -> Vec<Arc<Volume3D>> {
        let Some((st_idx, se_idx)) = self.viewports.get(vp_idx).and_then(|v| v.selected) else {
            return Vec::new();
        };
        let Some(study) = self.study_at(st_idx) else {
            return Vec::new();
        };
        let Some(gid) = study.series.get(se_idx).and_then(|s| s.mpr_group_id) else {
            return Vec::new();
        };
        let Some(group) = study.mpr_groups.get(gid) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for &other_se in &group.series_indices {
            if other_se != se_idx
                && let Some(v) = self.volumes.get(&(st_idx, other_se)) {
                    out.push(v.clone());
                }
        }
        out
    }

    pub(super) fn viewport_slice_key(&self, vp_idx: usize, slice: usize) -> Option<SliceKey> {
        let (study, series) = self.viewports.get(vp_idx)?.selected?;
        Some(SliceKey {
            scan_gen: self.scan_gen,
            study,
            series,
            slice,
        })
    }

    pub(crate) fn viewport_frame(&self, vp_idx: usize) -> Option<&Arc<Frame>> {
        let vp = self.viewports.get(vp_idx)?;
        if vp.mpr_enabled
            && let Some((_, ref f, _)) = vp.mpr_cached {
                return Some(f);
            }
        let slice = vp.slice;
        let key = self.viewport_slice_key(vp_idx, slice)?;
        self.cache.get(&key)
    }

    pub(super) fn ensure_mpr_frame(&mut self, vp_idx: usize) -> Option<(Arc<Frame>, SliceKey)> {
        if !self.viewports.get(vp_idx)?.mpr_enabled {
            return None;
        }
        let vol = self.viewport_volume(vp_idx)?.clone();
        if self.viewports[vp_idx].mpr_plane.is_none() {
            let mut p = MprPlane3D::from_volume_native(&vol);
            let cur_s = self.viewports[vp_idx].slice.min(vol.depth.saturating_sub(1));
            let z_lps = vol.voxel_to_lps([
                vol.width as f64 * 0.5,
                vol.height as f64 * 0.5,
                cur_s as f64,
            ]);
            p.offset_mm = dot3(sub3(z_lps, p.center_lps), p.normal);
            self.viewports[vp_idx].mpr_plane = Some(p);
        }
        let plane = self.viewports[vp_idx].mpr_plane.clone()?;
        if let Some((ref cached_p, ref cached_f, cached_k)) = self.viewports[vp_idx].mpr_cached
            && *cached_p == plane {
                return Some((cached_f.clone(), cached_k));
            }
        let siblings = if self.blend_siblings {
            self.viewport_sibling_volumes(vp_idx)
        } else {
            Vec::new()
        };
        let frame = vol.reslice_plane(&siblings, &plane);
        let vp = &mut self.viewports[vp_idx];
        vp.mpr_seq = vp.mpr_seq.wrapping_add(1);
        let key = SliceKey {
            scan_gen: self.scan_gen | (1u64 << 62),
            study: vp_idx,
            series: 0,
            slice: vp.mpr_seq,
        };
        vp.mpr_cached = Some((plane, frame.clone(), key));
        Some((frame, key))
    }

    /// Returns `(volume, plane_geom, metadata_frame)` when `vp_idx` can render MPR directly
    /// from the GPU 3D texture without CPU reslicing.
    pub(super) fn mpr_gpu_target(
        &mut self,
        vp_idx: usize,
    ) -> Option<(Arc<Volume3D>, crate::volume::MprPlaneGeometry, Arc<Frame>)> {
        let gpu = self.gpu?;
        let vp = &self.viewports[vp_idx];
        if !vp.mpr_enabled || vp.convolution_idx.is_some() {
            return None;
        }
        let vol = self.viewport_volume(vp_idx)?.clone();
        if vol.width as u32 > gpu.max_texture_3d
            || vol.height as u32 > gpu.max_texture_3d
            || vol.depth as u32 > gpu.max_texture_3d
        {
            return None;
        }
        if self.blend_siblings && self.viewport_sibling_volumes(vp_idx).len() > 1 {
            return None;
        }
        if self.viewports[vp_idx].mpr_plane.is_none() {
            let mut p = MprPlane3D::from_volume_native(&vol);
            let cur_s = self.viewports[vp_idx].slice.min(vol.depth.saturating_sub(1));
            let z_lps = vol.voxel_to_lps([
                vol.width as f64 * 0.5,
                vol.height as f64 * 0.5,
                cur_s as f64,
            ]);
            p.offset_mm = dot3(sub3(z_lps, p.center_lps), p.normal);
            self.viewports[vp_idx].mpr_plane = Some(p);
        }
        let plane = self.viewports[vp_idx].mpr_plane.as_ref()?;
        let geom = vol.mpr_plane_geometry(plane);
        if geom.width > gpu.max_texture || geom.height > gpu.max_texture {
            return None;
        }
        let meta_frame = Arc::new(vol.frame_with_pixels(&geom, Vec::new()));
        Some((vol, geom, meta_frame))
    }

    /// Which standard plane the viewport is showing right now, for lighting up the
    /// Ax / Cor / Sag buttons. The series' own acquisition plane counts as its plane
    /// even when the scanner tilted it (gantry tilt); any other plane must be within
    /// about 3° of a patient axis, otherwise it is oblique and no button lights up.
    pub(crate) fn viewport_display_plane(&self, vp_idx: usize) -> crate::dicom_index::DominantPlane {
        use crate::dicom_index::DominantPlane;
        let Some(vp) = self.viewports.get(vp_idx) else {
            return DominantPlane::Unknown;
        };
        let native = self.viewport_series(vp_idx).map(|s| s.dominant_plane);
        let Some(plane) = self.effective_mpr_plane(vp_idx).filter(|_| vp.mpr_enabled) else {
            return native.unwrap_or(DominantPlane::Unknown);
        };
        let n = plane.normal;
        if let Some(vol) = self.viewport_volume(vp_idx) {
            let nat = MprPlane3D::from_volume_native(vol).normal;
            let dot = (n[0] * nat[0] + n[1] * nat[1] + n[2] * nat[2]).abs();
            if dot > 0.9998 {
                return native.unwrap_or(DominantPlane::Unknown);
            }
        }
        let (ax, ay, az) = (n[0].abs(), n[1].abs(), n[2].abs());
        const ON_AXIS: f64 = 0.9986; // cos 3°
        if az > ON_AXIS {
            DominantPlane::Axial
        } else if ay > ON_AXIS {
            DominantPlane::Coronal
        } else if ax > ON_AXIS {
            DominantPlane::Sagittal
        } else {
            DominantPlane::Oblique
        }
    }

    pub(crate) fn effective_mpr_plane(&self, vp_idx: usize) -> Option<MprPlane3D> {
        let vp = self.viewports.get(vp_idx)?;
        if vp.mpr_enabled
            && let Some(ref p) = vp.mpr_plane {
                return Some(p.clone());
            }
        let vol = self.viewport_volume(vp_idx)?;
        let mut p = MprPlane3D::from_volume_native(vol);
        let cur_s = vp.slice.min(vol.depth.saturating_sub(1));
        let z_lps = vol.voxel_to_lps([
            vol.width as f64 * 0.5,
            vol.height as f64 * 0.5,
            cur_s as f64,
        ]);
        p.offset_mm = dot3(sub3(z_lps, p.center_lps), p.normal);
        Some(p)
    }

    pub(super) fn sync_slice_index_from_mpr_plane(&mut self, vp_idx: usize) {
        let Some(vol) = self.viewport_volume(vp_idx).cloned() else {
            return;
        };
        let Some(plane) = self.viewports[vp_idx].mpr_plane.clone() else {
            return;
        };
        let count = self.viewport_slice_count(vp_idx);
        if count <= 1 {
            return;
        }
        let half = vol.projected_half_span_mm(plane.normal).max(1.0);
        let t = ((plane.offset_mm / half) * 0.5 + 0.5).clamp(0.0, 1.0);
        self.viewports[vp_idx].slice = (t * (count - 1) as f64).round() as usize;
    }

    pub(crate) fn set_viewport_mpr_plane(
        &mut self,
        vp_idx: usize,
        plane: MprPlane3D,
        synchronize_others: bool,
    ) {
        self.viewports[vp_idx].mpr_enabled = true;
        self.viewports[vp_idx].mpr_plane = Some(plane);
        self.sync_slice_index_from_mpr_plane(vp_idx);
        if synchronize_others && self.sync_mode != SliceSynchronization::Off {
            self.synchronize_mpr_followers(vp_idx);
        }
    }

    pub(super) fn synchronize_mpr_followers(&mut self, leader_idx: usize) -> bool {
        if !self.ortho_mpr_active && (!self.link_cut || self.sync_mode == SliceSynchronization::Off) {
            return false;
        }
        let cell_count = self.tiling.cell_count();
        if cell_count <= 1 {
            return false;
        }
        let Some(leader_sel) = self.viewports[leader_idx].selected else {
            return false;
        };
        let Some(leader_plane) = self.effective_mpr_plane(leader_idx) else {
            return false;
        };

        let mut synced_any = false;
        let mut updates: Vec<(usize, MprPlane3D)> = Vec::new();
        let mut slice_2d_updates: Vec<(usize, usize)> = Vec::new();
        for other_idx in 0..cell_count {
            if !self.are_viewports_linked(leader_idx, other_idx) {
                continue;
            }
            let Some(other_sel) = self.viewports[other_idx].selected else {
                continue;
            };
            let other_mpr = self.viewports[other_idx].mpr_enabled;
            if other_sel == leader_sel {
                if other_mpr && let Some(mut other_plane) = self.effective_mpr_plane(other_idx) {
                    if dot3(leader_plane.normal, other_plane.normal).abs() > 0.99 {
                        other_plane.offset_mm = leader_plane.offset_mm;
                        updates.push((other_idx, other_plane));
                    } else {
                        let lps = leader_plane.slice_center_lps();
                        other_plane.offset_mm =
                            dot3(sub3(lps, other_plane.center_lps), other_plane.normal);
                        updates.push((other_idx, other_plane));
                    }
                }
                continue;
            }
            if let Some(reg) = self.registrations.get(&(leader_sel, other_sel)) {
                let leader_lps = leader_plane.slice_center_lps();
                let target_lps = if reg.is_reliable() {
                    reg.transform_point(leader_lps)
                } else {
                    leader_lps
                };
                if other_mpr && let Some(mut other_plane) = self.effective_mpr_plane(other_idx) {
                    // Never force-change the follower's plane orientation (Ax/Cor/Sag);
                    // only project the cut position onto the follower's own normal.
                    other_plane.offset_mm =
                        dot3(sub3(target_lps, other_plane.center_lps), other_plane.normal);
                    updates.push((other_idx, other_plane));
                } else if let Some(vol) = self.volumes.get(&other_sel)
                    && vol.depth > 0
                {
                    let vox = vol.lps_to_voxel(target_lps);
                    let mapped = (vox[2].round() as isize).clamp(0, vol.depth as isize - 1) as usize;
                    slice_2d_updates.push((other_idx, mapped));
                }
            }
        }

        for (other_idx, target_plane) in updates {
            self.viewports[other_idx].mpr_plane = Some(target_plane);
            self.sync_slice_index_from_mpr_plane(other_idx);
            synced_any = true;
        }
        for (other_idx, mapped_slice) in slice_2d_updates {
            self.viewports[other_idx].slice = mapped_slice;
            synced_any = true;
        }
        synced_any
    }

    /// Manually locks the current 3D plane & slice offset of all visible follower viewports
    /// to the active leader viewport (`⚓ Lock 3D Sync`).
    pub(crate) fn lock_current_3d_alignment(&mut self) {
        self.ensure_default_link_pair();
        self.link_cut = true;
        let leader_idx = self.active_viewport;
        let Some(leader_sel) = self.viewports[leader_idx].selected else {
            return;
        };
        let Some(leader_plane) = self.effective_mpr_plane(leader_idx) else {
            return;
        };
        let leader_slice_pt = leader_plane.slice_center_lps();
        let cell_count = self.tiling.cell_count();

        let mut new_regs = Vec::new();
        for other_idx in 0..cell_count {
            if !self.are_viewports_linked(leader_idx, other_idx) {
                continue;
            }
            let Some(other_sel) = self.viewports[other_idx].selected else {
                continue;
            };
            if other_sel == leader_sel {
                continue;
            }
            let Some(other_plane) = self.effective_mpr_plane(other_idx) else {
                continue;
            };
            let other_slice_pt = other_plane.slice_center_lps();
            // Build the orthogonal rotation mapping `leader_plane` axes `(u, v, n)` to `other_plane` axes `(u, v, n)`.
            let u_a = leader_plane.u_axis;
            let v_a = leader_plane.v_axis;
            let n_a = leader_plane.normal;
            let u_b = other_plane.u_axis;
            let v_b = other_plane.v_axis;
            let n_b = other_plane.normal;
            // R = [u_b | v_b | n_b] * [u_a | v_a | n_a]^T
            let mut rot = [[0.0f64; 3]; 3];
            for r in 0..3 {
                for c in 0..3 {
                    rot[r][c] = u_b[r] * u_a[c] + v_b[r] * v_a[c] + n_b[r] * n_a[c];
                }
            }
            let reg_fwd = RigidTransform3D {
                pivot_source_lps: leader_slice_pt,
                pivot_target_lps: other_slice_pt,
                rotation: rot,
                correlation: 1.0,
            };
            let rot_inv = [
                [rot[0][0], rot[1][0], rot[2][0]],
                [rot[0][1], rot[1][1], rot[2][1]],
                [rot[0][2], rot[1][2], rot[2][2]],
            ];
            let reg_rev = RigidTransform3D {
                pivot_source_lps: other_slice_pt,
                pivot_target_lps: leader_slice_pt,
                rotation: rot_inv,
                correlation: 1.0,
            };
            new_regs.push(((leader_sel, other_sel), reg_fwd));
            new_regs.push(((other_sel, leader_sel), reg_rev));
        }
        for (pair, reg) in new_regs {
            self.registrations.insert(pair, reg);
        }
        self.message = Some("Locked 3D slice & angle alignment across viewports.".into());
    }

    /// Optimal side-by-side comparison mode: links Zoom (`= mm` same physical size), Cut,
    /// and optionally WL/WW across the selected viewports, enables 3D MPR where volumes exist,
    /// and aligns XYZ planes (orientation + slice height) across the compared viewports.
    pub(crate) fn compare_align_linked_viewports(&mut self) {
        let cell_count = self.tiling.cell_count();
        let chosen: Vec<usize> = (0..cell_count)
            .filter(|&i| {
                self.viewports[i].selected.is_some()
                    && self.compare_vp_selected.get(i).copied().unwrap_or(true)
            })
            .collect();
        if chosen.len() < 2 {
            return;
        }
        for (i, vp) in self.viewports.iter_mut().enumerate().take(cell_count) {
            vp.link_group = if chosen.contains(&i) { 1 } else { 0 };
        }
        if !chosen.contains(&self.active_viewport) {
            self.active_viewport = chosen[0];
        }

        self.link_zoom = true;
        self.link_zoom_same_size = true;
        self.link_wl = self.compare_link_wl;
        self.link_cut = true;

        let leader_idx = self.active_viewport;
        let Some(leader_sel) = self.viewports[leader_idx].selected else {
            return;
        };
        if self.viewport_volume(leader_idx).is_some() && !self.viewports[leader_idx].mpr_enabled {
            self.viewports[leader_idx].mpr_enabled = true;
        }
        let leader_win = if self.compare_link_wl {
            self.viewports[leader_idx]
                .window
                .or_else(|| self.viewport_frame(leader_idx).map(|f| self.effective_window(leader_idx, f)))
        } else {
            None
        };
        let leader_pan = self.viewports[leader_idx].pan;
        let leader_plane = self.effective_mpr_plane(leader_idx);

        let mat_vec = |m: [[f64; 3]; 3], v: [f64; 3]| -> [f64; 3] {
            [
                m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
                m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
                m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
            ]
        };

        for &other_idx in &chosen {
            if other_idx == leader_idx {
                continue;
            }
            let Some(other_sel) = self.viewports[other_idx].selected else {
                continue;
            };
            if let Some(w) = leader_win {
                self.viewports[other_idx].window = Some(w);
            }
            self.viewports[other_idx].pan = leader_pan;

            if self.viewport_volume(other_idx).is_some() {
                self.viewports[other_idx].mpr_enabled = true;
            }
            if let Some(lp) = leader_plane.as_ref()
                && let Some(mut op) = self.effective_mpr_plane(other_idx)
            {
                let leader_lps = lp.slice_center_lps();
                if let Some(reg) = self.registrations.get(&(leader_sel, other_sel)).cloned() {
                    let target_lps = reg.transform_point(leader_lps);
                    op.u_axis = mat_vec(reg.rotation, lp.u_axis);
                    op.v_axis = mat_vec(reg.rotation, lp.v_axis);
                    op.normal = mat_vec(reg.rotation, lp.normal);
                    op.offset_mm = dot3(sub3(target_lps, op.center_lps), op.normal);
                } else {
                    op.u_axis = lp.u_axis;
                    op.v_axis = lp.v_axis;
                    op.normal = lp.normal;
                    op.offset_mm = dot3(sub3(leader_lps, op.center_lps), op.normal);
                }
                self.viewports[other_idx].mpr_plane = Some(op);
                self.viewports[other_idx].mpr_cached = None;
                self.sync_slice_index_from_mpr_plane(other_idx);
            }
        }

        self.apply_same_size_zoom_from(leader_idx);
        self.message = Some(match self.lang {
            crate::i18n::Lang::Es => {
                "Comparación activa: Zoom (= mm), Corte y planos XYZ sincronizados".into()
            }
            crate::i18n::Lang::En => {
                "Compare active: Zoom (= mm), Cut & XYZ planes synchronized".into()
            }
        });
    }

    pub(super) fn assign_series_to_viewport(&mut self, vp_idx: usize, sel: Option<(usize, usize)>) {
        if vp_idx >= self.viewports.len() {
            return;
        }
        if self.viewports[vp_idx].selected == sel {
            return;
        }
        self.viewports[vp_idx].reset_series_state(sel);
        self.wl_drag = None;
        self.scroll_drag = None;
        self.rot_drag = None;
        self.drawing.clear();
        self.angle_pending = None;

        if let Some((study, series)) = sel {
            // Queue the middle slice urgently so the series default window is ready immediately.
            if let Some(se) = self.series_at((study, series))
                && let Some(idx) = defaults::reference_index(se.slices.len()) {
                    let s = &se.slices[idx];
                    self.loader.request(Job {
                        key: SliceKey {
                            scan_gen: self.scan_gen,
                            study,
                            series,
                            slice: idx,
                        },
                        path: s.path.clone(),
                        frame: s.frame,
                        urgent: true,
                        distance: 0,
                    });
                }
        }
    }

    /// Sets the multi-series comparison tiling layout and auto-populates newly exposed
    /// viewports with consecutive series from the active study.
    /// Updates the shared 3D Orthogonal MPR crosshair position (LPS mm) and reslices
    /// all visible Axial / Coronal / Sagittal panes to pass through `pt_lps`.
    pub(super) fn set_ortho_crosshair_lps(&mut self, pt_lps: [f64; 3]) {
        self.ortho_crosshair_lps = Some(pt_lps);
        let count = self.tiling.cell_count().min(3);
        for vp_idx in 0..count {
            if !self.viewports[vp_idx].mpr_enabled {
                continue;
            }
            if let Some(mut plane) = self.viewports[vp_idx].mpr_plane.clone() {
                plane.offset_mm = dot3(sub3(pt_lps, plane.center_lps), plane.normal);
                self.viewports[vp_idx].mpr_plane = Some(plane);
                self.sync_slice_index_from_mpr_plane(vp_idx);
            }
        }
    }

    /// Returns `(study_idx, series_idx)` for a native `plane` sibling inside `sel`'s MPR group, if present.
    pub(crate) fn mpr_group_series_for_plane(
        &self,
        sel: (usize, usize),
        plane: crate::dicom_index::DominantPlane,
    ) -> Option<(usize, usize)> {
        let (st_idx, se_idx) = sel;
        let study = self.study_at(st_idx)?;
        let gid = study.series.get(se_idx)?.mpr_group_id?;
        let group = study.mpr_groups.get(gid)?;
        let target_se = match plane {
            crate::dicom_index::DominantPlane::Axial => group.axial_series,
            crate::dicom_index::DominantPlane::Coronal => group.coronal_series,
            crate::dicom_index::DominantPlane::Sagittal => group.sagittal_series,
            crate::dicom_index::DominantPlane::Oblique
            | crate::dicom_index::DominantPlane::Unknown => None,
        }?;
        Some((st_idx, target_se))
    }

    /// Switches `vp_idx` to `target_plane`: if the active series's MPR group already contains a
    /// native high-definition series for `target_plane`, switches to that native series at the
    /// matching anatomical position instead of reslicing a lower-resolution axis.
    pub(crate) fn switch_viewport_to_plane(
        &mut self,
        vp_idx: usize,
        target_plane: crate::dicom_index::DominantPlane,
    ) {
        let Some(cur_sel) = self.viewports.get(vp_idx).and_then(|v| v.selected) else {
            return;
        };
        let current_lps = self
            .effective_mpr_plane(vp_idx)
            .map(|p| add3(p.center_lps, scale3(p.normal, p.offset_mm)));

        if let Some(sib_sel) = self.mpr_group_series_for_plane(cur_sel, target_plane)
            && sib_sel != cur_sel {
                let win = self.viewports[vp_idx].window;
                let clut = self.viewports[vp_idx].clut_name.clone();
                let inv = self.viewports[vp_idx].invert;
                let conv = self.viewports[vp_idx].convolution_idx;
                let mpr_enabled = self.viewports[vp_idx].mpr_enabled;

                self.assign_series_to_viewport(vp_idx, Some(sib_sel));
                {
                    let vp = &mut self.viewports[vp_idx];
                    vp.window = win;
                    vp.clut_name = clut;
                    vp.invert = inv;
                    vp.convolution_idx = conv;
                    vp.conv_cached = None;
                    vp.mpr_enabled = mpr_enabled;
                }
                if let Some(sib_vol) = self.volumes.get(&sib_sel).cloned() {
                    let mut p = MprPlane3D::from_volume_native(&sib_vol);
                    if let Some(lps) = current_lps {
                        p.offset_mm = dot3(sub3(lps, p.center_lps), p.normal);
                    }
                    self.set_viewport_mpr_plane(vp_idx, p, true);
                } else if mpr_enabled {
                    self.pending_mpr_activate = Some((vp_idx, sib_sel, false));
                }
                return;
            }

        if let Some(vol) = self.viewport_volume(vp_idx).cloned() {
            let mut plane = match target_plane {
                crate::dicom_index::DominantPlane::Axial => MprPlane3D::axial(vol.center_lps),
                crate::dicom_index::DominantPlane::Coronal => MprPlane3D::coronal(vol.center_lps),
                crate::dicom_index::DominantPlane::Sagittal => MprPlane3D::sagittal(vol.center_lps),
                crate::dicom_index::DominantPlane::Oblique
                | crate::dicom_index::DominantPlane::Unknown => MprPlane3D::from_volume_native(&vol),
            };
            if let Some(lps) = current_lps {
                plane.offset_mm = dot3(sub3(lps, plane.center_lps), plane.normal);
            }
            self.set_viewport_mpr_plane(vp_idx, plane, true);
        }
    }

    /// Toggles the classic Horos 3-pane Orthogonal MPR layout (`Axial | Coronal | Sagittal`)
    /// for the active viewport's 3D volume.
    pub(crate) fn toggle_orthogonal_mpr(&mut self) {
        if self.ortho_mpr_active {
            self.ortho_mpr_active = false;
            self.ortho_crosshair_lps = None;
            self.crosshair_drag = None;
            let restore = self.ortho_saved_tiling.take().unwrap_or_default();
            self.set_tiling(restore);
            if let Some(vp) = self.viewports.get_mut(self.active_viewport) {
                vp.mpr_enabled = false;
                vp.mpr_plane = None;
                vp.mpr_cached = None;
            }
            return;
        }

        let src_idx = self.active_viewport;
        let Some(sel) = self.viewports[src_idx].selected else {
            return;
        };
        let Some(vol) = self.volumes.get(&sel).cloned() else {
            return;
        };

        self.ortho_saved_tiling = Some(self.tiling);
        let win = self.viewports[src_idx].window;
        let clut = self.viewports[src_idx].clut_name.clone();
        let inv = self.viewports[src_idx].invert;
        let conv = self.viewports[src_idx].convolution_idx;

        self.set_tiling(ViewportTiling::new(1, 3));
        self.ortho_mpr_active = true;
        let center = vol.center_lps;
        self.ortho_crosshair_lps = Some(center);

        let planes = [
            (
                crate::dicom_index::DominantPlane::Axial,
                MprPlane3D::axial(center),
            ),
            (
                crate::dicom_index::DominantPlane::Coronal,
                MprPlane3D::coronal(center),
            ),
            (
                crate::dicom_index::DominantPlane::Sagittal,
                MprPlane3D::sagittal(center),
            ),
        ];
        for (vp_idx, (anat, fallback_plane)) in planes.into_iter().enumerate() {
            let native_sel = self
                .mpr_group_series_for_plane(sel, anat)
                .filter(|&s| self.volumes.contains_key(&s));
            let pane_sel = native_sel.unwrap_or(sel);
            let pane_plane = if native_sel.is_some() {
                if let Some(native_vol) = self.volumes.get(&pane_sel) {
                    let mut p = MprPlane3D::from_volume_native(native_vol);
                    p.offset_mm = dot3(sub3(center, p.center_lps), p.normal);
                    p
                } else {
                    fallback_plane
                }
            } else {
                fallback_plane
            };
            self.assign_series_to_viewport(vp_idx, Some(pane_sel));
            let vp = &mut self.viewports[vp_idx];
            vp.window = win;
            vp.clut_name = clut.clone();
            vp.invert = inv;
            vp.convolution_idx = conv;
            vp.conv_cached = None;
            vp.mpr_enabled = true;
            vp.mpr_plane = Some(pane_plane);
            self.sync_slice_index_from_mpr_plane(vp_idx);
        }
        self.active_viewport = 0;
    }

    pub(crate) fn set_tiling(&mut self, tiling: ViewportTiling) {
        self.tiling = tiling;
        self.maximized_viewport = None;
        let count = tiling.cell_count();
        if self.active_viewport >= count {
            self.active_viewport = 0;
        }
        self.populate_unassigned_viewports();
    }

    pub(super) fn populate_unassigned_viewports(&mut self) {
        let Some(index) = &self.index else {
            return;
        };
        if index.studies.is_empty() {
            return;
        }
        let base_sel = self
            .viewports[self.active_viewport]
            .selected
            .or_else(|| (!index.studies[0].series.is_empty()).then_some((0, 0)));
        let Some((study_idx, start_series)) = base_sel else {
            return;
        };
        let series_len = index.studies[study_idx].series.len();
        if series_len == 0 {
            return;
        }

        let cell_count = self.tiling.cell_count();
        let mut to_assign = Vec::new();
        for (vp_idx, vp) in self.viewports.iter().enumerate().take(cell_count) {
            if vp.selected.is_none() {
                let candidate = start_series + vp_idx;
                if candidate < series_len {
                    to_assign.push((vp_idx, (study_idx, candidate)));
                }
            }
        }
        for (vp_idx, sel) in to_assign {
            self.assign_series_to_viewport(vp_idx, Some(sel));
        }
    }

    pub(crate) fn set_viewport_slice(&mut self, vp_idx: usize, idx: isize, synchronize_others: bool) {
        let n = self.viewport_slice_count(vp_idx);
        if n == 0 {
            return;
        }
        let prev_slice = self.viewports[vp_idx].slice;
        let new_slice = idx.clamp(0, n as isize - 1) as usize;
        if new_slice == prev_slice && !synchronize_others {
            return;
        }
        self.viewports[vp_idx].slice = new_slice;
        if self.viewports[vp_idx].mpr_enabled
            && let (Some(vol), Some(mut plane)) = (
                self.viewport_volume(vp_idx).cloned(),
                self.viewports[vp_idx].mpr_plane.clone(),
            ) {
                let half = vol.projected_half_span_mm(plane.normal);
                let t = if n > 1 {
                    new_slice as f64 / (n - 1) as f64
                } else {
                    0.5
                };
                let delta_off = (t * 2.0 - 1.0) * half - plane.offset_mm;
                plane.offset_mm = (t * 2.0 - 1.0) * half;
                let normal = plane.normal;
                self.viewports[vp_idx].mpr_plane = Some(plane);
                if self.ortho_mpr_active && vp_idx < 3
                    && let Some(cur_pt) = self.ortho_crosshair_lps {
                        self.ortho_crosshair_lps = Some(add3(cur_pt, scale3(normal, delta_off)));
                    }
            }
        if vp_idx == self.active_viewport {
            self.drawing.clear();
            self.angle_pending = None;
        }

        if synchronize_others
            && self.link_cut
            && self.sync_mode != SliceSynchronization::Off
            && !self.ortho_mpr_active
        {
            self.synchronize_mpr_followers(vp_idx);
            self.synchronize_from_leader(vp_idx, prev_slice, new_slice);
        }
    }

    pub(crate) fn step_viewport_slice(&mut self, vp_idx: usize, delta: isize) {
        let cur = self.viewports[vp_idx].slice as isize;
        self.set_viewport_slice(vp_idx, cur + delta, true);
    }

    pub(crate) fn all_series_pairs(&self) -> Vec<(usize, usize)> {
        let Some(index) = &self.index else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (si, st) in index.studies.iter().enumerate() {
            for ri in 0..st.series.len() {
                out.push((si, ri));
            }
        }
        out
    }

    pub(crate) fn step_active_series(&mut self, delta: isize) {
        let pairs = self.all_series_pairs();
        if pairs.is_empty() {
            return;
        }
        let vp = self.active_viewport;
        let cur_sel = self.viewports[vp].selected;
        let cur_pos = cur_sel
            .and_then(|sel| pairs.iter().position(|&p| p == sel))
            .unwrap_or(0) as isize;
        let next_pos = (cur_pos + delta).clamp(0, pairs.len() as isize - 1) as usize;
        self.assign_series_to_viewport(vp, Some(pairs[next_pos]));
    }

    /// Synchronizes all other visible viewports to the leader viewport `leader_idx`
    /// using IsiX's `SliceSynchronizer` rules.
    pub(super) fn synchronize_from_leader(
        &mut self,
        leader_idx: usize,
        leader_prev_slice: usize,
        leader_new_slice: usize,
    ) {
        let mode = self.sync_mode;
        if !self.link_cut || mode == SliceSynchronization::Off {
            return;
        }
        let cell_count = self.tiling.cell_count();
        if cell_count <= 1 {
            return;
        }

        // Gather leader geometry from the index so synchronization works even before
        // adjacent frames finish decoding.
        let leader_info: Option<LeaderSyncInfo> = self.viewport_series(leader_idx).and_then(|se| {
            let sel = self.viewports[leader_idx].selected?;
            let prev_ref = se.slices.get(leader_prev_slice)?;
            let new_ref = se.slices.get(leader_new_slice)?;
            let orient = new_ref.orientation.map(|v| v as f32);
            let prev_sync = Self::sync_slice_from_ref(prev_ref);
            let new_sync = Self::sync_slice_from_ref(new_ref);
            let center_3d = if new_ref.is_3d_plane {
                let sp = new_ref.spacing.unwrap_or((1.0, 1.0));
                let origin = new_ref.position.unwrap_or([0.0; 3]).map(|v| v as f32);
                Some(ImagePlaneCoordinates::slice_center_patient(
                    se.columns as isize,
                    se.rows as isize,
                    origin,
                    &orient,
                    sp.1,
                    sp.0,
                ))
            } else {
                None
            };
            Some((
                sel,
                orient,
                prev_sync,
                new_sync,
                center_3d,
                se.frame_of_reference_uid.clone(),
            ))
        });
        let Some((leader_sel, leader_orient, leader_prev_sync, leader_new_sync, leader_center_3d, leader_for)) =
            leader_info
        else {
            return;
        };

        // In IsiX, Relative mode passes `before` as `targetCurrentSlice` and `after` as `sourceSlice`.
        let mut updates: Vec<(usize, usize)> = Vec::new();
        for other_idx in 0..cell_count {
            if !self.are_viewports_linked(leader_idx, other_idx) {
                continue;
            }
            let Some(other_sel) = self.viewports[other_idx].selected else {
                continue;
            };
            if other_sel == leader_sel {
                // Same series open in two viewports (e.g. different window/CLUT): keep slices in lockstep.
                updates.push((other_idx, leader_new_slice));
                continue;
            }
            // If 3D registration is active on `other_idx`, `synchronize_mpr_followers` already updated its plane or 2D slice.
            if self.registrations.contains_key(&(leader_sel, other_sel))
                && (self.viewports[other_idx].mpr_enabled || self.volumes.contains_key(&other_sel))
            {
                continue;
            }
            let Some(other_se) = self.series_at(other_sel) else {
                continue;
            };
            if other_se.slices.is_empty() {
                continue;
            }
            let other_cur_idx = self.viewports[other_idx]
                .slice
                .min(other_se.slices.len() - 1);
            let other_cur_ref = &other_se.slices[other_cur_idx];
            let other_orient = other_cur_ref.orientation.map(|v| v as f32);
            let target_slices: Vec<SynchronizableSlice> = other_se
                .slices
                .iter()
                .map(Self::sync_slice_from_ref)
                .collect();
            let same_for = match (&leader_for, &other_se.frame_of_reference_uid) {
                (Some(a), Some(b)) => a == b,
                // If FrameOfReferenceUID was omitted by the scanner, treat series within the same study as sharing FOR.
                (None, None) => leader_sel.0 == other_sel.0,
                _ => false,
            };

            // When comparing two different studies before 3D volume registration finishes,
            // align by normalized physical stack progress rather than raw slice index.
            if !same_for && mode == SliceSynchronization::Absolute {
                let leader_len = self
                    .series_at(leader_sel)
                    .map_or(1, |s| s.slices.len())
                    .max(1);
                let frac = if leader_len > 1 {
                    leader_new_slice as f64 / (leader_len - 1) as f64
                } else {
                    0.5
                };
                let mapped =
                    (frac * (other_se.slices.len().saturating_sub(1)) as f64).round() as usize;
                updates.push((other_idx, mapped));
                continue;
            }

            let target_current_for_sync = if mode == SliceSynchronization::Relative {
                leader_prev_sync
            } else {
                target_slices[other_cur_idx]
            };

            let target = SliceSynchronizer::synchronize(
                mode,
                leader_new_sync,
                &leader_orient,
                leader_new_slice,
                false,
                leader_center_3d,
                target_current_for_sync,
                &other_orient,
                &target_slices,
                other_cur_idx,
                false,
                same_for,
            );

            match target {
                SynchronizationTarget::None => {}
                SynchronizationTarget::SliceIndex(idx) => {
                    updates.push((other_idx, idx));
                }
                SynchronizationTarget::PatientPoint(pt) => {
                    let planes: Vec<SlicePlaneParams> = other_se
                        .slices
                        .iter()
                        .map(|s| {
                            let sp = s.spacing.unwrap_or((1.0, 1.0));
                            SlicePlaneParams {
                                origin: s.position.unwrap_or([0.0; 3]).map(|v| v as f32),
                                orientation: s.orientation.map(|v| v as f32),
                                spacing_x: sp.1,
                                spacing_y: sp.0,
                            }
                        })
                        .collect();
                    if let Some((best_idx, _)) = ImagePlaneCoordinates::nearest_slice(&planes, pt) {
                        updates.push((other_idx, best_idx));
                    }
                }
            }
        }

        for (other_idx, new_idx) in updates {
            self.viewports[other_idx].slice = new_idx;
        }
    }

    pub(super) fn sync_slice_from_ref(s: &SliceRef) -> SynchronizableSlice {
        SynchronizableSlice {
            is_3d_plane: s.is_3d_plane,
            slice_location: s.slice_location.unwrap_or(0.0),
            origin_z: s.position.map_or(0.0, |p| p[2]),
        }
    }

}

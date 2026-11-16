//! 2D and 3D spatial convolution filters and preset catalogue, ported from:
//! - `Modules/IsiXImaging/Sources/BufferOperations/ImageConvolution.swift`
//! - `Modules/IsiXImaging/Sources/BufferOperations/SeriesConvolution.swift`
//! - `Modules/IsiXImaging/Sources/ViewerPipeline/ConvolutionKernelBook.swift`
//! - `Modules/IsiXPreferences/Sources/DefaultPreferences+ImagePresets.swift`

#![allow(dead_code)]

use rayon::prelude::*;
use std::ops::Range;

/// A convolution kernel — the sharpen, bone, edge, and smoothing filters of the
/// 2D viewer — applied to a slice with edge extension (`kvImageEdgeExtend`).
///
/// Mirrors `ImageConvolution` in `ImageConvolution.swift`.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageConvolution {
    /// Up to 5 × 5 weights, row by row.
    pub weights: [f32; 25],
    /// 3 or 5.
    pub size: usize,
    /// The sum the weights are divided by; zero leaves them as they are.
    pub normalization: f32,
}

impl ImageConvolution {
    pub fn new(weights_slice: &[f32], size: usize, normalization: f32) -> Self {
        let mut weights = [0.0f32; 25];
        let count = weights_slice.len().min(25);
        weights[..count].copy_from_slice(&weights_slice[..count]);
        Self {
            weights,
            size,
            normalization,
        }
    }

    /// Returns the normalized kernel weights (`size * size` active entries).
    pub fn normalized_kernel(&self) -> [f32; 25] {
        SeriesConvolution::kernel(&self.weights, self.normalization)
    }

    /// Convolves a 2D `f32` buffer of `width × height` with edge extension (`kvImageEdgeExtend`),
    /// repeating the first pixel across the first row (`result.update(repeating: first, count: width)`)
    /// exactly as `ImageConvolution.filtered(_:width:height:)` does in IsiX.
    pub fn filtered(&self, pixels: &[f32], width: usize, height: usize) -> Vec<f32> {
        let len = width.saturating_mul(height);
        if len == 0 || pixels.len() < len || (self.size != 3 && self.size != 5) {
            return pixels.to_vec();
        }
        let kernel = self.normalized_kernel();
        let mut result = convolve_planar_f32(pixels, width, height, width, &kernel, self.size);
        let first = pixels[0];
        for v in result.iter_mut().take(width) {
            *v = first;
        }
        result
    }

    /// Convolves an RGBA8 buffer (`[u8]`, 4 bytes per pixel) leaving alpha unchanged,
    /// using integer weights and divisor (`ImageConvolution.filtered(argb:width:height:)`).
    pub fn filtered_rgba(&self, pixels: &[u8], width: usize, height: usize) -> Vec<u8> {
        let len = width.saturating_mul(height).saturating_mul(4);
        if len == 0 || pixels.len() < len || (self.size != 3 && self.size != 5) {
            return pixels.to_vec();
        }
        let size = self.size;
        let half = (size / 2) as isize;
        let mut whole = [0i32; 25];
        for (dst, &weight) in whole.iter_mut().zip(&self.weights[..size * size]) {
            *dst = (weight as i32) as i16 as i32;
        }
        let divisor = (self.normalization as i32).max(0);
        let mut out = vec![0u8; len];
        let w_isize = width as isize;
        let h_isize = height as isize;

        out.par_chunks_exact_mut(width * 4)
            .enumerate()
            .for_each(|(y, row_out)| {
                let y_isize = y as isize;
                for x in 0..width {
                    let x_isize = x as isize;
                    let mut sum_r = 0i32;
                    let mut sum_g = 0i32;
                    let mut sum_b = 0i32;
                    for ky in 0..size {
                        let sy = (y_isize + ky as isize - half).clamp(0, h_isize - 1) as usize;
                        let row_off = sy * width * 4;
                        let k_row = ky * size;
                        for kx in 0..size {
                            let sx = (x_isize + kx as isize - half).clamp(0, w_isize - 1) as usize;
                            let w = whole[k_row + kx];
                            let idx = row_off + sx * 4;
                            sum_r += pixels[idx] as i32 * w;
                            sum_g += pixels[idx + 1] as i32 * w;
                            sum_b += pixels[idx + 2] as i32 * w;
                        }
                    }
                    let (r, g, b) = if divisor != 0 {
                        (sum_r / divisor, sum_g / divisor, sum_b / divisor)
                    } else {
                        (sum_r, sum_g, sum_b)
                    };
                    let out_idx = x * 4;
                    row_out[out_idx] = r.clamp(0, 255) as u8;
                    row_out[out_idx + 1] = g.clamp(0, 255) as u8;
                    row_out[out_idx + 2] = b.clamp(0, 255) as u8;
                    row_out[out_idx + 3] = pixels[(y * width + x) * 4 + 3];
                }
            });
        out
    }
}

/// 2D planar float convolution with edge extension (`kvImageEdgeExtend`),
/// where `src_row_stride` is the number of `f32` elements between consecutive rows.
fn convolve_planar_f32(
    src: &[f32],
    width: usize,
    height: usize,
    src_row_stride: usize,
    kernel: &[f32; 25],
    size: usize,
) -> Vec<f32> {
    let mut dst = vec![0.0f32; width * height];
    let half = (size / 2) as isize;
    let w_isize = width as isize;
    let h_isize = height as isize;

    for y in 0..height {
        let y_isize = y as isize;
        for x in 0..width {
            let x_isize = x as isize;
            let mut acc = 0.0f32;
            for ky in 0..size {
                let sy = (y_isize + ky as isize - half).clamp(0, h_isize - 1) as usize;
                let row_base = sy * src_row_stride;
                let k_row = ky * size;
                for kx in 0..size {
                    let sx = (x_isize + kx as isize - half).clamp(0, w_isize - 1) as usize;
                    acc += src[row_base + sx] * kernel[k_row + kx];
                }
            }
            dst[y * width + x] = acc;
        }
    }
    dst
}

/// A convolution filter applied across a whole series on multiple cores.
///
/// Mirrors `SeriesConvolution` in `SeriesConvolution.swift`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeriesConvolution {
    pub workers: usize,
}

impl Default for SeriesConvolution {
    fn default() -> Self {
        Self::new(
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1),
        )
    }
}

impl SeriesConvolution {
    pub fn new(workers: usize) -> Self {
        Self {
            workers: workers.max(1),
        }
    }

    /// The shares of `count` items: share `i` is `i·count/workers ..< (i+1)·count/workers`.
    pub fn shares(&self, count: usize) -> Vec<Range<usize>> {
        (0..self.workers)
            .map(|worker| {
                let start = (worker.wrapping_mul(count)) / self.workers;
                let end = ((worker.wrapping_add(1)).wrapping_mul(count)) / self.workers;
                start..end
            })
            .collect()
    }

    /// The weights of the pass across the slices: the 25 weights divided by their
    /// normalization, unless it is zero.
    pub fn kernel(weights: &[f32; 25], normalization: f32) -> [f32; 25] {
        let mut out = [0.0f32; 25];
        for i in 0..25 {
            out[i] = if normalization != 0.0 {
                weights[i] / normalization
            } else {
                weights[i]
            };
        }
        out
    }

    /// Filters `volume` (`depth` slices of `width × height` floats) across its slices:
    /// for each row of the slices, the `width × depth` plane of that row across all slices
    /// is convolved with the `size × size` `kernel` (edges extended) and written back.
    pub fn filter_across_slices(
        &self,
        volume: &mut [f32],
        width: usize,
        height: usize,
        depth: usize,
        kernel: &[f32; 25],
        size: usize,
    ) {
        let slice_len = width.saturating_mul(height);
        if slice_len == 0 || depth == 0 || volume.len() < slice_len.saturating_mul(depth) {
            return;
        }
        for row in 0..height {
            let row_slice = &volume[row * width..];
            let plane = convolve_planar_f32(row_slice, width, depth, slice_len, kernel, size);
            for z in 0..depth {
                let dst_start = z * slice_len + row * width;
                let src_start = z * width;
                volume[dst_start..dst_start + width]
                    .copy_from_slice(&plane[src_start..src_start + width]);
            }
        }
    }
}

/// Stored convolution filters and their 5 × 5 editor grid layout.
///
/// Mirrors `ConvolutionKernelBook` in `ConvolutionKernelBook.swift`.
pub struct ConvolutionKernelBook;

#[derive(Debug, Clone, PartialEq)]
pub struct ConvolutionKernel {
    pub name: &'static str,
    pub size: usize,
    pub normalization: f32,
    pub weights: [f32; 25],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KernelCell {
    pub row: usize,
    pub column: usize,
}

impl ConvolutionKernel {
    pub const fn new(
        name: &'static str,
        size: usize,
        normalization: f32,
        weights: [f32; 25],
    ) -> Self {
        Self {
            name,
            size,
            normalization,
            weights,
        }
    }

    pub fn to_convolution(&self) -> ImageConvolution {
        ImageConvolution {
            weights: self.weights,
            size: self.size,
            normalization: self.normalization,
        }
    }
}

impl ConvolutionKernelBook {
    pub const GRID_SIDE: usize = 5;

    /// Whether a filter of `size` uses the cell at `row`, `column`: a 3 × 3 filter
    /// uses only the nine in the middle (`1..=3`); `None` for unsupported sizes.
    pub fn uses(row: usize, column: usize, size: usize) -> Option<bool> {
        match size {
            3 => Some((1..=3).contains(&row) && (1..=3).contains(&column)),
            5 => Some(true),
            _ => None,
        }
    }

    /// The cells a filter of `size` uses, in the order the editor reads its weights:
    /// column by column, top to bottom.
    pub fn reading_order(size: usize) -> Vec<KernelCell> {
        let mut cells = Vec::new();
        for column in 0..Self::GRID_SIDE {
            for row in 0..Self::GRID_SIDE {
                if Self::uses(row, column, size) == Some(true) {
                    cells.push(KernelCell { row, column });
                }
            }
        }
        cells
    }

    /// The cells the editor writes a stored filter's weights into (row by row,
    /// the transpose of `reading_order`).
    pub fn writing_order(size: usize) -> Vec<KernelCell> {
        Self::reading_order(size)
            .into_iter()
            .map(|c| KernelCell {
                row: c.column,
                column: c.row,
            })
            .collect()
    }

    /// The sum of the cells in the editor, parsing each string as a float (or 0).
    pub fn sum(texts: &[&str]) -> f32 {
        texts
            .iter()
            .map(|s| s.trim().parse::<f32>().unwrap_or(0.0))
            .sum()
    }
}

const fn k3(name: &'static str, m: [f32; 9], norm: f32) -> ConvolutionKernel {
    let mut w = [0.0f32; 25];
    let mut i = 0;
    while i < 9 {
        w[i] = m[i];
        i += 1;
    }
    ConvolutionKernel::new(name, 3, norm, w)
}

const fn k5(name: &'static str, w: [f32; 25], norm: f32) -> ConvolutionKernel {
    ConvolutionKernel::new(name, 5, norm, w)
}

/// All 22 factory 2D convolution presets shipped in Horos (`DefaultPreferences+ImagePresets.swift`).
pub static FACTORY_CONVOLUTIONS: &[ConvolutionKernel] = &[
    k3(
        "Bone Filter 3x3",
        [-1.0, -1.0, -1.0, -1.0, 9.0, -1.0, -1.0, -1.0, -1.0],
        1.0,
    ),
    k3(
        "Sharpen 3x3",
        [-1.0, 0.0, -1.0, 0.0, 7.0, 0.0, -1.0, 0.0, -1.0],
        3.0,
    ),
    k5(
        "Sharpen 5x5",
        [
            -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 2.0, 2.0, 2.0, -1.0, -1.0, 2.0, 8.0, 2.0, -1.0,
            -1.0, 2.0, 2.0, 2.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0,
        ],
        8.0,
    ),
    k5(
        "Basic Smooth 5x5",
        [
            1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 4.0, 4.0, 4.0, 1.0, 1.0, 4.0, 12.0, 4.0, 1.0, 1.0, 4.0,
            4.0, 4.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        60.0,
    ),
    k5(
        "Gaussian blur",
        [
            1.0, 1.0, 2.0, 1.0, 1.0, 1.0, 2.0, 4.0, 2.0, 1.0, 2.0, 4.0, 8.0, 4.0, 2.0, 1.0, 2.0,
            4.0, 2.0, 1.0, 1.0, 1.0, 2.0, 1.0, 1.0,
        ],
        52.0,
    ),
    k3(
        "Blur 3x3",
        [1.0, 2.0, 1.0, 2.0, 4.0, 2.0, 1.0, 2.0, 1.0],
        16.0,
    ),
    k5(
        "Blur 5x5",
        [
            1.0, 1.0, 2.0, 1.0, 1.0, 1.0, 2.0, 3.0, 2.0, 1.0, 2.0, 3.0, 4.0, 3.0, 2.0, 1.0, 2.0,
            3.0, 2.0, 1.0, 1.0, 1.0, 2.0, 1.0, 1.0,
        ],
        44.0,
    ),
    k3(
        "Lowpass",
        [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        9.0,
    ),
    k5(
        "Highpass 5x5",
        [
            0.0, -1.0, -1.0, -1.0, 0.0, -1.0, 2.0, -4.0, 2.0, -1.0, -1.0, -4.0, 13.0, -4.0, -1.0,
            -1.0, 2.0, -4.0, 2.0, -1.0, 0.0, -1.0, -1.0, -1.0, 0.0,
        ],
        1.0,
    ),
    k3(
        "Edge 3x3",
        [1.0, -2.0, 1.0, -2.0, 4.0, -2.0, 1.0, -2.0, 1.0],
        0.0,
    ),
    k3(
        "Laplacian 4",
        [0.0, -1.0, 0.0, -1.0, 4.0, -1.0, 0.0, -1.0, 0.0],
        0.0,
    ),
    k3(
        "Laplacian 8",
        [-1.0, -1.0, -1.0, -1.0, 8.0, -1.0, -1.0, -1.0, -1.0],
        0.0,
    ),
    k5(
        "Laplacian",
        [
            -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 24.0, -1.0,
            -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0,
        ],
        0.0,
    ),
    k5(
        "Hat",
        [
            0.0, 0.0, -1.0, 0.0, 0.0, 0.0, -1.0, -2.0, -1.0, 0.0, -1.0, -2.0, 16.0, -2.0, -1.0,
            0.0, -1.0, -2.0, -1.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0,
        ],
        0.0,
    ),
    k3("Emboss", [-1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0], 0.0),
    k3(
        "Emboss heavy",
        [-1.0, -1.0, 0.0, -1.0, 0.0, 1.0, 0.0, 1.0, 1.0],
        0.0,
    ),
    k3(
        "Emboss north",
        [1.0, 2.0, 1.0, 0.0, 0.0, 0.0, -1.0, -2.0, -1.0],
        0.0,
    ),
    k3(
        "Emboss west",
        [1.0, 0.0, -1.0, 2.0, 0.0, -2.0, 1.0, 0.0, -1.0],
        0.0,
    ),
    k3(
        "Emboss diagonal",
        [0.0, 1.0, 0.0, -1.0, 0.0, 1.0, 0.0, -1.0, 0.0],
        0.0,
    ),
    k3(
        "Excessive edges",
        [1.0, 1.0, 1.0, 1.0, -7.0, 1.0, 1.0, 1.0, 1.0],
        1.0,
    ),
    k5(
        "Inverted blur",
        [
            3.0, 3.0, 2.0, 3.0, 3.0, 3.0, 2.0, 1.0, 2.0, 3.0, 2.0, 1.0, 0.0, 1.0, 2.0, 3.0, 2.0,
            1.0, 2.0, 3.0, 3.0, 3.0, 2.0, 3.0, 3.0,
        ],
        56.0,
    ),
    k5(
        "Negative blur",
        [
            0.0, 0.0, -1.0, 0.0, 0.0, 0.0, -1.0, -2.0, -1.0, 0.0, -1.0, -2.0, -3.0, -2.0, -1.0,
            0.0, -1.0, -2.0, -1.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0,
        ],
        -23.0,
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convolution_keeps_a_flat_image_and_repeats_the_first_pixel_on_the_first_row() {
        let mut weights = [0.0f32; 25];
        weights[0..9].copy_from_slice(&[0.0, -1.0, 0.0, -1.0, 5.0, -1.0, 0.0, -1.0, 0.0]);
        let flat = vec![3.0f32; 20];
        let conv = ImageConvolution::new(&weights, 3, 1.0);
        let result = conv.filtered(&flat, 5, 4);
        assert_eq!(result, flat);
    }

    #[test]
    fn shares_cover_every_item_once() {
        for workers in [1, 2, 3, 7, 16] {
            let conv = SeriesConvolution::new(workers);
            for count in [0, 1, 2, 5, 16, 100] {
                let shares = conv.shares(count);
                assert_eq!(shares.len(), workers);
                let flat: Vec<usize> = shares.iter().cloned().flatten().collect();
                let expected: Vec<usize> = (0..count).collect();
                assert_eq!(flat, expected);
            }
        }
    }

    #[test]
    fn kernel_divides_by_a_non_zero_normalization() {
        let mut weights = [0.0f32; 25];
        for (i, w) in weights.iter_mut().enumerate() {
            *w = i as f32;
        }
        let k4 = SeriesConvolution::kernel(&weights, 4.0);
        for i in 0..25 {
            assert_eq!(k4[i], weights[i] / 4.0);
        }
        let k0 = SeriesConvolution::kernel(&weights, 0.0);
        assert_eq!(k0, weights);
    }

    #[test]
    fn editor_cells_are_the_former_orders() {
        for size in [3, 5, 4, 0] {
            let mut former_read = Vec::new();
            let mut former_write = Vec::new();
            for x in 0..5 {
                for y in 0..5 {
                    if size == 3 && !(1..=3).contains(&x) || size == 3 && !(1..=3).contains(&y) {
                        continue;
                    }
                    if size != 3 && size != 5 {
                        continue;
                    }
                    former_read.push(KernelCell { row: y, column: x });
                    former_write.push(KernelCell { row: x, column: y });
                }
            }
            assert_eq!(ConvolutionKernelBook::reading_order(size), former_read);
            assert_eq!(ConvolutionKernelBook::writing_order(size), former_write);
        }
        assert_eq!(ConvolutionKernelBook::uses(0, 2, 3), Some(false));
        assert_eq!(ConvolutionKernelBook::uses(2, 2, 3), Some(true));
        assert_eq!(ConvolutionKernelBook::uses(0, 0, 5), Some(true));
        assert_eq!(ConvolutionKernelBook::uses(0, 0, 4), None);
        assert_eq!(
            ConvolutionKernelBook::sum(&["1", "2.5", "", "x", "-0.5"]),
            3.0
        );
    }

    #[test]
    fn factory_catalogue_has_all_22_horos_convolutions() {
        assert_eq!(FACTORY_CONVOLUTIONS.len(), 22);
        assert_eq!(FACTORY_CONVOLUTIONS[0].name, "Bone Filter 3x3");
        assert_eq!(FACTORY_CONVOLUTIONS[0].normalization, 1.0);
    }
}

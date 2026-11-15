//! GPU windowing for grey slices via wgpu (`eframe::egui_wgpu`).
//!
//! Raw `f32` values (after rescale slope/intercept and any VOI LUT table) are
//! uploaded once per slice as an `R32Float` texture and cached across viewports.
//! Each viewport slot has its own uniform buffer and 256×1 `Rgba8Unorm` CLUT
//! texture so multiple viewports in a comparison layout (`1×2`, `2×2`, `3×3`)
//! render simultaneously in a single frame with zero CPU re-windowing.

use std::collections::HashMap;
use std::sync::Arc;

use eframe::egui::{PaintCallbackInfo, Rect, Shape};
use eframe::egui_wgpu::{self, wgpu};

use crate::frame::{Frame, Pixels};
use crate::imaging::clut::ColorLookupTable;
use crate::imaging::Window;
use crate::loader::SliceKey;
use crate::volume::{MprPlaneGeometry, SlabMode, Volume3D};

const MAX_CACHED_TEXTURES: usize = 24;
const MAX_CACHED_VOLUMES: usize = 4;

#[derive(Clone, Copy, Debug)]
pub struct GpuInfo {
    pub max_texture: u32,
    pub max_texture_3d: u32,
}

const SHADER: &str = r#"
struct Uniforms {
    low: f32,
    inv_span: f32,
    high: f32,
    invert: u32,
    width: u32,
    height: u32,
    use_clut: u32,
    bilinear: u32,
    rect_min: vec2<f32>,
    rect_size: vec2<f32>,
    uv_origin: vec2<f32>,
    uv_u: vec2<f32>,
    uv_v: vec2<f32>,
    _pad1: vec2<f32>,
}

@group(0) @binding(0) var t_slice: texture_2d<f32>;
@group(0) @binding(1) var<uniform> u: Uniforms;
@group(0) @binding(2) var t_clut: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> @builtin(position) vec4<f32> {
    // Full-screen triangle covering the viewport clip rect.
    let x = f32(i32(idx & 1u) * 4 - 1);
    let y = f32(i32(idx & 2u) * 2 - 1);
    return vec4<f32>(x, y, 0.0, 1.0);
}

fn sample_windowed(coord: vec2<i32>) -> vec4<f32> {
    let cx = clamp(coord.x, 0, i32(u.width) - 1);
    let cy = clamp(coord.y, 0, i32(u.height) - 1);
    let v = textureLoad(t_slice, vec2<i32>(cx, cy), 0).r;

    var g: f32;
    if u.inv_span <= 0.0 {
        g = select(0.0, 1.0, v >= u.high);
    } else {
        g = clamp((v - u.low) * u.inv_span, 0.0, 1.0);
    }
    if u.invert != 0u {
        g = 1.0 - g;
    }
    if u.use_clut != 0u {
        let idx = clamp(i32(g * 255.0 + 0.5), 0, 255);
        let c = textureLoad(t_clut, vec2<i32>(idx, 0), 0);
        return vec4<f32>(c.rgb, 1.0);
    }
    return vec4<f32>(g, g, g, 1.0);
}

@fragment
fn fs_main(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let rel = (frag.xy - u.rect_min) / u.rect_size;
    if rel.x < 0.0 || rel.x >= 1.0 || rel.y < 0.0 || rel.y >= 1.0 {
        discard;
    }
    let tc = u.uv_origin + rel.x * u.uv_u + rel.y * u.uv_v;
    if tc.x < 0.0 || tc.x >= 1.0 || tc.y < 0.0 || tc.y >= 1.0 {
        discard;
    }
    if u.bilinear != 0u {
        let p = tc * vec2<f32>(f32(u.width), f32(u.height)) - vec2<f32>(0.5, 0.5);
        let p0 = vec2<i32>(floor(p));
        let f = fract(p);
        let c00 = sample_windowed(p0);
        let c10 = sample_windowed(p0 + vec2<i32>(1, 0));
        let c01 = sample_windowed(p0 + vec2<i32>(0, 1));
        let c11 = sample_windowed(p0 + vec2<i32>(1, 1));
        return mix(mix(c00, c10, f.x), mix(c01, c11, f.x), f.y);
    }
    let x = clamp(i32(tc.x * f32(u.width)), 0, i32(u.width) - 1);
    let y = clamp(i32(tc.y * f32(u.height)), 0, i32(u.height) - 1);
    return sample_windowed(vec2<i32>(x, y));
}
"#;

#[repr(C)]
#[derive(Clone, Copy)]
struct Uniforms {
    low: f32,
    inv_span: f32,
    high: f32,
    invert: u32,
    width: u32,
    height: u32,
    use_clut: u32,
    bilinear: u32,
    rect_min: [f32; 2],
    rect_size: [f32; 2],
    uv_origin: [f32; 2],
    uv_u: [f32; 2],
    uv_v: [f32; 2],
    _pad1: [f32; 2],
}

fn uniform_bytes<T>(u: &T) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts((u as *const T).cast::<u8>(), std::mem::size_of::<T>())
    }
}

const SHADER_3D: &str = r#"
struct Uniforms3D {
    low: f32,
    inv_span: f32,
    high: f32,
    invert: u32,
    width: u32,
    height: u32,
    use_clut: u32,
    vol_w: u32,
    vol_h: u32,
    vol_d: u32,
    slab_mode: u32,
    slab_steps: u32,
    voxel_scale: f32,
    voxel_offset: f32,
    bg_value: f32,
    _pad0: u32,
    rect_min: vec2<f32>,
    rect_size: vec2<f32>,
    uv_origin: vec2<f32>,
    uv_u: vec2<f32>,
    uv_v: vec2<f32>,
    _pad1: vec2<f32>,
    vox_origin: vec4<f32>,
    vox_u_step: vec4<f32>,
    vox_v_step: vec4<f32>,
    vox_slab_start: vec4<f32>,
    vox_slab_delta: vec4<f32>,
}

@group(0) @binding(0) var t_vol: texture_3d<i32>;
@group(0) @binding(1) var<uniform> u: Uniforms3D;
@group(0) @binding(2) var t_clut: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> @builtin(position) vec4<f32> {
    let x = f32(i32(idx & 1u) * 4 - 1);
    let y = f32(i32(idx & 2u) * 2 - 1);
    return vec4<f32>(x, y, 0.0, 1.0);
}

fn load_vox(x: i32, y: i32, z: i32) -> f32 {
    let q = textureLoad(t_vol, vec3<i32>(x, y, z), 0).r;
    return f32(q) * u.voxel_scale + u.voxel_offset;
}

fn sample_trilinear_3d(vox: vec3<f32>) -> vec2<f32> {
    if vox.x < 0.0 || vox.y < 0.0 || vox.z < 0.0
        || vox.x > f32(u.vol_w - 1u)
        || vox.y > f32(u.vol_h - 1u)
        || vox.z > f32(u.vol_d - 1u)
    {
        return vec2<f32>(u.bg_value, 0.0);
    }
    let x0 = i32(floor(vox.x));
    let y0 = i32(floor(vox.y));
    let z0 = i32(floor(vox.z));
    let x1 = min(x0 + 1, i32(u.vol_w) - 1);
    let y1 = min(y0 + 1, i32(u.vol_h) - 1);
    let z1 = min(z0 + 1, i32(u.vol_d) - 1);

    let tx = vox.x - f32(x0);
    let ty = vox.y - f32(y0);
    let tz = vox.z - f32(z0);

    let c000 = load_vox(x0, y0, z0);
    let c100 = load_vox(x1, y0, z0);
    let c010 = load_vox(x0, y1, z0);
    let c110 = load_vox(x1, y1, z0);
    let c001 = load_vox(x0, y0, z1);
    let c101 = load_vox(x1, y0, z1);
    let c011 = load_vox(x0, y1, z1);
    let c111 = load_vox(x1, y1, z1);

    let c00 = mix(c000, c100, tx);
    let c10 = mix(c010, c110, tx);
    let c01 = mix(c001, c101, tx);
    let c11 = mix(c011, c111, tx);
    let c0 = mix(c00, c10, ty);
    let c1 = mix(c01, c11, ty);
    return vec2<f32>(mix(c0, c1, tz), 1.0);
}

@fragment
fn fs_main(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let rel = (frag.xy - u.rect_min) / u.rect_size;
    if rel.x < 0.0 || rel.x >= 1.0 || rel.y < 0.0 || rel.y >= 1.0 {
        discard;
    }
    let tc = u.uv_origin + rel.x * u.uv_u + rel.y * u.uv_v;
    if tc.x < 0.0 || tc.x >= 1.0 || tc.y < 0.0 || tc.y >= 1.0 {
        discard;
    }

    let px = tc.x * f32(u.width) - 0.5;
    let py = tc.y * f32(u.height) - 0.5;
    let base_vox = u.vox_origin.xyz + px * u.vox_u_step.xyz + py * u.vox_v_step.xyz;

    var v: f32 = u.bg_value;
    if u.slab_mode == 0u || u.slab_steps <= 1u {
        v = sample_trilinear_3d(base_vox).x;
    } else if u.slab_mode == 1u {
        var best: f32 = -1e30;
        var any_hit: bool = false;
        var p = base_vox + u.vox_slab_start.xyz;
        for (var i: u32 = 0u; i < u.slab_steps; i = i + 1u) {
            let s = sample_trilinear_3d(p);
            if s.y > 0.5 {
                if !any_hit || s.x > best {
                    best = s.x;
                }
                any_hit = true;
            }
            p = p + u.vox_slab_delta.xyz;
        }
        if any_hit { v = best; }
    } else if u.slab_mode == 2u {
        var best: f32 = 1e30;
        var any_hit: bool = false;
        var p = base_vox + u.vox_slab_start.xyz;
        for (var i: u32 = 0u; i < u.slab_steps; i = i + 1u) {
            let s = sample_trilinear_3d(p);
            if s.y > 0.5 {
                if !any_hit || s.x < best {
                    best = s.x;
                }
                any_hit = true;
            }
            p = p + u.vox_slab_delta.xyz;
        }
        if any_hit { v = best; }
    } else {
        var sum: f32 = 0.0;
        var cnt: f32 = 0.0;
        var p = base_vox + u.vox_slab_start.xyz;
        for (var i: u32 = 0u; i < u.slab_steps; i = i + 1u) {
            let s = sample_trilinear_3d(p);
            if s.y > 0.5 {
                sum = sum + s.x;
                cnt = cnt + 1.0;
            }
            p = p + u.vox_slab_delta.xyz;
        }
        if cnt > 0.0 { v = sum / cnt; }
    }

    var g: f32;
    if u.inv_span <= 0.0 {
        g = select(0.0, 1.0, v >= u.high);
    } else {
        g = clamp((v - u.low) * u.inv_span, 0.0, 1.0);
    }
    if u.invert != 0u {
        g = 1.0 - g;
    }
    if u.use_clut != 0u {
        let idx = clamp(i32(g * 255.0 + 0.5), 0, 255);
        let c = textureLoad(t_clut, vec2<i32>(idx, 0), 0);
        return vec4<f32>(c.rgb, 1.0);
    }
    return vec4<f32>(g, g, g, 1.0);
}
"#;

#[repr(C)]
#[derive(Clone, Copy)]
struct Uniforms3D {
    low: f32,
    inv_span: f32,
    high: f32,
    invert: u32,
    width: u32,
    height: u32,
    use_clut: u32,
    vol_w: u32,
    vol_h: u32,
    vol_d: u32,
    slab_mode: u32,
    slab_steps: u32,
    voxel_scale: f32,
    voxel_offset: f32,
    bg_value: f32,
    _pad0: u32,
    rect_min: [f32; 2],
    rect_size: [f32; 2],
    uv_origin: [f32; 2],
    uv_u: [f32; 2],
    uv_v: [f32; 2],
    _pad1: [f32; 2],
    vox_origin: [f32; 4],
    vox_u_step: [f32; 4],
    vox_v_step: [f32; 4],
    vox_slab_start: [f32; 4],
    vox_slab_delta: [f32; 4],
}

struct UploadedSlice {
    key: SliceKey,
    view: wgpu::TextureView,
}

struct UploadedVolume {
    gpu_id: u64,
    view: wgpu::TextureView,
}

struct ViewportGpuSlot {
    uniform_buf: wgpu::Buffer,
    clut_tex: wgpu::Texture,
    clut_view: wgpu::TextureView,
    bind_group: Option<wgpu::BindGroup>,
    bound_slice: Option<SliceKey>,
    uploaded_clut: Option<ColorLookupTable>,
}

struct ViewportGpu3DSlot {
    uniform_buf: wgpu::Buffer,
    clut_tex: wgpu::Texture,
    clut_view: wgpu::TextureView,
    bind_group: Option<wgpu::BindGroup>,
    bound_volume: Option<u64>,
    uploaded_clut: Option<ColorLookupTable>,
}

fn upload_clut_if_changed(
    queue: &wgpu::Queue,
    clut_tex: &wgpu::Texture,
    uploaded_clut: &mut Option<ColorLookupTable>,
    clut: Option<&ColorLookupTable>,
) {
    if let Some(clut) = clut
        && uploaded_clut.as_ref() != Some(clut)
    {
        let mut rgba = [0u8; 256 * 4];
        for i in 0..256 {
            rgba[i * 4] = clut.red[i];
            rgba[i * 4 + 1] = clut.green[i];
            rgba[i * 4 + 2] = clut.blue[i];
            rgba[i * 4 + 3] = 255;
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: clut_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256 * 4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 256,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        *uploaded_clut = Some(clut.clone());
    }
}

fn create_clut_texture(device: &wgpu::Device) -> (wgpu::Texture, wgpu::TextureView) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("rdicom-slice-clut"),
        size: wgpu::Extent3d {
            width: 256,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    (tex, view)
}

struct Resources {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    pipeline_3d: wgpu::RenderPipeline,
    layout_3d: wgpu::BindGroupLayout,
    slices: Vec<UploadedSlice>,
    volumes: Vec<UploadedVolume>,
    slots: HashMap<usize, ViewportGpuSlot>,
    slots_3d: HashMap<usize, ViewportGpu3DSlot>,
}

impl Resources {
    fn get_or_upload_slice(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: SliceKey,
        width: u32,
        height: u32,
        pixels: &[f32],
    ) {
        if let Some(pos) = self.slices.iter().position(|s| s.key == key) {
            let entry = self.slices.remove(pos);
            self.slices.push(entry);
            return;
        }

        while self.slices.len() >= MAX_CACHED_TEXTURES {
            let evicted = self.slices.remove(0);
            for slot in self.slots.values_mut() {
                if slot.bound_slice == Some(evicted.key) {
                    slot.bind_group = None;
                    slot.bound_slice = None;
                }
            }
        }

        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rdicom-slice-r32f"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let bytes: &[u8] =
            unsafe { std::slice::from_raw_parts(pixels.as_ptr().cast::<u8>(), pixels.len() * 4) };
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            size,
        );
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        self.slices.push(UploadedSlice { key, view });
    }

    fn get_or_upload_volume(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vol: &Volume3D,
    ) {
        if let Some(pos) = self.volumes.iter().position(|v| v.gpu_id == vol.gpu_id) {
            let entry = self.volumes.remove(pos);
            self.volumes.push(entry);
            return;
        }

        while self.volumes.len() >= MAX_CACHED_VOLUMES {
            let evicted = self.volumes.remove(0);
            for slot in self.slots_3d.values_mut() {
                if slot.bound_volume == Some(evicted.gpu_id) {
                    slot.bind_group = None;
                    slot.bound_volume = None;
                }
            }
        }

        let size = wgpu::Extent3d {
            width: vol.width as u32,
            height: vol.height as u32,
            depth_or_array_layers: vol.depth as u32,
        };
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rdicom-volume-r16s"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::R16Sint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(vol.voxels.as_ptr().cast::<u8>(), vol.voxels.len() * 2)
        };
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(vol.width as u32 * 2),
                rows_per_image: Some(vol.height as u32),
            },
            size,
        );
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        self.volumes.push(UploadedVolume {
            gpu_id: vol.gpu_id,
            view,
        });
    }
}

/// Installs the wgpu slice shader pipeline at startup and returns GPU limits.
pub fn install(cc: &eframe::CreationContext<'_>) -> Option<GpuInfo> {
    let rs = cc.wgpu_render_state.as_ref()?;
    let device = &rs.device;

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("rdicom-slice"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });

    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("rdicom-slice-bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("rdicom-slice-pl"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("rdicom-slice-rp"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(rs.target_format.into())],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    let shader_3d = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("rdicom-volume-mpr"),
        source: wgpu::ShaderSource::Wgsl(SHADER_3D.into()),
    });

    let layout_3d = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("rdicom-volume-mpr-bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Sint,
                    view_dimension: wgpu::TextureViewDimension::D3,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    });

    let pipeline_layout_3d = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("rdicom-volume-mpr-pl"),
        bind_group_layouts: &[Some(&layout_3d)],
        immediate_size: 0,
    });

    let pipeline_3d = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("rdicom-volume-mpr-rp"),
        layout: Some(&pipeline_layout_3d),
        vertex: wgpu::VertexState {
            module: &shader_3d,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader_3d,
            entry_point: Some("fs_main"),
            targets: &[Some(rs.target_format.into())],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    let limits = device.limits();
    let max_texture = limits.max_texture_dimension_2d;
    let max_texture_3d = limits.max_texture_dimension_3d;

    rs.renderer
        .write()
        .callback_resources
        .insert(Resources {
            pipeline,
            layout,
            pipeline_3d,
            layout_3d,
            slices: Vec::with_capacity(MAX_CACHED_TEXTURES),
            volumes: Vec::with_capacity(MAX_CACHED_VOLUMES),
            slots: HashMap::new(),
            slots_3d: HashMap::new(),
        });
    Some(GpuInfo {
        max_texture,
        max_texture_3d,
    })
}

/// UV mapping from screen quad `[0,1]^2` into normalized slice texel coordinates `[0,1]^2`.
#[derive(Clone, Copy, Debug)]
pub struct QuadUvMapping {
    pub origin: [f32; 2],
    pub u: [f32; 2],
    pub v: [f32; 2],
}

impl Default for QuadUvMapping {
    fn default() -> Self {
        Self {
            origin: [0.0, 0.0],
            u: [1.0, 0.0],
            v: [0.0, 1.0],
        }
    }
}

impl QuadUvMapping {
    /// Computes the UV mapping for an in-plane rotation (degrees) and X/Y flip.
    pub fn from_orientation(rotation_degrees: f32, x_flipped: bool, y_flipped: bool) -> Self {
        let mut rad = -(rotation_degrees as f64) * std::f64::consts::PI / 180.0;
        if x_flipped != y_flipped {
            rad = -rad;
        }
        let c = rad.cos() as f32;
        let s = rad.sin() as f32;

        let sx = if x_flipped { -1.0f32 } else { 1.0f32 };
        let sy = if y_flipped { -1.0f32 } else { 1.0f32 };

        let u = [sx * c, sx * s];
        let v = [-sy * s, sy * c];
        let origin = [
            0.5 - 0.5 * (u[0] + v[0]),
            0.5 - 0.5 * (u[1] + v[1]),
        ];
        Self { origin, u, v }
    }
}

pub struct ImageCallback {
    pub viewport_id: usize,
    pub frame: Arc<Frame>,
    pub key: SliceKey,
    pub image_rect_px: Rect,
    pub window: Window,
    pub invert: bool,
    pub clut: Option<ColorLookupTable>,
    pub uv_map: QuadUvMapping,
    pub bilinear: bool,
}

impl ImageCallback {
    pub fn into_shape(self, clip_rect: Rect) -> Shape {
        Shape::Callback(egui_wgpu::Callback::new_paint_callback(clip_rect, self))
    }
}

impl egui_wgpu::CallbackTrait for ImageCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Pixels::Gray(ref pixels) = self.frame.pixels else {
            return Vec::new();
        };
        let Some(res) = resources.get_mut::<Resources>() else {
            return Vec::new();
        };

        res.get_or_upload_slice(
            device,
            queue,
            self.key,
            self.frame.width,
            self.frame.height,
            pixels,
        );

        let slot = res.slots.entry(self.viewport_id).or_insert_with(|| {
            let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rdicom-slice-uniform"),
                size: std::mem::size_of::<Uniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let (clut_tex, clut_view) = create_clut_texture(device);
            ViewportGpuSlot {
                uniform_buf,
                clut_tex,
                clut_view,
                bind_group: None,
                bound_slice: None,
                uploaded_clut: None,
            }
        });

        upload_clut_if_changed(
            queue,
            &slot.clut_tex,
            &mut slot.uploaded_clut,
            self.clut.as_ref(),
        );

        if (slot.bound_slice != Some(self.key) || slot.bind_group.is_none())
            && let Some(uploaded) = res.slices.iter().find(|s| s.key == self.key) {
                let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("rdicom-slice-bg"),
                    layout: &res.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&uploaded.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: slot.uniform_buf.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&slot.clut_view),
                        },
                    ],
                });
                slot.bind_group = Some(bg);
                slot.bound_slice = Some(self.key);
            }

        let low = self.window.low();
        let high = self.window.high();
        let span = high - low;
        let u = Uniforms {
            low,
            inv_span: if span > 0.0 { 1.0 / span } else { 0.0 },
            high,
            invert: u32::from(self.invert),
            width: self.frame.width,
            height: self.frame.height,
            use_clut: u32::from(self.clut.is_some()),
            bilinear: u32::from(self.bilinear),
            rect_min: [self.image_rect_px.min.x, self.image_rect_px.min.y],
            rect_size: [
                self.image_rect_px.width().max(1.0),
                self.image_rect_px.height().max(1.0),
            ],
            uv_origin: self.uv_map.origin,
            uv_u: self.uv_map.u,
            uv_v: self.uv_map.v,
            _pad1: [0.0, 0.0],
        };
        queue.write_buffer(&slot.uniform_buf, 0, uniform_bytes(&u));
        Vec::new()
    }

    fn paint(
        &self,
        _info: PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(res) = resources.get::<Resources>() else {
            return;
        };
        let Some(slot) = res.slots.get(&self.viewport_id) else {
            return;
        };
        let Some(bg) = &slot.bind_group else {
            return;
        };
        render_pass.set_pipeline(&res.pipeline);
        render_pass.set_bind_group(0, bg, &[]);
        render_pass.draw(0..3, 0..1);
    }
}

/// GPU 3D-texture MPR reslicing & windowing callback.
pub struct VolumeMprCallback {
    pub viewport_id: usize,
    pub volume: Arc<Volume3D>,
    pub geom: MprPlaneGeometry,
    pub image_rect_px: Rect,
    pub window: Window,
    pub invert: bool,
    pub clut: Option<ColorLookupTable>,
    pub uv_map: QuadUvMapping,
}

impl VolumeMprCallback {
    pub fn into_shape(self, clip_rect: Rect) -> Shape {
        Shape::Callback(egui_wgpu::Callback::new_paint_callback(clip_rect, self))
    }
}

impl egui_wgpu::CallbackTrait for VolumeMprCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(res) = resources.get_mut::<Resources>() else {
            return Vec::new();
        };

        res.get_or_upload_volume(device, queue, &self.volume);

        let slot = res.slots_3d.entry(self.viewport_id).or_insert_with(|| {
            let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rdicom-volume-mpr-uniform"),
                size: std::mem::size_of::<Uniforms3D>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let (clut_tex, clut_view) = create_clut_texture(device);
            ViewportGpu3DSlot {
                uniform_buf,
                clut_tex,
                clut_view,
                bind_group: None,
                bound_volume: None,
                uploaded_clut: None,
            }
        });

        upload_clut_if_changed(
            queue,
            &slot.clut_tex,
            &mut slot.uploaded_clut,
            self.clut.as_ref(),
        );

        if (slot.bound_volume != Some(self.volume.gpu_id) || slot.bind_group.is_none())
            && let Some(uploaded) = res.volumes.iter().find(|v| v.gpu_id == self.volume.gpu_id)
        {
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("rdicom-volume-mpr-bg"),
                layout: &res.layout_3d,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&uploaded.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: slot.uniform_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&slot.clut_view),
                    },
                ],
            });
            slot.bind_group = Some(bg);
            slot.bound_volume = Some(self.volume.gpu_id);
        }

        let low = self.window.low();
        let high = self.window.high();
        let span = high - low;
        let slab_mode = match self.geom.slab_mode {
            SlabMode::Thin => 0u32,
            SlabMode::Mip => 1u32,
            SlabMode::MinIp => 2u32,
            SlabMode::Mean => 3u32,
        };
        let u = Uniforms3D {
            low,
            inv_span: if span > 0.0 { 1.0 / span } else { 0.0 },
            high,
            invert: u32::from(self.invert),
            width: self.geom.width,
            height: self.geom.height,
            use_clut: u32::from(self.clut.is_some()),
            vol_w: self.volume.width as u32,
            vol_h: self.volume.height as u32,
            vol_d: self.volume.depth as u32,
            slab_mode,
            slab_steps: self.geom.slab_steps,
            voxel_scale: self.volume.voxel_scale,
            voxel_offset: self.volume.voxel_offset,
            bg_value: self.volume.background_value(),
            _pad0: 0,
            rect_min: [self.image_rect_px.min.x, self.image_rect_px.min.y],
            rect_size: [
                self.image_rect_px.width().max(1.0),
                self.image_rect_px.height().max(1.0),
            ],
            uv_origin: self.uv_map.origin,
            uv_u: self.uv_map.u,
            uv_v: self.uv_map.v,
            _pad1: [0.0, 0.0],
            vox_origin: [
                self.geom.vox_origin[0],
                self.geom.vox_origin[1],
                self.geom.vox_origin[2],
                0.0,
            ],
            vox_u_step: [
                self.geom.vox_u_step[0],
                self.geom.vox_u_step[1],
                self.geom.vox_u_step[2],
                0.0,
            ],
            vox_v_step: [
                self.geom.vox_v_step[0],
                self.geom.vox_v_step[1],
                self.geom.vox_v_step[2],
                0.0,
            ],
            vox_slab_start: [
                self.geom.vox_slab_start[0],
                self.geom.vox_slab_start[1],
                self.geom.vox_slab_start[2],
                0.0,
            ],
            vox_slab_delta: [
                self.geom.vox_slab_delta[0],
                self.geom.vox_slab_delta[1],
                self.geom.vox_slab_delta[2],
                0.0,
            ],
        };
        queue.write_buffer(&slot.uniform_buf, 0, uniform_bytes(&u));
        Vec::new()
    }

    fn paint(
        &self,
        _info: PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(res) = resources.get::<Resources>() else {
            return;
        };
        let Some(slot) = res.slots_3d.get(&self.viewport_id) else {
            return;
        };
        let Some(bg) = &slot.bind_group else {
            return;
        };
        render_pass.set_pipeline(&res.pipeline_3d);
        render_pass.set_bind_group(0, bg, &[]);
        render_pass.draw(0..3, 0..1);
    }
}

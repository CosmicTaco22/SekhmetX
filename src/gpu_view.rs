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

const MAX_CACHED_TEXTURES: usize = 24;

#[derive(Clone, Copy, Debug)]
pub struct GpuInfo {
    pub max_texture: u32,
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

fn uniform_bytes(u: &Uniforms) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(
            (u as *const Uniforms).cast::<u8>(),
            std::mem::size_of::<Uniforms>(),
        )
    }
}

struct UploadedSlice {
    key: SliceKey,
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

struct Resources {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    slices: Vec<UploadedSlice>,
    slots: HashMap<usize, ViewportGpuSlot>,
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

    let max_texture = device.limits().max_texture_dimension_2d;

    rs.renderer
        .write()
        .callback_resources
        .insert(Resources {
            pipeline,
            layout,
            slices: Vec::with_capacity(MAX_CACHED_TEXTURES),
            slots: HashMap::new(),
        });
    Some(GpuInfo { max_texture })
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
            let clut_tex = device.create_texture(&wgpu::TextureDescriptor {
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
            let clut_view = clut_tex.create_view(&wgpu::TextureViewDescriptor::default());
            ViewportGpuSlot {
                uniform_buf,
                clut_tex,
                clut_view,
                bind_group: None,
                bound_slice: None,
                uploaded_clut: None,
            }
        });

        if let Some(ref clut) = self.clut {
            if slot.uploaded_clut.as_ref() != Some(clut) {
                let mut rgba = [0u8; 256 * 4];
                for i in 0..256 {
                    rgba[i * 4] = clut.red[i];
                    rgba[i * 4 + 1] = clut.green[i];
                    rgba[i * 4 + 2] = clut.blue[i];
                    rgba[i * 4 + 3] = 255;
                }
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &slot.clut_tex,
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
                slot.uploaded_clut = Some(clut.clone());
            }
        }

        if slot.bound_slice != Some(self.key) || slot.bind_group.is_none() {
            if let Some(uploaded) = res.slices.iter().find(|s| s.key == self.key) {
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

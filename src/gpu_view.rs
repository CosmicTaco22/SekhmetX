//! The image drawn by the graphics card.
//!
//! Each slice's values go to the GPU once, as a 32-bit float texture (or RGBA
//! for colour images). Changing the window only rewrites a 48-byte uniform: the
//! shader applies Horos's window to each texel, then interpolates between
//! texels, the order Horos uses (window to bytes, then the bytes are filtered),
//! so it matches the CPU path in `imaging::window`.
//!
//! The quad covers the whole viewport and the fragment shader places the image
//! itself, so a zoomed or panned image partly off screen is never distorted.

use std::sync::Arc;

use eframe::egui;
use eframe::egui_wgpu::{self, wgpu};

use crate::frame::{Frame, Pixels};
use crate::imaging::Window;

const SHADER: &str = r#"
struct Params {
    img_min: vec2<f32>,   // image rectangle, framebuffer pixels
    img_size: vec2<f32>,
    tex_size: vec2<f32>,
    low: f32,
    high: f32,
    invert: f32,
    colour: f32,
    srgb_target: f32,
    _pad: f32,
};

@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var grey: texture_2d<f32>;
@group(0) @binding(2) var rgba: texture_2d<f32>;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    // One triangle that covers the viewport.
    let x = f32((i << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(i & 2u) * 2.0 - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}

// Horos's straight window to a grey byte: low is black, high is white.
fn window_grey(v: f32) -> f32 {
    if (p.high <= p.low) {
        return select(0.0, 255.0, v >= p.high);
    }
    if (v <= p.low) { return 0.0; }
    if (v >= p.high) { return 255.0; }
    return floor((v - p.low) * (255.0 / (p.high - p.low)) + 0.5);
}

// Horos's colour table: each channel byte through the window, truncated.
fn window_channel(c: f32) -> f32 {
    let index = round(c * 255.0);
    var difference = trunc(p.high - p.low);
    if (difference == 0.0) { difference = 1.0; }
    return clamp(trunc((index - p.low) * 255.0 / difference), 0.0, 255.0);
}

fn shade(t: vec2<i32>) -> vec3<f32> {
    let last = vec2<i32>(p.tex_size) - vec2<i32>(1, 1);
    let c = clamp(t, vec2<i32>(0, 0), last);
    var g: vec3<f32>;
    if (p.colour > 0.5) {
        let s = textureLoad(rgba, c, 0).rgb;
        g = vec3<f32>(window_channel(s.r), window_channel(s.g), window_channel(s.b));
    } else {
        g = vec3<f32>(window_grey(textureLoad(grey, c, 0).r));
    }
    if (p.invert > 0.5) { g = vec3<f32>(255.0) - g; }
    return g;
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

@fragment
fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = (pos.xy - p.img_min) / p.img_size;
    if (any(uv < vec2<f32>(0.0)) || any(uv >= vec2<f32>(1.0))) {
        discard;
    }
    // Bilinear between the four nearest texels, each windowed first.
    let t = uv * p.tex_size - 0.5;
    let base = floor(t);
    let f = t - base;
    let i = vec2<i32>(base);
    let top = mix(shade(i), shade(i + vec2<i32>(1, 0)), f.x);
    let bottom = mix(shade(i + vec2<i32>(0, 1)), shade(i + vec2<i32>(1, 1)), f.x);
    var g = mix(top, bottom, f.y) / 255.0;
    if (p.srgb_target > 0.5) {
        g = srgb_to_linear(g);
    }
    return vec4<f32>(g, 1.0);
}
"#;

/// Identifies the slice whose values are on the GPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SliceKey {
    pub series: u64,
    pub index: usize,
}

struct Uploaded {
    key: SliceKey,
    size: (u32, u32),
    colour: bool,
    _texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

/// GPU objects, kept in egui-wgpu's callback resources.
struct Resources {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    dummy_grey: wgpu::TextureView,
    dummy_rgba: wgpu::TextureView,
    srgb_target: bool,
    current: Option<Uploaded>,
}

/// What the app needs to know about the GPU.
#[derive(Debug, Clone, Copy)]
pub struct GpuInfo {
    pub max_texture: u32,
}

/// Sets the pipeline up once at startup. Returns `None` when eframe isn't
/// drawing through wgpu.
pub fn install(cc: &eframe::CreationContext<'_>) -> Option<GpuInfo> {
    let rs = cc.wgpu_render_state.as_ref()?;
    let device = &rs.device;

    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("rdicom_image"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    };
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("rdicom_image_layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            texture_entry(1),
            texture_entry(2),
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("rdicom_image_pipeline_layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("rdicom_image_pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs"),
            targets: &[Some(wgpu::ColorTargetState {
                format: rs.target_format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        multiview_mask: None,
        cache: None,
    });
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rdicom_image_params"),
        size: 48,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let dummy = |format| {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("rdicom_dummy"),
                size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default())
    };
    let resources = Resources {
        pipeline,
        layout,
        uniform,
        dummy_grey: dummy(wgpu::TextureFormat::R32Float),
        dummy_rgba: dummy(wgpu::TextureFormat::Rgba8Unorm),
        srgb_target: rs.target_format.is_srgb(),
        current: None,
    };
    rs.renderer.write().callback_resources.insert(resources);
    Some(GpuInfo { max_texture: device.limits().max_texture_dimension_2d })
}

/// One draw of one slice.
pub struct ImageCallback {
    pub frame: Arc<Frame>,
    pub key: SliceKey,
    pub image_rect_px: egui::Rect,
    pub window: Window,
    pub invert: bool,
}

impl ImageCallback {
    pub fn into_shape(self, viewport: egui::Rect) -> egui::Shape {
        egui::Shape::Callback(egui_wgpu::Callback::new_paint_callback(viewport, self))
    }
}

impl egui_wgpu::CallbackTrait for ImageCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(res) = resources.get_mut::<Resources>() else { return Vec::new() };
        let f = &self.frame;
        let colour = matches!(f.pixels, Pixels::Rgba(_));

        // Upload the slice only when it changed.
        if res.current.as_ref().map(|u| u.key) != Some(self.key) {
            let size = wgpu::Extent3d { width: f.width, height: f.height, depth_or_array_layers: 1 };
            let format = if colour { wgpu::TextureFormat::Rgba8Unorm } else { wgpu::TextureFormat::R32Float };
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("rdicom_slice"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let bytes: &[u8] = match &f.pixels {
                Pixels::Gray(v) => bytemuck::cast_slice(v),
                Pixels::Rgba(v) => v,
            };
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytes,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * f.width), rows_per_image: Some(f.height) },
                size,
            );
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let (grey_view, rgba_view) = if colour { (&res.dummy_grey, &view) } else { (&view, &res.dummy_rgba) };
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("rdicom_slice_bind_group"),
                layout: &res.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: res.uniform.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(grey_view) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(rgba_view) },
                ],
            });
            res.current = Some(Uploaded { key: self.key, size: (f.width, f.height), colour, _texture: texture, bind_group });
        }

        // The window: 48 bytes, every frame.
        let r = self.image_rect_px;
        let params: [f32; 12] = [
            r.min.x,
            r.min.y,
            r.width(),
            r.height(),
            f.width as f32,
            f.height as f32,
            self.window.low(),
            self.window.high(),
            if self.invert { 1.0 } else { 0.0 },
            if colour { 1.0 } else { 0.0 },
            if res.srgb_target { 1.0 } else { 0.0 },
            0.0,
        ];
        queue.write_buffer(&res.uniform, 0, bytemuck::cast_slice(&params));
        Vec::new()
    }

    fn paint(&self, _info: egui::PaintCallbackInfo, pass: &mut wgpu::RenderPass<'static>, resources: &egui_wgpu::CallbackResources) {
        let Some(res) = resources.get::<Resources>() else { return };
        let Some(up) = &res.current else { return };
        if up.size != (self.frame.width, self.frame.height) || up.colour != matches!(self.frame.pixels, Pixels::Rgba(_)) {
            return;
        }
        pass.set_pipeline(&res.pipeline);
        pass.set_bind_group(0, &up.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

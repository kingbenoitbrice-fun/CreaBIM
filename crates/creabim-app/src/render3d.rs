//! The 3D viewport: renders the model with wgpu into an offscreen texture
//! that egui shows like an image. Any number of these can exist, which is
//! what lets one model appear in several live views at once.

use eframe::egui;
use eframe::egui_wgpu::{self, wgpu};
use glam::Mat4;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    light: [f32; 4],
}

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SAMPLES: u32 = 4;

const SHADER: &str = r#"
struct U { view_proj: mat4x4<f32>, light: vec4<f32> };
@group(0) @binding(0) var<uniform> u: U;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
};
struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec3<f32>,
};

@vertex fn vs_main(v: VIn) -> VOut {
    var o: VOut;
    o.clip = u.view_proj * vec4<f32>(v.pos, 1.0);
    o.normal = v.normal;
    o.color = v.color;
    return o;
}

@fragment fn fs_shaded(i: VOut) -> @location(0) vec4<f32> {
    let n = normalize(i.normal);
    let diffuse = max(dot(n, normalize(u.light.xyz)), 0.0);
    let sky = 0.5 + 0.5 * n.z;
    // colours are given in sRGB; the target is sRGB, so shade in linear space
    let c = pow(i.color, vec3<f32>(2.2)) * (0.30 + 0.25 * sky + 0.55 * diffuse);
    return vec4<f32>(c, 1.0);
}

@fragment fn fs_flat(i: VOut) -> @location(0) vec4<f32> {
    return vec4<f32>(pow(i.color, vec3<f32>(2.2)), 1.0);
}
"#;

struct Mesh {
    vertices: wgpu::Buffer,
    indices: Option<wgpu::Buffer>,
    count: u32,
}

struct Targets {
    size: [u32; 2],
    msaa: wgpu::TextureView,
    resolve: wgpu::TextureView,
    depth: wgpu::TextureView,
}

pub struct Viewport3d {
    shaded: wgpu::RenderPipeline,
    lines: wgpu::RenderPipeline,
    overlay_lines: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    triangles: Option<Mesh>,
    line_mesh: Option<Mesh>,
    overlay_mesh: Option<Mesh>,
    targets: Option<Targets>,
    texture_id: Option<egui::TextureId>,
}

impl Viewport3d {
    pub fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("creabim_shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("creabim_uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("creabim_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("creabim_bg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("creabim_layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let pipeline = |topology, fragment: &str, depth_compare| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(fragment),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_main"),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3],
                    })],
                    compilation_options: Default::default(),
                },
                primitive: wgpu::PrimitiveState {
                    topology,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(depth_compare != wgpu::CompareFunction::Always),
                    depth_compare: Some(depth_compare),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: SAMPLES,
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fragment),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        Self {
            shaded: pipeline(
                wgpu::PrimitiveTopology::TriangleList,
                "fs_shaded",
                wgpu::CompareFunction::Less,
            ),
            lines: pipeline(
                wgpu::PrimitiveTopology::LineList,
                "fs_flat",
                wgpu::CompareFunction::LessEqual,
            ),
            overlay_lines: pipeline(
                wgpu::PrimitiveTopology::LineList,
                "fs_flat",
                wgpu::CompareFunction::Always,
            ),
            uniforms,
            bind_group,
            triangles: None,
            line_mesh: None,
            overlay_mesh: None,
            targets: None,
            texture_id: None,
        }
    }

    /// Replace the geometry: shaded triangles, depth-tested lines, and lines
    /// drawn on top of everything (2D drawings shown in 3D space).
    pub fn set_geometry(
        &mut self,
        device: &wgpu::Device,
        vertices: &[Vertex],
        indices: &[u32],
        lines: &[Vertex],
        overlay: &[Vertex],
    ) {
        let buffer = |data: &[u8], usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: data,
                usage,
            })
        };
        self.triangles = (!indices.is_empty()).then(|| Mesh {
            vertices: buffer(bytemuck::cast_slice(vertices), wgpu::BufferUsages::VERTEX),
            indices: Some(buffer(
                bytemuck::cast_slice(indices),
                wgpu::BufferUsages::INDEX,
            )),
            count: indices.len() as u32,
        });
        let line_mesh = |v: &[Vertex]| {
            (!v.is_empty()).then(|| Mesh {
                vertices: buffer(bytemuck::cast_slice(v), wgpu::BufferUsages::VERTEX),
                indices: None,
                count: v.len() as u32,
            })
        };
        self.line_mesh = line_mesh(lines);
        self.overlay_mesh = line_mesh(overlay);
    }

    fn ensure_targets(&mut self, rs: &egui_wgpu::RenderState, size: [u32; 2]) {
        if self.targets.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let device = &rs.device;
        let texture = |samples, format, usage| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: None,
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let t = Targets {
            size,
            msaa: texture(
                SAMPLES,
                COLOR_FORMAT,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            ),
            resolve: texture(
                1,
                COLOR_FORMAT,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            ),
            depth: texture(
                SAMPLES,
                DEPTH_FORMAT,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            ),
        };
        let mut renderer = rs.renderer.write();
        match self.texture_id {
            Some(id) => renderer.update_egui_texture_from_wgpu_texture(
                device,
                &t.resolve,
                wgpu::FilterMode::Linear,
                id,
            ),
            None => {
                self.texture_id = Some(renderer.register_native_texture(
                    device,
                    &t.resolve,
                    wgpu::FilterMode::Linear,
                ))
            }
        }
        self.targets = Some(t);
    }

    /// Render a frame and return the egui texture holding it.
    pub fn render(
        &mut self,
        rs: &egui_wgpu::RenderState,
        size: [u32; 2],
        view_proj: Mat4,
        background: [f64; 3],
    ) -> egui::TextureId {
        let size = [size[0].max(1), size[1].max(1)];
        self.ensure_targets(rs, size);
        let u = Uniforms {
            view_proj: view_proj.to_cols_array_2d(),
            light: [0.4, -0.6, 0.85, 0.0],
        };
        rs.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&u));
        let t = self.targets.as_ref().expect("ensured above");
        let mut encoder = rs.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("creabim_3d"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.msaa,
                    resolve_target: Some(&t.resolve),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: background[0],
                            g: background[1],
                            b: background[2],
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Discard,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &t.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.bind_group, &[]);
            for (pipeline, mesh) in [
                (&self.shaded, &self.triangles),
                (&self.lines, &self.line_mesh),
                (&self.overlay_lines, &self.overlay_mesh),
            ] {
                let Some(m) = mesh else { continue };
                pass.set_pipeline(pipeline);
                pass.set_vertex_buffer(0, m.vertices.slice(..));
                match &m.indices {
                    Some(ib) => {
                        pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..m.count, 0, 0..1);
                    }
                    None => pass.draw(0..m.count, 0..1),
                }
            }
        }
        rs.queue.submit([encoder.finish()]);
        self.texture_id.expect("registered in ensure_targets")
    }
}
